use std::convert::Infallible;

use anyhow::Context as _;
use axum::body::Body;
use futures_util::StreamExt;
use http::StatusCode;
use hyper::{
    Request, Response, body::Incoming, client::conn::http1 as client_http1,
    server::conn::http1 as server_http1, service::service_fn, upgrade,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    Connector,
    tungstenite::{self, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;
use tokio_yamux::{Config as YamuxConfig, Session};

use crate::ws_io::{WsIoReadMessage, WsMessageStreamIo};

pub struct RelayClientConfig {
    pub ws_url: String,
    pub bearer_token: String,
    pub local_addr: String,
    pub shutdown: CancellationToken,
    /// When true, incoming CONNECT requests will be tunneled to the target
    /// address specified in the CONNECT URI (must be localhost).
    pub tunnel_enabled: bool,
}

/// Connects the relay client control channel and starts handling inbound streams.
///
/// Returns when shutdown is requested or when the control channel disconnects/errors.
pub async fn start_relay_client(config: RelayClientConfig) -> anyhow::Result<()> {
    let mut request = config
        .ws_url
        .clone()
        .into_client_request()
        .context("Failed to build WS request")?;

    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", config.bearer_token)
            .parse()
            .context("Invalid auth header")?,
    );

    let mut tls_builder = native_tls::TlsConnector::builder();
    if cfg!(debug_assertions) {
        tls_builder.danger_accept_invalid_certs(true);
    }
    let tls_connector = tls_builder
        .build()
        .context("Failed to build TLS connector")?;

    let (ws_stream, _response) = tokio_tungstenite::connect_async_tls_with_config(
        request,
        None,
        false,
        Some(Connector::NativeTls(tls_connector)),
    )
    .await
    .context("Failed to connect relay control channel")?;

    let ws_io = WsMessageStreamIo::new(ws_stream, read_client_message, write_client_message);
    let mut session = Session::new_client(ws_io, YamuxConfig::default());
    let mut control = session.control();

    tracing::debug!("Relay control channel connected");

    let shutdown = config.shutdown;
    let local_addr = config.local_addr;
    let tunnel_enabled = config.tunnel_enabled;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                control.close().await;
                return Ok(());
            }
            inbound = session.next() => {
                let stream = inbound
                    .ok_or_else(|| anyhow::anyhow!("Relay control channel closed"))?
                    .map_err(|e| anyhow::anyhow!("Relay yamux session error: {e}"))?;

                let local_addr = local_addr.clone();
                let tunnel_enabled = tunnel_enabled;
                tokio::spawn(async move {
                    if let Err(error) = handle_inbound_stream(stream, local_addr, tunnel_enabled).await {
                        tracing::warn!(?error, "Relay stream handling failed");
                    }
                });
            }
        }
    }
}

async fn handle_inbound_stream(
    stream: tokio_yamux::StreamHandle,
    local_addr: String,
    tunnel_enabled: bool,
) -> anyhow::Result<()> {
    let io = TokioIo::new(stream);

    server_http1::Builder::new()
        .serve_connection(
            io,
            service_fn(move |request: Request<Incoming>| {
                let local_addr = local_addr.clone();
                async move {
                    if request.method() == hyper::Method::CONNECT {
                        handle_connect_tunnel(request, tunnel_enabled).await
                    } else {
                        proxy_to_local(request, local_addr).await
                    }
                }
            }),
        )
        .with_upgrades()
        .await
        .context("Yamux stream server connection failed")
}

