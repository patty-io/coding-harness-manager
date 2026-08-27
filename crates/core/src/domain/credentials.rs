//! Secrets are NEVER stored in SQLite — only references.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CredentialKind {
    Keychain,
    WindowsCredentialManager,
    Libsecret,
    Env,
    Vault,
}

impl CredentialKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::WindowsCredentialManager => "windows-credential-manager",
            Self::Libsecret => "libsecret",
            Self::Env => "env",
            Self::Vault => "vault",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "windows-credential-manager" => Self::WindowsCredentialManager,
            "libsecret" => Self::Libsecret,
            "env" => Self::Env,
            "vault" => Self::Vault,
            _ => Self::Keychain,
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
