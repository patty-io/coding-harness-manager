//! Codex writer: folds model adds into the per-provider config file.

use chm_harness_sdk::adapter::types::ValidationReport;
use toml_edit::{DocumentMut, Item, Table, Value};

/// Folds one model add into the provider file document.
pub fn fold_provider(
    doc: &mut DocumentMut,
    provider_id: &str,
    model_id: &str,
    base_url: &str,
    env_key: Option<&str>,
    wire_api: &str,
) {
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

/// Validates every changed file in a native plan (the files apply will touch).
pub fn validate_changed_files(
    plan: &chm_harness_sdk::adapter::types::NativePlan,
) -> ValidationReport {
    let mut errors = Vec::new();
    for change in &plan.changes {
        let path = &change.file_path;
        if let Some(after) = &change.after
            && after.parse::<DocumentMut>().is_err()
        {
            errors.push(format!("{path}: serialized TOML invalid"));
        }
    }
    ValidationReport {
        ok: errors.is_empty(),
        errors,
    }
}
