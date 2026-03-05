//! Remote Access Service
//! Manages remote access, tunnel, and TOTP authentication

use std::net::IpAddr;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use base32::Alphabet;
use qrcode::QrCode;
use qrcode::render::svg;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, RwLock};
use totp_rs::{Algorithm, Secret, TOTP};

/// Remote access status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAccessStatus {
    pub enabled: bool,
    pub server: ServerStatus,
    pub tunnel: TunnelStatus,
    pub clients: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub token: Option<String>,
    pub local_url: Option<String>,
    pub lan_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelStatus {
    pub status: TunnelState,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TunnelState {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpStatus {
    pub bound: bool,
}

/// Internal state for remote access
struct RemoteAccessState {
    enabled: bool,
    port: u16,
    token: String,
    tunnel_process: Option<Child>,
    tunnel_url: Option<String>,
    tunnel_state: TunnelState,
    tunnel_error: Option<String>,
    totp_secret: Option<String>,
    pending_totp_secret: Option<String>,
    clients: u32,
}

impl Default for RemoteAccessState {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 3000,
            token: generate_random_token(),
            tunnel_process: None,
            tunnel_url: None,
            tunnel_state: TunnelState::Stopped,
            tunnel_error: None,
            totp_secret: None,
            pending_totp_secret: None,
            clients: 0,
        }
    }
}

/// Remote Access Service
#[derive(Clone)]
pub struct RemoteAccessService {
    state: Arc<RwLock<RemoteAccessState>>,
    status_tx: broadcast::Sender<RemoteAccessStatus>,
}

