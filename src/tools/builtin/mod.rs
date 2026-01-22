//! Built-in tools that extend tark's capabilities.
//!
//! These are native Rust implementations that avoid external dependencies:
//! - `thinking`: Structured step-by-step reasoning
//! - `memory`: Persistent cross-session memory storage

pub mod memory;
pub mod thinking;

// Re-export main types for convenient access
#[allow(unused_imports)]
pub use memory::{
    MemoryDeleteTool, MemoryEntry, MemoryListTool, MemoryQueryTool, MemoryStoreTool, TarkMemory,
};
#[allow(unused_imports)]
pub use thinking::{ThinkTool, ThinkingSummary, ThinkingTracker, Thought};
