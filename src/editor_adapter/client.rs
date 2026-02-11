use super::types::EditorContextV1;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterLocation {
    pub file: String,
    pub line: usize,
    #[serde(default)]
    pub col: usize,
    #[serde(default)]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterSymbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterDiagnostic {
    pub path: String,
    pub line: usize,
    pub col: usize,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterCursor {
    pub path: String,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterBufferInfo {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub modified: bool,
    pub filetype: String,
}

pub struct EditorAdapterClient {
    base_url: String,
    auth_token: Option<String>,
    client: reqwest::Client,
}

impl EditorAdapterClient {
    pub fn from_context(context: &EditorContextV1, timeout: Duration) -> Option<Self> {
        let base_url = context.endpoint.base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return None;
        }

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .ok()
            .unwrap_or_default();

        Some(Self {
            base_url: base_url.to_string(),
            auth_token: context.endpoint.auth_token.clone(),
            client,
        })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, url);
        if let Some(token) = &self.auth_token {
            req = req.bearer_auth(token);
        }
        req
    }

    async fn post_json(&self, path: &str, body: Value) -> Result<Value> {
        let response = self
            .request(reqwest::Method::POST, path)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("failed to call {}", path))?;

        if !response.status().is_success() {
            anyhow::bail!("adapter endpoint {} returned {}", path, response.status());
        }

        response
            .json::<Value>()
            .await
            .with_context(|| format!("invalid JSON from {}", path))
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        let response = self
            .request(reqwest::Method::GET, path)
            .send()
            .await
            .with_context(|| format!("failed to call {}", path))?;

        if !response.status().is_success() {
            anyhow::bail!("adapter endpoint {} returned {}", path, response.status());
        }

        response
            .json::<Value>()
            .await
            .with_context(|| format!("invalid JSON from {}", path))
    }

    pub async fn health(&self) -> Result<()> {
        self.get_json("/editor/health").await.map(|_| ())
    }

    pub async fn definition(
        &self,
        file: &str,
        line: usize,
        col: usize,
    ) -> Result<Vec<AdapterLocation>> {
        let data = self
            .post_json(
                "/editor/definition",
                json!({ "file": file, "line": line, "col": col }),
            )
            .await?;
        let locations = data
            .get("locations")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(locations).unwrap_or_default())
    }

    pub async fn references(
        &self,
        file: &str,
        line: usize,
        col: usize,
    ) -> Result<Vec<AdapterLocation>> {
        let data = self
            .post_json(
                "/editor/references",
                json!({ "file": file, "line": line, "col": col }),
            )
            .await?;
        let locations = data
            .get("references")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(locations).unwrap_or_default())
    }

    pub async fn hover(&self, file: &str, line: usize, col: usize) -> Result<Option<String>> {
        let data = self
            .post_json(
                "/editor/hover",
                json!({ "file": file, "line": line, "col": col }),
            )
            .await?;
        Ok(data
            .get("hover")
            .and_then(Value::as_str)
            .map(ToString::to_string))
    }

    pub async fn symbols(&self, file: &str) -> Result<Vec<AdapterSymbol>> {
        let data = self
            .post_json("/editor/symbols", json!({ "file": file }))
            .await?;
        let symbols = data
            .get("symbols")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(symbols).unwrap_or_default())
    }

    pub async fn diagnostics(&self, path: Option<&str>) -> Result<Vec<AdapterDiagnostic>> {
        let payload = if let Some(path) = path {
            json!({ "path": path })
        } else {
            json!({})
        };
        let data = self.post_json("/editor/diagnostics", payload).await?;
        let diagnostics = data
            .get("diagnostics")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(diagnostics).unwrap_or_default())
    }

    pub async fn cursor(&self) -> Result<AdapterCursor> {
        let data = self.get_json("/editor/cursor").await?;
        serde_json::from_value(data).context("invalid cursor payload")
    }

    pub async fn buffers(&self) -> Result<Vec<AdapterBufferInfo>> {
        let data = self.get_json("/editor/buffers").await?;
        let buffers = data
            .get("buffers")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(buffers).unwrap_or_default())
    }

    pub async fn buffer_content(&self, path: &str) -> Result<Value> {
        self.post_json("/editor/buffer-content", json!({ "path": path }))
            .await
    }

    pub async fn open_file(
        &self,
        path: &str,
        line: Option<usize>,
        col: Option<usize>,
    ) -> Result<Value> {
        self.post_json(
            "/editor/open-file",
            json!({ "path": path, "line": line, "col": col }),
        )
        .await
    }
}
