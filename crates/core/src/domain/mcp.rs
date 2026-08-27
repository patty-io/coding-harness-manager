//! Canonical MCP servers. V1 exposes global scope; schema supports project.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
    Unknown,
}

impl McpTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "stdio" => Self::Stdio,
            "http" => Self::Http,
            "sse" => Self::Sse,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScopeType {
    Global,
    Project,
}

impl ScopeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "project" => Self::Project,
            _ => Self::Global,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServer {
    pub id: Uuid,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: serde_json::Map<String, serde_json::Value>,
    pub scope_type: ScopeType,
    pub scope_path: Option<String>,
    pub provenance: serde_json::Value,
    pub enabled: bool,
}
