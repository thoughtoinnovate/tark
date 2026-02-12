use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ACP_VERSION: &str = "2";

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InitializeParams {
    #[serde(default)]
    pub client: Option<AcpClientInfo>,
    #[serde(default)]
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AcpClientInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionCreateParams {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionSetModeParams {
    pub session_id: String,
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextUpdateParams {
    pub session_id: String,
    #[serde(default)]
    pub active_file: Option<String>,
    #[serde(default)]
    pub cursor: Option<CursorPos>,
    #[serde(default)]
    pub selection: Option<SelectionContext>,
    #[serde(default)]
    pub active_excerpt: Option<String>,
    #[serde(default)]
    pub buffers: Vec<BufferSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CursorPos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SelectionContext {
    #[serde(default)]
    pub start_line: usize,
    #[serde(default)]
    pub start_col: usize,
    #[serde(default)]
    pub end_line: usize,
    #[serde(default)]
    pub end_col: usize,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BufferSummary {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub modified: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageParams {
    pub session_id: String,
    pub message: String,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CancelParams {
    pub session_id: String,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloseParams {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalRespondParams {
    pub session_id: String,
    pub request_id: String,
    pub interaction_id: String,
    pub decision: String,
    #[serde(default)]
    pub selected_pattern: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestionnaireRespondParams {
    pub session_id: String,
    pub request_id: String,
    pub interaction_id: String,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub answers: std::collections::HashMap<String, Value>,
}
