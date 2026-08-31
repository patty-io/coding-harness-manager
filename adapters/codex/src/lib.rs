//! Codex adapter for documented provider profile files and token helpers.

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

pub struct CodexAdapter;

impl HarnessAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::none()
            .with_route_deployment(RouteDeploymentCapabilities {
                provider_topology: ProviderTopology::Multiple,
                // Current Codex config reference documents `responses` as
                // the only valid custom-provider wire API. Do not generate a
                // `wire_api = "chat"` file that Codex will reject.
                protocols: vec![Protocol::OpenAiResponses],
                credential_targets: vec![CredentialTarget::CommandHelper],
                model_identity: ModelIdentityRules {
                    case_sensitive: true,
                    allow_namespaced_ids: true,
                },
                // Codex exposes a model context-window override, but no
                // portable per-model max-input/max-output fields.
                metadata: ModelMetadataCapabilities {
                    context_window: true,
                    max_input: false,
                    max_output: false,
                },
            })
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
        let raw = std::fs::read_to_string(config_path)?;
        let home =
            chm_harness_sdk::adapter::helpers::install_home_from_config(config_path, ".codex");
        parser::parse_main_config(&raw, &home)
    }

    fn plan(
        &self,
        plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
        install: &HarnessInstallation,
    ) -> Result<NativePlan, AdapterError> {
        if !parse_version_supported(install.version.as_deref(), &["0.150", "0.149"]) {
            return Ok(NativePlan {
                changes: vec![],
                links: vec![],
                protected_changes: vec![],
                warnings: vec![format!(
                    "Codex {:?} untested — read-only mode",
                    install.version
                )],
            });
        }
        let mut warnings = vec![];
        let home = chm_harness_sdk::adapter::helpers::install_home_from_config(
            install.config_path.as_deref().unwrap_or(""),
            ".codex",
        );
        // Codex profile files are the native unit of model selection. A profile
        // has one top-level `model`, so a provider with several library models
        // needs one profile file per model instead of repeatedly overwriting
        // `<provider>.config.toml`.
        let mut by_file: std::collections::BTreeMap<String, (String, toml_edit::DocumentMut)> =
            std::collections::BTreeMap::new();
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
                                "Codex model {} is missing its provider identity",
                                a.identity
                            ))
                        })?;
                    let model_id = a
                        .payload
                        .get("remote_model_id")
                        .and_then(|v| v.as_str())
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "Codex model {} is missing its remote model id",
                                a.identity
                            ))
                        })?;
                    let wire_api = wire_api(a)?;
                    let base_url = a
                        .payload
                        .get("base_url")
                        .and_then(|v| v.as_str())
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "Codex provider {provider_id} is missing its base URL"
                            ))
                        })?;
                    let credential_kind = a
                        .payload
                        .get("credential_kind")
                        .or_else(|| {
                            a.payload
                                .get("overrides")
                                .and_then(|value| value.get("native_provider_config"))
                                .and_then(|value| value.get("credential_kind"))
                        })
                        .and_then(|value| value.as_str());
                    let env_key = a
                        .payload
                        .get("env_key")
                        .or_else(|| a.payload.get("api_key_env"))
                        .and_then(|value| value.as_str())
                        .filter(|_| credential_kind == Some("env"));
                    let credential_ref_id = credential_ref_id(a);
                    let file = select_add_file(&home, provider_id, model_id, &by_file);
                    let file = file.display().to_string();
                    let (_raw, doc) = by_file.entry(file.clone()).or_insert_with(|| {
                        let raw = std::fs::read_to_string(&file).unwrap_or_default();
                        let doc: toml_edit::DocumentMut = raw.parse().unwrap_or_default();
                        (raw, doc)
                    });
                    writer::fold_provider_with_metadata(
                        doc,
                        provider_id,
                        model_id,
                        base_url,
                        env_key,
                        wire_api,
                        writer::ProviderMetadata {
                            context_window: a
                                .payload
                                .get("context_window")
                                .and_then(|v| v.as_i64()),
                            max_output: a.payload.get("max_output").and_then(|v| v.as_i64()),
                            credential_ref_id: (env_key.is_none())
                                .then_some(credential_ref_id)
                                .flatten(),
                        },
                    );
                }
                PlanAction::Update(u) if u.kind == "model" => {
                    let provider_id = u
                        .native_provider_id
                        .as_deref()
                        .or_else(|| {
                            u.desired
                                .get("overrides")
                                .and_then(|value| value.get("native_provider_id"))
                                .and_then(|value| value.as_str())
                        })
                        .ok_or_else(|| {
                            AdapterError::Invalid(format!(
                                "Codex model {} is missing its provider identity",
                                u.identity
                            ))
                        })?;
                    let files = matching_provider_files(&home, provider_id, &u.identity, &by_file);
                    let mut updated = false;
                    for path in files {
                        let file = path.display().to_string();
                        let (_raw, doc) =
                            by_file.entry(file).or_insert_with(|| load_document(&path));
                        updated |= writer::update_provider(
                            doc,
                            provider_id,
                            &u.identity,
                            u.desired.get("context_window").and_then(|v| v.as_i64()),
                        );
                    }
                    if !updated {
                        warnings.push(format!(
                            "update skipped: model {} not found in any Codex profile for provider {}",
                            u.identity, provider_id
                        ));
                    }
                }
                PlanAction::Remove(r) if r.kind == "model" => {
                    let Some(provider_id) = r.native_provider_id.as_deref() else {
                        warnings.push(format!(
                            "remove skipped: model {} has no Codex provider identity",
                            r.identity
                        ));
                        continue;
                    };
                    let files = matching_provider_files(&home, provider_id, &r.identity, &by_file);
                    let mut removed = false;
                    for path in files {
                        let file = path.display().to_string();
                        let (_raw, doc) =
                            by_file.entry(file).or_insert_with(|| load_document(&path));
                        removed |= writer::remove_provider(doc, Some(provider_id), &r.identity);
                    }
                    if !removed {
                        warnings.push(format!(
                            "remove skipped: model {} not found in any Codex profile for provider {}",
                            r.identity, provider_id
                        ));
                    }
                }
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => {
                    warnings.push(format!("conflict on {}: {}", c.identity, c.reason))
                }
                PlanAction::Add(a) => warnings.push(format!(
                    "{} action for {} not supported by codex writer yet",
                    a.kind, a.identity
                )),
                PlanAction::Update(u) => warnings.push(format!(
                    "{} update for {} is not supported by Codex writer yet",
                    u.kind, u.identity
                )),
                PlanAction::Remove(r) => warnings.push(format!(
                    "{} removal for {} is not supported by Codex writer yet",
                    r.kind, r.identity
                )),
                _ => {}
            }
        }
        let changes: Vec<chm_harness_sdk::adapter::types::NativeChange> = by_file
            .into_iter()
            .filter_map(|(file, (raw, doc))| {
                let after = doc.to_string();
                (raw != after).then_some(chm_harness_sdk::adapter::types::NativeChange {
                    file_path: file,
                    before: Some(raw),
                    after: Some(after),
                })
            })
            .collect();
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

    fn validate(&self, _install: &HarnessInstallation) -> Result<ValidationReport, AdapterError> {
        // the sync flow passes the plan; adapter-level validate checks the main config
        Ok(writer::validate_config(&home_config(
            _install,
            "config.toml",
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
        ".codex",
    );
    home.join(".codex").join(name).display().to_string()
}

fn codex_dir(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".codex")
}

fn load_document(path: &std::path::Path) -> (String, toml_edit::DocumentMut) {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let doc = raw.parse().unwrap_or_default();
    (raw, doc)
}

fn file_component(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    output.truncate(80);
    let output = output.trim_matches('-').to_string();
    if output.is_empty() {
        "provider".into()
    } else {
        output
    }
}

fn profile_has_selection(
    path: &std::path::Path,
    by_file: &std::collections::BTreeMap<String, (String, toml_edit::DocumentMut)>,
) -> bool {
    if let Some((_, document)) = by_file.get(&path.display().to_string()) {
        return document.get("model").is_some();
    }
    if !path.is_file() {
        return false;
    }
    load_document(path).1.get("model").is_some()
}

fn select_add_file(
    home: &std::path::Path,
    provider_id: &str,
    model_id: &str,
    by_file: &std::collections::BTreeMap<String, (String, toml_edit::DocumentMut)>,
) -> std::path::PathBuf {
    let directory = codex_dir(home);
    let base = directory.join(format!("{}.config.toml", file_component(provider_id)));
    if !profile_has_selection(&base, by_file) {
        return base;
    }

    let stem = format!(
        "{}-{}",
        file_component(provider_id),
        file_component(model_id)
    );
    let mut candidate = directory.join(format!("{stem}.config.toml"));
    let mut suffix = 2;
    while profile_has_selection(&candidate, by_file) {
        candidate = directory.join(format!("{stem}-{suffix}.config.toml"));
        suffix += 1;
    }
    candidate
}

fn matching_provider_files(
    home: &std::path::Path,
    provider_id: &str,
    model_id: &str,
    by_file: &std::collections::BTreeMap<String, (String, toml_edit::DocumentMut)>,
) -> Vec<std::path::PathBuf> {
    let mut paths = std::collections::BTreeSet::new();
    for (file, (_, document)) in by_file {
        if writer::contains_provider_model(document, provider_id, model_id) {
            paths.insert(std::path::PathBuf::from(file));
        }
    }
    let directory = codex_dir(home);
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("toml")
                || !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".config.toml"))
            {
                continue;
            }
            if let Some((_, document)) = by_file.get(&path.display().to_string()) {
                if writer::contains_provider_model(document, provider_id, model_id) {
                    paths.insert(path);
                }
                continue;
            }
            let (_, document) = load_document(&path);
            if writer::contains_provider_model(&document, provider_id, model_id) {
                paths.insert(path);
            }
        }
    }
    if paths.is_empty() {
        paths.insert(directory.join(format!("{}.config.toml", file_component(provider_id))));
    }
    paths.into_iter().collect()
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

