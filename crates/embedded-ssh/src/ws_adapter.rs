//! AsyncRead + AsyncWrite adapter for axum WebSocket.
//!
//! Bridges axum's WebSocket (stream/sink of messages) into a byte-level
//! AsyncRead + AsyncWrite suitable for russh's `run_stream`.

use std::{io, pin::Pin, task::Poll};

use axum::extract::ws::{Message, WebSocket};
use relay_tunnel::ws_io::{WsIoReadMessage, WsMessageStreamIo};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct AxumWsStreamIo {
    inner: AxumWsIoInner,
}

type AxumWsIoInner =
    WsMessageStreamIo<WebSocket, Message, fn(Message) -> WsIoReadMessage, fn(Vec<u8>) -> Message>;

impl AxumWsStreamIo {
    pub fn new(ws: WebSocket) -> Self {
        Self {
            inner: WsMessageStreamIo::new(ws, read_axum_message, write_axum_message),
        }
    }
}

impl AsyncRead for AxumWsStreamIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for AxumWsStreamIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.as_mut().get_mut();
        Pin::new(&mut this.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

fn read_axum_message(message: Message) -> WsIoReadMessage {
    match message {
        Message::Binary(data) => WsIoReadMessage::Data(data.to_vec()),
        Message::Text(text) => WsIoReadMessage::Data(text.as_bytes().to_vec()),
        Message::Close(_) => WsIoReadMessage::Eof,
        _ => WsIoReadMessage::Skip,
    }
}

fn write_axum_message(bytes: Vec<u8>) -> Message {
    Message::Binary(bytes.into())
}
