use crate::parser::{home_for_install, path_for_rel, resolve_config_path};
use crate::{ConfigFormat, DetectionSpec};
use chm_core::domain::mcp::{McpServer, McpTransport};
use chm_harness_sdk::adapter::plan::{PlanAction, ReconciliationPlan};
use chm_harness_sdk::adapter::protected::{
    ProtectedChangePlan, ProtectedOperation, ProtectedTarget,
};
use chm_harness_sdk::adapter::types::{AdapterError, NativeChange, NativePlan};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, value as toml_value};

pub(crate) fn plan(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    match spec.id {
        "kimi-cli" => plan_kimi(spec, reconciliation, install),
        "continue" => plan_continue(spec, reconciliation, install),
        "aider" => plan_aider(spec, reconciliation, install),
        "gemini-cli" => plan_gemini(spec, reconciliation, install),
        "qwen-code" => plan_qwen(spec, reconciliation, install),
        "cline" => plan_cline(spec, reconciliation, install),
        "goose" => plan_goose(spec, reconciliation, install),
        _ => plan_json_mcp(spec, reconciliation, install),
    }
}

fn plan_kimi(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let path = resolve_config_path(spec, install, &home);
    if matches!(
        format_for_path(&path, spec.format),
        ConfigFormat::Json | ConfigFormat::Jsonc
    ) {
        return plan_kimi_json(spec, reconciliation, install, &path);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| AdapterError::NotFound(e.to_string()))?;
    let mut doc = raw
        .parse::<DocumentMut>()
        .map_err(|e| AdapterError::Parse {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
    let mut warnings = Vec::new();
    let mut folded = false;
    let mut protected_changes = Vec::new();
    let mut protected_providers = std::collections::HashSet::new();
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let provider = add.native_provider_id.as_deref().or_else(|| {
                    add.payload
                        .get("native_provider_id")
                        .and_then(Value::as_str)
                });
                let Some(provider) = provider else {
                    warnings.push(format!(
                        "Kimi model {} skipped: a provider is required",
                        add.identity
                    ));
                    continue;
                };
                let providers = doc
                    .entry("providers")
                    .or_insert(Item::Table(Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid("Kimi providers must be a table".into())
                    })?;
                let provider_config = providers
                    .entry(provider)
                    .or_insert(Item::Table(Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid(format!("Kimi provider {provider} must be a table"))
                    })?;
                provider_config["type"] = toml_value(kimi_provider_type(add));
                if let Some(base_url) = add.payload.get("base_url").and_then(Value::as_str) {
                    provider_config["base_url"] = toml_value(base_url);
                }
                let mut model = Table::new();
                model["provider"] = toml_value(provider);
                let wire_model = add
                    .payload
                    .get("overrides")
                    .and_then(|v| v.get("wire_model"))
                    .and_then(Value::as_str)
                    .or_else(|| add.payload.get("remote_model_id").and_then(Value::as_str))
                    .unwrap_or(&add.identity);
                model["model"] = toml_value(wire_model);
                model["max_context_size"] = toml_value(
                    add.payload
                        .get("context_window")
                        .and_then(Value::as_i64)
                        .unwrap_or(1),
                );
                if let Some(v) = add.payload.get("max_input").and_then(Value::as_i64) {
                    model["max_input_size"] = toml_value(v);
                }
                if let Some(v) = add.payload.get("max_output").and_then(Value::as_i64) {
                    model["max_output_size"] = toml_value(v);
                }
                let display = add
                    .payload
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or(&add.identity);
                model["display_name"] = toml_value(display);
                let models = doc
                    .entry("models")
                    .or_insert(Item::Table(Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| AdapterError::Invalid("Kimi models must be a table".into()))?;
                models[&add.identity] = Item::Table(model);
                if let Some(credential_ref_id) = add
                    .payload
                    .get("credential_ref_id")
                    .and_then(Value::as_str)
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    && protected_providers.insert(provider.to_string())
                {
                    protected_changes.push(ProtectedChangePlan {
                        target: ProtectedTarget::KimiTomlFile {
                            path: path.display().to_string(),
                            provider_id: provider.to_string(),
                        },
                        credential_ref_id,
                        operation: ProtectedOperation::Upsert,
                    });
                }
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "model" => {
                let Some(models) = doc.get_mut("models").and_then(Item::as_table_mut) else {
                    warnings.push(format!("Kimi model {} not found", update.identity));
                    continue;
                };
                let Some(model) = models
                    .get_mut(&update.identity)
                    .and_then(Item::as_table_mut)
                else {
                    warnings.push(format!("Kimi model {} not found", update.identity));
                    continue;
                };
                set_toml_string(model, "display_name", update.desired.get("display_name"));
                set_toml_i64(
                    model,
                    "max_context_size",
                    update.desired.get("context_window"),
                );
                set_toml_i64(model, "max_input_size", update.desired.get("max_input"));
                set_toml_i64(model, "max_output_size", update.desired.get("max_output"));
                folded = true;
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                let Some(models) = doc.get_mut("models").and_then(Item::as_table_mut) else {
                    warnings.push(format!("Kimi model {} not found", remove.identity));
                    continue;
                };
                if models.remove(&remove.identity).is_some() {
                    folded = true;
                } else {
                    warnings.push(format!("Kimi model {} not found", remove.identity));
                }
            }
            // Kimi's MCP declarations are a separate mcp.json document (the
            // legacy TOML mcp_servers table is not a current write target).
            PlanAction::Add(add) if add.kind == "mcp" => {}
            PlanAction::Update(update) if update.kind == "mcp" => {}
            PlanAction::Remove(remove) if remove.kind == "mcp" => {}
            PlanAction::Unsupported(unsupported) => warnings.push(format!(
                "unsupported {} {}: {}",
                unsupported.kind, unsupported.identity, unsupported.reason
            )),
            PlanAction::Conflict(conflict) => warnings.push(format!(
                "conflict on {}: {}",
                conflict.identity, conflict.reason
            )),
            _ => {}
        }
    }
    let mut changes = if folded {
        vec![NativeChange {
            file_path: path.display().to_string(),
            before: Some(raw),
            after: Some(doc.to_string()),
        }]
    } else {
        vec![]
    };

    if reconciliation.actions.iter().any(is_mcp_action)
        && let Some(change) = plan_kimi_mcp(spec, reconciliation, install, &mut warnings)?
    {
        changes.push(change);
    }
    Ok(NativePlan {
        changes,
        links: vec![],
        protected_changes,
        warnings,
    })
}

fn kimi_provider_type(add: &chm_harness_sdk::adapter::plan::AddAction) -> &'static str {
    match add
        .payload
        .get("protocol")
        .and_then(Value::as_str)
        .or_else(|| {
            add.payload
                .get("overrides")
                .and_then(|value| value.get("protocol"))
                .and_then(Value::as_str)
        })
        .unwrap_or("openai-chat")
    {
        "anthropic-messages" => "anthropic",
        "openai-responses" => "openai_responses",
        _ => "openai",
    }
}

fn plan_kimi_json(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
    path: &Path,
) -> Result<NativePlan, AdapterError> {
    let raw = std::fs::read_to_string(path).map_err(|e| AdapterError::NotFound(e.to_string()))?;
    let mut doc = parse_json(&raw, path)?;
    let root = doc
        .as_object_mut()
        .ok_or_else(|| AdapterError::Invalid("Kimi config root must be an object".into()))?;
    let mut warnings = Vec::new();
    let mut folded = false;
    let mut protected_changes = Vec::new();
    let mut protected_providers = std::collections::HashSet::new();
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let provider = add.native_provider_id.as_deref().or_else(|| {
                    add.payload
                        .get("native_provider_id")
                        .and_then(Value::as_str)
                });
                let Some(provider) = provider else {
                    warnings.push(format!(
                        "Kimi model {} skipped: a provider is required",
                        add.identity
                    ));
                    continue;
                };
                let providers = root
                    .entry("providers")
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid("Kimi providers must be an object".into())
                    })?;
                let provider_config = providers
                    .entry(provider.to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid(format!("Kimi provider {provider} must be an object"))
                    })?;
                provider_config
                    .insert("type".into(), Value::String(kimi_provider_type(add).into()));
                if let Some(base_url) = add.payload.get("base_url").and_then(Value::as_str) {
                    provider_config.insert("base_url".into(), Value::String(base_url.into()));
                }
                let wire_model = add
                    .payload
                    .get("overrides")
                    .and_then(|value| value.get("wire_model"))
                    .and_then(Value::as_str)
                    .or_else(|| add.payload.get("remote_model_id").and_then(Value::as_str))
                    .unwrap_or(&add.identity);
                let display = add
                    .payload
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or(&add.identity);
                let mut model = Map::new();
                model.insert("provider".into(), Value::String(provider.into()));
                model.insert("model".into(), Value::String(wire_model.into()));
                model.insert(
                    "max_context_size".into(),
                    Value::Number(
                        add.payload
                            .get("context_window")
                            .and_then(Value::as_i64)
                            .unwrap_or(1)
                            .into(),
                    ),
                );
                model.insert("display_name".into(), Value::String(display.into()));
                if let Some(value) = add.payload.get("max_input").and_then(Value::as_i64) {
                    model.insert("max_input_size".into(), Value::Number(value.into()));
                }
                if let Some(value) = add.payload.get("max_output").and_then(Value::as_i64) {
                    model.insert("max_output_size".into(), Value::Number(value.into()));
                }
                root.entry("models")
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| AdapterError::Invalid("Kimi models must be an object".into()))?
                    .insert(add.identity.clone(), Value::Object(model));
                if let Some(credential_ref_id) = add
                    .payload
                    .get("credential_ref_id")
                    .and_then(Value::as_str)
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    && protected_providers.insert(provider.to_string())
                {
                    protected_changes.push(ProtectedChangePlan {
                        target: ProtectedTarget::KimiJsonFile {
                            path: path.display().to_string(),
                            provider_id: provider.to_string(),
                        },
                        credential_ref_id,
                        operation: ProtectedOperation::Upsert,
                    });
                }
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "model" => {
                let Some(model) = root
                    .get_mut("models")
                    .and_then(Value::as_object_mut)
                    .and_then(|models| models.get_mut(&update.identity))
                    .and_then(Value::as_object_mut)
                else {
                    warnings.push(format!("Kimi model {} not found", update.identity));
                    continue;
                };
                if let Some(value) = update.desired.get("display_name").and_then(Value::as_str) {
                    model.insert("display_name".into(), Value::String(value.into()));
                }
                for (field, key) in [
                    ("context_window", "max_context_size"),
                    ("max_input", "max_input_size"),
                    ("max_output", "max_output_size"),
                ] {
                    if let Some(desired) = update.desired.get(field) {
                        if let Some(value) = desired.as_i64() {
                            model.insert(key.into(), Value::Number(value.into()));
                        } else if desired.is_null() {
                            model.remove(key);
                        }
                    }
                }
                folded = true;
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                if let Some(models) = root.get_mut("models").and_then(Value::as_object_mut) {
                    folded |= models.remove(&remove.identity).is_some();
                }
            }
            PlanAction::Add(add) if add.kind == "mcp" => {}
            PlanAction::Update(update) if update.kind == "mcp" => {}
            PlanAction::Remove(remove) if remove.kind == "mcp" => {}
            _ => warnings_for_action(action, &mut warnings),
        }
    }
    let mut changes = if folded {
        vec![NativeChange {
            file_path: path.display().to_string(),
            before: Some(raw),
            after: Some(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".into())),
        }]
    } else {
        vec![]
    };
    if reconciliation.actions.iter().any(is_mcp_action)
        && let Some(change) = plan_kimi_mcp(spec, reconciliation, install, &mut warnings)?
    {
        changes.push(change);
    }
    Ok(NativePlan {
        changes,
        links: vec![],
        protected_changes,
        warnings,
    })
}

