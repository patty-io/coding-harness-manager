use crate::parser::{home_for_install, path_for_rel, resolve_config_path};
use crate::{ConfigFormat, DetectionSpec};
use chm_core::domain::mcp::{McpServer, McpTransport};
use chm_harness_sdk::adapter::plan::{PlanAction, ReconciliationPlan};
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
        "gemini-cli" => plan_gemini(spec, reconciliation, install),
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
                if doc
                    .get("providers")
                    .and_then(Item::as_table)
                    .and_then(|providers| providers.get(provider))
                    .is_none()
                {
                    warnings.push(format!(
                        "Kimi model {} skipped: provider {provider} is not defined",
                        add.identity
                    ));
                    continue;
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
        warnings,
    })
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
                    .as_object()
                    .ok_or_else(|| {
                        AdapterError::Invalid("Kimi providers must be an object".into())
                    })?;
                if !providers.contains_key(provider) {
                    warnings.push(format!(
                        "Kimi model {} skipped: provider {provider} is not defined",
                        add.identity
                    ));
                    continue;
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
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let model = yaml_model_from_add(add);
                yaml_array(&mut doc, "models").push(model);
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
    for action in &reconciliation.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let yaml = yaml_model_from_add(add);
                let json =
                    serde_json::to_value(yaml).map_err(|e| AdapterError::Invalid(e.to_string()))?;
                json_array(&mut doc, "models").push(json);
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
    Ok(json_plan(path, raw, doc, folded, warnings))
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
                    .unwrap_or("custom");
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
                let provider = update.native_provider_id.as_deref().unwrap_or("custom");
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
                let provider = remove.native_provider_id.as_deref().unwrap_or("custom");
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
    let mut result = NativePlan::default();
    let mut documents: Vec<GooseDocument> = Vec::new();
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
                    Err(error) => {
                        result.warnings.push(format!(
                            "cannot read Goose provider {}: {error}",
                            path.display()
                        ));
                        continue;
                    }
                };
                let doc = parse_json(&raw, &path)?;
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
    candidate.is_file().then_some(candidate)
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
    doc.as_object_mut()?.get_mut("models")?.as_array_mut()
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

fn yaml_model_from_add(add: &chm_harness_sdk::adapter::plan::AddAction) -> serde_yaml::Value {
    let mut obj = Map::new();
    obj.insert("name".into(), Value::String(add.identity.clone()));
    obj.insert(
        "provider".into(),
        Value::String(
            add.native_provider_id
                .clone()
                .or_else(|| {
                    add.payload
                        .get("native_provider_id")
                        .and_then(Value::as_str)
                        .map(String::from)
                })
                .unwrap_or_else(|| "openai".into()),
        ),
    );
    let wire = add
        .payload
        .get("overrides")
        .and_then(|v| v.get("wire_model"))
        .and_then(Value::as_str)
        .or_else(|| add.payload.get("remote_model_id").and_then(Value::as_str))
        .unwrap_or(&add.identity);
    obj.insert("model".into(), Value::String(wire.into()));
    if let Some(api_base) = add.payload.get("api_base").and_then(Value::as_str) {
        obj.insert("apiBase".into(), Value::String(api_base.into()));
    }
    if let Some(context) = add.payload.get("context_window").and_then(Value::as_i64) {
        obj.insert(
            "defaultCompletionOptions".into(),
            serde_json::json!({"contextLength": context}),
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
