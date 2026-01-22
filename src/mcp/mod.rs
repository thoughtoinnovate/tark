//! MCP (Model Context Protocol) client implementation.
//!
//! Enables tark to connect to external MCP servers and use their tools.

#![allow(dead_code)]

pub mod client;
pub mod transport;
pub mod types;
pub mod wrapper;

// Re-export main types
pub use client::McpServerManager;
pub use transport::StdioTransport;
pub use types::{ConnectionStatus, McpContent, McpToolDef, McpToolResult, ServerCapabilities};
pub use wrapper::{wrap_server_tools, McpToolWrapper};
