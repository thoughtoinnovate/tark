//! Ollama reliability demo/integration tests (P1.5).
//!
//! Spins up a mock Ollama HTTP server (python3, loopback) and drives the
//! real `OllamaProvider` through it: warmup + streaming success, missing
//! model remediation, unreachable-server remediation, and cold-load timeout
//! surfacing. Single sequential test: timeout knobs are process-global env.

use std::sync::{Arc, Mutex};
use tark_cli::llm::{LlmProvider, LlmResponse, Message, MessageContent, OllamaProvider, Role};

/// Minimal mock Ollama: /api/tags, /api/generate (0.5s; 30s for
/// `slow-model`), /api/chat (streaming NDJSON or single JSON).
/// `flaky-model` fails the first two `/api/chat` calls with 500 (retry
/// coverage); `notools-model` rejects any `/api/chat` carrying `tools`
/// with 400 (tools-stripped fallback coverage).
const MOCK_OLLAMA_PY: &str = r#"
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CALLS = {}

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _send(self, code, body, content_type="application/json"):
        data = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/api/tags":
            self._send(200, json.dumps({"models": [
                {"name": "mock-model:latest", "size": 1, "modified_at": ""},
                {"name": "slow-model:latest", "size": 1, "modified_at": ""},
                {"name": "flaky-model:latest", "size": 1, "modified_at": ""},
                {"name": "notools-model:latest", "size": 1, "modified_at": ""},
            ]}))
        else:
            self._send(404, "{}")

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        try:
            body = json.loads(self.rfile.read(length) or b"{}")
        except Exception:
            body = {}
        if self.path == "/api/generate":
            if body.get("model") == "slow-model":
                time.sleep(30)
                return
            time.sleep(0.5)
            self._send(200, json.dumps({"response": "ok", "done": True}))
        elif self.path == "/api/chat":
            model = body.get("model", "")
            if model == "flaky-model":
                n = CALLS.get("flaky", 0) + 1
                CALLS["flaky"] = n
                if n <= 2:
                    self._send(500, json.dumps({"error": "model overloaded"}))
                    return
            if model == "notools-model" and "tools" in body:
                self._send(400, json.dumps({"error": "this model does not support tools"}))
                return
            if body.get("stream"):
                self.send_response(200)
                self.send_header("Content-Type", "application/x-ndjson")
                self.end_headers()
                for chunk in ["hel", "lo"]:
                    self.wfile.write((json.dumps(
                        {"message": {"role": "assistant", "content": chunk},
                         "done": False}) + "\n").encode())
                    self.wfile.flush()
                self.wfile.write((json.dumps({"done": True}) + "\n").encode())
                self.wfile.flush()
            else:
                self._send(200, json.dumps(
                    {"message": {"role": "assistant", "content": "hello"}}))
        else:
            self._send(404, "{}")

server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(f"PORT {server.server_address[1]}", flush=True)
server.serve_forever()
"#;

fn user_message(text: &str) -> Message {
    Message {
        role: Role::User,
        content: MessageContent::Text(text.to_string()),
        tool_call_id: None,
    }
}

/// Timeout knobs are process-global env: serialize tests that mutate them.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn ollama_mock_end_to_end() {
    // Timeout knobs are process-global: serialize. The guard is held across
    // awaits, which is safe here (single critical section per test, no lock
    // ordering, test-only).
    #![allow(clippy::await_holding_lock)]
    let _env = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("mock_ollama.py");
    std::fs::write(&script, MOCK_OLLAMA_PY).expect("write mock");

    // `python3` with a `python` fallback (Windows runners may only
    // provide the latter).
    let python = ["python3", "python"]
        .into_iter()
        .find(|exe| {
            std::process::Command::new(exe)
                .arg("--version")
                .output()
                .is_ok()
        })
        .unwrap_or("python3");
    let mut child = tokio::process::Command::new(python)
        .arg(&script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn mock ollama");

    // Read the PORT line the mock prints on startup.
    let port: u16 = {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let stdout = child.stdout.take().expect("stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            reader.read_line(&mut line),
        )
        .await
        .expect("mock startup")
        .expect("read");
        // Stash the reader back so the pipe stays open.
        child.stdout = Some(reader.into_inner());
        line.trim()
            .strip_prefix("PORT ")
            .expect("port line")
            .parse()
            .expect("port")
    };
    let base_url = format!("http://127.0.0.1:{port}");

    // 1. Warmup + streaming success against the mock.
    std::env::set_var("OLLAMA_INITIAL_TIMEOUT", "30");
    std::env::set_var("OLLAMA_CHUNK_TIMEOUT", "30");
    let provider = OllamaProvider::new()
        .expect("provider")
        .with_base_url(&base_url)
        .with_model("mock-model");
    let collected = Arc::new(Mutex::new(String::new()));
    let sink = collected.clone();
    let callback: Box<dyn Fn(tark_cli::llm::StreamEvent) + Send + Sync> = Box::new(move |event| {
        if let tark_cli::llm::StreamEvent::TextDelta(delta) = event {
            sink.lock().unwrap().push_str(&delta);
        }
    });
    let response = provider
        .chat_streaming(&[user_message("hi")], None, callback, None)
        .await
        .expect("streaming chat");
    assert!(
        matches!(response, LlmResponse::Text { .. }),
        "expected text, got {response:?}"
    );
    assert_eq!(response.text(), Some("hello"));
    assert_eq!(collected.lock().unwrap().as_str(), "hello");

    // 2. Missing model fails fast with a pull remediation.
    let missing = OllamaProvider::new()
        .expect("provider")
        .with_base_url(&base_url)
        .with_model("missing-model");
    let err = missing
        .chat(&[user_message("hi")], None)
        .await
        .expect_err("must fail");
    assert!(
        err.to_string().contains("ollama pull missing-model"),
        "unexpected error: {err}"
    );

    // 3. Unreachable server fails fast with a serve remediation.
    let down = OllamaProvider::new()
        .expect("provider")
        .with_base_url("http://127.0.0.1:1")
        .with_model("mock-model");
    let err = down
        .chat(&[user_message("hi")], None)
        .await
        .expect_err("must fail");
    assert!(
        err.to_string().contains("ollama serve"),
        "unexpected error: {err}"
    );

    // 4. Cold load past the initial timeout surfaces the knob, not a hang.
    std::env::set_var("OLLAMA_INITIAL_TIMEOUT", "2");
    let slow = OllamaProvider::new()
        .expect("provider")
        .with_base_url(&base_url)
        .with_model("slow-model");
    let err = slow
        .chat(&[user_message("hi")], None)
        .await
        .expect_err("must fail");
    assert!(
        err.to_string().contains("OLLAMA_INITIAL_TIMEOUT"),
        "unexpected error: {err}"
    );

    std::env::remove_var("OLLAMA_INITIAL_TIMEOUT");
    std::env::remove_var("OLLAMA_CHUNK_TIMEOUT");
    let _ = child.kill().await;
}

