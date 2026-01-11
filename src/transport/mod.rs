//! Transport layer for CLI, HTTP, and stdio communication

pub mod cli;
pub mod dashboard;
pub mod http;
pub mod plugin_cli;

pub use http::update_status;
