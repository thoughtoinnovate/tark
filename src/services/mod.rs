//! Service layer for tark
//!
//! Services provide abstraction layers between tools and storage,
//! handling business logic and coordination.

pub mod plan_service;

pub use plan_service::PlanService;
