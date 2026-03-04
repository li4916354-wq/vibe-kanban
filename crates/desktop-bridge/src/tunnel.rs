//! TCP port tunnel manager.
//!
//! Creates local TCP listeners that tunnel to remote relay hosts via WebSocket.
//! Each tunnel bridges `localhost:{local_port}` → WS → relay proxy → host backend → `localhost:{remote_port}`.

use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

use crate::signing::SigningContext;

/// Key for deduplicating tunnels.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct TunnelKey {
    relay_session_base_url: String,
    api_path: String,
}

struct ActiveTunnel {
    local_port: u16,
    cancel: CancellationToken,
}

#[derive(Default)]
pub struct TunnelManager {
    tunnels: Arc<Mutex<HashMap<TunnelKey, ActiveTunnel>>>,
}

impl TunnelManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create a tunnel to the embedded SSH session endpoint.
    /// Returns the local port to connect to.
    pub async fn get_or_create_ssh_tunnel(
        &self,
        relay_session_base_url: &str,
        signing_ctx: &SigningContext,
    ) -> anyhow::Result<u16> {
        let local_port = self
            .get_or_create_tunnel_for_path(relay_session_base_url, signing_ctx, "/api/ssh-session")
            .await?;
        tracing::info!(local_port, "SSH session tunnel created");
        Ok(local_port)
    }

    async fn get_or_create_tunnel_for_path(
        &self,
        relay_session_base_url: &str,
        signing_ctx: &SigningContext,
        api_path: &str,
    ) -> anyhow::Result<u16> {
        let key = TunnelKey {
            relay_session_base_url: relay_session_base_url.to_string(),
            api_path: api_path.to_string(),
        };

        // Check for existing healthy tunnel
        {
            let tunnels = self.tunnels.lock().await;
            if let Some(tunnel) = tunnels.get(&key) {
                if !tunnel.cancel.is_cancelled() {
                    return Ok(tunnel.local_port);
                }
            }
        }

        // Create new tunnel
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("Failed to bind local tunnel listener")?;
        let local_port = listener.local_addr()?.port();

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let relay_session_base_url = relay_session_base_url.to_string();
        let signing_ctx = signing_ctx.clone();
        let tunnels = self.tunnels.clone();
        let key_clone = key.clone();
        let api_path = api_path.to_string();

        tokio::spawn(async move {
            run_tunnel_listener(
                listener,
                &relay_session_base_url,
                &signing_ctx,
                &api_path,
                cancel_clone,
            )
            .await;

            // Clean up on exit
            tunnels.lock().await.remove(&key_clone);
        });

        self.tunnels
            .lock()
            .await
            .insert(key, ActiveTunnel { local_port, cancel });

        Ok(local_port)
    }
}

async fn run_tunnel_listener(
    listener: TcpListener,
    relay_session_base_url: &str,
    signing_ctx: &SigningContext,
    api_path: &str,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = listener.accept() => {
                match result {
                    Ok((tcp_stream, _addr)) => {
                        let relay_session_base_url = relay_session_base_url.to_string();
                        let signing_ctx = signing_ctx.clone();
                        let api_path = api_path.to_string();
                        tokio::spawn(async move {
                            if let Err(error) = bridge_tcp_to_relay(
                                tcp_stream, &relay_session_base_url, &signing_ctx, &api_path,
                            ).await {
                                tracing::warn!(?error, "Tunnel bridge failed");
                            }
                        });
                    }
                    Err(error) => {
                        tracing::warn!(?error, "Tunnel accept failed");
                        break;
                    }
                }
            }
        }
    }
}

/// Bridge a single TCP connection to the relay via signed WebSocket.
async fn bridge_tcp_to_relay(
    mut tcp_stream: tokio::net::TcpStream,
    relay_session_base_url: &str,
    signing_ctx: &SigningContext,
    api_path: &str,
) -> anyhow::Result<()> {
    let ws_url = build_signed_ws_url(relay_session_base_url, signing_ctx, api_path)?;

    let request = ws_url
        .into_client_request()
        .context("Failed to build WS request")?;

    let mut tls_builder = native_tls::TlsConnector::builder();
    if cfg!(debug_assertions) {
        tls_builder.danger_accept_invalid_certs(true);
    }
    let tls_connector = tls_builder.build().context("Failed to build TLS")?;

    let (ws_stream, _response) = tokio_tungstenite::connect_async_tls_with_config(
        request,
        None,
        false,
        Some(tokio_tungstenite::Connector::NativeTls(tls_connector)),
    )
    .await
    .context("Failed to connect relay tunnel WS")?;

    let mut ws_io = WsStreamIo::new(ws_stream);

    tokio::io::copy_bidirectional(&mut tcp_stream, &mut ws_io)
        .await
        .context("Tunnel copy ended")?;

    Ok(())
}

