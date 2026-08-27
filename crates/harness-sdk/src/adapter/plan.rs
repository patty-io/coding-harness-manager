//! Plan types + summary rendering.

use crate::adapter::types::{HarnessMcp, HarnessModel, HarnessSkill};
use chm_core::domain::mcp::McpServer;
use chm_core::domain::models::ModelRoute;
use chm_core::domain::skills::Skill;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Append,
    ReplaceManaged,
}

#[derive(Debug, Clone, Default)]
pub struct DesiredState {
    pub routes: Vec<ModelRoute>,
    pub mcp_servers: Vec<McpServer>,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, Default)]
pub struct ActualState {
    pub routes: Vec<HarnessModel>,
    pub mcp: Vec<HarnessMcp>,
    pub skills: Vec<HarnessSkill>,
    pub managed_flags: HashMap<String, bool>,
}

#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error("invalid desired state: {0}")]
    InvalidDesired(String),
    #[error("invalid actual state: {0}")]
    InvalidActual(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAction {
    pub kind: String,
    pub identity: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAction {
    pub kind: String,
    pub identity: String,
    pub changed_fields: Vec<String>,
    pub desired: serde_json::Value,
    pub current: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveAction {
    pub kind: String,
    pub identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnchangedAction {
    pub kind: String,
    pub identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictAction {
    pub kind: String,
    pub identity: String,
    pub reason: String,
    pub desired: serde_json::Value,
    pub current: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedAction {
    pub kind: String,
    pub identity: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanAction {
    Add(AddAction),
    Update(UpdateAction),
    Remove(RemoveAction),
    Unchanged(UnchangedAction),
    Conflict(ConflictAction),
    Unsupported(UnsupportedAction),
    NoOp(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconciliationPlan {
    pub actions: Vec<PlanAction>,
}

impl ReconciliationPlan {
    pub fn count(&self, kind: &str) -> usize {
        self.actions
            .iter()
            .filter(|a| {
                matches!(a, PlanAction::Add(x) if x.kind == kind)
                    || matches!(a, PlanAction::Update(x) if x.kind == kind)
                    || matches!(a, PlanAction::Remove(x) if x.kind == kind)
            })
            .count()
    }

    /// Dry-run line (project plan §31).
    pub fn summary(&self) -> String {
        let models = self.count("model");
        let mcp = self.count("mcp");
        let skills = self.count("skill");
        format!(
            "{n} items will change; {models} model records updated; {mcp} MCP server(s) added; {skills} skill(s) linked; 0 unmanaged settings modified.",
            n = models + mcp + skills
        )
    }
}
