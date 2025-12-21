//! LSP server implementation

mod code_action;
mod completion;
mod diagnostics;
mod document;
mod hover;
mod server;

pub use server::run_lsp_server;

