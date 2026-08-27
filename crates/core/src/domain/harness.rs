//! Harness installations and bindings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HarnessType {
    ClaudeCode,
    Codex,
    OpenCode,
    Pi,
    Reasonix,
}

impl HarnessType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
            Self::Reasonix => "reasonix",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "claude-code" => Self::ClaudeCode,
            "codex" => Self::Codex,
            "opencode" => Self::OpenCode,
            "pi" => Self::Pi,
            _ => Self::Reasonix,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstallationStatus {
    Detected,
    Installed,
    ConfigMissing,
    Error,
}

impl InstallationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::Installed => "installed",
            Self::ConfigMissing => "config-missing",
            Self::Error => "error",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "installed" => Self::Installed,
            "config-missing" => Self::ConfigMissing,
            "error" => Self::Error,
            _ => Self::Detected,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessInstallation {
    pub id: Uuid,
    pub harness_type: HarnessType,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub config_path: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub status: InstallationStatus,
}

impl HarnessInstallation {
    pub fn status_v(&self) -> &'static str {
        self.status.as_str()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessModelBinding {
    pub id: Uuid,
    pub harness_installation_id: Uuid,
    pub model_route_id: Uuid,
    pub native_id: String,
    pub native_config: serde_json::Value,
    pub managed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BindingType {
    Symlink,
    Junction,
    Copy,
    Native,
    Unsupported,
}

impl BindingType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Symlink => "symlink",
            Self::Junction => "junction",
            Self::Copy => "copy",
            Self::Native => "native",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "symlink" => Self::Symlink,
            "junction" => Self::Junction,
            "copy" => Self::Copy,
            "native" => Self::Native,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessMcpBinding {
    pub id: Uuid,
    pub harness_installation_id: Uuid,
    pub mcp_server_id: Uuid,
    pub native_name: String,
    pub native_config: serde_json::Value,
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessSkillBinding {
    pub id: Uuid,
    pub harness_installation_id: Uuid,
    pub skill_id: Uuid,
    pub target_path: String,
    pub binding_type: BindingType,
    pub managed: bool,
    pub status: String,
}
