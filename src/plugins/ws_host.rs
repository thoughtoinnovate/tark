use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::http::header::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::Message;
use url::Url;

const WS_MAX_MESSAGE_BYTES: usize = 64 * 1024;

static WS_MANAGER: Lazy<WsManager> = Lazy::new(WsManager::new);

#[derive(Debug)]
pub struct WsRecvResult {
    pub message: Option<String>,
    pub closed: bool,
    pub error: Option<String>,
}

pub fn connect(url: &str, headers: &[(String, String)], allowed_http: &[String]) -> Result<u64> {
    WS_MANAGER.connect(url, headers, allowed_http)
}

pub fn send(handle: u64, data: &str) -> Result<()> {
    WS_MANAGER.send(handle, data)
}

pub fn recv(handle: u64, timeout_ms: u64, max_bytes: usize) -> Result<WsRecvResult> {
    WS_MANAGER.recv(handle, timeout_ms, max_bytes)
}

pub fn close(handle: u64) -> Result<()> {
    WS_MANAGER.close(handle)
}

struct WsManager {
    next_id: AtomicU64,
    conns: Mutex<HashMap<u64, WsConn>>,
}

struct WsConn {
    send_tx: mpsc::UnboundedSender<String>,
    recv_rx: std::sync::mpsc::Receiver<WsEvent>,
    closed: Arc<AtomicBool>,
}

enum WsEvent {
    Message(String),
    Closed(String),
}

impl WsManager {
    fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            conns: Mutex::new(HashMap::new()),
        }
    }

    fn connect(
        &self,
        url: &str,
        headers: &[(String, String)],
        allowed_http: &[String],
    ) -> Result<u64> {
        let parsed = Url::parse(url).context("Invalid websocket URL")?;
        if parsed.scheme() != "wss" {
            anyhow::bail!("Only wss:// URLs are allowed");
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("Missing websocket host"))?;
        if !is_allowed_host(host, allowed_http) {
            anyhow::bail!("WebSocket host not allowed: {}", host);
        }

        let handle = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (send_tx, mut send_rx) = mpsc::unbounded_channel::<String>();
        let (recv_tx, recv_rx) = std::sync::mpsc::channel::<WsEvent>();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_flag = Arc::clone(&closed);
        let url = url.to_string();
        let headers = headers.to_vec();

        let runtime = tokio::runtime::Handle::try_current()
            .context("WebSocket connect requires a Tokio runtime")?;
        runtime.spawn(async move {
            let mut request = match tokio_tungstenite::tungstenite::http::Request::builder()
                .method("GET")
                .uri(&url)
                .body(())
            {
                Ok(req) => req,
                Err(err) => {
                    let _ = recv_tx.send(WsEvent::Closed(err.to_string()));
                    closed_flag.store(true, Ordering::Relaxed);
                    return;
                }
            };
            {
                let headers_mut = request.headers_mut();
                for (key, value) in headers {
                    if let (Ok(header_name), Ok(header_value)) = (
                        HeaderName::from_bytes(key.as_bytes()),
                        HeaderValue::from_str(&value),
                    ) {
                        headers_mut.insert(header_name, header_value);
                    }
                }
            }

            let ws_stream = tokio_tungstenite::connect_async(request).await;
            let mut ws_stream = match ws_stream {
                Ok((stream, _)) => stream,
                Err(err) => {
                    let _ = recv_tx.send(WsEvent::Closed(err.to_string()));
                    closed_flag.store(true, Ordering::Relaxed);
                    return;
                }
            };

            loop {
                tokio::select! {
                    Some(outbound) = send_rx.recv() => {
                        if ws_stream.send(Message::Text(outbound)).await.is_err() {
                            let _ = recv_tx.send(WsEvent::Closed("send_failed".to_string()));
                            break;
                        }
                    }
                    inbound = ws_stream.next() => {
                        match inbound {
                            Some(Ok(Message::Text(text))) => {
                                let _ = recv_tx.send(WsEvent::Message(text));
                            }
                            Some(Ok(Message::Close(frame))) => {
                                let reason = frame
                                    .as_ref()
                                    .map(|f| format!("close:{} {}", f.code, f.reason))
                                    .unwrap_or_else(|| "closed".to_string());
                                let _ = recv_tx.send(WsEvent::Closed(reason));
                                break;
                            }
                            Some(Ok(_)) => {}
                            Some(Err(err)) => {
                                let _ = recv_tx.send(WsEvent::Closed(err.to_string()));
                                break;
                            }
                            None => {
                                let _ = recv_tx.send(WsEvent::Closed("eof".to_string()));
                                break;
                            }
                        }
                    }
                }
            }

            closed_flag.store(true, Ordering::Relaxed);
        });

        let mut conns = self.conns.lock().unwrap();
        conns.insert(
            handle,
            WsConn {
                send_tx,
                recv_rx,
                closed,
            },
        );

        Ok(handle)
    }

    fn send(&self, handle: u64, data: &str) -> Result<()> {
        let conns = self.conns.lock().unwrap();
        let conn = conns
            .get(&handle)
            .ok_or_else(|| anyhow::anyhow!("Unknown websocket handle"))?;
        conn.send_tx
            .send(data.to_string())
            .map_err(|_| anyhow::anyhow!("WebSocket send failed"))?;
        Ok(())
    }

    fn recv(&self, handle: u64, timeout_ms: u64, max_bytes: usize) -> Result<WsRecvResult> {
        let conns = self.conns.lock().unwrap();
        let conn = conns
            .get(&handle)
            .ok_or_else(|| anyhow::anyhow!("Unknown websocket handle"))?;
        let timeout = std::time::Duration::from_millis(timeout_ms);
        let max_bytes = max_bytes.min(WS_MAX_MESSAGE_BYTES);

        match conn.recv_rx.recv_timeout(timeout) {
            Ok(WsEvent::Message(msg)) => {
                if msg.len() > max_bytes {
                    Ok(WsRecvResult {
                        message: None,
                        closed: false,
                        error: Some("message_too_large".to_string()),
                    })
                } else {
                    Ok(WsRecvResult {
                        message: Some(msg),
                        closed: false,
                        error: None,
                    })
                }
            }
            Ok(WsEvent::Closed(reason)) => Ok(WsRecvResult {
                message: None,
                closed: true,
                error: Some(reason),
            }),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(WsRecvResult {
                message: None,
                closed: conn.closed.load(Ordering::Relaxed),
                error: None,
            }),
            Err(_) => Ok(WsRecvResult {
                message: None,
                closed: true,
                error: Some("recv_failed".to_string()),
            }),
        }
    }

    fn close(&self, handle: u64) -> Result<()> {
        let mut conns = self.conns.lock().unwrap();
        conns.remove(&handle);
        Ok(())
    }
}

fn is_allowed_host(host: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix("*.") {
            host == suffix || host.ends_with(&format!(".{}", suffix))
        } else {
            host == pattern
        }
    })
}
