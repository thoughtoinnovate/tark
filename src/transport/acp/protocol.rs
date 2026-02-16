use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ACP_VERSION: &str = "2";
pub const ACP_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponseEnvelope {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: Value,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Implementation {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct InitializeParams {
    #[serde(default)]
    #[serde(alias = "protocolVersion")]
    pub protocol_version: Option<u32>,
    #[serde(default)]
    #[serde(alias = "clientCapabilities")]
    pub client_capabilities: Option<Value>,
    #[serde(default)]
    #[serde(alias = "clientInfo")]
    pub client_info: Option<Implementation>,

    // Legacy tark ACP initialize fields (kept for compatibility parsing)
    #[serde(default)]
    pub client: Option<AcpClientInfo>,
    #[serde(default)]
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AcpClientInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionNewParams {
    #[serde(alias = "cwd")]
    pub cwd: String,
    #[serde(default)]
    #[serde(alias = "mcpServers")]
    pub mcp_servers: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionLoadParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(alias = "cwd")]
    pub cwd: String,
    #[serde(default)]
    #[serde(alias = "mcpServers")]
    pub mcp_servers: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionSetModeParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(alias = "modeId")]
    pub mode_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionSetConfigOptionParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(alias = "configId")]
    pub config_id: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptRequestParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    pub prompt: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CancelParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloseParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextUpdateParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(default)]
    #[serde(alias = "activeFile")]
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ResourceLink {
        #[serde(default)]
        uri: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },
    Resource {
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestPermissionResponseResult {
    pub outcome: RequestPermissionOutcome,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RequestPermissionOutcome {
    Cancelled,
    Selected {
        #[serde(alias = "optionId")]
        option_id: String,
    },
}

pub fn prompt_to_text(prompt: &[ContentBlock]) -> String {
    let mut parts: Vec<String> = Vec::new();

    for block in prompt {
        match block {
            ContentBlock::Text { text } => {
                if !text.trim().is_empty() {
                    parts.push(text.clone());
                }
            }
            ContentBlock::Resource { text } => {
                if let Some(text_value) = text {
                    if !text_value.trim().is_empty() {
                        parts.push(text_value.clone());
                    }
                }
            }
            ContentBlock::ResourceLink { uri, title } => {
                let mut line = String::from("[Resource]");
                if let Some(title_value) = title {
                    if !title_value.trim().is_empty() {
                        line.push(' ');
                        line.push_str(title_value);
                    }
                }
                if let Some(uri_value) = uri {
                    if !uri_value.trim().is_empty() {
                        line.push_str(" <");
                        line.push_str(uri_value);
                        line.push('>');
                    }
                }
                parts.push(line);
            }
            ContentBlock::Unsupported => {}
        }
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn initialize_params_parses_upstream_fields() {
        let params: InitializeParams = serde_json::from_value(json!({
            "protocolVersion": 1,
            "clientCapabilities": { "fs": { "readTextFile": true } },
            "clientInfo": { "name": "nvim", "version": "0.1.0" }
        }))
        .expect("initialize params should parse");

        assert_eq!(params.protocol_version, Some(1));
        assert_eq!(
            params.client_info.as_ref().map(|c| c.name.as_str()),
            Some("nvim")
        );
        assert!(params.client_capabilities.is_some());
    }

    #[test]
    fn prompt_to_text_merges_supported_content_blocks() {
        let prompt = vec![
            ContentBlock::Text {
                text: "Explain this function".to_string(),
            },
            ContentBlock::Resource {
                text: Some("fn main() {}".to_string()),
            },
            ContentBlock::ResourceLink {
                uri: Some("file:///tmp/main.rs".to_string()),
                title: Some("main.rs".to_string()),
            },
        ];

        let merged = prompt_to_text(&prompt);
        assert!(merged.contains("Explain this function"));
        assert!(merged.contains("fn main() {}"));
        assert!(merged.contains("[Resource] main.rs <file:///tmp/main.rs>"));
    }
}