fn plan_kimi_mcp(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
    warnings: &mut Vec<String>,
) -> Result<Option<NativeChange>, AdapterError> {
    let home = home_for_install(spec, install);
    let config_path = resolve_config_path(spec, install, &home);
    let path = kimi_mcp_path(spec, &config_path, &home);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let mut doc = parse_json(&raw, &path)?;
    let mut folded = false;
    let mcp_plan = ReconciliationPlan {
        actions: reconciliation
            .actions
            .iter()
            .filter(|action| is_mcp_action(action))
            .cloned()
            .collect(),
    };
    append_json_mcp_actions(spec, &mcp_plan, install, warnings, &mut folded, &mut doc)?;
    Ok(folded.then(|| NativeChange {
        file_path: path.display().to_string(),
        before: Some(raw),
        after: Some(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".into())),
    }))
}

fn kimi_mcp_path(spec: &DetectionSpec, config: &Path, home: &Path) -> PathBuf {
    if config.file_name().and_then(|name| name.to_str()) == Some("mcp.json") {
        return config.to_path_buf();
    }
    // An explicitly supplied config file normally keeps mcp.json beside it.
    if let Some(sibling) = config.parent().map(|parent| parent.join("mcp.json"))
        && sibling.is_file()
    {
        return sibling;
    }
    for rel in spec.mcp_rels {
        let path = path_for_rel(spec, home, rel);
        if path.is_file() {
            return path;
        }
    }
    config
        .parent()
        .map(|parent| parent.join("mcp.json"))
        .unwrap_or_else(|| path_for_rel(spec, home, ".kimi/mcp.json"))
}

fn plan_qwen(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let path = resolve_config_path(spec, install, &home);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let mut doc = parse_json(&raw, &path)?;
    let mut warnings = Vec::new();
    let mut folded = false;
    let mut protected_changes = Vec::new();
    let mut protected_keys = std::collections::HashSet::new();

    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let provider = add
                    .native_provider_id
                    .as_deref()
                    .or_else(|| {
                        add.payload
                            .get("native_provider_id")
                            .and_then(Value::as_str)
                    })
                    .ok_or_else(|| {
                        AdapterError::Invalid(format!(
                            "Qwen model {} is missing its provider identity",
                            add.identity
                        ))
                    })?;
                let env_key = qwen_env_key(add, provider);
                let protocol = qwen_protocol_key(add);
                let models = doc
                    .as_object_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid("Qwen settings root must be an object".into())
                    })?
                    .entry("modelProviders")
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid("Qwen modelProviders must be an object".into())
                    })?;
                let entries = models
                    .entry(provider.to_string())
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid(format!(
                            "Qwen modelProviders.{provider} must be an array"
                        ))
                    })?;
                let mut model = Map::new();
                model.insert("id".into(), Value::String(add.identity.clone()));
                model.insert(
                    "name".into(),
                    Value::String(
                        add.payload
                            .get("display_name")
                            .and_then(Value::as_str)
                            .unwrap_or(&add.identity)
                            .into(),
                    ),
                );
                model.insert("envKey".into(), Value::String(env_key.clone()));
                if let Some(base_url) = add.payload.get("base_url").and_then(Value::as_str) {
                    model.insert("baseUrl".into(), Value::String(base_url.into()));
                }
                let mut generation = Map::new();
                if let Some(context) = add.payload.get("context_window").and_then(Value::as_i64) {
                    generation.insert("contextWindowSize".into(), Value::Number(context.into()));
                }
                if let Some(max_output) = add.payload.get("max_output").and_then(Value::as_i64) {
                    generation.insert(
                        "samplingParams".into(),
                        serde_json::json!({"max_tokens": max_output}),
                    );
                }
                if !generation.is_empty() {
                    model.insert("generationConfig".into(), Value::Object(generation));
                }
                if let Some(existing) = entries.iter_mut().find(|entry| {
                    entry.get("id").and_then(Value::as_str) == Some(add.identity.as_str())
                }) {
                    *existing = Value::Object(model);
                } else {
                    entries.push(Value::Object(model));
                }
                let protocols = doc
                    .as_object_mut()
                    .expect("Qwen settings root checked")
                    .entry("providerProtocol")
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid("Qwen providerProtocol must be an object".into())
                    })?;
                protocols.insert(provider.into(), Value::String(protocol));
                if let Some(credential_ref_id) = add
                    .payload
                    .get("credential_ref_id")
                    .and_then(Value::as_str)
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    && protected_keys.insert(env_key.clone())
                {
                    protected_changes.push(ProtectedChangePlan {
                        target: ProtectedTarget::EnvFile {
                            path: home.join(".qwen/.env").display().to_string(),
                            key: env_key,
                        },
                        credential_ref_id,
                        operation: ProtectedOperation::Upsert,
                    });
                }
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "model" => {
                let provider = update.native_provider_id.as_deref().or_else(|| {
                    update
                        .desired
                        .get("overrides")
                        .and_then(|value| value.get("native_provider_id"))
                        .and_then(Value::as_str)
                });
                let Some(provider) = provider else {
                    warnings.push(format!(
                        "Qwen model {} has no provider identity",
                        update.identity
                    ));
                    continue;
                };
                let Some(model) = doc
                    .get_mut("modelProviders")
                    .and_then(Value::as_object_mut)
                    .and_then(|providers| providers.get_mut(provider))
                    .and_then(Value::as_array_mut)
                    .and_then(|models| {
                        models.iter_mut().find(|model| {
                            model.get("id").and_then(Value::as_str)
                                == Some(update.identity.as_str())
                        })
                    })
                else {
                    warnings.push(format!("Qwen model {} not found", update.identity));
                    continue;
                };
                if let Some(object) = model.as_object_mut() {
                    if let Some(name) = update.desired.get("display_name").and_then(Value::as_str) {
                        object.insert("name".into(), Value::String(name.into()));
                    }
                    let generation = object
                        .entry("generationConfig")
                        .or_insert_with(|| Value::Object(Map::new()))
                        .as_object_mut()
                        .ok_or_else(|| {
                            AdapterError::Invalid("Qwen generationConfig must be an object".into())
                        })?;
                    if let Some(context) =
                        update.desired.get("context_window").and_then(Value::as_i64)
                    {
                        generation
                            .insert("contextWindowSize".into(), Value::Number(context.into()));
                    } else {
                        generation.remove("contextWindowSize");
                    }
                    if let Some(max_output) =
                        update.desired.get("max_output").and_then(Value::as_i64)
                    {
                        generation.insert(
                            "samplingParams".into(),
                            serde_json::json!({"max_tokens": max_output}),
                        );
                    } else {
                        generation.remove("samplingParams");
                    }
                }
                folded = true;
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                let mut removed = false;
                if let Some(providers) =
                    doc.get_mut("modelProviders").and_then(Value::as_object_mut)
                {
                    for models in providers.values_mut().filter_map(Value::as_array_mut) {
                        let before = models.len();
                        models.retain(|model| {
                            model.get("id").and_then(Value::as_str)
                                != Some(remove.identity.as_str())
                        });
                        removed |= before != models.len();
                    }
                }
                if !removed {
                    warnings.push(format!("Qwen model {} not found", remove.identity));
                } else {
                    folded = true;
                }
            }
            PlanAction::Unsupported(_) | PlanAction::Conflict(_) => {
                warnings_for_action(action, &mut warnings)
            }
            _ => {}
        }
    }
    // Qwen keeps MCP declarations in the same JSON settings document.  Keep
    // the model-provider handling above native to Qwen while reusing the
    // format-aware MCP writer for the existing mcpServers surface.
    append_json_mcp_actions(
        spec,
        reconciliation,
        install,
        &mut warnings,
        &mut folded,
        &mut doc,
    )?;
    Ok(json_plan_with_protected(
        path,
        raw,
        doc,
        folded,
        warnings,
        protected_changes,
    ))
}

