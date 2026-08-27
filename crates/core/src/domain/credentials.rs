//! Secrets are NEVER stored in SQLite — only references.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

crate::wire_serializable_enum!(CredentialKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    Keychain,
    WindowsCredentialManager,
    Libsecret,
    Env,
    Vault,
    Unknown,
}

impl CredentialKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::WindowsCredentialManager => "windows-credential-manager",
            Self::Libsecret => "libsecret",
            Self::Env => "env",
            Self::Vault => "vault",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "keychain" => Self::Keychain,
            "windows-credential-manager" => Self::WindowsCredentialManager,
            "libsecret" => Self::Libsecret,
            "env" => Self::Env,
            "vault" => Self::Vault,
            _ => Self::Unknown,
        }
    }
}

/// A reference to a secret: either an OS-native store entry or an env var name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialRef {
    pub id: Uuid,
    pub kind: CredentialKind,
    /// e.g. "coding-harness-manager/providers/<uuid>" (keychain) or "ZAI_API_KEY" (env)
    pub reference: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
