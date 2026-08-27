//! Reasonix read-only adapter.

pub mod parser;
pub mod writer;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::parse_version_supported;
use chm_harness_sdk::adapter::plan::PlanAction;
use chm_harness_sdk::adapter::types::{
    AdapterError, ApplyResult, HarnessAdapter, HarnessCapabilities, NativePlan, ParsedState,
    ValidationReport,
};

pub struct ReasonixAdapter;

impl HarnessAdapter for ReasonixAdapter {
    fn id(&self) -> &'static str {
        "reasonix"
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

    fn plan(
        &self,
        plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
        install: &HarnessInstallation,
    ) -> Result<NativePlan, AdapterError> {
        if !parse_version_supported(install.version.as_deref(), &["1.31"]) {
            return Ok(NativePlan {
                changes: vec![],
                links: vec![],
                warnings: vec![format!(
                    "Reasonix {:?} untested — read-only mode",
                    install.version
                )],
            });
        }
        let mut changes = vec![];
        let mut warnings = vec![];
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
            install.config_path.as_deref().unwrap_or(""),
            ".reasonix",
        );
        let config_path = home.join(".reasonix/config.toml").display().to_string();
        for action in &plan.actions {
            match action {
                PlanAction::Add(a) if a.kind == "model" => {
                    let provider_id = a
                        .payload
                        .get("native_provider_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("custom");
                    let kind = a
                        .payload
                        .get("capabilities")
                        .and_then(|c| c.get("kind"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("openai");
                    let model_id = a
                        .payload
                        .get("remote_model_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let base_url = a
                        .payload
                        .get("base_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("https://api.example.com/v1");
                    let env_key = a.payload.get("api_key_env").and_then(|v| v.as_str());
                    changes.push(writer::plan_provider_add(
                        &config_path,
                        provider_id,
                        kind,
                        base_url,
                        model_id,
                        env_key,
                    ));
                }
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => {
                    warnings.push(format!("conflict on {}: {}", c.identity, c.reason))
                }
                _ => {}
            }
        }
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
        writer::apply_native_plan(native_plan).map_err(|e| AdapterError::Invalid(e))
    }

    fn validate(&self, install: &HarnessInstallation) -> Result<ValidationReport, AdapterError> {
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
            install.config_path.as_deref().unwrap_or(""),
            ".reasonix",
        );
        Ok(writer::validate_config(
            &home.join(".reasonix/config.toml").display().to_string(),
        ))
    }

    fn rollback(
        &self,
        _install: &HarnessInstallation,
        _native_plan: &NativePlan,
    ) -> Result<(), AdapterError> {
        Ok(())
    }
}
