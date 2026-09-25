//! Pi writer: folds model adds into ONE cumulative models.json change.

use chm_harness_sdk::adapter::types::ValidationReport;
use serde_json::{Map, Value};

const PROVIDER_CONFIGURATION_KEYS: [&str; 4] = ["baseUrl", "headers", "compat", "modelOverrides"];

fn provider_has_configuration(provider: &Value) -> bool {
    provider
        .get("models")
        .and_then(Value::as_array)
        .is_some_and(|models| !models.is_empty())
        || PROVIDER_CONFIGURATION_KEYS
            .iter()
            .any(|key| provider.get(*key).is_some_and(|value| !value.is_null()))
}

/// Parses models.json into a document (providers always present).
pub fn parse_document(raw: &str) -> Result<Value, String> {
    let mut doc: Value =
        serde_json::from_str(raw).map_err(|e| format!("models.json is not valid JSON: {e}"))?;
    let root = doc
        .as_object_mut()
        .ok_or_else(|| "models.json must be an object".to_string())?;
    let providers = root
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "models.json providers must be an object".to_string())?;
    for (name, pv) in providers.iter_mut() {
        let obj = pv
            .as_object_mut()
            .ok_or_else(|| format!("provider {name} must be an object"))?;
        let models = obj.entry("models").or_insert_with(|| Value::Array(vec![]));
        if !models.is_array() {
            return Err(format!("provider {name} models must be an array"));
        }
    }
    Ok(doc)
}

pub fn fold_model(
    doc: &mut Value,
    provider_id: &str,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
) {
    let providers = doc
        .as_object_mut()
        .expect("document must be an object")
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("providers must be an object");
    let pv = providers
        .entry(provider_id)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("provider entry must be an object");
    let models = pv
        .entry("models")
        .or_insert_with(|| Value::Array(vec![]))
        .as_array_mut()
        .expect("models must be an array");
    if !models
        .iter()
        .any(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id))
    {
        let mut entry = Map::new();
        entry.insert("id".into(), Value::String(model_id.into()));
        entry.insert("name".into(), Value::String(display_name.into()));
        if let Some(ctx) = context_window {
            entry.insert("contextWindow".into(), Value::Number(ctx.into()));
        }
        models.push(Value::Object(entry));
    }
}

/// Maps a CHM endpoint protocol to the `api` implementation name Pi requires
/// on every provider (or model) entry.
pub fn api_for_protocol(protocol: &str) -> Option<&'static str> {
    match protocol {
        "openai-chat" => Some("openai-completions"),
        "openai-responses" => Some("openai-responses"),
        "anthropic-messages" => Some("anthropic-messages"),
        "openrouter-openai" => Some("openai-completions"),
        _ => None,
    }
}

/// Ensure a provider entry declares the `api` implementation name Pi expects.
/// CHM only fills it when the entry does not declare one yet, so a
/// hand-configured `api` always wins.
pub fn configure_provider_api(doc: &mut Value, provider_id: &str, api: &str) {
    let Some(provider) = doc
        .as_object_mut()
        .and_then(|root| root.get_mut("providers"))
        .and_then(Value::as_object_mut)
        .and_then(|providers| providers.get_mut(provider_id))
        .and_then(|provider| provider.as_object_mut())
    else {
        return;
    };
    provider
        .entry("api")
        .or_insert_with(|| Value::String(api.to_string()));
}

/// Ensure a provider entry declares the `baseUrl` Pi requires when defining
/// custom models. CHM only fills it when the entry does not declare one yet,
/// so a hand-configured `baseUrl` always wins.
pub fn configure_provider_base_url(doc: &mut Value, provider_id: &str, base_url: &str) {
    if base_url.trim().is_empty() {
        return;
    }
    let Some(provider) = doc
        .as_object_mut()
        .and_then(|root| root.get_mut("providers"))
        .and_then(Value::as_object_mut)
        .and_then(|providers| providers.get_mut(provider_id))
        .and_then(|provider| provider.as_object_mut())
    else {
        return;
    };
    provider
        .entry("baseUrl")
        .or_insert_with(|| Value::String(base_url.to_string()));
}