fn qwen_protocol_key(add: &chm_harness_sdk::adapter::plan::AddAction) -> String {
    let protocol = add
        .payload
        .get("protocol")
        .and_then(Value::as_str)
        .or_else(|| {
            add.payload
                .get("overrides")
                .and_then(|value| value.get("protocol"))
                .and_then(Value::as_str)
        })
        .unwrap_or("openai-chat");
    match protocol {
        "anthropic-messages" => "anthropic",
        "openai-responses" | "openrouter-openai" | "openai-chat" => "openai",
        other => other,
    }
    .to_string()
}

fn qwen_env_key(add: &chm_harness_sdk::adapter::plan::AddAction, provider: &str) -> String {
    add.payload
        .get("api_key_env")
        .and_then(Value::as_str)
        .or_else(|| add.payload.get("env_key").and_then(Value::as_str))
        .filter(|key| !key.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
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
        })
}

fn json_plan_with_protected(
    path: PathBuf,
    raw: String,
    doc: Value,
    folded: bool,
    warnings: Vec<String>,
    protected_changes: Vec<ProtectedChangePlan>,
) -> NativePlan {
    let mut plan = json_plan(path, raw, doc, folded, warnings);
    plan.protected_changes = protected_changes;
    plan
}

fn plan_continue(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let path = resolve_config_path(spec, install, &home);
    let raw = std::fs::read_to_string(&path).map_err(|e| AdapterError::NotFound(e.to_string()))?;
    let format = format_for_path(&path, spec.format);
    if format == ConfigFormat::Json {
        return plan_continue_json(spec, reconciliation, install);
    }
    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&raw).map_err(|e| AdapterError::Parse {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
    let mut warnings = Vec::new();
    let mut folded = false;
    let mut protected_changes = Vec::new();
    let mut protected_keys = std::collections::HashSet::new();
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let model = yaml_model_from_add(add);
                yaml_array(&mut doc, "models").push(model);
                if let Some(credential_ref_id) = add_credential_ref_id(add)
                    && let Some(provider) = continue_native_provider_id(add)
                {
                    let key = continue_env_key(add, provider);
                    if protected_keys.insert(key.clone()) {
                        protected_changes.push(ProtectedChangePlan {
                            target: ProtectedTarget::EnvFile {
                                path: home.join(".continue/.env").display().to_string(),
                                key,
                            },
                            credential_ref_id,
                            operation: ProtectedOperation::Upsert,
                        });
                    }
                }
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "model" => {
                if let Some(model) = find_yaml_model(&mut doc, &update.identity) {
                    if let Some(name) = update.desired.get("display_name").and_then(Value::as_str) {
                        model["name"] = serde_yaml::Value::String(name.into());
                    }
                    if let Some(context) = update.desired.get("context_window") {
                        let options = model
                            .entry(serde_yaml::Value::String("defaultCompletionOptions".into()))
                            .or_insert_with(|| {
                                serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
                            })
                            .as_mapping_mut()
                            .expect("defaultCompletionOptions must be an object");
                        if let Some(context) = context.as_i64() {
                            options.insert(
                                serde_yaml::Value::String("contextLength".into()),
                                serde_yaml::Value::Number(context.into()),
                            );
                        } else {
                            options.remove(serde_yaml::Value::String("contextLength".into()));
                        }
                    }
                    if let Some(max_tokens) = update.desired.get("max_output") {
                        let options = model
                            .entry(serde_yaml::Value::String("defaultCompletionOptions".into()))
                            .or_insert_with(|| {
                                serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
                            })
                            .as_mapping_mut()
                            .expect("defaultCompletionOptions must be an object");
                        if let Some(max_tokens) = max_tokens.as_i64() {
                            options.insert(
                                serde_yaml::Value::String("maxTokens".into()),
                                serde_yaml::Value::Number(max_tokens.into()),
                            );
                        } else {
                            options.remove(serde_yaml::Value::String("maxTokens".into()));
                        }
                    }
                    folded = true;
                } else {
                    warnings.push(format!("Continue model {} not found", update.identity));
                }
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                let models = yaml_array(&mut doc, "models");
                let before = models.len();
                models.retain(|model| {
                    model
                        .get("name")
                        .and_then(serde_yaml::Value::as_str)
                        .or_else(|| model.get("model").and_then(serde_yaml::Value::as_str))
                        != Some(remove.identity.as_str())
                });
                if before != models.len() {
                    folded = true;
                } else {
                    warnings.push(format!("Continue model {} not found", remove.identity));
                }
            }
            PlanAction::Add(add) if add.kind == "mcp" => {
                yaml_array(&mut doc, "mcpServers").push(yaml_mcp_from_add(spec, add)?);
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "mcp" => {
                if let Some(current) = find_yaml_mcp(&mut doc, &update.identity) {
                    let desired: McpServer = serde_json::from_value(update.desired.clone())
                        .map_err(|e| AdapterError::Invalid(format!("MCP payload: {e}")))?;
                    let mut native = native_mcp(spec, &desired);
                    native
                        .as_object_mut()
                        .expect("native MCP is an object")
                        .insert("name".into(), Value::String(update.identity.clone()));
                    let native = serde_json_to_yaml(&native);
                    merge_yaml_native_mcp(current, &native);
                    folded = true;
                }
            }
            PlanAction::Remove(remove) if remove.kind == "mcp" => {
                let mcps = yaml_array(&mut doc, "mcpServers");
                let before = mcps.len();
                mcps.retain(|mcp| {
                    mcp.get("name")
                        .or_else(|| mcp.get("model"))
                        .and_then(serde_yaml::Value::as_str)
                        != Some(remove.identity.as_str())
                });
                folded |= before != mcps.len();
            }
            PlanAction::Unsupported(unsupported) => warnings.push(format!(
                "unsupported {} {}: {}",
                unsupported.kind, unsupported.identity, unsupported.reason
            )),
            PlanAction::Conflict(conflict) => warnings.push(format!(
                "conflict on {}: {}",
                conflict.identity, conflict.reason
            )),
            _ => {}
        }
    }
    Ok(NativePlan {
        changes: if folded {
            vec![NativeChange {
                file_path: path.display().to_string(),
                before: Some(raw),
                after: Some(
                    serde_yaml::to_string(&doc)
                        .map_err(|e| AdapterError::Invalid(e.to_string()))?,
                ),
            }]
        } else {
            vec![]
        },
        links: vec![],
        protected_changes,
        warnings,
    })
}

fn plan_continue_json(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let path = resolve_config_path(spec, install, &home);
    let raw = std::fs::read_to_string(&path).map_err(|e| AdapterError::NotFound(e.to_string()))?;
    let mut doc = parse_json(&raw, &path)?;
    let mut warnings = Vec::new();
    let mut folded = false;
    let mut protected_changes = Vec::new();
    let mut protected_keys = std::collections::HashSet::new();
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let yaml = yaml_model_from_add(add);
                let json =
                    serde_json::to_value(yaml).map_err(|e| AdapterError::Invalid(e.to_string()))?;
                json_array(&mut doc, "models").push(json);
                if let Some(credential_ref_id) = add_credential_ref_id(add)
                    && let Some(provider) = continue_native_provider_id(add)
                {
                    let key = continue_env_key(add, provider);
                    if protected_keys.insert(key.clone()) {
                        protected_changes.push(ProtectedChangePlan {
                            target: ProtectedTarget::EnvFile {
                                path: home.join(".continue/.env").display().to_string(),
                                key,
                            },
                            credential_ref_id,
                            operation: ProtectedOperation::Upsert,
                        });
                    }
                }
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "model" => {
                if let Some(model) = find_json_model(&mut doc, &update.identity) {
                    if let Some(name) = update.desired.get("display_name").and_then(Value::as_str) {
                        model.insert("name".into(), Value::String(name.into()));
                    }
                    if let Some(context) = update.desired.get("context_window") {
                        if let Some(context) = context.as_i64() {
                            model.insert("defaultCompletionOptions".into(), {
                                let mut options = model
                                    .get("defaultCompletionOptions")
                                    .and_then(Value::as_object)
                                    .cloned()
                                    .unwrap_or_default();
                                options
                                    .insert("contextLength".into(), Value::Number(context.into()));
                                Value::Object(options)
                            });
                        } else if let Some(options) = model
                            .get_mut("defaultCompletionOptions")
                            .and_then(Value::as_object_mut)
                        {
                            options.remove("contextLength");
                        }
                    }
                    if let Some(max_tokens) = update.desired.get("max_output") {
                        if let Some(max_tokens) = max_tokens.as_i64() {
                            model.insert("defaultCompletionOptions".into(), {
                                let mut options = model
                                    .get("defaultCompletionOptions")
                                    .and_then(Value::as_object)
                                    .cloned()
                                    .unwrap_or_default();
                                options
                                    .insert("maxTokens".into(), Value::Number(max_tokens.into()));
                                Value::Object(options)
                            });
                        } else if let Some(options) = model
                            .get_mut("defaultCompletionOptions")
                            .and_then(Value::as_object_mut)
                        {
                            options.remove("maxTokens");
                        }
                    }
                    folded = true;
                } else {
                    warnings.push(format!("Continue model {} not found", update.identity));
                }
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                let models = json_array(&mut doc, "models");
                let before = models.len();
                models.retain(|model| {
                    model
                        .get("name")
                        .or_else(|| model.get("model"))
                        .and_then(Value::as_str)
                        != Some(remove.identity.as_str())
                });
                folded |= before != models.len();
            }
            PlanAction::Add(add) if add.kind == "mcp" => {
                let server: McpServer = serde_json::from_value(add.payload.clone())
                    .map_err(|e| AdapterError::Invalid(format!("MCP payload: {e}")))?;
                let mut native = native_mcp(spec, &server);
                native
                    .as_object_mut()
                    .expect("native MCP is an object")
                    .insert("name".into(), Value::String(add.identity.clone()));
                json_array(&mut doc, "mcpServers").push(native);
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "mcp" => {
                let server: McpServer = serde_json::from_value(update.desired.clone())
                    .map_err(|e| AdapterError::Invalid(format!("MCP payload: {e}")))?;
                if let Some(current) = find_json_mcp(&mut doc, &update.identity) {
                    let mut native = native_mcp(spec, &server);
                    native
                        .as_object_mut()
                        .expect("native MCP is an object")
                        .insert("name".into(), Value::String(update.identity.clone()));
                    merge_native_mcp(current, native);
                    folded = true;
                }
            }
            PlanAction::Remove(remove) if remove.kind == "mcp" => {
                let mcps = json_array(&mut doc, "mcpServers");
                let before = mcps.len();
                mcps.retain(|mcp| {
                    mcp.get("name").and_then(Value::as_str) != Some(remove.identity.as_str())
                });
                folded |= before != mcps.len();
            }
            _ => warnings_for_action(action, &mut warnings),
        }
    }
    Ok(json_plan_with_protected(
        path,
        raw,
        doc,
        folded,
        warnings,
        protected_changes,
    ))
}

