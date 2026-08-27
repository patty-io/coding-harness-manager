//! Codex writer: edits the per-provider config file (~/.codex/<id>.config.toml).

use chm_filesystem::atomic_write;
use chm_harness_sdk::adapter::types::{ApplyResult, NativeChange, NativePlan, ValidationReport};
use toml_edit::{DocumentMut, Item, Table, Value};

/// Writes the provider file: model_providers.<id> (base_url/env_key/wire_api)
/// + top-level `model` selection. One file, one rewrite.
pub fn plan_provider_file(
    file_path: &str,
    provider_id: &str,
    model_id: &str,
    base_url: &str,
    env_key: Option<&str>,
    wire_api: &str,
) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| String::new());
    let after = merge_provider_file(&raw, provider_id, model_id, base_url, env_key, wire_api);
    NativeChange {
        file_path: file_path.to_string(),
        before: Some(raw),
        after: Some(after),
    }
}

pub fn merge_provider_file(
    raw: &str,
    provider_id: &str,
    model_id: &str,
    base_url: &str,
    env_key: Option<&str>,
    wire_api: &str,
) -> String {
    let mut doc: DocumentMut = raw.parse().unwrap_or_default();
    doc["model"] = Item::Value(Value::from(format!("{provider_id}/{model_id}")));
    doc["model_provider"] = Item::Value(Value::from(provider_id));
    let mps_key = format!("model_providers.{provider_id}");
    let table = doc
        .entry(&mps_key)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .expect("model_providers.<id> must be a table");
    table["name"] = Item::Value(Value::from(provider_id));
    table["base_url"] = Item::Value(Value::from(base_url));
    table["wire_api"] = Item::Value(Value::from(wire_api));
    if let Some(key) = env_key {
        table["env_key"] = Item::Value(Value::from(key));
    }
    doc.to_string()
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
        Ok(raw) => match raw.parse::<DocumentMut>() {
            Ok(_) => ValidationReport {
                ok: true,
                errors: vec![],
            },
            Err(e) => ValidationReport {
                ok: false,
                errors: vec![format!("config.toml invalid: {e}")],
            },
        },
        Err(e) => ValidationReport {
            ok: false,
            errors: vec![format!("cannot read: {e}")],
        },
    }
}