/// Canonical pi thinking levels, in pi's presentation order.
pub const THINKING_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// Write pi's model-level thinking controls.
///
/// `reasoning` marks the model as thinking-capable. `thinking_levels` lists
/// the levels pi should offer for it; because pi treats *omitted* levels as
/// "use the provider default through `high`" and *`null`* as "unsupported,
/// hide it", a declared list becomes a complete `thinkingLevelMap` where
/// listed levels map to their own provider value and every other canonical
/// level is explicitly nulled. An empty/absent list leaves any existing map
/// alone so pi's defaults keep working.
pub fn set_model_thinking(
    doc: &mut Value,
    provider_id: Option<&str>,
    model_id: &str,
    reasoning: Option<bool>,
    thinking_levels: Option<&[String]>,
) -> bool {
    let Some(providers) = doc
        .as_object_mut()
        .and_then(|o| o.get_mut("providers"))
        .and_then(|p| p.as_object_mut())
    else {
        return false;
    };
    let declared_levels = thinking_levels.filter(|levels| !levels.is_empty());
    let enabled = reasoning.unwrap_or(false) || declared_levels.is_some();
    let mut found = false;
    for (pname, pv) in providers.iter_mut() {
        if provider_id.is_some_and(|wanted| !wanted.eq_ignore_ascii_case(pname)) {
            continue;
        }
        let Some(models) = pv.get_mut("models").and_then(|m| m.as_array_mut()) else {
            continue;
        };
        for m in models.iter_mut() {
            if m.get("id").and_then(|v| v.as_str()) != Some(model_id) {
                continue;
            }
            let Some(obj) = m.as_object_mut() else {
                continue;
            };
            found = true;
            if enabled {
                obj.insert("reasoning".into(), Value::Bool(true));
                if let Some(levels) = declared_levels {
                    let mut map = Map::new();
                    for level in THINKING_LEVELS {
                        let value = if levels.iter().any(|declared| declared == level) {
                            Value::String(level.to_string())
                        } else {
                            Value::Null
                        };
                        map.insert(level.to_string(), value);
                    }
                    obj.insert("thinkingLevelMap".into(), Value::Object(map));
                }
            } else if reasoning == Some(false) {
                obj.insert("reasoning".into(), Value::Bool(false));
                obj.remove("thinkingLevelMap");
            }
            // `None` with no levels: nothing declared, so pi's existing or
            // built-in thinking configuration is left untouched.
        }
    }
    found
}

/// Set the native Pi limits on one model entry while preserving all other
/// provider/model metadata. Pi calls the output limit `maxTokens`.
pub fn set_model_limits(
    doc: &mut Value,
    provider_id: &str,
    model_id: &str,
    context_window: Option<i64>,
    max_output: Option<i64>,
) -> bool {
    let Some(provider) = doc
        .as_object_mut()
        .and_then(|root| root.get_mut("providers"))
        .and_then(Value::as_object_mut)
        .and_then(|providers| providers.get_mut(provider_id))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let Some(models) = provider.get_mut("models").and_then(Value::as_array_mut) else {
        return false;
    };
    let Some(model) = models
        .iter_mut()
        .find(|model| model.get("id").and_then(Value::as_str) == Some(model_id))
    else {
        return false;
    };
    let Some(model) = model.as_object_mut() else {
        return false;
    };
    if let Some(context) = context_window {
        model.insert("contextWindow".into(), Value::Number(context.into()));
    } else {
        model.remove("contextWindow");
    }
    if let Some(output) = max_output {
        model.insert("maxTokens".into(), Value::Number(output.into()));
    } else {
        model.remove("maxTokens");
    }
    true
}