fn plan_aider(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let path = resolve_config_path(spec, install, &home);
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: serde_yaml::Value = if raw.trim().is_empty() {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    } else {
        serde_yaml::from_str(&raw).map_err(|error| AdapterError::Parse {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?
    };
    if !doc.is_mapping() {
        return Err(AdapterError::Invalid(
            "Aider config root must be a YAML mapping".into(),
        ));
    }

    let metadata_path = home.join(".aider.model.metadata.json");
    let metadata_raw = std::fs::read_to_string(&metadata_path).unwrap_or_else(|_| "{}".into());
    let mut metadata = parse_json(&metadata_raw, &metadata_path)?;
    if !metadata.is_object() {
        return Err(AdapterError::Invalid(
            "Aider model metadata must be a JSON object".into(),
        ));
    }

    let mut warnings = Vec::new();
    let mut folded = false;
    let mut metadata_folded = false;
    let mut protected_changes = Vec::new();
    let mut protected_keys = std::collections::HashSet::new();
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let provider = aider_provider_type(add);
                let wire = add
                    .payload
                    .get("overrides")
                    .and_then(|value| value.get("wire_model"))
                    .and_then(Value::as_str)
                    .or_else(|| add.payload.get("remote_model_id").and_then(Value::as_str))
                    .unwrap_or(&add.identity);
                let qualified = aider_qualified_model(provider, wire);
                aider_set_yaml(
                    &mut doc,
                    "model",
                    serde_yaml::Value::String(qualified.clone()),
                );
                if let Some(base_url) = add
                    .payload
                    .get("base_url")
                    .and_then(Value::as_str)
                    .or_else(|| add.payload.get("api_base").and_then(Value::as_str))
                {
                    match provider {
                        "anthropic" => aider_add_set_env(&mut doc, "ANTHROPIC_API_BASE", base_url),
                        _ => aider_set_yaml(
                            &mut doc,
                            "openai-api-base",
                            serde_yaml::Value::String(base_url.into()),
                        ),
                    }
                }
                if let Some(credential_ref_id) = add_credential_ref_id(add)
                    && let Some(native_provider) = continue_native_provider_id(add)
                {
                    let key = aider_env_key(provider, native_provider);
                    aider_set_yaml(
                        &mut doc,
                        "env-file",
                        serde_yaml::Value::String(home.join(".aider/.env").display().to_string()),
                    );
                    if protected_keys.insert(key.clone()) {
                        protected_changes.push(ProtectedChangePlan {
                            target: ProtectedTarget::EnvFile {
                                path: home.join(".aider/.env").display().to_string(),
                                key,
                            },
                            credential_ref_id,
                            operation: ProtectedOperation::Upsert,
                        });
                    }
                }
                aider_merge_metadata(&mut metadata, &qualified, add);
                metadata_folded = true;
                aider_set_yaml(
                    &mut doc,
                    "model-metadata-file",
                    serde_yaml::Value::String(metadata_path.display().to_string()),
                );
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "model" => {
                let current = doc
                    .get("model")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default();
                if current == update.identity
                    || current
                        .strip_prefix("openai/")
                        .or_else(|| current.strip_prefix("anthropic/"))
                        .or_else(|| current.strip_prefix("openrouter/"))
                        == Some(update.identity.as_str())
                {
                    if let Some(context) = update.desired.get("context_window")
                        && context.is_null()
                        && let Some(entry) = metadata
                            .as_object_mut()
                            .and_then(|entries| entries.get_mut(current))
                            .and_then(Value::as_object_mut)
                    {
                        entry.remove("max_input_tokens");
                    }
                    aider_merge_metadata_from_update(&mut metadata, current, update);
                    metadata_folded = true;
                } else {
                    warnings.push(format!(
                        "Aider model {} is not the active model; only the active global model can be updated",
                        update.identity
                    ));
                }
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                let current = doc
                    .get("model")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let current_wire = current
                    .strip_prefix("openai/")
                    .or_else(|| current.strip_prefix("anthropic/"))
                    .or_else(|| current.strip_prefix("openrouter/"))
                    .unwrap_or(current.as_str());
                if current == remove.identity || current_wire == remove.identity {
                    if let Some(mapping) = doc.as_mapping_mut() {
                        mapping.remove(serde_yaml::Value::String("model".into()));
                    }
                    if let Some(entries) = metadata.as_object_mut() {
                        entries.remove(&current);
                        entries.remove(remove.identity.as_str());
                    }
                    folded = true;
                    metadata_folded = true;
                } else {
                    warnings.push(format!(
                        "Aider model {} is not the active model; nothing removed",
                        remove.identity
                    ));
                }
            }
            _ => warnings_for_action(action, &mut warnings),
        }
    }

    let mut changes = Vec::new();
    if folded {
        changes.push(NativeChange {
            file_path: path.display().to_string(),
            before: Some(raw),
            after: Some(
                serde_yaml::to_string(&doc)
                    .map_err(|error| AdapterError::Invalid(error.to_string()))?,
            ),
        });
    }
    if metadata_folded {
        changes.push(NativeChange {
            file_path: metadata_path.display().to_string(),
            before: Some(metadata_raw),
            after: Some(
                serde_json::to_string_pretty(&metadata)
                    .map_err(|error| AdapterError::Invalid(error.to_string()))?,
            ),
        });
    }
    Ok(NativePlan {
        changes,
        links: vec![],
        protected_changes,
        warnings,
    })
}

fn aider_provider_type(add: &chm_harness_sdk::adapter::plan::AddAction) -> &'static str {
    match add_protocol(add) {
        "anthropic-messages" => "anthropic",
        "openrouter-openai" => "openrouter",
        _ => "openai",
    }
}

fn aider_qualified_model(provider: &str, wire: &str) -> String {
    let prefix = format!("{provider}/");
    if wire.starts_with(&prefix) {
        wire.to_string()
    } else {
        format!("{prefix}{wire}")
    }
}

fn aider_env_key(provider: &str, _native_provider: &str) -> String {
    match provider {
        "anthropic" => "ANTHROPIC_API_KEY".into(),
        "openrouter" => "OPENROUTER_API_KEY".into(),
        _ => "OPENAI_API_KEY".into(),
    }
}

fn aider_set_yaml(doc: &mut serde_yaml::Value, key: &str, value: serde_yaml::Value) {
    if let Some(mapping) = doc.as_mapping_mut() {
        mapping.insert(serde_yaml::Value::String(key.into()), value);
    }
}

fn aider_add_set_env(doc: &mut serde_yaml::Value, key: &str, value: &str) {
    let Some(mapping) = doc.as_mapping_mut() else {
        return;
    };
    let entry = mapping
        .entry(serde_yaml::Value::String("set-env".into()))
        .or_insert_with(|| serde_yaml::Value::Sequence(Vec::new()));
    let Some(values) = entry.as_sequence_mut() else {
        return;
    };
    let prefix = format!("{key}=");
    values.retain(|item| {
        !item
            .as_str()
            .is_some_and(|value| value.starts_with(&prefix))
    });
    values.push(serde_yaml::Value::String(format!("{key}={value}")));
}

fn aider_merge_metadata(
    metadata: &mut Value,
    qualified: &str,
    add: &chm_harness_sdk::adapter::plan::AddAction,
) {
    let Some(entries) = metadata.as_object_mut() else {
        return;
    };
    let mut entry = entries
        .get(qualified)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(context) = add.payload.get("context_window").and_then(Value::as_i64) {
        entry.insert("max_input_tokens".into(), Value::Number(context.into()));
    }
    if let Some(max_output) = add.payload.get("max_output").and_then(Value::as_i64) {
        entry.insert("max_output_tokens".into(), Value::Number(max_output.into()));
        entry.insert("max_tokens".into(), Value::Number(max_output.into()));
    }
    entry.insert(
        "litellm_provider".into(),
        Value::String(aider_provider_type(add).into()),
    );
    entries.insert(qualified.into(), Value::Object(entry));
}

