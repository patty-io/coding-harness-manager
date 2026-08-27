//! Claude Code read-only adapter.

pub mod parser;
pub mod writer;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::parse_version_supported;
use chm_harness_sdk::adapter::plan::PlanAction;
use chm_harness_sdk::adapter::types::{
    AdapterError, ApplyResult, HarnessAdapter, HarnessCapabilities, NativePlan, ParsedState,
    ValidationReport,
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

    fn plan(
        &self,
        plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
        install: &HarnessInstallation,
    ) -> Result<NativePlan, AdapterError> {
        if !parse_version_supported(install.version.as_deref(), &["2.1", "2.0"]) {
            return Ok(NativePlan {
                changes: vec![],
                links: vec![],
                warnings: vec![format!(
                    "Claude Code {:?} untested — read-only mode",
                    install.version
                )],
            });
        }
        let mut warnings = vec![];
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
            install.config_path.as_deref().unwrap_or(""),
            ".claude",
        );
        let settings_path = home.join(".claude/settings.json").display().to_string();
        let raw = std::fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".into());
        let mut doc = writer::parse_document(&raw).map_err(|e| AdapterError::Parse {
            path: settings_path.clone(),
            detail: e,
        })?;
        let mut folded = false;
        for action in &plan.actions {
            match action {
                PlanAction::Add(a) if a.kind == "model" => {
                    let role = a
                        .payload
                        .get("capabilities")
                        .and_then(|c| c.get("role"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("default");
                    let model_id = a
                        .payload
                        .get("remote_model_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let env_key = match role {
                        "opus" => "ANTHROPIC_DEFAULT_OPUS_MODEL",
                        "sonnet" => "ANTHROPIC_DEFAULT_SONNET_MODEL",
                        "haiku" => "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                        _ => "ANTHROPIC_MODEL",
                    };
                    writer::fold_env(&mut doc, env_key, model_id);
                    folded = true;
                }
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => {
                    warnings.push(format!("conflict on {}: {}", c.identity, c.reason))
                }
                PlanAction::Add(a) => warnings.push(format!(
                    "{} action for {} not supported by claude-code writer yet",
                    a.kind, a.identity
                )),
                _ => {}
            }
        }
        let changes = if folded {
            vec![chm_harness_sdk::adapter::types::NativeChange {
                file_path: settings_path,
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
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
            install.config_path.as_deref().unwrap_or(""),
            ".claude",
        );
        Ok(writer::validate_config(
            &home.join(".claude/settings.json").display().to_string(),
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