/// Write Pi's `input` array on one model entry.
///
/// Pi's schema only admits `text` and `image` there — and it validates the
/// whole file, so a wider declaration (audio/video/pdf) would make Pi discard
/// every custom provider. Narrowing happens here, at the harness boundary.
///
/// `None` means CHM has no declaration: the entry is left untouched instead of
/// being declared text-only, so a hand-written `input` keeps working.
pub fn set_model_input(
    doc: &mut Value,
    provider_id: Option<&str>,
    model_id: &str,
    input_modalities: Option<&[String]>,
) -> bool {
    let Some(modalities) = input_modalities else {
        return false;
    };
    let pi_input: Vec<String> =
        chm_harness_sdk::adapter::capabilities::canonicalize_input_modalities(modalities)
            .into_iter()
            .filter(|value| value == "text" || value == "image")
            .collect();
    let value = Value::Array(
        pi_input
            .into_iter()
            .map(Value::String)
            .collect::<Vec<Value>>(),
    );
    let Some(providers) = doc
        .as_object_mut()
        .and_then(|root| root.get_mut("providers"))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let mut found = false;
    for (pname, provider) in providers.iter_mut() {
        if provider_id.is_some_and(|wanted| !wanted.eq_ignore_ascii_case(pname)) {
            continue;
        }
        let Some(models) = provider.get_mut("models").and_then(Value::as_array_mut) else {
            continue;
        };
        for model in models.iter_mut() {
            if model.get("id").and_then(Value::as_str) != Some(model_id) {
                continue;
            }
            let Some(obj) = model.as_object_mut() else {
                continue;
            };
            obj.insert("input".into(), value.clone());
            found = true;
        }
    }
    found
}

/// Ensure a provider has a secret-free API-key reference. Pi resolves
/// environment names and `!command` values when it sends a request; CHM
/// writes the actual key to auth.json in the apply coordinator.
pub fn configure_provider_auth(
    doc: &mut Value,
    provider_id: &str,
    credential_kind: Option<&str>,
    credential_reference: Option<&str>,
    credential_ref_id: Option<uuid::Uuid>,
) {
    let (Some(kind), Some(reference)) = (credential_kind, credential_reference) else {
        return;
    };
    let value = match kind {
        // Pi's models.json uses shell-style interpolation for environment
        // values. A bare `MY_API_KEY` is a literal according to Pi's docs,
        // so always emit the explicit `$` marker when CHM stores an Env ref.
        "env" => {
            if reference.starts_with('$') {
                reference.to_string()
            } else {
                format!("${reference}")
            }
        }
        // Stored secrets resolve through CHM's credential helper rather than
        // a platform-specific command: the helper reads macOS Keychain or
        // Windows Credential Manager, so the same models.json works on both.
        // Pi treats a leading `!` as "run this through the shell".
        "keychain" => match credential_ref_id {
            Some(id) => format!(
                "!{}",
                chm_harness_sdk::adapter::helpers::credential_helper_shell_command(id)
            ),
            // No reference id to point at — keep the historical macOS form so
            // existing entries stay reproducible for callers without an id.
            None => format!(
                "!security find-generic-password -w -s 'coding-harness-manager' -a '{}'",
                reference
                    .strip_prefix("coding-harness-manager/")
                    .unwrap_or(reference)
            ),
        },
        _ => return,
    };
    let Some(provider) = doc
        .as_object_mut()
        .and_then(|root| root.get_mut("providers"))
        .and_then(|providers| providers.as_object_mut())
        .and_then(|providers| providers.get_mut(provider_id))
        .and_then(|provider| provider.as_object_mut())
    else {
        return;
    };
    provider
        .entry("apiKey")
        .or_insert_with(|| Value::String(value));
}

/// Update an existing model entry (matched by id) under any provider.
/// Returns false when no provider carries that model id.
pub fn update_model(
    doc: &mut Value,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
) -> bool {
    update_model_in_provider_with_limits(doc, None, model_id, display_name, context_window, None)
}

/// Update only the matching provider's model entry. `None` retains the
/// backwards-compatible provider-agnostic behavior used by old callers.
pub fn update_model_in_provider(
    doc: &mut Value,
    provider_id: Option<&str>,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
) -> bool {
    update_model_in_provider_with_limits(
        doc,
        provider_id,
        model_id,
        display_name,
        context_window,
        None,
    )
}

