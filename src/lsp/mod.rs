//! LSP server implementation
//!
//! Deliberate capability subset only (requirement R9; not complete LSP 3.18):
//! initialize/shutdown/exit, incremental (+full) sync with UTF-16 positions,
//! completion, hover, `refactor` code actions, version-guarded diagnostics,
//! cancellation, and workspace folders. Stateless and read-only: no agent
//! sessions, MCP connections, permission state, or persistent policy state.

mod code_action;
mod completion;
mod diagnostics;
mod document;
mod hover;
mod server;

pub use server::run_lsp_server;