impl RemoteAccessService {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        Self {
            state: Arc::new(RwLock::new(RemoteAccessState::default())),
            status_tx,
        }
    }

    /// Get current status
    pub async fn get_status(&self) -> RemoteAccessStatus {
        let state = self.state.read().await;
        self.build_status(&state)
    }

    /// Enable remote access
    pub async fn enable(&self, port: Option<u16>) -> Result<RemoteAccessStatus> {
        let mut state = self.state.write().await;

        if state.enabled {
            return Ok(self.build_status(&state));
        }

        state.enabled = true;
        if let Some(p) = port {
            state.port = p;
        }

        let status = self.build_status(&state);
        let _ = self.status_tx.send(status.clone());

        tracing::info!("[RemoteAccess] Enabled on port {}", state.port);
        Ok(status)
    }

    /// Disable remote access
    pub async fn disable(&self) -> Result<()> {
        let mut state = self.state.write().await;

        // Stop tunnel if running
        if let Some(mut process) = state.tunnel_process.take() {
            let _ = process.kill().await;
        }

        state.enabled = false;
        state.tunnel_state = TunnelState::Stopped;
        state.tunnel_url = None;
        state.tunnel_error = None;

        let status = self.build_status(&state);
        let _ = self.status_tx.send(status);

        tracing::info!("[RemoteAccess] Disabled");
        Ok(())
    }

    /// Enable tunnel for internet access
    pub async fn enable_tunnel(&self) -> Result<TunnelStatus> {
        let mut state = self.state.write().await;

        if !state.enabled {
            return Err(anyhow!("Remote access must be enabled first"));
        }

        if state.tunnel_state == TunnelState::Running {
            return Ok(TunnelStatus {
                status: state.tunnel_state.clone(),
                url: state.tunnel_url.clone(),
                error: None,
            });
        }

        state.tunnel_state = TunnelState::Starting;
        state.tunnel_error = None;

        let status = self.build_status(&state);
        let _ = self.status_tx.send(status);

        // Start cloudflared tunnel
        let port = state.port;
        drop(state); // Release lock before spawning

        let result = self.start_tunnel(port).await;

        let mut state = self.state.write().await;
        match result {
            Ok(url) => {
                state.tunnel_url = Some(url.clone());
                state.tunnel_state = TunnelState::Running;
                state.tunnel_error = None;

                let status = self.build_status(&state);
                let _ = self.status_tx.send(status);

                tracing::info!("[RemoteAccess] Tunnel started: {}", url);
                Ok(TunnelStatus {
                    status: TunnelState::Running,
                    url: Some(url),
                    error: None,
                })
            }
            Err(e) => {
                state.tunnel_state = TunnelState::Error;
                state.tunnel_error = Some(e.to_string());

                let status = self.build_status(&state);
                let _ = self.status_tx.send(status);

                tracing::error!("[RemoteAccess] Tunnel failed: {}", e);
                Err(e)
            }
        }
    }

    /// Disable tunnel
    pub async fn disable_tunnel(&self) -> Result<()> {
        let mut state = self.state.write().await;

        if let Some(mut process) = state.tunnel_process.take() {
            let _ = process.kill().await;
        }

        state.tunnel_state = TunnelState::Stopped;
        state.tunnel_url = None;
        state.tunnel_error = None;

        let status = self.build_status(&state);
        let _ = self.status_tx.send(status);

        tracing::info!("[RemoteAccess] Tunnel stopped");
        Ok(())
    }

    /// Generate QR code for remote access
    pub async fn generate_qr_code(&self, include_token: bool) -> Result<String> {
        let state = self.state.read().await;

        if !state.enabled {
            return Err(anyhow!("Remote access is not enabled"));
        }

        let url = if let Some(ref tunnel_url) = state.tunnel_url {
            tunnel_url.clone()
        } else if let Some(lan_ip) = get_lan_ip() {
            format!("http://{}:{}", lan_ip, state.port)
        } else {
            format!("http://localhost:{}", state.port)
        };

        let final_url = if include_token {
            format!("{}?token={}", url, state.token)
        } else {
            url
        };

        let code = QrCode::new(final_url.as_bytes())?;
        let svg = code.render::<svg::Color>()
            .min_dimensions(200, 200)
            .build();

        // Convert to data URL
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            svg.as_bytes(),
        );
        Ok(format!("data:image/svg+xml;base64,{}", encoded))
    }

    /// Set custom password
    pub async fn set_password(&self, password: &str) -> Result<()> {
        if password.len() < 4 || password.len() > 32 {
            return Err(anyhow!("Password must be 4-32 characters"));
        }

        // Validate alphanumeric
        if !password.chars().all(|c| c.is_alphanumeric()) {
            return Err(anyhow!("Password must be alphanumeric"));
        }

        let mut state = self.state.write().await;
        state.token = password.to_string();

        let status = self.build_status(&state);
        let _ = self.status_tx.send(status);

        tracing::info!("[RemoteAccess] Password updated");
        Ok(())
    }

    /// Get TOTP status
    pub async fn get_totp_status(&self) -> TotpStatus {
        let state = self.state.read().await;
        TotpStatus {
            bound: state.totp_secret.is_some(),
        }
    }

    /// Generate TOTP secret for binding
    pub async fn generate_totp_secret(&self) -> Result<(String, String)> {
        let secret = generate_totp_secret();

        // Store pending secret
        {
            let mut state = self.state.write().await;
            state.pending_totp_secret = Some(secret.clone());
        }

        // Generate QR code
        let _totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(secret.clone()).to_bytes().unwrap(),
        )?;

        let qr_url = format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}",
            "VibeKanban",
            "Remote Access",
            secret,
            "VibeKanban"
        );
        let code = QrCode::new(qr_url.as_bytes())?;
        let svg = code.render::<svg::Color>()
            .min_dimensions(200, 200)
            .build();

        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            svg.as_bytes(),
        );
        let qr_code = format!("data:image/svg+xml;base64,{}", encoded);

        Ok((secret, qr_code))
    }

    /// Bind TOTP with verification code
    pub async fn bind_totp(&self, code: &str) -> Result<()> {
        let mut state = self.state.write().await;

        let pending_secret = state.pending_totp_secret.clone()
            .ok_or_else(|| anyhow!("No pending TOTP secret"))?;

        // Verify the code
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(pending_secret.clone()).to_bytes().unwrap(),
        )?;

        if !totp.check_current(code)? {
            return Err(anyhow!("Invalid verification code"));
        }

        // Bind the secret
        state.totp_secret = Some(pending_secret);
        state.pending_totp_secret = None;

        tracing::info!("[RemoteAccess] TOTP bound successfully");
        Ok(())
    }

    /// Unbind TOTP
    pub async fn unbind_totp(&self) -> Result<()> {
        let mut state = self.state.write().await;
        state.totp_secret = None;
        state.pending_totp_secret = None;

        tracing::info!("[RemoteAccess] TOTP unbound");
        Ok(())
    }

    /// Verify TOTP code
    pub async fn verify_totp(&self, code: &str) -> Result<bool> {
        let state = self.state.read().await;

        let secret = state.totp_secret.as_ref()
            .ok_or_else(|| anyhow!("TOTP not bound"))?;

        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(secret.clone()).to_bytes().unwrap(),
        )?;

        Ok(totp.check_current(code)?)
    }

    /// Subscribe to status changes
    pub fn subscribe_status(&self) -> broadcast::Receiver<RemoteAccessStatus> {
        self.status_tx.subscribe()
    }

    // Private helpers

    fn build_status(&self, state: &RemoteAccessState) -> RemoteAccessStatus {
        let lan_ip = get_lan_ip();

        RemoteAccessStatus {
            enabled: state.enabled,
            server: ServerStatus {
                running: state.enabled,
                port: state.port,
                token: if state.enabled { Some(state.token.clone()) } else { None },
                local_url: if state.enabled {
                    Some(format!("http://localhost:{}", state.port))
                } else {
                    None
                },
                lan_url: if state.enabled {
                    lan_ip.map(|ip| format!("http://{}:{}", ip, state.port))
                } else {
                    None
                },
            },
            tunnel: TunnelStatus {
                status: state.tunnel_state.clone(),
                url: state.tunnel_url.clone(),
                error: state.tunnel_error.clone(),
            },
            clients: state.clients,
        }
    }

    async fn start_tunnel(&self, port: u16) -> Result<String> {
        // Try to find cloudflared
        let cloudflared = which::which("cloudflared")
            .map_err(|_| anyhow!("cloudflared not found. Please install it first."))?;

        let mut child = Command::new(cloudflared)
            .args([
                "tunnel",
                "--url",
                &format!("http://localhost:{}", port),
                "--protocol",
                "http2",
                "--no-autoupdate",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Read stderr to find the tunnel URL
        let stderr = child.stderr.take()
            .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

        let url = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Self::read_tunnel_url(stderr),
        )
        .await
        .map_err(|_| anyhow!("Timeout waiting for tunnel URL"))??;

        // Store the process
        {
            let mut state = self.state.write().await;
            state.tunnel_process = Some(child);
        }

        Ok(url)
    }

    async fn read_tunnel_url(
        stderr: tokio::process::ChildStderr,
    ) -> Result<String> {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            // Look for trycloudflare.com URL
            if let Some(start) = line.find("https://") {
                if line.contains("trycloudflare.com") {
                    let end = line[start..].find(' ').unwrap_or(line.len() - start);
                    return Ok(line[start..start + end].to_string());
                }
            }
        }

        Err(anyhow!("Could not find tunnel URL in output"))
    }
}

impl Default for RemoteAccessService {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a random 8-character alphanumeric token
fn generate_random_token() -> String {
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

/// Generate a TOTP secret
fn generate_totp_secret() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..20).map(|_| rng.r#gen()).collect();
    base32::encode(Alphabet::Rfc4648 { padding: false }, &bytes)
}

/// Get LAN IP address
fn get_lan_ip() -> Option<IpAddr> {
    local_ip_address::local_ip().ok()
}
