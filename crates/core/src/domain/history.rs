//! Sync transactions and config snapshots (audit + rollback support).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

crate::wire_serializable_enum!(TransactionType);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Sync,
    Import,
    Rollback,
    Restore,
}

impl TransactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Import => "import",
            Self::Rollback => "rollback",
            Self::Restore => "restore",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "import" => Self::Import,
            "rollback" => Self::Rollback,
            "restore" => Self::Restore,
            _ => Self::Sync,
        }
    }
}

crate::wire_serializable_enum!(TransactionStatus);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Running,
    Succeeded,
    Failed,
}

impl TransactionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            _ => Self::Running,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncTransaction {
    pub id: Uuid,
    pub transaction_type: TransactionType,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: TransactionStatus,
    pub summary: Option<String>,
    pub plan: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigSnapshot {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub harness_installation_id: Uuid,
    pub path: String,
    pub before_content: Option<String>,
    pub after_content: Option<String>,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
}
