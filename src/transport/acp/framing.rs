//! Newline-delimited JSON (NDJSON) framing for ACP v1 over stdio (R8 S20).
//!
//! Every message is one JSON value followed by `\n`. The legacy
//! `Content-Length` envelope is intentionally NOT supported: a peer sending
//! it receives a parse error, never silent reinterpretation.

use anyhow::Result;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

/// Read one NDJSON frame: a single non-empty line.
///
/// Blank lines are skipped (tolerated keep-alives). Lines longer than
/// `max_frame_bytes` are rejected fail-closed. `Ok(None)` means clean EOF.
pub async fn read_frame(
    reader: &mut (impl AsyncBufReadExt + Unpin),
    max_frame_bytes: usize,
) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let bytes = reader.read_until(b'\n', &mut line).await?;
        if bytes == 0 {
            return Ok(None);
        }
        if line.len() > max_frame_bytes {
            anyhow::bail!("Frame too large: {} > {}", line.len(), max_frame_bytes);
        }
        // Strip one trailing newline (tolerate CRLF).
        while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        return Ok(Some(line));
    }
}

/// Write one NDJSON frame: payload bytes followed by `\n`.
pub async fn write_frame(writer: &mut (impl AsyncWriteExt + Unpin), payload: &[u8]) -> Result<()> {
    if payload.contains(&b'\n') {
        anyhow::bail!(
            "Refusing to emit multi-line NDJSON frame: payload must be a single JSON value"
        );
    }
    if payload.is_empty() {
        anyhow::bail!("Refusing to emit empty NDJSON frame");
    }
    writer.write_all(payload).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

/// Adapter exposing `AsyncWrite` over a locked `tokio::io::Stdout`.
///
/// `tokio::sync::MutexGuard` does not implement `AsyncWrite`, so the shared
/// server writer needs this newtype to reuse [`write_frame`].
struct LockedStdoutWriter<'a> {
    guard: tokio::sync::MutexGuard<'a, tokio::io::Stdout>,
}

impl AsyncWrite for LockedStdoutWriter<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.guard).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.guard).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.guard).poll_shutdown(cx)
    }
}

/// Write one NDJSON frame through a shared locked stdout.
pub async fn write_stdout_frame(
    writer: &Arc<Mutex<tokio::io::Stdout>>,
    payload: &[u8],
) -> Result<()> {
    let guard = writer.lock().await;
    write_frame(&mut LockedStdoutWriter { guard }, payload).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn ndjson_round_trip() {
        let (client, server) = tokio::io::duplex(4096);
        let (client_r, _client_w) = tokio::io::split(client);
        let mut reader = BufReader::new(client_r);
        let mut server_w = server;

        write_frame(&mut server_w, br#"{"jsonrpc":"2.0","id":1}"#)
            .await
            .unwrap();
        let frame = read_frame(&mut reader, 1024).await.unwrap().unwrap();
        assert_eq!(frame, br#"{"jsonrpc":"2.0","id":1}"#);
    }

    #[tokio::test]
    async fn blank_lines_skipped_and_crlf_tolerated() {
        let (client, server) = tokio::io::duplex(4096);
        let (client_r, _client_w) = tokio::io::split(client);
        let mut reader = BufReader::new(client_r);
        let mut server_w = server;

        use tokio::io::AsyncWriteExt as _;
        server_w.write_all(b"\n\r\n").await.unwrap();
        write_frame(&mut server_w, b"{}").await.unwrap();
        // Manually append a CRLF-terminated frame.
        server_w.write_all(b"{\"a\":1}\r\n").await.unwrap();
        server_w.shutdown().await.unwrap();

        assert_eq!(read_frame(&mut reader, 1024).await.unwrap().unwrap(), b"{}");
        assert_eq!(
            read_frame(&mut reader, 1024).await.unwrap().unwrap(),
            b"{\"a\":1}"
        );
        assert!(read_frame(&mut reader, 1024).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn oversize_frame_rejected() {
        let (client, server) = tokio::io::duplex(4096);
        let (client_r, _client_w) = tokio::io::split(client);
        let mut reader = BufReader::new(client_r);
        let mut server_w = server;

        use tokio::io::AsyncWriteExt as _;
        server_w.write_all(b"{\"big\":true}").await.unwrap();
        server_w.write_all(b"\n").await.unwrap();
        assert!(read_frame(&mut reader, 4).await.is_err());
    }

    #[tokio::test]
    async fn multiline_and_empty_payloads_rejected() {
        let (_r, w) = tokio::io::duplex(64);
        let (_rr, mut ww) = tokio::io::split(w);
        assert!(write_frame(&mut ww, b"{\"a\":1}\n{\"b\":2}").await.is_err());
        assert!(write_frame(&mut ww, b"").await.is_err());
    }
}