fn aider_merge_metadata_from_update(
    metadata: &mut Value,
    qualified: &str,
    update: &chm_harness_sdk::adapter::plan::UpdateAction,
) {
    let Some(entries) = metadata.as_object_mut() else {
        return;
    };
    let mut entry = entries
        .get(qualified)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (field, metadata_key) in [
        ("context_window", "max_input_tokens"),
        ("max_output", "max_output_tokens"),
    ] {
        match update.desired.get(field).and_then(Value::as_i64) {
            Some(value) => {
                entry.insert(metadata_key.into(), Value::Number(value.into()));
                if field == "max_output" {
                    entry.insert("max_tokens".into(), Value::Number(value.into()));
                }
            }
            None if update.desired.get(field).is_some_and(Value::is_null) => {
                entry.remove(metadata_key);
                if field == "max_output" {
                    entry.remove("max_tokens");
                }
            }
            _ => {}
        }
    }
    entries.insert(qualified.into(), Value::Object(entry));
}

fn plan_gemini(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let path = resolve_config_path(spec, install, &home);
    let raw = std::fs::read_to_string(&path).map_err(|e| AdapterError::NotFound(e.to_string()))?;
    let mut doc = parse_json(&raw, &path)?;
    let mut warnings = Vec::new();
    let mut folded = false;
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let model = add
                    .payload
                    .get("overrides")
                    .and_then(|v| v.get("wire_model"))
                    .and_then(Value::as_str)
                    .or_else(|| add.payload.get("remote_model_id").and_then(Value::as_str))
                    .unwrap_or(&add.identity);
                doc.as_object_mut()
                    .expect("Gemini config root must be an object")
                    .entry("modelConfigs")
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| {
                        AdapterError::Invalid("Gemini modelConfigs must be an object".into())
                    })?
                    .insert(
                        add.identity.clone(),
                        serde_json::json!({"modelConfig": {"model": model}}),
                    );
                folded = true;
            }
            PlanAction::Update(update) if update.kind == "model" => {
                let Some(config) = doc
                    .get_mut("modelConfigs")
                    .and_then(Value::as_object_mut)
                    .and_then(|configs| configs.get_mut(&update.identity))
                    .and_then(Value::as_object_mut)
                else {
                    warnings.push(format!("Gemini model {} not found", update.identity));
                    continue;
                };
                if let Some(name) = update.desired.get("display_name").and_then(Value::as_str) {
                    config.insert("displayName".into(), Value::String(name.into()));
                }
                folded = true;
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                if let Some(configs) = doc.get_mut("modelConfigs").and_then(Value::as_object_mut) {
                    folded |= configs.remove(&remove.identity).is_some();
                }
            }
            _ => warnings_for_action(action, &mut warnings),
        }
    }
    append_json_mcp_actions(
        spec,
        reconciliation,
        install,
        &mut warnings,
        &mut folded,
        &mut doc,
    )?;
    Ok(json_plan(path, raw, doc, folded, warnings))
}

fn plan_json_mcp(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let main_path = resolve_config_path(spec, install, &home);
    let mcp_path = json_mcp_path(spec, &main_path, &home);
    let raw = std::fs::read_to_string(&mcp_path).unwrap_or_else(|_| "{}".into());
    let mut doc = parse_json(&raw, &mcp_path)?;
    let mut warnings = Vec::new();
    let mut folded = false;
    append_json_mcp_actions(
        spec,
        reconciliation,
        install,
        &mut warnings,
        &mut folded,
        &mut doc,
    )?;
    Ok(json_plan(mcp_path, raw, doc, folded, warnings))
}

fn append_json_mcp_actions(
    _spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    _install: &chm_core::domain::harness::HarnessInstallation,
    warnings: &mut Vec<String>,
    folded: &mut bool,
    doc: &mut Value,
) -> Result<(), AdapterError> {
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "mcp" => {
                let server: McpServer = serde_json::from_value(add.payload.clone())
                    .map_err(|e| AdapterError::Invalid(format!("MCP payload: {e}")))?;
                let object = mcp_object_for(_spec, doc);
                let native = native_mcp(_spec, &server);
                if let Some(current) = object.get_mut(&add.identity) {
                    merge_native_mcp(current, native);
                } else {
                    object.insert(add.identity.clone(), native);
                }
                *folded = true;
            }
            PlanAction::Update(update) if update.kind == "mcp" => {
                let server: McpServer = serde_json::from_value(update.desired.clone())
                    .map_err(|e| AdapterError::Invalid(format!("MCP payload: {e}")))?;
                let object = mcp_object_for(_spec, doc);
                let native = native_mcp(_spec, &server);
                if let Some(current) = object.get_mut(&update.identity) {
                    merge_native_mcp(current, native);
                } else {
                    object.insert(update.identity.clone(), native);
                }
                *folded = true;
            }
            PlanAction::Remove(remove) if remove.kind == "mcp" => {
                if mcp_object_for(_spec, doc)
                    .remove(&remove.identity)
                    .is_some()
                {
                    *folded = true;
                }
            }
            PlanAction::Unsupported(unsupported) => warnings.push(format!(
                "unsupported {} {}: {}",
                unsupported.kind, unsupported.identity, unsupported.reason
            )),
            PlanAction::Conflict(conflict) => warnings.push(format!(
                "conflict on {}: {}",
                conflict.identity, conflict.reason
            )),
            _ => {}
        }
    }
    Ok(())
}

