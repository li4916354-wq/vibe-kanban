//! AsyncRead + AsyncWrite adapter for axum WebSocket.
//!
//! Bridges axum's WebSocket (stream/sink of messages) into a byte-level
//! AsyncRead + AsyncWrite suitable for russh's `run_stream`.

use std::{io, pin::Pin, task::Poll};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, Stream};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct AxumWsStreamIo {
    ws: WebSocket,
    read_buf: bytes::BytesMut,
}

impl AxumWsStreamIo {
    pub fn new(ws: WebSocket) -> Self {
        Self {
            ws,
            read_buf: bytes::BytesMut::new(),
        }
    }
}

impl AsyncRead for AxumWsStreamIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
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

impl AsyncWrite for AxumWsStreamIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let this = self.as_mut().get_mut();
        std::task::ready!(Pin::new(&mut this.ws).poll_ready(cx))
            .map_err(|e| io::Error::other(e.to_string()))?;
        Pin::new(&mut this.ws)
            .start_send(Message::Binary(buf.to_vec().into()))
            .map_err(|e| io::Error::other(e.to_string()))?;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        std::task::ready!(Pin::new(&mut this.ws).poll_flush(cx))
            .map_err(|e| io::Error::other(e.to_string()))?;
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
