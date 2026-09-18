//! MCP (Model Context Protocol) client implementation.
//!
//! Enables tark to connect to external MCP servers and use their tools.

#![allow(dead_code)]

pub mod client;
pub mod transport;
pub mod trust;
pub mod types;
pub mod wrapper;

// Re-export main types
pub use client::{conformance_check, McpServerManager};
pub use transport::{ActiveTransport, StdioTransport, StreamableHttpTransport};
pub use trust::{McpTrustRecord, McpTrustStore};
pub use types::{
    ConformanceCheck, ConformanceReport, ConnectionState, ConnectionStatus, ExtensionGate,
    HttpMcpConfig, McpContent, McpError, McpInspectSummary, McpResourceDef, McpServerEndpoint,
    McpServerTransport, McpToolDef, McpToolResult, ServerCapabilities, MCP_PROTOCOL_REVISION,
};
pub use wrapper::{wrap_server_tools, McpToolWrapper};
