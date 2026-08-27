//! Pi read-only adapter.

pub mod parser;
pub mod writer;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::parse_version_supported;
use chm_harness_sdk::adapter::plan::PlanAction;
use chm_harness_sdk::adapter::types::{
    AdapterError, ApplyResult, HarnessAdapter, HarnessCapabilities, NativePlan, ParsedState,
    ValidationReport,
};

pub struct PiAdapter;

impl HarnessAdapter for PiAdapter {
    fn id(&self) -> &'static str {
        "pi"
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

    fn plan(
        &self,
        plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
        install: &HarnessInstallation,
    ) -> Result<NativePlan, AdapterError> {
        if !parse_version_supported(install.version.as_deref(), &["0.84"]) {
            return Ok(NativePlan {
                changes: vec![],
                links: vec![],
                warnings: vec![format!(
                    "Pi {:?} untested — read-only mode",
                    install.version
                )],
            });
        }
        let mut warnings = vec![];
        let config_path = home_config(install, "models.json");
        let raw = std::fs::read_to_string(&config_path)
            .unwrap_or_else(|_| r#"{"providers": {}}"#.to_string());
        let mut doc = writer::parse_document(&raw).map_err(|e| AdapterError::Parse {
            path: config_path.clone(),
            detail: e,
        })?;
        let mut folded = false;
        for action in &plan.actions {
            match action {
                PlanAction::Add(a) if a.kind == "model" => {
                    let provider_id = a
                        .payload
                        .get("native_provider_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("custom");
                    let model_id = a
                        .payload
                        .get("remote_model_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let display = a
                        .payload
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(model_id);
                    writer::fold_model(
                        &mut doc,
                        provider_id,
                        model_id,
                        display,
                        a.payload.get("context_window").and_then(|v| v.as_i64()),
                    );
                    folded = true;
                }
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => {
                    warnings.push(format!("conflict on {}: {}", c.identity, c.reason))
                }
                PlanAction::Add(a) => warnings.push(format!(
                    "{} action for {} not supported by pi writer yet",
                    a.kind, a.identity
                )),
                _ => {}
            }
        }
        let changes = if folded {
            vec![chm_harness_sdk::adapter::types::NativeChange {
                file_path: config_path,
                before: Some(raw),
                after: Some(writer::serialize(&doc)),
            }]
        } else {
            vec![]
        };
        Ok(NativePlan {
            changes,
            links: vec![],
            warnings,
        })
    }

    fn apply(
        &self,
        _install: &HarnessInstallation,
        native_plan: &NativePlan,
    ) -> Result<ApplyResult, AdapterError> {
        chm_harness_sdk::adapter::helpers::apply_native_plan(native_plan)
            .map_err(AdapterError::Invalid)
    }

    fn validate(&self, install: &HarnessInstallation) -> Result<ValidationReport, AdapterError> {
        Ok(writer::validate_config(&home_config(
            install,
            "models.json",
        )))
    }

    fn rollback(
        &self,
        _install: &HarnessInstallation,
        _native_plan: &NativePlan,
    ) -> Result<(), AdapterError> {
        Ok(())
    }
}

fn home_config(install: &HarnessInstallation, name: &str) -> String {
    let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
        install.config_path.as_deref().unwrap_or(""),
        ".pi",
    );
    home.join(".pi/agent").join(name).display().to_string()
}
