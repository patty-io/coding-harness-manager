//! Configuration sets: reusable bundles of routes/MCP/skills/profiles.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SetItemType {
    ModelRoute,
    McpServer,
    Skill,
    LaunchProfile,
}

impl SetItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ModelRoute => "model_route",
            Self::McpServer => "mcp_server",
            Self::Skill => "skill",
            Self::LaunchProfile => "launch_profile",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "mcp_server" => Self::McpServer,
            "skill" => Self::Skill,
            "launch_profile" => Self::LaunchProfile,
            _ => Self::ModelRoute,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationSet {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationSetItem {
    pub id: Uuid,
    pub configuration_set_id: Uuid,
    pub item_type: SetItemType,
    pub item_id: Uuid,
}