/// Handles an HTTP CONNECT request by tunneling to the target specified in the
/// request URI. The target must be a localhost address (127.0.0.1 or localhost).
///
/// Responds with 200, upgrades the connection, then copies bytes bidirectionally
/// between the upgraded stream and a TCP connection to the target.
async fn handle_connect_tunnel(
    mut request: Request<Incoming>,
    tunnel_enabled: bool,
) -> Result<Response<Body>, Infallible> {
    if !tunnel_enabled {
        return Ok(simple_response(
            StatusCode::FORBIDDEN,
            "Tunneling not enabled on this host",
        ));
    }

    let target_addr = match validate_connect_target(&request) {
        Ok(addr) => addr,
        Err(msg) => return Ok(simple_response(StatusCode::BAD_REQUEST, msg)),
    };

    let request_upgrade = upgrade::on(&mut request);

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap_or_else(|_| Response::new(Body::empty()));

    tokio::spawn(async move {
        let Ok(upgraded) = request_upgrade.await else {
            tracing::warn!("Tunnel upgrade failed");
            return;
        };

        let mut tcp_stream = match TcpStream::connect(&target_addr).await {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(?error, %target_addr, "Failed to connect to tunnel target");
                return;
            }
        };

        let mut upgraded = TokioIo::new(upgraded);

        if let Err(error) = tokio::io::copy_bidirectional(&mut upgraded, &mut tcp_stream).await {
            tracing::debug!(?error, "Tunnel copy ended");
        }
    });

    Ok(response)
}

/// Parse and validate the CONNECT target from the request URI.
/// Only allows localhost targets (127.0.0.1 or localhost).
fn validate_connect_target(request: &Request<Incoming>) -> Result<String, &'static str> {
    let uri = request.uri();
    let authority = uri
        .authority()
        .map(|a| a.as_str())
        .unwrap_or_else(|| uri.path());

    if authority.is_empty() {
        return Err("Missing CONNECT target");
    }

    // Parse host:port from authority
    let (host, port_str) = if let Some(colon_pos) = authority.rfind(':') {
        (&authority[..colon_pos], &authority[colon_pos + 1..])
    } else {
        return Err("CONNECT target must include port");
    };

    let _port: u16 = port_str
        .parse()
        .map_err(|_| "Invalid port in CONNECT target")?;

    // Only allow localhost targets
    if host != "127.0.0.1" && host != "localhost" && host != "::1" {
        return Err("CONNECT target must be localhost");
    }

    Ok(authority.to_string())
}

async fn proxy_to_local(
    mut request: Request<Incoming>,
    local_addr: String,
) -> Result<Response<Body>, Infallible> {
    request
        .headers_mut()
        .insert("x-vk-relayed", http::HeaderValue::from_static("1"));

    // TODO: fix dev servers
    let local_stream = match TcpStream::connect(local_addr.as_str()).await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::warn!(
                ?error,
                "Failed to connect to local server for relay request"
            );
            return Ok(simple_response(
                StatusCode::BAD_GATEWAY,
                "Failed to connect to local server",
            ));
        }
    };

    let (mut sender, connection) = match client_http1::Builder::new()
        .handshake(TokioIo::new(local_stream))
        .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(?error, "Failed to create local proxy HTTP connection");
            return Ok(simple_response(
                StatusCode::BAD_GATEWAY,
                "Failed to initialize local proxy connection",
            ));
        }
    };

    tokio::spawn(async move {
        if let Err(error) = connection.with_upgrades().await {
            tracing::debug!(?error, "Local proxy connection closed");
        }
    });

    let request_upgrade = upgrade::on(&mut request);

    let mut response = match sender.send_request(request).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(?error, "Local proxy request failed");
            return Ok(simple_response(
                StatusCode::BAD_GATEWAY,
                "Local proxy request failed",
            ));
        }
    };

    if response.status() == StatusCode::SWITCHING_PROTOCOLS {
        let response_upgrade = upgrade::on(&mut response);
        tokio::spawn(async move {
            let mut from_remote = TokioIo::new(request_upgrade.await?);
            let mut to_local = TokioIo::new(response_upgrade.await?);
            tokio::io::copy_bidirectional(&mut from_remote, &mut to_local).await?;
            Ok::<_, anyhow::Error>(())
        });
    }

    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

fn simple_response(status: StatusCode, body: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::from(body)))
}

fn read_client_message(message: tungstenite::Message) -> WsIoReadMessage {
    match message {
        tungstenite::Message::Binary(data) => WsIoReadMessage::Data(data.to_vec()),
        tungstenite::Message::Text(text) => WsIoReadMessage::Data(text.as_bytes().to_vec()),
        tungstenite::Message::Close(_) => WsIoReadMessage::Eof,
        _ => WsIoReadMessage::Skip,
    }
}

fn write_client_message(bytes: Vec<u8>) -> tungstenite::Message {
    tungstenite::Message::Binary(bytes)
}
