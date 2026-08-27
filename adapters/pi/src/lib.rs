//! Pi read-only adapter.

pub mod parser;

use std::path::Path;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::types::{
    AdapterError, HarnessAdapter, HarnessCapabilities, ParsedState,
};
use chm_harness_sdk::definition::Platform;

pub struct PiAdapter;

impl HarnessAdapter for PiAdapter {
    fn id(&self) -> &'static str {
        "pi"
    }

    fn detect(&self, home: &Path, path_env: Option<&str>) -> Option<HarnessInstallation> {
        let def = chm_harness_sdk::definition::tier1_definitions()
            .into_iter()
            .find(|d| d.id == "pi")?;
        chm_harness_sdk::adapter::helpers::detect_one(&def, home, Platform::MacOs, path_env)
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
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(config_path, ".pi");
        let agent_dir = home.join(".pi/agent");
        let models_raw =
            chm_harness_sdk::adapter::helpers::read_optional(&agent_dir.join("models.json"))?;
        let mcp_raw =
            chm_harness_sdk::adapter::helpers::read_optional(&agent_dir.join("mcp.json"))?;
        let settings_raw =
            chm_harness_sdk::adapter::helpers::read_optional(&agent_dir.join("settings.json"))?;
        if models_raw.is_none() && mcp_raw.is_none() && settings_raw.is_none() {
            // legacy TOML layout (pre-0.8x) is read in Phase 8; warn for now
            let mut state = ParsedState::default();
            state
                .warnings
                .push("no Pi JSON config found; legacy config.toml layout not yet parsed".into());
            return Ok(state);
        }
        parser::parse_config(
            models_raw.as_deref(),
            mcp_raw.as_deref(),
            settings_raw.as_deref(),
            &home,
        )
    }
}
