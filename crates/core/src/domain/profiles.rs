//! Launch profiles: harness + route + endpoint + env + role mappings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::harness::HarnessType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleMapping {
    /// e.g. "opus", "sonnet", "haiku"
    pub role: String,
    /// remote model id to substitute for that role
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchProfile {
    pub id: Uuid,
    pub name: String,
    pub harness_type: HarnessType,
    pub model_route_id: Option<Uuid>,
    pub provider_endpoint_id: Option<Uuid>,
    pub env: serde_json::Map<String, serde_json::Value>,
    pub role_mappings: Vec<RoleMapping>,
    pub native_overrides: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
