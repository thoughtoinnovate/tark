//! Built-in tools that extend tark's capabilities.
//!
//! These are native Rust implementations that avoid external dependencies:
//! - `thinking`: Structured step-by-step reasoning
//! - `memory`: Persistent cross-session memory storage
//! - `todo`: Session-scoped todo list tracking

pub mod memory;
pub mod thinking;
pub mod todo;

// Re-export main types for convenient access
#[allow(unused_imports)]
pub use memory::{
    MemoryDeleteTool, MemoryEntry, MemoryListTool, MemoryQueryTool, MemoryStoreTool, TarkMemory,
};
#[allow(unused_imports)]
pub use thinking::{ThinkTool, ThinkingSummary, ThinkingTracker, Thought};
#[allow(unused_imports)]
pub use todo::{TodoItem, TodoStatus, TodoSummary, TodoTool, TodoTracker};