fn plan_cline(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let mut result = plan_json_mcp(spec, reconciliation, install)?;
    // Cline's documented custom model catalog lives in models.json. Keep it
    // separate from providers.json so credentials and provider settings are
    // never rewritten by model sync.
    let home = home_for_install(spec, install);
    let path = path_for_rel(spec, &home, ".cline/data/settings/models.json");
    let model_actions: Vec<_> = reconciliation
        .actions
        .iter()
        .filter(|action| is_model_action(action))
        .collect();
    if model_actions.is_empty() {
        return Ok(result);
    }
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let mut doc = parse_json(&raw, &path)?;
    let mut folded = false;
    let mut warnings = Vec::new();
    for action in model_actions {
        match action {
            PlanAction::Add(add) => {
                let provider = add
                    .native_provider_id
                    .as_deref()
                    .or_else(|| {
                        add.payload
                            .get("native_provider_id")
                            .and_then(Value::as_str)
                    })
                    .ok_or_else(|| {
                        AdapterError::Invalid(format!(
                            "Cline model {} is missing its provider identity",
                            add.identity
                        ))
                    })?;
                match cline_model_collection(&mut doc, provider) {
                    ClineModelCollection::Array(models) => {
                        if models
                            .iter()
                            .any(|model| cline_model_value_id(model) == Some(add.identity.as_str()))
                        {
                            warnings.push(format!("Cline model {} already exists", add.identity));
                        } else {
                            models.push(cline_model_from_add(add, true));
                            folded = true;
                        }
                    }
                    ClineModelCollection::Map(models) => {
                        if models.contains_key(&add.identity) {
                            warnings.push(format!("Cline model {} already exists", add.identity));
                        } else {
                            models.insert(add.identity.clone(), cline_model_from_add(add, false));
                            folded = true;
                        }
                    }
                }
            }
            PlanAction::Update(update) => {
                let provider = update.native_provider_id.as_deref().ok_or_else(|| {
                    AdapterError::Invalid(format!(
                        "Cline model {} is missing its provider identity",
                        update.identity
                    ))
                })?;
                if let Some(model) = find_cline_model(&mut doc, provider, &update.identity) {
                    if let Some(name) = update.desired.get("display_name").and_then(Value::as_str) {
                        model["name"] = Value::String(name.into());
                    }
                    if let Some(context) = update.desired.get("context_window") {
                        if let Some(context) = context.as_i64() {
                            model["contextWindow"] = Value::Number(context.into());
                        } else if let Some(object) = model.as_object_mut() {
                            object.remove("contextWindow");
                        }
                    }
                    if let Some(max_tokens) = update.desired.get("max_output") {
                        if let Some(max_tokens) = max_tokens.as_i64() {
                            model["maxTokens"] = Value::Number(max_tokens.into());
                        } else if let Some(object) = model.as_object_mut() {
                            object.remove("maxTokens");
                        }
                    }
                    folded = true;
                } else {
                    warnings.push(format!("Cline model {} not found", update.identity));
                }
            }
            PlanAction::Remove(remove) => {
                let provider = remove.native_provider_id.as_deref().ok_or_else(|| {
                    AdapterError::Invalid(format!(
                        "Cline model {} is missing its provider identity",
                        remove.identity
                    ))
                })?;
                match cline_model_collection(&mut doc, provider) {
                    ClineModelCollection::Array(models) => {
                        let before = models.len();
                        models.retain(|model| {
                            cline_model_value_id(model) != Some(remove.identity.as_str())
                        });
                        folded |= before != models.len();
                    }
                    ClineModelCollection::Map(models) => {
                        folded |= models.remove(&remove.identity).is_some();
                    }
                }
            }
            _ => {}
        }
    }
    if folded {
        result.changes.push(NativeChange {
            file_path: path.display().to_string(),
            before: Some(raw),
            after: Some(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".into())),
        });
    }
    result.warnings.extend(warnings);
    Ok(result)
}

fn plan_goose(
    spec: &DetectionSpec,
    reconciliation: &ReconciliationPlan,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<NativePlan, AdapterError> {
    let home = home_for_install(spec, install);
    let config_path = resolve_config_path(spec, install, &home);
    let mut result = NativePlan::default();
    let mut documents: Vec<GooseDocument> = Vec::new();
    let mut secrets_keys = std::collections::HashSet::new();
    for action in &reconciliation.actions {
        if !is_model_action(action) {
            warnings_for_action(action, &mut result.warnings);
            continue;
        }

        let Some(path) = goose_model_path(action, spec, &home) else {
            result.warnings.push(format!(
                "Goose model {} is not backed by a writable custom provider JSON file",
                goose_action_identity(action)
            ));
            continue;
        };
        let index = match documents.iter().position(|document| document.path == path) {
            Some(index) => index,
            None => {
                let raw = match std::fs::read_to_string(&path) {
                    Ok(raw) => raw,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(error) => {
                        result.warnings.push(format!(
                            "cannot read Goose provider {}: {error}",
                            path.display()
                        ));
                        continue;
                    }
                };
                let doc = if raw.trim().is_empty() {
                    goose_new_provider_document(action)?
                } else {
                    parse_json(&raw, &path)?
                };
                documents.push(GooseDocument {
                    path,
                    raw,
                    doc,
                    folded: false,
                });
                documents.len() - 1
            }
        };
        apply_goose_model_action(action, &mut documents[index], &mut result.warnings);
        if let Some(credential_ref_id) = goose_credential_ref_id(action)
            && let Some(key) = goose_api_key_env(action)
            && secrets_keys.insert(key.clone())
        {
            result.protected_changes.push(ProtectedChangePlan {
                target: ProtectedTarget::GooseSecretsFile {
                    path: path_for_rel(spec, &home, ".config/goose/secrets.yaml")
                        .display()
                        .to_string(),
                    key,
                },
                credential_ref_id,
                operation: ProtectedOperation::Upsert,
            });
        }
    }

    for document in documents {
        if document.folded {
            result.changes.push(NativeChange {
                file_path: document.path.display().to_string(),
                before: Some(document.raw),
                after: Some(
                    serde_json::to_string_pretty(&document.doc)
                        .map_err(|error| AdapterError::Invalid(error.to_string()))?,
                ),
            });
        }
    }

    if !result.protected_changes.is_empty() {
        let raw = std::fs::read_to_string(&config_path).unwrap_or_default();
        let mut config: serde_yaml::Value = if raw.trim().is_empty() {
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
        } else {
            serde_yaml::from_str(&raw).map_err(|error| AdapterError::Parse {
                path: config_path.display().to_string(),
                detail: error.to_string(),
            })?
        };
        let root = config.as_mapping_mut().ok_or_else(|| {
            AdapterError::Invalid("Goose config root must be a YAML mapping".into())
        })?;
        let key = serde_yaml::Value::String("GOOSE_DISABLE_KEYRING".into());
        let already_disabled = root
            .get(&key)
            .and_then(serde_yaml::Value::as_bool)
            .unwrap_or(false)
            || root
                .get(&key)
                .and_then(serde_yaml::Value::as_str)
                .is_some_and(|value| matches!(value, "1" | "true" | "yes"));
        if !already_disabled {
            root.insert(key, serde_yaml::Value::Bool(true));
            result.changes.push(NativeChange {
                file_path: config_path.display().to_string(),
                before: Some(raw),
                after: Some(
                    serde_yaml::to_string(&config)
                        .map_err(|error| AdapterError::Invalid(error.to_string()))?,
                ),
            });
        }
    }
    Ok(result)
}

struct GooseDocument {
    path: PathBuf,
    raw: String,
    doc: Value,
    folded: bool,
}

fn goose_custom_dir(spec: &DetectionSpec, home: &Path) -> PathBuf {
    path_for_rel(spec, home, ".config/goose/custom_providers")
}

fn goose_action_identity(action: &PlanAction) -> &str {
    match action {
        PlanAction::Add(action) => &action.identity,
        PlanAction::Update(action) => &action.identity,
        PlanAction::Remove(action) => &action.identity,
        _ => "unknown",
    }
}

fn goose_action_provider(action: &PlanAction) -> Option<&str> {
    match action {
        PlanAction::Add(action) => action.native_provider_id.as_deref().or_else(|| {
            action
                .payload
                .get("native_provider_id")
                .and_then(Value::as_str)
        }),
        PlanAction::Update(action) => action.native_provider_id.as_deref().or_else(|| {
            action
                .desired
                .get("overrides")
                .and_then(|value| value.get("native_provider_id"))
                .and_then(Value::as_str)
        }),
        PlanAction::Remove(action) => action.native_provider_id.as_deref(),
        _ => None,
    }
}

fn goose_action_config_file(action: &PlanAction) -> Option<&str> {
    match action {
        PlanAction::Add(action) => action
            .payload
            .get("overrides")
            .and_then(|value| value.get("config_file"))
            .and_then(Value::as_str),
        PlanAction::Update(action) => action
            .desired
            .get("overrides")
            .and_then(|value| value.get("config_file"))
            .and_then(Value::as_str)
            .or_else(|| {
                action
                    .current
                    .get("overrides")
                    .and_then(|value| value.get("config_file"))
                    .and_then(Value::as_str)
            }),
        PlanAction::Remove(_) => None,
        _ => None,
    }
}

fn goose_model_path(action: &PlanAction, spec: &DetectionSpec, home: &Path) -> Option<PathBuf> {
    let custom_dir = goose_custom_dir(spec, home);
    if let Some(config_file) = goose_action_config_file(action) {
        let candidate = PathBuf::from(config_file);
        if candidate.parent() == Some(custom_dir.as_path())
            && candidate
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("json")
        {
            return Some(candidate);
        }
    }
    let provider = goose_action_provider(action)?.trim();
    if provider.is_empty() || matches!(provider, "openai" | "anthropic" | "google" | "ollama") {
        return None;
    }
    let candidate = custom_dir.join(format!("{}.json", goose_slug(provider)));
    Some(candidate)
}

fn goose_action_payload(action: &PlanAction) -> Option<&Value> {
    match action {
        PlanAction::Add(action) => Some(&action.payload),
        _ => None,
    }
}

fn goose_native_provider_config(action: &PlanAction) -> Option<&Value> {
    goose_action_payload(action)
        .and_then(|payload| payload.get("overrides"))
        .and_then(|overrides| overrides.get("native_provider_config"))
}

fn goose_credential_ref_id(action: &PlanAction) -> Option<uuid::Uuid> {
    goose_action_payload(action)
        .and_then(|payload| {
            payload.get("credential_ref_id").or_else(|| {
                goose_native_provider_config(action)
                    .and_then(|config| config.get("credential_ref_id"))
            })
        })
        .and_then(Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
}

fn goose_api_key_env(action: &PlanAction) -> Option<String> {
    let payload = goose_action_payload(action)?;
    payload
        .get("api_key_env")
        .or_else(|| payload.get("env_key"))
        .or_else(|| {
            goose_native_provider_config(action).and_then(|config| config.get("api_key_env"))
        })
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|key| !key.trim().is_empty())
        .or_else(|| {
            goose_action_provider(action)
                .filter(|provider| !provider.trim().is_empty())
                .map(|provider| {
                    format!("CHM_{}_API_KEY", goose_slug(provider).to_ascii_uppercase())
                })
        })
}

fn goose_engine(action: &PlanAction) -> &'static str {
    let protocol = goose_action_payload(action)
        .and_then(|payload| payload.get("protocol"))
        .and_then(Value::as_str)
        .or_else(|| {
            goose_native_provider_config(action)
                .and_then(|config| config.get("protocol"))
                .and_then(Value::as_str)
        })
        .unwrap_or("openai-chat");
    match protocol {
        "anthropic-messages" => "anthropic",
        "custom" => "openai",
        _ => "openai",
    }
}

fn goose_new_provider_document(action: &PlanAction) -> Result<Value, AdapterError> {
    let provider = goose_action_provider(action).ok_or_else(|| {
        AdapterError::Invalid(format!(
            "Goose model {} is missing its provider identity",
            goose_action_identity(action)
        ))
    })?;
    let payload = goose_action_payload(action);
    let display = payload
        .and_then(|payload| payload.get("display_name"))
        .and_then(Value::as_str)
        .unwrap_or(provider);
    let base_url = payload
        .and_then(|payload| payload.get("base_url"))
        .and_then(Value::as_str)
        .or_else(|| {
            goose_native_provider_config(action)
                .and_then(|config| config.get("base_url"))
                .and_then(Value::as_str)
        });
    let mut doc = Map::new();
    doc.insert("name".into(), Value::String(provider.into()));
    doc.insert("engine".into(), Value::String(goose_engine(action).into()));
    doc.insert("display_name".into(), Value::String(display.into()));
    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        doc.insert("base_url".into(), Value::String(base_url.into()));
    }
    if let Some(env_key) = goose_api_key_env(action) {
        doc.insert("api_key_env".into(), Value::String(env_key));
        doc.insert(
            "requires_auth".into(),
            Value::Bool(goose_credential_ref_id(action).is_some()),
        );
    } else {
        doc.insert("requires_auth".into(), Value::Bool(false));
    }
    doc.insert("supports_streaming".into(), Value::Bool(true));
    doc.insert("models".into(), Value::Array(Vec::new()));
    Ok(Value::Object(doc))
}

fn goose_slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            slug.push(character.to_ascii_lowercase());
        } else {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "custom_provider".into()
    } else {
        slug.into()
    }
}

fn goose_models_mut(doc: &mut Value) -> Option<&mut Vec<Value>> {
    let object = doc.as_object_mut()?;
    let models = object
        .entry("models")
        .or_insert_with(|| Value::Array(Vec::new()));
    models.as_array_mut()
}

fn goose_model_id(value: &Value) -> Option<&str> {
    value
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| value.get("model").and_then(Value::as_str))
}

