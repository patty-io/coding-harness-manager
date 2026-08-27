//! Claude Code writer: env-block edits to ~/.claude/settings.json.

use chm_filesystem::atomic_write;
use chm_harness_sdk::adapter::types::{ApplyResult, NativeChange, NativePlan, ValidationReport};
use serde_json::{Map, Value};

const ROLE_ENV: &[(&str, &str)] = &[
    ("opus", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
    ("sonnet", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
    ("haiku", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
];

/// Sets the role env var for a model add with a `role` capability.
pub fn plan_role_model(file_path: &str, role: &str, model_id: &str) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| "{}".into());
    let env_key = ROLE_ENV
        .iter()
        .find(|(r, _)| *r == role)
        .map(|(_, k)| *k)
        .unwrap_or("ANTHROPIC_MODEL");
    let after = merge_env(&raw, env_key, model_id);
    NativeChange {
        file_path: file_path.to_string(),
        before: Some(raw),
        after: Some(after),
    }
}

/// Sets an env var in settings.json's `env` block, preserving all other keys.
pub fn merge_env(raw: &str, key: &str, value: &str) -> String {
    let mut doc: Value = serde_json::from_str(raw).unwrap_or(Value::Object(Map::new()));
    let env = doc
        .as_object_mut()
        .unwrap()
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    env.insert(key.to_string(), Value::String(value.to_string()));
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
                errors: vec![format!("settings.json invalid: {e}")],
            },
        },
        Err(e) => ValidationReport {
            ok: false,
            errors: vec![format!("cannot read: {e}")],
        },
    }
}
