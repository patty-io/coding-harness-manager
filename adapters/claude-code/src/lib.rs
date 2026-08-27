//! Claude Code read-only adapter.

pub mod parser;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::types::{
    AdapterError, HarnessAdapter, HarnessCapabilities, ParsedState,
};

pub struct ClaudeCodeAdapter;

impl HarnessAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::none()
            .with_models(true)
            .with_providers(true)
            .with_mcp_global(true)
            .with_global_skills(true)
            .with_profiles(true)
            .with_runtime_env(true)
            .with_model_aliases(true)
            .with_symlinked_skills(true)
    }

    fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError> {
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
            install.config_path.as_deref().unwrap_or(""),
            ".claude",
        );
        let settings_raw =
            chm_harness_sdk::adapter::helpers::read_optional(&home.join(".claude/settings.json"))?;
        let claude_json_raw =
            chm_harness_sdk::adapter::helpers::read_optional(&home.join(".claude.json"))?;
        parser::parse_config(settings_raw.as_deref(), claude_json_raw.as_deref(), &home)
    }
}
