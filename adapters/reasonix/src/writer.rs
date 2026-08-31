//! Reasonix writer: [[providers]] append to ~/.reasonix/config.toml.

use chm_filesystem::atomic_write;
use chm_harness_sdk::adapter::types::{ApplyResult, NativeChange, NativePlan, ValidationReport};
use toml_edit::{Array, DocumentMut, Item, Value};

/// Appends (or updates) a [[providers]] entry. Returns the change.
pub fn plan_provider_add(
    file_path: &str,
    provider_id: &str,
    kind: &str,
    base_url: &str,
    model_id: &str,
    api_key_env: Option<&str>,
) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| String::new());
    let after = merge_provider(&raw, provider_id, kind, base_url, model_id, api_key_env);
    NativeChange {
        file_path: file_path.to_string(),
        before: Some(raw),
        after: Some(after),
    }
}

pub fn merge_provider(
    raw: &str,
    provider_id: &str,
    kind: &str,
    base_url: &str,
    model_id: &str,
    api_key_env: Option<&str>,
) -> String {
    let mut doc: DocumentMut = raw.parse().unwrap_or_default();
    // update an existing [[providers]] entry with the same name, else append
    let mut found = false;
    if let Some(Item::ArrayOfTables(providers)) = doc.get_mut("providers") {
        for table in providers.iter_mut() {
            if table.get("name").and_then(|v| v.as_str()) == Some(provider_id) {
                table["kind"] = Item::Value(Value::from(kind));
                table["base_url"] = Item::Value(Value::from(base_url));
                let mut models = table
                    .get("models")
                    .and_then(Item::as_value)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !models.iter().any(|model| model.as_str() == Some(model_id)) {
                    models.push(model_id);
                }
                table["models"] = Item::Value(Value::Array(models));
                if let Some(key) = api_key_env {
                    table["api_key_env"] = Item::Value(Value::from(key));
                }
                found = true;
                break;
            }
        }
    }
    if !found {
        let mut entry = toml_edit::Table::new();
        entry["name"] = Item::Value(Value::from(provider_id));
        entry["kind"] = Item::Value(Value::from(kind));
        entry["base_url"] = Item::Value(Value::from(base_url));
        let mut models = Array::new();
        models.push(model_id);
        entry["models"] = Item::Value(Value::Array(models));
        if let Some(key) = api_key_env {
            entry["api_key_env"] = Item::Value(Value::from(key));
        }
        doc["providers"]
            .or_insert(Item::ArrayOfTables(Default::default()))
            .as_array_of_tables_mut()
            .expect("providers array of tables")
            .push(entry);
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

/// Folds one provider/model add directly into a document.
pub fn fold_provider(
    doc: &mut DocumentMut,
    provider_id: &str,
    kind: &str,
    base_url: &str,
    model_id: &str,
    api_key_env: Option<&str>,
) {
    let after = merge_provider(
        &doc.to_string(),
        provider_id,
        kind,
        base_url,
        model_id,
        api_key_env,
    );
    *doc = after.parse().unwrap_or_else(|_| DocumentMut::new());
}

/// Update provider-level model metadata while preserving the provider's
/// existing model list. Reasonix stores context/output limits on the
/// provider entry rather than on each model string.
pub fn update_model(
    doc: &mut DocumentMut,
    provider_id: &str,
    model_id: &str,
    context_window: Option<i64>,
    max_output: Option<i64>,
) -> bool {
    let Some(Item::ArrayOfTables(providers)) = doc.get_mut("providers") else {
        return false;
    };
    let mut found = false;
    for table in providers.iter_mut() {
        if table.get("name").and_then(|value| value.as_str()) != Some(provider_id) {
            continue;
        }
        let present = table
            .get("models")
            .and_then(Item::as_value)
            .and_then(Value::as_array)
            .is_some_and(|models| models.iter().any(|model| model.as_str() == Some(model_id)));
        if !present {
            continue;
        }
        match context_window {
            Some(value) => table["context_window"] = Item::Value(Value::from(value)),
            None => {
                table.remove("context_window");
            }
        }
        match max_output {
            Some(value) => table["max_output_tokens"] = Item::Value(Value::from(value)),
            None => {
                table.remove("max_output_tokens");
            }
        }
        found = true;
    }
    found
}

/// Remove one model from a provider's model list without deleting the
/// provider itself. The latter may still hold credentials or other models.
pub fn remove_model(doc: &mut DocumentMut, provider_id: &str, model_id: &str) -> bool {
    let Some(Item::ArrayOfTables(providers)) = doc.get_mut("providers") else {
        return false;
    };
    for table in providers.iter_mut() {
        if table.get("name").and_then(|value| value.as_str()) != Some(provider_id) {
            continue;
        }
        let Some(models) = table
            .get_mut("models")
            .and_then(Item::as_value_mut)
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        let before = models.len();
        models.retain(|model| model.as_str() != Some(model_id));
        return before != models.len();
    }
    false
}
