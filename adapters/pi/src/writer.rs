//! Pi writer: folds model adds into ONE cumulative models.json change.

use chm_harness_sdk::adapter::types::ValidationReport;
use serde_json::{Map, Value};

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

pub fn serialize(doc: &Value) -> String {
    serde_json::to_string_pretty(doc).unwrap_or_else(|_| "{}".into())
}

pub fn validate_config(file_path: &str) -> ValidationReport {
    match std::fs::read_to_string(file_path) {
        Ok(raw) => match parse_document(&raw) {
            Ok(_) => ValidationReport {
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
