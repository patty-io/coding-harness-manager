//! OpenCode adapter.

pub mod parser;
pub mod writer;

use std::path::Path;

use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::provider::Protocol;
use chm_harness_sdk::adapter::parse_version_supported;
use chm_harness_sdk::adapter::plan::PlanAction;
use chm_harness_sdk::adapter::protected::{
    JsonAuthFormat, ProtectedChangePlan, ProtectedOperation, ProtectedTarget,
};
use chm_harness_sdk::adapter::route::{
    CredentialTarget, ModelIdentityRules, ModelMetadataCapabilities, ProviderTopology,
    RouteDeploymentCapabilities,
};
use chm_harness_sdk::adapter::types::{
    AdapterError, ApplyResult, HarnessAdapter, HarnessCapabilities, NativeChange, NativePlan,
    ParsedState, ValidationReport,
};

pub struct OpenCodeAdapter;

impl HarnessAdapter for OpenCodeAdapter {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::none()
            .with_route_deployment(RouteDeploymentCapabilities {
                provider_topology: ProviderTopology::Multiple,
                protocols: vec![
                    Protocol::OpenAiChatCompletions,
                    Protocol::OpenAiResponses,
                    Protocol::AnthropicMessages,
                    Protocol::OpenRouterOpenAi,
                ],
                credential_targets: vec![
                    CredentialTarget::NativeSecretStore,
                    CredentialTarget::HarnessEnvFile,
                ],
                model_identity: ModelIdentityRules {
                    case_sensitive: true,
                    allow_namespaced_ids: true,
                },
                metadata: ModelMetadataCapabilities {
                    context_window: true,
                    max_input: true,
                    max_output: true,
                },
            })
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
        if !parse_version_supported(
            install.version.as_deref(),
            &["0.30", "0.31", "0.32", "1.1", "1.18"],
        ) {
            return Ok(NativePlan {
                changes: vec![],
                links: vec![],
                protected_changes: vec![],
                warnings: vec![format!(
                    "OpenCode {:?} untested — read-only mode",
                    install.version
                )],
            });
        }
        let mut warnings = vec![];
        let config_path = install.config_path.as_deref().unwrap_or("").to_string();
        let raw = std::fs::read_to_string(&config_path).unwrap_or_else(|_| "{}".into());
        let mut doc = writer::parse_document(&raw).map_err(|e| AdapterError::Parse {
            path: config_path.clone(),
            detail: e,
        })?;
        let mut folded = false;
        let mut protected_changes = Vec::new();
        let mut protected_providers = std::collections::HashSet::new();

        for action in &plan.actions {
            match action {
                PlanAction::Add(a) if a.kind == "model" => {
                    let provider_id = a
                        .payload
                        .get("native_provider_id")
                        .and_then(|v| v.as_str())
                        .or(a.native_provider_id.as_deref())
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "OpenCode model {} is missing its provider identity",
                                a.identity
                            ))
                        })?;
                    let model_id = a
                        .payload
                        .get("remote_model_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "OpenCode model {} is missing its remote model id",
                                a.identity
                            ))
                        })?;
                    let display = a
                        .payload
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(model_id);
                    writer::fold_model_with_provider(
                        &mut doc,
                        provider_id,
                        model_id,
                        display,
                        a.payload.get("context_window").and_then(|v| v.as_i64()),
                        a.payload
                            .get("capabilities")
                            .unwrap_or(&serde_json::json!({})),
                        a.payload
                            .get("overrides")
                            .and_then(|overrides| overrides.get("native_provider_config")),
                    );
                    let _ = writer::update_model_in_provider(
                        &mut doc,
                        Some(provider_id),
                        model_id,
                        display,
                        a.payload.get("context_window").and_then(|v| v.as_i64()),
                        a.payload.get("max_output").and_then(|v| v.as_i64()),
                    );
                    if let Some(credential_ref_id) = a
                        .payload
                        .get("credential_ref_id")
                        .or_else(|| {
                            a.payload
                                .get("overrides")
                                .and_then(|value| value.get("native_provider_config"))
                                .and_then(|value| value.get("credential_ref_id"))
                        })
                        .and_then(|value| value.as_str())
                        .filter(|_| {
                            a.payload
                                .get("overrides")
                                .and_then(|value| value.get("native_provider_config"))
                                .and_then(|value| value.get("credential_kind"))
                                .and_then(|value| value.as_str())
                                != Some("env")
                        })
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                        && protected_providers.insert(provider_id.to_string())
                    {
                        protected_changes.push(ProtectedChangePlan {
                            target: ProtectedTarget::JsonAuthFile {
                                path: auth_path(&config_path),
                                provider_id: provider_id.to_string(),
                                format: JsonAuthFormat::OpenCode,
                            },
                            credential_ref_id,
                            operation: ProtectedOperation::Upsert,
                        });
                    }
                    folded = true;
                }
                PlanAction::Add(a) if a.kind == "mcp" => {
                    let name = a.identity.clone();
                    let spec: chm_core::domain::mcp::McpServer =
                        serde_json::from_value(a.payload.clone()).map_err(|e| {
                            AdapterError::Parse {
                                path: config_path.clone(),
                                detail: format!("mcp payload not a McpServer: {e}"),
                            }
                        })?;
                    writer::fold_mcp(&mut doc, &name, &spec);
                    folded = true;
                }
                PlanAction::Update(u) if u.kind == "model" => {
                    let provider_id = u.native_provider_id.as_deref().or_else(|| {
                        u.desired
                            .get("overrides")
                            .and_then(|value| value.get("native_provider_id"))
                            .and_then(|value| value.as_str())
                    });
                    let display = u
                        .desired
                        .get("display_name")
                        .and_then(|value| value.as_str())
                        .unwrap_or(&u.identity);
                    let context = u
                        .desired
                        .get("context_window")
                        .and_then(|value| value.as_i64());
                    let max_output = u.desired.get("max_output").and_then(|value| value.as_i64());
                    if writer::update_model_in_provider(
                        &mut doc,
                        provider_id,
                        &u.identity,
                        display,
                        context,
                        max_output,
                    ) {
                        folded = true;
                    } else {
                        warnings.push(format!(
                            "update skipped: model {} not found in OpenCode config",
                            u.identity
                        ));
                    }
                }
                PlanAction::Remove(r) if r.kind == "model" => {
                    if writer::remove_model_in_provider(
                        &mut doc,
                        r.native_provider_id.as_deref(),
                        &r.identity,
                    ) {
                        folded = true;
                    } else {
                        warnings.push(format!(
                            "remove skipped: model {} not found in OpenCode config",
                            r.identity
                        ));
                    }
                }
                PlanAction::Update(u) => warnings.push(format!(
                    "{} update for {} is not supported by OpenCode writer yet",
                    u.kind, u.identity
                )),
                PlanAction::Remove(r) => warnings.push(format!(
                    "{} removal for {} is not supported by OpenCode writer yet",
                    r.kind, r.identity
                )),
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => {
                    warnings.push(format!("conflict on {}: {}", c.identity, c.reason))
                }
                _ => {}
            }
        }
        let changes = if folded {
            vec![NativeChange {
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
            protected_changes,
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

fn auth_path(config_path: &str) -> String {
    let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
        config_path,
        ".config/opencode",
    );
    let xdg = home.join(".local/share/opencode/auth.json");
    let config_dir = home.join(".config/opencode/auth.json");
    if xdg.exists() || !config_dir.exists() {
        xdg.display().to_string()
    } else {
        config_dir.display().to_string()
    }
}
