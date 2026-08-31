//! Claude Code adapter for the documented settings, model, and MCP surfaces.

pub mod parser;
pub mod writer;

use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::provider::Protocol;
use chm_harness_sdk::adapter::parse_version_supported;
use chm_harness_sdk::adapter::plan::PlanAction;
use chm_harness_sdk::adapter::route::{
    CredentialTarget, ModelIdentityRules, ModelMetadataCapabilities, ProviderTopology,
    RouteDeploymentCapabilities,
};
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
            .with_route_deployment(RouteDeploymentCapabilities {
                provider_topology: ProviderTopology::SingleGlobalOverride,
                protocols: vec![Protocol::AnthropicMessages],
                credential_targets: vec![CredentialTarget::CommandHelper],
                model_identity: ModelIdentityRules {
                    case_sensitive: true,
                    allow_namespaced_ids: true,
                },
                metadata: ModelMetadataCapabilities {
                    context_window: false,
                    max_input: false,
                    max_output: false,
                },
            })
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
                protected_changes: vec![],
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
                    let model_id = a
                        .payload
                        .get("remote_model_id")
                        .and_then(|v| v.as_str())
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "Claude Code model {} is missing its remote model id",
                                a.identity
                            ))
                        })?;
                    let base_url = a
                        .payload
                        .get("base_url")
                        .and_then(|v| v.as_str())
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "Claude Code provider for {} is missing its base URL",
                                a.identity
                            ))
                        })?;
                    writer::fold_gateway(&mut doc, model_id, base_url, credential_ref_id(a));
                    folded = true;
                }
                PlanAction::Update(u) if u.kind == "model" => {
                    let base_url = u
                        .desired
                        .get("overrides")
                        .and_then(|value| value.get("base_url"))
                        .and_then(|value| value.as_str())
                        .or_else(|| u.desired.get("base_url").and_then(|value| value.as_str()));
                    if writer::update_gateway(
                        &mut doc,
                        &u.identity,
                        base_url,
                        credential_ref_id_from_value(&u.desired),
                    ) {
                        folded = true;
                    } else {
                        warnings.push(format!(
                            "update skipped: model {} is not the active Claude Code model",
                            u.identity
                        ));
                    }
                }
                PlanAction::Remove(r) if r.kind == "model" => {
                    if writer::remove_gateway(&mut doc, &r.identity) {
                        folded = true;
                    } else {
                        warnings.push(format!(
                            "remove skipped: model {} is not the active Claude Code model",
                            r.identity
                        ));
                    }
                }
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => {
                    warnings.push(format!("conflict on {}: {}", c.identity, c.reason))
                }
                PlanAction::Add(a) => warnings.push(format!(
                    "{} action for {} not supported by claude-code writer yet",
                    a.kind, a.identity
                )),
                PlanAction::Update(u) => warnings.push(format!(
                    "{} update for {} is not supported by Claude Code writer yet",
                    u.kind, u.identity
                )),
                PlanAction::Remove(r) => warnings.push(format!(
                    "{} removal for {} is not supported by Claude Code writer yet",
                    r.kind, r.identity
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
            protected_changes: vec![],
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

fn credential_ref_id(action: &chm_harness_sdk::adapter::plan::AddAction) -> Option<uuid::Uuid> {
    credential_ref_id_from_value(&action.payload)
}

fn credential_ref_id_from_value(value: &serde_json::Value) -> Option<uuid::Uuid> {
    value
        .get("credential_ref_id")
        .or_else(|| {
            value
                .get("overrides")
                .and_then(|overrides| overrides.get("native_provider_config"))
                .and_then(|config| config.get("credential_ref_id"))
        })
        .and_then(|value| value.as_str())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
}
