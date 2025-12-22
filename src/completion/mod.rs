//! Code completion engine

mod cache;
mod engine;
mod fim;

pub use cache::CompletionCache;
#[allow(unused_imports)]
pub use engine::CompletionResponse;
pub use engine::{CompletionEngine, CompletionRequest};
pub use fim::FimBuilder;
