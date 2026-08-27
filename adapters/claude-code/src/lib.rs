//! Claude Code read-only adapter.

pub mod parser;

use std::path::Path;

use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::{
    AdapterError, HarnessAdapter, HarnessCapabilities, ParsedState,
};
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::paths::{find_executable, resolve_config_path};
use chm_harness_sdk::detect::version::detect_version;
use chrono::Utc;
use uuid::Uuid;

pub struct ClaudeCodeAdapter;

impl HarnessAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn detect(&self, home: &Path, path_env: Option<&str>) -> Option<HarnessInstallation> {
        let def = chm_harness_sdk::definition::tier1_definitions()
            .into_iter()
            .find(|d| d.id == "claude-code")?;
        let exe = def
            .executable_names
            .iter()
            .find_map(|n| find_executable(n, path_env));
        let config = resolve_config_path(&def, home, Platform::MacOs);
        if exe.is_none() && config.is_none() {
            return None;
        }
        let version = exe.as_ref().and_then(|e| detect_version(e, &["--version"]));
        Some(HarnessInstallation {
            id: Uuid::new_v4(),
            harness_type: HarnessType::ClaudeCode,
            executable_path: exe,
            version,
            config_path: config.map(|c| c.display().to_string()),
            detected_at: Utc::now(),
            last_scanned_at: Some(Utc::now()),
            status: InstallationStatus::Installed,
        })
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
        let home = install_home(install.config_path.as_deref().unwrap_or(""));
        let settings_path = home.join(".claude/settings.json");
        let settings_raw = settings_path
            .exists()
            .then(|| std::fs::read_to_string(&settings_path))
            .transpose()?;
        let claude_json_path = home.join(".claude.json");
        let claude_json_raw = claude_json_path
            .exists()
            .then(|| std::fs::read_to_string(&claude_json_path))
            .transpose()?;
        parser::parse_config(settings_raw.as_deref(), claude_json_raw.as_deref(), &home)
    }
}

fn install_home(config_path: &str) -> std::path::PathBuf {
    if !config_path.is_empty() {
        let path = Path::new(config_path);
        for ancestor in path.ancestors() {
            if ancestor.file_name().is_some_and(|f| f == ".claude") {
                return ancestor.parent().map(Path::to_path_buf).unwrap_or_default();
            }
        }
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}
