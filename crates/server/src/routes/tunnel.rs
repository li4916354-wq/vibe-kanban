use axum::{
    extract::{Query, ws::WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TunnelQuery {
    pub port: u16,
}

pub async fn tunnel_ws(Query(query): Query<TunnelQuery>, ws: WebSocketUpgrade) -> Response {
    if query.port == 0 {
        return (StatusCode::BAD_REQUEST, "Invalid port").into_response();
    }

    let target_addr = format!("127.0.0.1:{}", query.port);

    ws.on_upgrade(move |socket| async move {
        if let Err(error) = bridge_ws_to_tcp(socket, &target_addr).await {
            tracing::warn!(?error, %target_addr, "WS tunnel bridge failed");
        }
    })
}

async fn bridge_ws_to_tcp(
    socket: axum::extract::ws::WebSocket,
    target_addr: &str,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let tcp_stream = tokio::net::TcpStream::connect(target_addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to {target_addr}: {e}"))?;

    let (tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);
    let (mut ws_sink, mut ws_stream) = socket.split();

    let ws_to_tcp = async {
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if tcp_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Text(text)) => {
                    if tcp_write.write_all(text.as_bytes()).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => continue,
            }
        }
    };

    let tcp_to_ws = async {
        let mut tcp_read = tcp_read;
        let mut buf = vec![0u8; 8192];
        loop {
            match tcp_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if ws_sink
                        .send(Message::Binary(buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    tokio::select! {
        _ = ws_to_tcp => {}
        _ = tcp_to_ws => {}
    }

    Ok(())
}
