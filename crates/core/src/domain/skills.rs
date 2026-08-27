//! Skills: metadata in SQLite, files on disk. Never blobs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillSourceType {
    Folder,
    Git,
    HarnessImport,
    Package,
    Remote,
}

impl SkillSourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Git => "git",
            Self::HarnessImport => "harness-import",
            Self::Package => "package",
            Self::Remote => "remote",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "git" => Self::Git,
            "harness-import" => Self::HarnessImport,
            "package" => Self::Package,
            "remote" => Self::Remote,
            _ => Self::Folder,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    pub id: Uuid,
    pub name: String,
    pub canonical_path: String,
    pub source_type: SkillSourceType,
    pub source_url: Option<String>,
    pub content_hash: Option<String>,
    pub provenance: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
