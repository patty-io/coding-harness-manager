//! Claude Code writer: folds role-model env vars into ONE settings.json change.

use chm_harness_sdk::adapter::types::ValidationReport;
use serde_json::{Map, Value};

pub fn parse_document(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|e| format!("settings.json is not valid JSON: {e}"))
}

/// Sets an env var in settings.json's `env` block, preserving all other keys.
pub fn fold_env(doc: &mut Value, key: &str, value: &str) {
    let env = doc
        .as_object_mut()
        .expect("document must be an object")
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("env must be an object");
    env.insert(key.to_string(), Value::String(value.to_string()));
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
