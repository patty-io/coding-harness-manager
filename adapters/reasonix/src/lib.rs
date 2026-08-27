//! Reasonix read-only adapter.

pub mod parser;

use std::path::Path;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::types::{
    AdapterError, HarnessAdapter, HarnessCapabilities, ParsedState,
};
use chm_harness_sdk::definition::Platform;

pub struct ReasonixAdapter;

impl HarnessAdapter for ReasonixAdapter {
    fn id(&self) -> &'static str {
        "reasonix"
    }

    fn detect(&self, home: &Path, path_env: Option<&str>) -> Option<HarnessInstallation> {
        let def = chm_harness_sdk::definition::tier1_definitions()
            .into_iter()
            .find(|d| d.id == "reasonix")?;
        chm_harness_sdk::adapter::helpers::detect_one(&def, home, Platform::MacOs, path_env)
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::none()
            .with_models(true)
            .with_providers(true)
            .with_global_skills(true)
            .with_runtime_env(true)
            .with_symlinked_skills(true)
    }

    fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError> {
        let config_path = install
            .config_path
            .as_ref()
            .ok_or_else(|| AdapterError::NotFound("config_path".into()))?;
        let raw = std::fs::read_to_string(config_path)?;
        let home =
            chm_harness_sdk::adapter::helpers::install_home_from_config(config_path, ".reasonix");
        parser::parse_config(&raw, &home)
    }
}