/// Build a signed WebSocket URL through the relay proxy path.
///
/// The relay session base URL is like:
///   `https://relay.example.com/relay/h/{host_id}/s/{session_id}`
///
/// We append the given API path and sign it with Ed25519.
fn build_signed_ws_url(
    relay_session_base_url: &str,
    signing_ctx: &SigningContext,
    api_path: &str,
) -> anyhow::Result<String> {
    let base = relay_session_base_url.trim_end_matches('/');

    // The path that gets signed and verified by the host backend
    let signed_path = crate::signing::sign_path(signing_ctx, "GET", api_path);

    if let Some(rest) = base.strip_prefix("https://") {
        Ok(format!("wss://{rest}{signed_path}"))
    } else if let Some(rest) = base.strip_prefix("http://") {
        Ok(format!("ws://{rest}{signed_path}"))
    } else {
        anyhow::bail!("Unexpected relay session URL scheme: {base}")
    }
}

// ------- Minimal WebSocket → AsyncRead/AsyncWrite adapter -------

use std::{io, pin::Pin, task::Poll};

use futures_util::{Sink, Stream};
use tokio::io::ReadBuf;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct WsStreamIo {
    ws: WsStream,
    read_buf: bytes::BytesMut,
    /// When true, a previous start_send completed but flush is still pending.
    flushing: bool,
}

impl WsStreamIo {
    fn new(ws: WsStream) -> Self {
        Self {
            ws,
            read_buf: bytes::BytesMut::new(),
            flushing: false,
        }
    }
}

impl AsyncRead for WsStreamIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        use tokio_tungstenite::tungstenite::Message;

        loop {
            let this = self.as_mut().get_mut();

            if !this.read_buf.is_empty() {
                let n = buf.remaining().min(this.read_buf.len());
                buf.put_slice(&this.read_buf.split_to(n));
                return Poll::Ready(Ok(()));
            }

            match std::task::ready!(Pin::new(&mut this.ws).poll_next(cx)) {
                Some(Ok(Message::Binary(data))) => {
                    this.read_buf.extend_from_slice(&data);
                }
                Some(Ok(Message::Text(text))) => {
                    this.read_buf.extend_from_slice(text.as_bytes());
                }
                Some(Ok(Message::Close(_))) | None => return Poll::Ready(Ok(())),
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Poll::Ready(Err(io::Error::other(e.to_string()))),
            }
        }
    }
}

impl AsyncWrite for WsStreamIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        use tokio_tungstenite::tungstenite::Message;

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let this = self.as_mut().get_mut();

        // If a previous write is still flushing, complete the flush first.
        if !this.flushing {
            std::task::ready!(Pin::new(&mut this.ws).poll_ready(cx))
                .map_err(|e| io::Error::other(e.to_string()))?;
            Pin::new(&mut this.ws)
                .start_send(Message::Binary(buf.to_vec().into()))
                .map_err(|e| io::Error::other(e.to_string()))?;
            this.flushing = true;
        }

        // Flush to ensure the frame is actually sent over the wire.
        std::task::ready!(Pin::new(&mut this.ws).poll_flush(cx))
            .map_err(|e| io::Error::other(e.to_string()))?;
        this.flushing = false;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        std::task::ready!(Pin::new(&mut this.ws).poll_flush(cx))
            .map_err(|e| io::Error::other(e.to_string()))?;
        this.flushing = false;
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        std::task::ready!(Pin::new(&mut this.ws).poll_close(cx))
            .map_err(|e| io::Error::other(e.to_string()))?;
        Poll::Ready(Ok(()))
    }
}

use tokio_tungstenite::tungstenite::client::IntoClientRequest;
