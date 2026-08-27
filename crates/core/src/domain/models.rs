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

crate::wire_serializable_enum!(CatalogStatus);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn parse_str(s: &str) -> Self {
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

impl ModelRoute {
    /// Builder with the common boilerplate filled in; callers set the
    /// endpoint/identity links and variable fields.
    pub fn new(
        remote_model_id: String,
        display_name: String,
        context_window: Option<i64>,
        capabilities: serde_json::Value,
        overrides: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            endpoint_id: Uuid::new_v4(), // real endpoint linking happens at import time
            model_identity_id: None,
            remote_model_id,
            display_name,
            context_window,
            max_input: None,
            max_output: None,
            capabilities,
            overrides,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