pub fn update_model_in_provider_with_limits(
    doc: &mut Value,
    provider_id: Option<&str>,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
    max_output: Option<i64>,
) -> bool {
    let Some(providers) = doc
        .as_object_mut()
        .and_then(|o| o.get_mut("providers"))
        .and_then(|p| p.as_object_mut())
    else {
        return false;
    };
    let mut found = false;
    for (pname, pv) in providers.iter_mut() {
        if provider_id.is_some_and(|wanted| !wanted.eq_ignore_ascii_case(pname)) {
            continue;
        }
        let Some(models) = pv.get_mut("models").and_then(|m| m.as_array_mut()) else {
            continue;
        };
        for m in models.iter_mut() {
            if m.get("id").and_then(|v| v.as_str()) == Some(model_id)
                && let Some(obj) = m.as_object_mut()
            {
                obj.insert("name".into(), Value::String(display_name.into()));
                match context_window {
                    Some(ctx) => {
                        obj.insert("contextWindow".into(), Value::Number(ctx.into()));
                    }
                    None => {
                        obj.remove("contextWindow");
                    }
                }
                match max_output {
                    Some(value) => {
                        obj.insert("maxTokens".into(), Value::Number(value.into()));
                    }
                    None => {
                        obj.remove("maxTokens");
                    }
                }
                found = true;
            }
        }
    }
    found
}

/// Remove a model entry (matched by id) from every provider that carries it.
/// Returns the number of entries removed.
pub fn remove_model(doc: &mut Value, model_id: &str) -> usize {
    remove_model_in_provider(doc, None, model_id)
}

/// Remove a model only from the provider that owns it. This is essential when
/// two providers intentionally expose the same remote model id.
pub fn remove_model_in_provider(
    doc: &mut Value,
    provider_id: Option<&str>,
    model_id: &str,
) -> usize {
    let Some(providers) = doc
        .as_object_mut()
        .and_then(|o| o.get_mut("providers"))
        .and_then(|p| p.as_object_mut())
    else {
        return 0;
    };
    let mut removed = 0;
    let mut empty_stubs = Vec::new();
    for (pname, pv) in providers.iter_mut() {
        if provider_id.is_some_and(|wanted| !wanted.eq_ignore_ascii_case(pname)) {
            continue;
        }
        let Some(models) = pv.get_mut("models").and_then(|m| m.as_array_mut()) else {
            continue;
        };
        let before = models.len();
        models.retain(|m| m.get("id").and_then(|v| v.as_str()) != Some(model_id));
        let removed_here = before - models.len();
        removed += removed_here;
        if removed_here > 0 && models.is_empty() && !provider_has_configuration(pv) {
            empty_stubs.push(pname.clone());
        }
    }
    for name in empty_stubs {
        providers.remove(&name);
    }
    removed
}

pub fn serialize(doc: &Value) -> String {
    serde_json::to_string_pretty(doc).unwrap_or_else(|_| "{}".into())
}

pub fn validate_config(file_path: &str) -> ValidationReport {
    match std::fs::read_to_string(file_path) {
        Ok(raw) => match parse_document(&raw).and_then(|doc| {
            let providers = doc
                .get("providers")
                .and_then(Value::as_object)
                .ok_or("models.json providers must be an object")?;
            for (name, provider) in providers {
                if !provider_has_configuration(provider) {
                    return Err(format!(
                        "provider {name} must specify baseUrl, headers, compat, modelOverrides, or at least one model"
                    ));
                }
            }
            Ok(())
        }) {
            Ok(()) => ValidationReport {
                ok: true,
                errors: vec![],
            },
            Err(e) => ValidationReport {
                ok: false,
                errors: vec![e],
            },
        },
        Err(e) => ValidationReport {
            ok: false,
            errors: vec![format!("cannot read: {e}")],
        },
    }
}
