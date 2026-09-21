//! Transport layer for CLI, HTTP, and stdio communication

pub mod acp;
pub mod cli;
pub mod dashboard;
pub mod http;
pub mod mcp_cli;
pub mod plugin_cli;
pub mod remote;
pub mod update;

pub use http::update_status;
