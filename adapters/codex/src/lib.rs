//! Codex read-only adapter.

pub mod parser;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::types::{
    AdapterError, HarnessAdapter, HarnessCapabilities, ParsedState,
};

pub struct CodexAdapter;

impl HarnessAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::none()
            .with_models(true)
            .with_providers(true)
            .with_mcp_global(true)
            .with_global_skills(true)
            .with_runtime_env(true)
    }

    fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError> {
        let config_path = install
            .config_path
            .as_ref()
            .ok_or_else(|| AdapterError::NotFound("config_path".into()))?;
        let raw = std::fs::read_to_string(config_path)?;
        let home =
            chm_harness_sdk::adapter::helpers::install_home_from_config(config_path, ".codex");
        parser::parse_main_config(&raw, &home)
    }
}
