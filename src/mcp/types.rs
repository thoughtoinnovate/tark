//! MCP protocol types and data structures.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Status of an MCP server connection
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    /// Not connected
    #[default]
    Disconnected,
    /// Currently attempting to connect
    Connecting,
    /// Successfully connected
    Connected,
    /// Connection failed with error message
    Failed(String),
}

impl ConnectionStatus {
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Get display string
    pub fn display(&self) -> &str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting...",
            Self::Connected => "Connected",
            Self::Failed(_) => "Failed",
        }
    }

    /// Get icon for TUI
    pub fn icon(&self) -> &str {
        match self {
            Self::Disconnected => "○",
            Self::Connecting => "◐",
            Self::Connected => "●",
            Self::Failed(_) => "✗",
        }
    }
}

/// Tool definition from MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    /// Tool name
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: String,
    /// JSON Schema for input parameters
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
}

/// Resource definition from MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceDef {
    /// Resource URI
    pub uri: String,
    /// Resource name
    pub name: String,
    /// Resource description
    #[serde(default)]
    pub description: String,
    /// MIME type
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Content returned by the tool
    pub content: Vec<McpContent>,
    /// Whether the call resulted in an error
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// Content item in MCP responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content (base64)
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// Resource reference
    #[serde(rename = "resource")]
    Resource { uri: String },
}

impl McpToolResult {
    /// Convert to string representation
    pub fn to_text(&self) -> String {
        self.content
            .iter()
            .map(|c| match c {
                McpContent::Text { text } => text.clone(),
                McpContent::Image { .. } => "[Image]".to_string(),
                McpContent::Resource { uri } => format!("[Resource: {}]", uri),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Server capabilities returned during initialization
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Whether server supports tools
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    /// Whether server supports resources
    #[serde(default)]
    pub resources: Option<ResourcesCapability>,
    /// Whether server supports prompts
    #[serde(default)]
    pub prompts: Option<PromptsCapability>,
}

/// Tools capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// Whether tool list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}

/// Resources capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesCapability {
    /// Whether resource list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
    /// Whether server supports subscriptions
    #[serde(default)]
    pub subscribe: bool,
}

/// Prompts capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptsCapability {
    /// Whether prompt list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}