fn wire_api(
    action: &chm_harness_sdk::adapter::plan::AddAction,
) -> Result<&'static str, AdapterError> {
    if let Some(value) = action.payload.get("wire_api").and_then(|v| v.as_str()) {
        return match value {
            "responses" => Ok("responses"),
            "chat" => Err(AdapterError::Invalid(
                "Codex custom providers currently support only the Responses API (wire_api = \"responses\")".into(),
            )),
            other => Err(AdapterError::Invalid(format!(
                "Codex does not support wire API {other}"
            ))),
        };
    }
    match action.payload.get("protocol").and_then(|v| v.as_str()) {
        Some("openai-responses") => Ok("responses"),
        Some("openai-chat") => Err(AdapterError::Invalid(
            "Codex custom providers currently support only the Responses API".into(),
        )),
        Some(other) => Err(AdapterError::Invalid(format!(
            "Codex does not support protocol {other}"
        ))),
        None => Ok("responses"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn second_model_from_one_provider_gets_its_own_profile_file() {
        let home = std::env::temp_dir().join(format!("chm-codex-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(home.join(".codex")).unwrap();
        let base = home.join(".codex/proxy.config.toml");
        fs::write(
            &base,
            "model = \"proxy/first\"\nmodel_provider = \"proxy\"\n",
        )
        .unwrap();

        let files = std::collections::BTreeMap::new();
        let selected = select_add_file(&home, "proxy", "second/model", &files);
        assert_eq!(selected, home.join(".codex/proxy-second-model.config.toml"));

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn matching_profiles_are_found_by_provider_and_model_not_filename() {
        let home = std::env::temp_dir().join(format!("chm-codex-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(
            home.join(".codex/custom-profile.config.toml"),
            "model = \"proxy/second\"\nmodel_provider = \"proxy\"\n",
        )
        .unwrap();

        let files = std::collections::BTreeMap::new();
        let matches = matching_provider_files(&home, "proxy", "second", &files);
        assert_eq!(
            matches,
            vec![home.join(".codex/custom-profile.config.toml")]
        );

        fs::remove_dir_all(home).unwrap();
    }
}