/// Spawns the mock and returns its base URL plus the child handle.
/// Each test gets a fresh server (per-model call counters stay isolated).
async fn spawn_mock() -> (String, tokio::process::Child) {
    let dir = tempfile::tempdir().expect("tempdir");
    // Leak the dir: the server needs the script for its whole lifetime.
    let script = dir.path().join("mock_ollama.py");
    std::fs::write(&script, MOCK_OLLAMA_PY).expect("write mock");
    std::mem::forget(dir);
    let python = ["python3", "python"]
        .into_iter()
        .find(|exe| {
            std::process::Command::new(exe)
                .arg("--version")
                .output()
                .is_ok()
        })
        .unwrap_or("python3");
    let mut child = tokio::process::Command::new(python)
        .arg(&script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn mock ollama");
    let port: u16 = {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let stdout = child.stdout.take().expect("stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            reader.read_line(&mut line),
        )
        .await
        .expect("mock startup")
        .expect("read");
        child.stdout = Some(reader.into_inner());
        line.trim()
            .strip_prefix("PORT ")
            .expect("port line")
            .parse()
            .expect("port")
    };
    (format!("http://127.0.0.1:{port}"), child)
}

fn sample_tool() -> tark_cli::llm::ToolDefinition {
    tark_cli::llm::ToolDefinition {
        name: "get_weather".to_string(),
        description: "Get weather for a city".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"],
        }),
    }
}

/// Transient 500s are retried: the mock fails twice, then answers.
#[tokio::test]
async fn ollama_mock_retries_transient_500s() {
    #![allow(clippy::await_holding_lock)]
    let _env = ENV_LOCK.lock().unwrap();
    std::env::set_var("OLLAMA_INITIAL_TIMEOUT", "30");
    std::env::set_var("OLLAMA_CHUNK_TIMEOUT", "30");
    let (base_url, mut child) = spawn_mock().await;
    let provider = OllamaProvider::new()
        .expect("provider")
        .with_base_url(&base_url)
        .with_model("flaky-model");
    let response = provider
        .chat(&[user_message("hi")], Some(&[sample_tool()]))
        .await
        .expect("retried chat succeeds");
    assert_eq!(response.text(), Some("hello"));
    std::env::remove_var("OLLAMA_INITIAL_TIMEOUT");
    std::env::remove_var("OLLAMA_CHUNK_TIMEOUT");
    let _ = child.kill().await;
}

/// Models rejecting the tools parameter get one tools-stripped retry.
#[tokio::test]
async fn ollama_mock_falls_back_without_tools() {
    #![allow(clippy::await_holding_lock)]
    let _env = ENV_LOCK.lock().unwrap();
    std::env::set_var("OLLAMA_INITIAL_TIMEOUT", "30");
    std::env::set_var("OLLAMA_CHUNK_TIMEOUT", "30");
    let (base_url, mut child) = spawn_mock().await;
    let provider = OllamaProvider::new()
        .expect("provider")
        .with_base_url(&base_url)
        .with_model("notools-model");
    let response = provider
        .chat(&[user_message("hi")], Some(&[sample_tool()]))
        .await
        .expect("fallback chat succeeds");
    assert_eq!(response.text(), Some("hello"));
    std::env::remove_var("OLLAMA_INITIAL_TIMEOUT");
    std::env::remove_var("OLLAMA_CHUNK_TIMEOUT");
    let _ = child.kill().await;
}
