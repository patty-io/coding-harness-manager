//! OpenCode adapter.

pub mod parser;
pub mod writer;

use std::path::Path;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::parse_version_supported;
use chm_harness_sdk::adapter::plan::PlanAction;
use chm_harness_sdk::adapter::types::{
    AdapterError, ApplyResult, HarnessAdapter, HarnessCapabilities, NativeLink, NativePlan,
    ParsedState, ValidationReport,
};

pub struct OpenCodeAdapter;

impl HarnessAdapter for OpenCodeAdapter {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::none()
            .with_models(true)
            .with_providers(true)
            .with_mcp_global(true)
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
        let config_dir = std::path::Path::new(config_path)
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        parser::parse_config(&raw, &config_dir)
    }

    fn plan(
        &self,
        plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
        install: &HarnessInstallation,
    ) -> Result<NativePlan, AdapterError> {
        // version gate: read-only for untested versions
        if !parse_version_supported(
            install.version.as_deref(),
            &["0.30", "0.31", "0.32", "1.1", "1.18"],
        ) {
            return Ok(NativePlan {
                changes: vec![],
                links: vec![],
                warnings: vec![format!(
                    "OpenCode {:?} untested — read-only mode",
                    install.version
                )],
            });
        }
        let mut changes: Vec<chm_harness_sdk::adapter::types::NativeChange> = vec![];
        let links: Vec<NativeLink> = vec![];
        let mut warnings = vec![];
        let config_path = install.config_path.as_deref().unwrap_or("").to_string();

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
                    changes.push(writer::plan_model_add(
                        &config_path,
                        provider_id,
                        model_id,
                        display,
                        a.payload.get("context_window").and_then(|v| v.as_i64()),
                        a.payload
                            .get("capabilities")
                            .unwrap_or(&serde_json::json!({})),
                    ));
                }
                PlanAction::Add(a) if a.kind == "mcp" => {
                    let spec = &a.payload;
                    let name = a.identity.as_str();
                    let command = spec
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("npx");
                    let args: Vec<String> = spec
                        .get("args")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    changes.push(writer::plan_mcp_add(&config_path, name, command, &args));
                }
                PlanAction::Remove(r) if r.kind == "model" => {
                    warnings.push(format!(
                        "model removal for {} deferred to phase 12 (bindings)",
                        r.identity
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
            links,
            warnings,
        })
    }

    fn apply(
        &self,
        _install: &HarnessInstallation,
        native_plan: &NativePlan,
    ) -> Result<ApplyResult, AdapterError> {
        writer::apply_native_plan(native_plan).map_err(AdapterError::Invalid)
    }

    fn validate(&self, install: &HarnessInstallation) -> Result<ValidationReport, AdapterError> {
        let path = install.config_path.as_deref().unwrap_or("").to_string();
        Ok(writer::validate_config(&path))
    }

    fn rollback(
        &self,
        _install: &HarnessInstallation,
        _native_plan: &NativePlan,
    ) -> Result<(), AdapterError> {
        // each apply_native_plan write is atomic + backed up; sync flow restores
        // from filesystem backups. Kept for adapters with link steps.
        Ok(())
    }
}
