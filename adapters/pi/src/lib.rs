//! Pi read-only adapter.

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

pub struct PiAdapter;

impl HarnessAdapter for PiAdapter {
    fn id(&self) -> &'static str {
        "pi"
    }

    fn detect(&self, home: &Path, path_env: Option<&str>) -> Option<HarnessInstallation> {
        let def = chm_harness_sdk::definition::tier1_definitions()
            .into_iter()
            .find(|d| d.id == "pi")?;
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
            harness_type: HarnessType::Pi,
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
            .with_runtime_env(true)
    }

    fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError> {
        let config_path = install
            .config_path
            .as_ref()
            .ok_or_else(|| AdapterError::NotFound("config_path".into()))?;
        let home = install_home(config_path);
        let agent_dir = home.join(".pi/agent");
        let read_opt = |name: &str| -> Result<Option<String>, AdapterError> {
            let p = agent_dir.join(name);
            if p.exists() {
                Ok(Some(std::fs::read_to_string(p)?))
            } else {
                Ok(None)
            }
        };
        let models_raw = read_opt("models.json")?;
        let mcp_raw = read_opt("mcp.json")?;
        let settings_raw = read_opt("settings.json")?;
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

fn install_home(config_path: &str) -> std::path::PathBuf {
    // config_path is ~/.pi/agent/models.json (or a legacy file) — derive home
    let path = Path::new(config_path);
    for ancestor in path.ancestors() {
        if ancestor.file_name().is_some_and(|f| f == ".pi") {
            return ancestor.parent().map(Path::to_path_buf).unwrap_or_default();
        }
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}