fn apply_goose_model_action(
    action: &PlanAction,
    document: &mut GooseDocument,
    warnings: &mut Vec<String>,
) {
    let Some(models) = goose_models_mut(&mut document.doc) else {
        warnings.push(format!(
            "Goose provider {} has no models array",
            document.path.display()
        ));
        return;
    };
    match action {
        PlanAction::Add(add) => {
            if models
                .iter()
                .any(|model| goose_model_id(model) == Some(add.identity.as_str()))
            {
                warnings.push(format!("Goose model {} already exists", add.identity));
                return;
            }
            let mut model = Map::new();
            model.insert("name".into(), Value::String(add.identity.clone()));
            if let Some(context) = add.payload.get("context_window").and_then(Value::as_i64) {
                model.insert("context_limit".into(), Value::Number(context.into()));
            }
            if let Some(display) = add.payload.get("display_name").and_then(Value::as_str)
                && display != add.identity
            {
                model.insert("alias".into(), Value::String(display.into()));
            }
            models.push(Value::Object(model));
            document.folded = true;
        }
        PlanAction::Update(update) => {
            let Some(model) = models
                .iter_mut()
                .find(|model| goose_model_id(model) == Some(update.identity.as_str()))
            else {
                warnings.push(format!("Goose model {} not found", update.identity));
                return;
            };
            let Some(object) = model.as_object_mut() else {
                warnings.push(format!("Goose model {} is not an object", update.identity));
                return;
            };
            if let Some(display) = update.desired.get("display_name").and_then(Value::as_str) {
                if display == update.identity {
                    object.remove("alias");
                } else {
                    object.insert("alias".into(), Value::String(display.into()));
                }
            }
            match update.desired.get("context_window").and_then(Value::as_i64) {
                Some(context) => {
                    object.insert("context_limit".into(), Value::Number(context.into()));
                }
                None => {
                    object.remove("context_limit");
                }
            }
            document.folded = true;
        }
        PlanAction::Remove(remove) => {
            let before = models.len();
            models.retain(|model| goose_model_id(model) != Some(remove.identity.as_str()));
            if before == models.len() {
                warnings.push(format!("Goose model {} not found", remove.identity));
            } else {
                document.folded = true;
            }
        }
        _ => {}
    }
}

fn warnings_for_action(action: &PlanAction, warnings: &mut Vec<String>) {
    match action {
        PlanAction::Unsupported(unsupported) => warnings.push(format!(
            "unsupported {} {}: {}",
            unsupported.kind, unsupported.identity, unsupported.reason
        )),
        PlanAction::Conflict(conflict) => warnings.push(format!(
            "conflict on {}: {}",
            conflict.identity, conflict.reason
        )),
        PlanAction::Add(add) if add.kind != "model" && add.kind != "mcp" => warnings.push(format!(
            "{} action for {} is not supported by this adapter",
            add.kind, add.identity
        )),
        PlanAction::Update(update) if update.kind != "model" && update.kind != "mcp" => warnings
            .push(format!(
                "{} action for {} is not supported by this adapter",
                update.kind, update.identity
            )),
        PlanAction::Remove(remove) if remove.kind != "model" && remove.kind != "mcp" => warnings
            .push(format!(
                "{} action for {} is not supported by this adapter",
                remove.kind, remove.identity
            )),
        _ => {}
    }
}

fn is_model_action(action: &PlanAction) -> bool {
    match action {
        PlanAction::Add(action) => action.kind == "model",
        PlanAction::Update(action) => action.kind == "model",
        PlanAction::Remove(action) => action.kind == "model",
        _ => false,
    }
}

fn is_mcp_action(action: &PlanAction) -> bool {
    match action {
        PlanAction::Add(action) => action.kind == "mcp",
        PlanAction::Update(action) => action.kind == "mcp",
        PlanAction::Remove(action) => action.kind == "mcp",
        _ => false,
    }
}

fn json_mcp_path(spec: &DetectionSpec, main: &Path, home: &Path) -> PathBuf {
    if matches!(
        main.file_name().and_then(|name| name.to_str()),
        Some("mcp.json" | "mcp_settings.json" | "cline_mcp_settings.json")
    ) {
        return main.to_path_buf();
    }
    for rel in spec.mcp_rels {
        let path = path_for_rel(spec, home, rel);
        if path.is_file() {
            return path;
        }
    }
    match spec.id {
        "cursor" => path_for_rel(spec, home, ".cursor/mcp.json"),
        "cline" => path_for_rel(spec, home, ".cline/data/settings/cline_mcp_settings.json"),
        "roo-code" => path_for_rel(spec, home, ".roo/mcp.json"),
        _ => main.to_path_buf(),
    }
}

fn parse_json(raw: &str, path: &Path) -> Result<Value, AdapterError> {
    if path.extension().and_then(|e| e.to_str()) == Some("jsonc") {
        return serde_json::from_reader(json_comments::StripComments::new(raw.as_bytes())).map_err(
            |e| AdapterError::Parse {
                path: path.display().to_string(),
                detail: e.to_string(),
            },
        );
    }
    serde_json::from_str(raw).map_err(|e| AdapterError::Parse {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

fn json_plan(
    path: PathBuf,
    raw: String,
    doc: Value,
    folded: bool,
    warnings: Vec<String>,
) -> NativePlan {
    NativePlan {
        changes: if folded {
            vec![NativeChange {
                file_path: path.display().to_string(),
                before: Some(raw),
                after: Some(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".into())),
            }]
        } else {
            vec![]
        },
        links: vec![],
        protected_changes: vec![],
        warnings,
    }
}

fn mcp_object_for<'a>(spec: &DetectionSpec, doc: &'a mut Value) -> &'a mut Map<String, Value> {
    let root = doc
        .as_object_mut()
        .expect("JSON config root must be an object");
    let key = if spec.id == "amp" {
        "amp.mcpServers"
    } else {
        "mcpServers"
    };
    root.entry(key)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("mcpServers must be an object")
}

/// Update only the fields CHM owns while retaining harness-specific MCP
/// options (timeouts, allowlists, startup flags, and future fields).
fn merge_native_mcp(existing: &mut Value, native: Value) {
    let Some(native_object) = native.as_object() else {
        *existing = native;
        return;
    };
    let Some(existing_object) = existing.as_object_mut() else {
        *existing = native;
        return;
    };
    for key in [
        "command",
        "args",
        "env",
        "url",
        "httpUrl",
        "type",
        "transport",
        "transportType",
        "headers",
        "disabled",
        "enabled",
    ] {
        existing_object.remove(key);
    }
    existing_object.extend(
        native_object
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
}

fn json_array<'a>(doc: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    doc.as_object_mut()
        .expect("JSON config root must be an object")
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("JSON collection must be an array")
}

fn find_json_model<'a>(doc: &'a mut Value, identity: &str) -> Option<&'a mut Map<String, Value>> {
    json_array(doc, "models").iter_mut().find_map(|model| {
        let hit = model
            .get("name")
            .or_else(|| model.get("model"))
            .and_then(Value::as_str)
            == Some(identity);
        hit.then(|| model.as_object_mut()).flatten()
    })
}

fn find_json_mcp<'a>(doc: &'a mut Value, identity: &str) -> Option<&'a mut Value> {
    json_array(doc, "mcpServers")
        .iter_mut()
        .find(|server| server.get("name").and_then(Value::as_str) == Some(identity))
}

fn native_mcp(spec: &DetectionSpec, server: &McpServer) -> Value {
    let mut entry = Map::new();
    match server.transport {
        McpTransport::Stdio => {
            if let Some(command) = &server.command {
                entry.insert("command".into(), Value::String(command.clone()));
            }
            if !server.args.is_empty() {
                entry.insert(
                    "args".into(),
                    Value::Array(server.args.iter().cloned().map(Value::String).collect()),
                );
            }
            let env = server
                .env
                .iter()
                .filter(|(key, _)| *key != "headers" && *key != "_direct_tools")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<_, _>>();
            if !env.is_empty() {
                entry.insert("env".into(), Value::Object(env));
            }
        }
        McpTransport::Sse => {
            if let Some(url) = &server.url {
                entry.insert("url".into(), Value::String(url.clone()));
            }
            if matches!(spec.id, "roo-code" | "continue" | "cline") {
                entry.insert("type".into(), Value::String("sse".into()));
            } else if spec.id == "kimi-cli" {
                entry.insert("transport".into(), Value::String("sse".into()));
            }
        }
        McpTransport::Http => {
            if let Some(url) = &server.url {
                if matches!(spec.id, "gemini-cli" | "qwen-code") {
                    entry.insert("httpUrl".into(), Value::String(url.clone()));
                } else {
                    entry.insert("url".into(), Value::String(url.clone()));
                }
            }
            if matches!(spec.id, "roo-code" | "continue") {
                entry.insert("type".into(), Value::String("streamable-http".into()));
            } else if spec.id == "cline" {
                entry.insert("type".into(), Value::String("streamableHttp".into()));
            }
        }
        McpTransport::Unknown => {}
    }
    if !server.enabled {
        if matches!(spec.id, "roo-code" | "cline") {
            entry.insert("disabled".into(), Value::Bool(true));
        } else if spec.id == "kimi-cli" {
            entry.insert("enabled".into(), Value::Bool(false));
        }
    }
    if let Some(headers) = server.env.get("headers") {
        entry.insert("headers".into(), headers.clone());
    }
    Value::Object(entry)
}

fn add_credential_ref_id(add: &chm_harness_sdk::adapter::plan::AddAction) -> Option<uuid::Uuid> {
    add.payload
        .get("credential_ref_id")
        .or_else(|| {
            add.payload
                .get("overrides")
                .and_then(|value| value.get("native_provider_config"))
                .and_then(|value| value.get("credential_ref_id"))
        })
        .and_then(Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
}

fn add_protocol(add: &chm_harness_sdk::adapter::plan::AddAction) -> &str {
    add.payload
        .get("protocol")
        .and_then(Value::as_str)
        .or_else(|| {
            add.payload
                .get("overrides")
                .and_then(|value| value.get("protocol"))
                .and_then(Value::as_str)
        })
        .unwrap_or("openai-chat")
}

fn continue_provider_type(add: &chm_harness_sdk::adapter::plan::AddAction) -> &'static str {
    match add_protocol(add) {
        "anthropic-messages" => "anthropic",
        "openrouter-openai" => "openrouter",
        // Continue uses the OpenAI provider for both chat-completions and
        // Responses-compatible OpenAI gateways, including custom endpoints.
        _ => "openai",
    }
}

