//! Pi writer: minimal-subtree edits to ~/.pi/agent/models.json.

use chm_filesystem::atomic_write;
use chm_harness_sdk::adapter::types::{ApplyResult, NativeChange, NativePlan, ValidationReport};
use serde_json::{Map, Value};

/// Adds a model entry to the native provider's `models` array (models.json).
pub fn plan_model_add(
    file_path: &str,
    provider_id: &str,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| r#"{"providers": {}}"#.into());
    let after = merge_model(&raw, provider_id, model_id, display_name, context_window);
    NativeChange {
        file_path: file_path.to_string(),
        before: Some(raw),
        after: Some(after),
    }
}

pub fn merge_model(
    raw: &str,
    provider_id: &str,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
) -> String {
    let mut doc: Value = serde_json::from_str(raw).unwrap_or(Value::Object(Map::new()));
    let providers = doc
        .as_object_mut()
        .unwrap()
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let pv = providers
        .entry(provider_id)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let models = pv
        .entry("models")
        .or_insert_with(|| Value::Array(vec![]))
        .as_array_mut()
        .unwrap();
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
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| raw.to_string())
}

pub fn apply_native_plan(plan: &NativePlan) -> Result<ApplyResult, String> {
    let mut result = ApplyResult {
        files_written: vec![],
        links_created: vec![],
    };
    for change in &plan.changes {
        let after = change.after.clone().ok_or("change without after")?;
        atomic_write(std::path::Path::new(&change.file_path), &after).map_err(|e| e.to_string())?;
        result.files_written.push(change.file_path.clone());
    }
    Ok(result)
}

pub fn validate_config(file_path: &str) -> ValidationReport {
    match std::fs::read_to_string(file_path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(_) => ValidationReport {
                ok: true,
                errors: vec![],
            },
            Err(e) => ValidationReport {
                ok: false,
                errors: vec![format!("models.json invalid: {e}")],
            },
        },
        Err(e) => ValidationReport {
            ok: false,
            errors: vec![format!("cannot read: {e}")],
        },
    }
}
