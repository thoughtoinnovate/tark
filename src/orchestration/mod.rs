//! Agent Orchestration - Meta-agent capabilities

pub mod agent_router;
pub mod multi_agent;
pub mod pipeline;

pub use agent_router::AgentRouter;
pub use multi_agent::MultiAgentCoordinator;
pub use pipeline::AgentPipeline;