fn continue_native_provider_id(add: &chm_harness_sdk::adapter::plan::AddAction) -> Option<&str> {
    add.native_provider_id.as_deref().or_else(|| {
        add.payload
            .get("native_provider_id")
            .and_then(Value::as_str)
    })
}

fn generated_env_key(provider: &str) -> String {
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

fn continue_env_key(add: &chm_harness_sdk::adapter::plan::AddAction, provider: &str) -> String {
    add.payload
        .get("api_key_env")
        .and_then(Value::as_str)
        .or_else(|| add.payload.get("env_key").and_then(Value::as_str))
        .filter(|key| !key.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| generated_env_key(provider))
}

fn yaml_model_from_add(add: &chm_harness_sdk::adapter::plan::AddAction) -> serde_yaml::Value {
    let mut obj = Map::new();
    obj.insert("name".into(), Value::String(add.identity.clone()));
    obj.insert(
        "provider".into(),
        Value::String(continue_provider_type(add).into()),
    );
    let wire = add
        .payload
        .get("overrides")
        .and_then(|v| v.get("wire_model"))
        .and_then(Value::as_str)
        .or_else(|| add.payload.get("remote_model_id").and_then(Value::as_str))
        .unwrap_or(&add.identity);
    obj.insert("model".into(), Value::String(wire.into()));
    if let Some(api_base) = add
        .payload
        .get("base_url")
        .and_then(Value::as_str)
        .or_else(|| add.payload.get("api_base").and_then(Value::as_str))
    {
        obj.insert("apiBase".into(), Value::String(api_base.into()));
    }
    let mut options = Map::new();
    if let Some(context) = add.payload.get("context_window").and_then(Value::as_i64) {
        options.insert("contextLength".into(), Value::Number(context.into()));
    }
    if let Some(max_tokens) = add.payload.get("max_output").and_then(Value::as_i64) {
        options.insert("maxTokens".into(), Value::Number(max_tokens.into()));
    }
    if !options.is_empty() {
        obj.insert("defaultCompletionOptions".into(), Value::Object(options));
    }
    if add_credential_ref_id(add).is_some()
        && let Some(provider) = continue_native_provider_id(add)
    {
        let key = continue_env_key(add, provider);
        obj.insert(
            "apiKey".into(),
            Value::String(format!("${{{{ secrets.{key} }}}}")),
        );
    }
    serde_json_to_yaml(&Value::Object(obj))
}

fn yaml_mcp_from_add(
    spec: &DetectionSpec,
    add: &chm_harness_sdk::adapter::plan::AddAction,
) -> Result<serde_yaml::Value, AdapterError> {
    let server: McpServer = serde_json::from_value(add.payload.clone())
        .map_err(|e| AdapterError::Invalid(format!("MCP payload: {e}")))?;
    let mut native = native_mcp(spec, &server);
    native
        .as_object_mut()
        .expect("native MCP is an object")
        .insert("name".into(), Value::String(add.identity.clone()));
    Ok(serde_json_to_yaml(&native))
}

fn serde_json_to_yaml(value: &Value) -> serde_yaml::Value {
    serde_yaml::to_value(value).unwrap_or(serde_yaml::Value::Null)
}

fn yaml_array<'a>(doc: &'a mut serde_yaml::Value, key: &str) -> &'a mut Vec<serde_yaml::Value> {
    let map = doc
        .as_mapping_mut()
        .expect("YAML config root must be a mapping");
    let value = map
        .entry(serde_yaml::Value::String(key.into()))
        .or_insert_with(|| serde_yaml::Value::Sequence(Vec::new()));
    value
        .as_sequence_mut()
        .expect("YAML collection must be a sequence")
}

fn find_yaml_model<'a>(
    doc: &'a mut serde_yaml::Value,
    identity: &str,
) -> Option<&'a mut serde_yaml::Mapping> {
    yaml_array(doc, "models").iter_mut().find_map(|model| {
        let hit = model
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .or_else(|| model.get("model").and_then(serde_yaml::Value::as_str))
            == Some(identity);
        hit.then(|| model.as_mapping_mut()).flatten()
    })
}

fn find_yaml_mcp<'a>(
    doc: &'a mut serde_yaml::Value,
    identity: &str,
) -> Option<&'a mut serde_yaml::Value> {
    yaml_array(doc, "mcpServers")
        .iter_mut()
        .find(|server| server.get("name").and_then(serde_yaml::Value::as_str) == Some(identity))
}

/// Update only the fields CHM owns in a Continue YAML MCP entry, retaining
/// extension-specific fields that may be added by newer Continue releases.
fn merge_yaml_native_mcp(existing: &mut serde_yaml::Value, native: &serde_yaml::Value) {
    let (Some(existing), Some(native)) = (existing.as_mapping_mut(), native.as_mapping()) else {
        *existing = native.clone();
        return;
    };
    for key in [
        "name",
        "command",
        "args",
        "env",
        "url",
        "httpUrl",
        "type",
        "transport",
        "transportType",
        "headers",
        "disabled",
        "enabled",
    ] {
        existing.remove(serde_yaml::Value::String(key.into()));
    }
    for (key, value) in native {
        existing.insert(key.clone(), value.clone());
    }
}

enum ClineModelCollection<'a> {
    Array(&'a mut Vec<Value>),
    Map(&'a mut Map<String, Value>),
}

fn cline_model_collection<'a>(doc: &'a mut Value, provider: &str) -> ClineModelCollection<'a> {
    let root = doc
        .as_object_mut()
        .expect("Cline models root must be an object");
    // Current models.json wraps provider entries under `providers`; older
    // files used provider ids directly at the root. Preserve whichever shape
    // is already present and use the current wrapper for a new document.
    let providers = if root.get("providers").is_some_and(Value::is_object) {
        root.get_mut("providers")
            .and_then(Value::as_object_mut)
            .expect("providers checked")
    } else if root.is_empty() {
        root.entry("providers")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("providers initialized as an object")
    } else {
        root
    };
    let entry = providers
        .entry(provider.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    match entry {
        Value::Array(models) => ClineModelCollection::Array(models),
        Value::Object(object) => {
            let models = object
                .entry("models")
                .or_insert_with(|| Value::Object(Map::new()));
            match models {
                Value::Array(models) => ClineModelCollection::Array(models),
                Value::Object(models) => ClineModelCollection::Map(models),
                other => {
                    *other = Value::Object(Map::new());
                    let Value::Object(models) = other else {
                        unreachable!("Cline models entry was just replaced with an object")
                    };
                    ClineModelCollection::Map(models)
                }
            }
        }
        other => {
            *other = Value::Object(Map::new());
            let Value::Object(object) = other else {
                unreachable!("Cline provider entry was just replaced with an object")
            };
            let models = object
                .entry("models")
                .or_insert_with(|| Value::Object(Map::new()));
            let Value::Object(models) = models else {
                unreachable!("Cline models entry was initialized as an object")
            };
            ClineModelCollection::Map(models)
        }
    }
}

fn find_cline_model<'a>(
    doc: &'a mut Value,
    provider: &str,
    identity: &str,
) -> Option<&'a mut Value> {
    match cline_model_collection(doc, provider) {
        ClineModelCollection::Array(models) => models
            .iter_mut()
            .find(|model| cline_model_value_id(model) == Some(identity)),
        ClineModelCollection::Map(models) => models.get_mut(identity),
    }
}

fn cline_model_value_id(value: &Value) -> Option<&str> {
    value
        .get("id")
        .or_else(|| value.get("name"))
        .or_else(|| value.get("model"))
        .and_then(Value::as_str)
}

fn cline_model_from_add(
    add: &chm_harness_sdk::adapter::plan::AddAction,
    include_id: bool,
) -> Value {
    let mut model = Map::new();
    if include_id {
        model.insert("id".into(), Value::String(add.identity.clone()));
    }
    model.insert(
        "name".into(),
        Value::String(
            add.payload
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(&add.identity)
                .into(),
        ),
    );
    if let Some(context) = add.payload.get("context_window").and_then(Value::as_i64) {
        model.insert("contextWindow".into(), Value::Number(context.into()));
    }
    if let Some(max_tokens) = add.payload.get("max_output").and_then(Value::as_i64) {
        model.insert("maxTokens".into(), Value::Number(max_tokens.into()));
    }
    Value::Object(model)
}

fn set_toml_string(table: &mut toml_edit::Table, key: &str, desired: Option<&Value>) {
    if let Some(value) = desired.and_then(Value::as_str) {
        table[key] = toml_value(value);
    }
}

fn set_toml_i64(table: &mut toml_edit::Table, key: &str, desired: Option<&Value>) {
    if let Some(desired) = desired {
        if let Some(value) = desired.as_i64() {
            table[key] = toml_value(value);
        } else if desired.is_null() {
            table.remove(key);
        }
    }
}

fn format_for_path(path: &Path, fallback: ConfigFormat) -> ConfigFormat {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => ConfigFormat::Json,
        Some("jsonc") => ConfigFormat::Jsonc,
        Some("toml") => ConfigFormat::Toml,
        Some("yml") | Some("yaml") => ConfigFormat::Yaml,
        _ => fallback,
    }
}
