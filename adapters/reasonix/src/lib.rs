//! Reasonix adapter for native model/provider configuration and credentials.

pub mod parser;
pub mod writer;

use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::provider::Protocol;
use chm_harness_sdk::adapter::parse_version_supported;
use chm_harness_sdk::adapter::plan::PlanAction;
use chm_harness_sdk::adapter::protected::{
    ProtectedChangePlan, ProtectedOperation, ProtectedTarget,
};
use chm_harness_sdk::adapter::route::{
    CredentialTarget, ModelIdentityRules, ModelMetadataCapabilities, ProviderTopology,
    RouteDeploymentCapabilities,
};
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
            .with_route_deployment(RouteDeploymentCapabilities {
                provider_topology: ProviderTopology::Multiple,
                protocols: vec![
                    Protocol::OpenAiChatCompletions,
                    Protocol::OpenAiResponses,
                    Protocol::AnthropicMessages,
                    Protocol::OpenRouterOpenAi,
                ],
                credential_targets: vec![CredentialTarget::HarnessEnvFile],
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
                protected_changes: vec![],
                warnings: vec![format!(
                    "Reasonix {:?} untested — read-only mode",
                    install.version
                )],
            });
        }
        let mut warnings = vec![];
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
            install.config_path.as_deref().unwrap_or(""),
            ".reasonix",
        );
        let config_path = home.join(".reasonix/config.toml").display().to_string();
        let raw = std::fs::read_to_string(&config_path).unwrap_or_default();
        let mut doc: toml_edit::DocumentMut =
            raw.parse::<toml_edit::DocumentMut>()
                .map_err(|error| AdapterError::Parse {
                    path: config_path.clone(),
                    detail: error.to_string(),
                })?;
        let mut folded = false;
        let mut protected_changes = Vec::new();
        let mut protected_keys = std::collections::HashSet::new();
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
                                "Reasonix model {} is missing its provider identity",
                                a.identity
                            ))
                        })?;
                    let kind = reasonix_provider_kind(a);
                    let model_id = a
                        .payload
                        .get("remote_model_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "Reasonix model {} is missing its remote model id",
                                a.identity
                            ))
                        })?;
                    let base_url = a
                        .payload
                        .get("base_url")
                        .and_then(|v| v.as_str())
                        .or_else(|| a.payload.get("api_base").and_then(|v| v.as_str()))
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "Reasonix provider {provider_id} is missing its base URL",
                            ))
                        })?;
                    let env_key = reasonix_env_key(a, provider_id);
                    writer::fold_provider(
                        &mut doc,
                        provider_id,
                        kind,
                        base_url,
                        model_id,
                        Some(&env_key),
                    );
                    if let Some(credential_ref_id) = credential_ref_id(a)
                        && protected_keys.insert(env_key.clone())
                    {
                        protected_changes.push(ProtectedChangePlan {
                            target: ProtectedTarget::EnvFile {
                                path: home.join(".reasonix/.env").display().to_string(),
                                key: env_key.clone(),
                            },
                            credential_ref_id,
                            operation: ProtectedOperation::Upsert,
                        });
                    }
                    folded = true;
                }
                PlanAction::Update(u) if u.kind == "model" => {
                    let provider_id = u.native_provider_id.as_deref().ok_or_else(|| {
                        AdapterError::Invalid(format!(
                            "Reasonix model {} is missing its provider identity",
                            u.identity
                        ))
                    })?;
                    let found = writer::update_model(
                        &mut doc,
                        provider_id,
                        &u.identity,
                        u.desired.get("context_window").and_then(|v| v.as_i64()),
                        u.desired.get("max_output").and_then(|v| v.as_i64()),
                    );
                    if found {
                        folded = true;
                    } else {
                        warnings.push(format!("Reasonix model {} not found", u.identity));
                    }
                }
                PlanAction::Remove(r) if r.kind == "model" => {
                    let provider_id = r.native_provider_id.as_deref().ok_or_else(|| {
                        AdapterError::Invalid(format!(
                            "Reasonix model {} is missing its provider identity",
                            r.identity
                        ))
                    })?;
                    if writer::remove_model(&mut doc, provider_id, &r.identity) {
                        folded = true;
                    } else {
                        warnings.push(format!("Reasonix model {} not found", r.identity));
                    }
                }
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => {
                    warnings.push(format!("conflict on {}: {}", c.identity, c.reason))
                }
                PlanAction::Add(a) => warnings.push(format!(
                    "{} action for {} not supported by reasonix writer yet",
                    a.kind, a.identity
                )),
                _ => {}
            }
        }
        let changes = if folded {
            vec![chm_harness_sdk::adapter::types::NativeChange {
                file_path: config_path,
                before: Some(raw),
                after: Some(doc.to_string()),
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

fn credential_ref_id(action: &chm_harness_sdk::adapter::plan::AddAction) -> Option<uuid::Uuid> {
    action
        .payload
        .get("credential_ref_id")
        .or_else(|| {
            action
                .payload
                .get("overrides")
                .and_then(|value| value.get("native_provider_config"))
                .and_then(|value| value.get("credential_ref_id"))
        })
        .and_then(|value| value.as_str())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
}

fn reasonix_provider_kind(action: &chm_harness_sdk::adapter::plan::AddAction) -> &'static str {
    match action
        .payload
        .get("protocol")
        .and_then(|value| value.as_str())
        .or_else(|| {
            action
                .payload
                .get("overrides")
                .and_then(|value| value.get("protocol"))
                .and_then(|value| value.as_str())
        })
        .unwrap_or("openai-chat")
    {
        "anthropic-messages" => "anthropic",
        _ => "openai",
    }
}

fn reasonix_env_key(action: &chm_harness_sdk::adapter::plan::AddAction, provider: &str) -> String {
    if let Some(key) = action
        .payload
        .get("api_key_env")
        .and_then(|value| value.as_str())
        .or_else(|| {
            action
                .payload
                .get("env_key")
                .and_then(|value| value.as_str())
        })
        .filter(|value| !value.trim().is_empty())
    {
        return key.to_string();
    }
    let mut key = String::from("CHM_");
    for character in provider.chars() {
        if character.is_ascii_alphanumeric() {
            key.push(character.to_ascii_uppercase());
        } else {
            key.push('_');
        }
    }
    key.push_str("_API_KEY");
    key
}
