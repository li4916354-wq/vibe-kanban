//! Generic TCP port tunnel via WebSocket.
//!
//! Bridges a WebSocket connection to a CONNECT tunnel through the relay
//! host's yamux session. This allows tunneling to any localhost port on
//! the connected host (SSH, dev servers, databases, etc.).

use axum::{
    Extension,
    extract::{Path, Query, State, ws::WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use super::super::{auth::RequestContext, db::hosts::HostRepository, state::RelayAppState};
use crate::{
    server::open_connect_tunnel,
    ws_io::{WsIoReadMessage, WsMessageStreamIo},
};

#[derive(Debug, Deserialize)]
pub struct TunnelQuery {
    pub port: u16,
}

/// `GET /v1/relay/hosts/{host_id}/tunnel?port={port}`
///
/// JWT-protected WebSocket endpoint. Opens a CONNECT tunnel to
/// `localhost:{port}` on the target host through the relay, then bridges
/// the WebSocket to the tunnel bidirectionally.
pub async fn relay_tunnel(
    State(state): State<RelayAppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(host_id): Path<Uuid>,
    Query(query): Query<TunnelQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let repo = HostRepository::new(&state.pool);
    if let Err(_) = repo.assert_host_access(host_id, ctx.user.id).await {
        return (StatusCode::FORBIDDEN, "Host access denied").into_response();
    }

    let relay = match state.relay_registry.get(&host_id).await {
        Some(relay) => relay,
        None => return (StatusCode::NOT_FOUND, "Host not connected").into_response(),
    };

    let target_addr = format!("127.0.0.1:{}", query.port);

    ws.on_upgrade(move |socket| async move {
        if let Err(error) = bridge_tunnel(socket, relay.control.clone(), &target_addr).await {
            tracing::warn!(?error, %host_id, port = query.port, "Tunnel bridge failed");
        }
    })
}

async fn bridge_tunnel(
    socket: axum::extract::ws::WebSocket,
    control: crate::server::SharedControl,
    target_addr: &str,
) -> anyhow::Result<()> {
    let mut tunnel = open_connect_tunnel(control.as_ref(), target_addr).await?;

    let ws_io = WsMessageStreamIo::new(socket, read_ws_message, write_ws_message);
    tokio::pin!(ws_io);

    tokio::io::copy_bidirectional(&mut ws_io, &mut tunnel).await?;

    Ok(())
}

fn read_ws_message(message: axum::extract::ws::Message) -> WsIoReadMessage {
    match message {
        axum::extract::ws::Message::Binary(data) => WsIoReadMessage::Data(data.to_vec()),
        axum::extract::ws::Message::Text(text) => WsIoReadMessage::Data(text.as_bytes().to_vec()),
        axum::extract::ws::Message::Close(_) => WsIoReadMessage::Eof,
        _ => WsIoReadMessage::Skip,
    }
}

fn write_ws_message(bytes: Vec<u8>) -> axum::extract::ws::Message {
    axum::extract::ws::Message::Binary(bytes.into())
}
