//! The stable harness adapter contract.

use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::mcp::McpServer;
use chm_core::domain::models::ModelRoute;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("harness {harness} version {version:?} is not supported by this adapter")]
    UnsupportedVersion {
        harness: String,
        version: Option<String>,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error in {path}: {detail}")]
    Parse { path: String, detail: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid state: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct HarnessCapabilities {
    pub supports_custom_models: bool,
    pub supports_custom_providers: bool,
    pub supports_model_catalog: bool,
    pub supports_profiles: bool,
    pub supports_mcp_global: bool,
    pub supports_mcp_project: bool,
    pub supports_global_skills: bool,
    pub supports_project_skills: bool,
    pub supports_runtime_env: bool,
    pub supports_model_aliases: bool,
    pub supports_symlinked_skills: bool,
}

impl HarnessCapabilities {
    pub fn none() -> Self {
        Self {
            supports_custom_models: false,
            supports_custom_providers: false,
            supports_model_catalog: false,
            supports_profiles: false,
            supports_mcp_global: false,
            supports_mcp_project: false,
            supports_global_skills: false,
            supports_project_skills: false,
            supports_runtime_env: false,
            supports_model_aliases: false,
            supports_symlinked_skills: false,
        }
    }

    pub fn with_models(mut self, v: bool) -> Self {
        self.supports_custom_models = v;
        self
    }
    pub fn with_providers(mut self, v: bool) -> Self {
        self.supports_custom_providers = v;
        self
    }
    pub fn with_mcp_global(mut self, v: bool) -> Self {
        self.supports_mcp_global = v;
        self
    }
    pub fn with_global_skills(mut self, v: bool) -> Self {
        self.supports_global_skills = v;
        self
    }
    pub fn with_profiles(mut self, v: bool) -> Self {
        self.supports_profiles = v;
        self
    }
    pub fn with_runtime_env(mut self, v: bool) -> Self {
        self.supports_runtime_env = v;
        self
    }
    pub fn with_model_aliases(mut self, v: bool) -> Self {
        self.supports_model_aliases = v;
        self
    }
    pub fn with_symlinked_skills(mut self, v: bool) -> Self {
        self.supports_symlinked_skills = v;
        self
    }
}

#[derive(Debug, Clone)]
pub struct HarnessModel {
    pub native_id: String,
    pub route: ModelRoute,
}

#[derive(Debug, Clone)]
pub struct HarnessMcp {
    pub native_name: String,
    pub server: McpServer,
}

#[derive(Debug, Clone)]
pub struct HarnessSkill {
    pub name: String,
    pub path: String,
    pub content_hash: Option<String>,
    pub symlinked: bool,
}

#[derive(Debug, Default, Clone)]
pub struct ParsedState {
    pub models: Vec<HarnessModel>,
    pub providers: Vec<serde_json::Value>,
    pub mcp: Vec<HarnessMcp>,
    pub skills: Vec<HarnessSkill>,
    pub profiles: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeChange {
    pub file_path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeLink {
    pub kind: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NativePlan {
    pub changes: Vec<NativeChange>,
    pub links: Vec<NativeLink>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub files_written: Vec<String>,
    pub links_created: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub ok: bool,
    pub errors: Vec<String>,
}

pub trait HarnessAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> HarnessCapabilities;
    fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError>;
    fn plan(
        &self,
        _plan: &crate::adapter::plan::ReconciliationPlan,
        _install: &HarnessInstallation,
    ) -> Result<NativePlan, AdapterError> {
        Err(AdapterError::UnsupportedVersion {
            harness: self.id().into(),
            version: None,
        })
    }
    fn apply(
        &self,
        _install: &HarnessInstallation,
        _native_plan: &NativePlan,
    ) -> Result<ApplyResult, AdapterError> {
        Err(AdapterError::UnsupportedVersion {
            harness: self.id().into(),
            version: None,
        })
    }
    fn validate(&self, _install: &HarnessInstallation) -> Result<ValidationReport, AdapterError> {
        Err(AdapterError::UnsupportedVersion {
            harness: self.id().into(),
            version: None,
        })
    }
    fn rollback(
        &self,
        _install: &HarnessInstallation,
        _native_plan: &NativePlan,
    ) -> Result<(), AdapterError> {
        Err(AdapterError::UnsupportedVersion {
            harness: self.id().into(),
            version: None,
        })
    }
}

/// Version gate: supported list uses two-component prefixes ("0.30").
/// None (undetectable version) is treated as supported — the caller adds a
/// read-only-safety warning instead of failing.
pub fn parse_version_supported(version: Option<&str>, supported: &[&str]) -> bool {
    match version {
        None => true,
        Some(v) => {
            let prefix: String = v.split('.').take(2).collect::<Vec<_>>().join(".");
            supported.iter().any(|s| prefix == *s)
        }
    }
}
