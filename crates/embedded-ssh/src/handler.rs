//! SSH session handler implementing `russh::server::Handler`.
//!
//! Handles public key authentication (matched against relay signing sessions),
//! shell/exec channels with PTY support, and SFTP subsystem requests.

use std::collections::HashMap;

use async_trait::async_trait;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use relay_control::signing::RelaySigningService;
use russh::{
    Channel, ChannelId, CryptoVec, Pty,
    server::{Auth, Msg, Session},
};
use russh_keys::PublicKey;
use tokio::sync::mpsc;

use crate::sftp::SftpHandler;

pub struct SshSessionHandler {
    relay_signing: RelaySigningService,
    channels: HashMap<ChannelId, ChannelState>,
    pending_env: HashMap<String, String>,
}

enum ChannelState {
    Pending {
        channel: Channel<Msg>,
        pty_params: Option<PtyParams>,
    },
    Active {
        writer_tx: mpsc::Sender<Vec<u8>>,
        pty_master: Box<dyn portable_pty::MasterPty + Send>,
    },
}

struct PtyParams {
    term: String,
    cols: u16,
    rows: u16,
}

impl SshSessionHandler {
    pub fn new(relay_signing: RelaySigningService) -> Self {
        Self {
            relay_signing,
            channels: HashMap::new(),
            pending_env: HashMap::new(),
        }
    }

    fn spawn_pty_session(
        &mut self,
        channel_id: ChannelId,
        command: Option<&str>,
        session: &mut Session,
    ) -> Result<(), anyhow::Error> {
        let state = self
            .channels
            .remove(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("Channel not found"))?;

        let (_channel, pty_params) = match state {
            ChannelState::Pending {
                channel,
                pty_params,
            } => (channel, pty_params),
            ChannelState::Active { .. } => {
                anyhow::bail!("Channel already has an active session");
            }
        };

        let (cols, rows, term) = match &pty_params {
            Some(p) => (p.cols, p.rows, p.term.clone()),
            None => (80, 24, "xterm-256color".to_string()),
        };

        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow::anyhow!("Failed to open PTY: {e}"))?;

        let mut cmd = match command {
            Some(cmd_str) => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                let mut cmd = CommandBuilder::new(&shell);
                cmd.arg("-c");
                cmd.arg(cmd_str);
                cmd
            }
            None => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                let mut cmd = CommandBuilder::new(&shell);
                cmd.arg("-l");
                cmd
            }
        };

        cmd.env("TERM", &term);
        for (k, v) in &self.pending_env {
            cmd.env(k, v);
        }

        if let Ok(home) = std::env::var("HOME") {
            cmd.cwd(&home);
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow::anyhow!("Failed to spawn command: {e}"))?;

        // Drop the slave side — the child owns it now
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| anyhow::anyhow!("Failed to clone PTY reader: {e}"))?;

        // Channel for writing stdin data to PTY
        let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(64);

        let mut writer = pair
            .master
            .take_writer()
            .map_err(|e| anyhow::anyhow!("Failed to take PTY writer: {e}"))?;

        // Background task: PTY writer (receives data from SSH channel → writes to PTY)
        tokio::spawn(async move {
            while let Some(data) = writer_rx.recv().await {
                use std::io::Write;
                if writer.write_all(&data).is_err() {
                    break;
                }
            }
        });

        // Background task: PTY reader → SSH channel data
        let handle = session.handle();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            loop {
                use std::io::Read;
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = CryptoVec::from_slice(&buf[..n]);
                        if handle.data(channel_id, data).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Wait for child to exit and send exit status
            let exit_code = match child.wait() {
                Ok(status) => status.exit_code(),
                Err(_) => 1,
            };
            let _ = handle
                .exit_status_request(channel_id, exit_code as u32)
                .await;
            let _ = handle.eof(channel_id).await;
            let _ = handle.close(channel_id).await;
        });

        self.channels.insert(
            channel_id,
            ChannelState::Active {
                writer_tx,
                pty_master: pair.master,
            },
        );

        let _ = session.channel_success(channel_id);
        Ok(())
    }
}

#[async_trait]
impl russh::server::Handler for SshSessionHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        // Extract raw Ed25519 bytes from the SSH public key
        let ed25519_key = match public_key.key_data().ed25519() {
            Some(key) => key,
            None => {
                return Ok(Auth::Reject {
                    proceed_with_methods: None,
                });
            }
        };

        let key_bytes: &[u8; 32] = ed25519_key.as_ref();

        if self
            .relay_signing
            .has_active_session_with_key(key_bytes)
            .await
        {
            tracing::info!("SSH auth accepted for Ed25519 key");
            Ok(Auth::Accept)
        } else {
            tracing::debug!("SSH auth rejected: no matching signing session");
            Ok(Auth::Reject {
                proceed_with_methods: None,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let id = channel.id();
        self.channels.insert(
            id,
            ChannelState::Pending {
                channel,
                pty_params: None,
            },
        );
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(ChannelState::Pending { pty_params, .. }) = self.channels.get_mut(&channel) {
            *pty_params = Some(PtyParams {
                term: term.to_string(),
                cols: col_width as u16,
                rows: row_height as u16,
            });
        }
        let _ = session.channel_success(channel);
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!(?channel, "Shell request");
        self.spawn_pty_session(channel, None, session)?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = std::str::from_utf8(data).unwrap_or("");
        tracing::info!(?channel, %command, "Exec request");
        self.spawn_pty_session(channel, Some(command), session)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(ChannelState::Active { writer_tx, .. }) = self.channels.get(&channel) {
            let _ = writer_tx.send(data.to_vec()).await;
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(ChannelState::Active { pty_master, .. }) = self.channels.get(&channel) {
            let _ = pty_master.resize(PtySize {
                rows: row_height as u16,
                cols: col_width as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        Ok(())
    }

    async fn env_request(
        &mut self,
        _channel: ChannelId,
        variable_name: &str,
        variable_value: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.pending_env
            .insert(variable_name.to_string(), variable_value.to_string());
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            tracing::info!(?channel_id, "SFTP subsystem request");

            if let Some(ChannelState::Pending { channel, .. }) = self.channels.remove(&channel_id) {
                let _ = session.channel_success(channel_id);
                let sftp_handler = SftpHandler::default();
                tokio::spawn(async move {
                    let stream = channel.into_stream();
                    russh_sftp::server::run(stream, sftp_handler).await;
                });
            } else {
                let _ = session.channel_failure(channel_id);
            }
        } else {
            let _ = session.channel_failure(channel_id);
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Drop the writer to signal EOF to the PTY
        if let Some(ChannelState::Active { writer_tx, .. }) = self.channels.get_mut(&channel) {
            // Dropping the sender will cause the writer task to exit
            let (replacement_tx, _) = mpsc::channel(1);
            let _ = std::mem::replace(writer_tx, replacement_tx);
        }
        Ok(())
    }
}
