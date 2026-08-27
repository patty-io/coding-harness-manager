//! Desired-state reconciliation engine. Pure: no I/O.

pub mod engine;
pub mod mcp_skills;
pub mod models;
pub mod plan;

pub use engine::*;
pub use plan::*;
