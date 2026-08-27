//! Model identities, catalog entries, and model routes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelIdentity {
    pub id: Uuid,
    pub canonical_id: String,
    pub display_name: String,
    pub family: Option<String>,
    pub models_dev_id: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CatalogStatus {
    Available,
    New,
    Missing,
    Deprecated,
    Unknown,
}

impl CatalogStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::New => "new",
            Self::Missing => "missing",
            Self::Deprecated => "deprecated",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "available" => Self::Available,
            "new" => Self::New,
            "missing" => Self::Missing,
            "deprecated" => Self::Deprecated,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCatalogModel {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub remote_model_id: String,
    pub raw_metadata: serde_json::Value,
    pub canonical_model_id: Option<Uuid>,
    pub match_confidence: Option<u8>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub missing_since: Option<DateTime<Utc>>,
    pub status: CatalogStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRoute {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub model_identity_id: Option<Uuid>,
    pub remote_model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub max_input: Option<i64>,
    pub max_output: Option<i64>,
    pub capabilities: serde_json::Value,
    pub overrides: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Route identity — the dedup key for the whole system.
pub fn route_identity(endpoint_id: Uuid, remote_model_id: &str) -> (Uuid, String) {
    (endpoint_id, remote_model_id.to_string())
}
