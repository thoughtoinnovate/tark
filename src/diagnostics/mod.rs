//! AI-powered code diagnostics

mod analyzer;

#[allow(unused_imports)]
pub use analyzer::Diagnostic;
pub use analyzer::{DiagnosticSeverity, DiagnosticsEngine};
