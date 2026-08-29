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
    if !doc.is_object() {
        return Err("models.json must be an object".into());
    }
    let providers = doc
        .as_object_mut()
        .unwrap()
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    for pv in providers.values_mut() {
        if let Some(obj) = pv.as_object_mut() {
            obj.entry("models").or_insert_with(|| Value::Array(vec![]));
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

/// Update an existing model entry (matched by id) under any provider.
/// Returns false when no provider carries that model id.
pub fn update_model(
    doc: &mut Value,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
) -> bool {
    let Some(providers) = doc
        .as_object_mut()
        .and_then(|o| o.get_mut("providers"))
        .and_then(|p| p.as_object_mut())
    else {
        return false;
    };
    let mut found = false;
    for (_pname, pv) in providers.iter_mut() {
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
                found = true;
            }
        }
    }
    found
}

/// Remove a model entry (matched by id) from every provider that carries it.
/// Returns the number of entries removed.
pub fn remove_model(doc: &mut Value, model_id: &str) -> usize {
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
