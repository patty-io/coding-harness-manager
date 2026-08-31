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

/// Configure Claude Code's documented gateway surface for one model route.
/// Claude has one active model/base URL slot, so the route capability layer
/// rejects multi-model or multi-provider bundles before this writer runs.
pub fn fold_gateway(
    doc: &mut Value,
    model_id: &str,
    base_url: &str,
    credential_ref_id: Option<uuid::Uuid>,
) {
    fold_env(doc, "ANTHROPIC_MODEL", model_id);
    fold_env(doc, "ANTHROPIC_BASE_URL", base_url);
    if let Some(credential_ref_id) = credential_ref_id {
        let root = doc.as_object_mut().expect("document must be an object");
        root.insert(
            "apiKeyHelper".into(),
            Value::String(
                chm_harness_sdk::adapter::helpers::credential_helper_shell_command(
                    credential_ref_id,
                ),
            ),
        );
    }
}

/// Update the active model and optional gateway metadata. Returns false when
/// the current settings select a different model, avoiding an unrelated user
/// setting being overwritten by a stale binding.
pub fn update_gateway(
    doc: &mut Value,
    model_id: &str,
    base_url: Option<&str>,
    credential_ref_id: Option<uuid::Uuid>,
) -> bool {
    let current = doc
        .get("env")
        .and_then(|value| value.get("ANTHROPIC_MODEL"))
        .and_then(|value| value.as_str());
    if current.is_some_and(|value| !value.eq_ignore_ascii_case(model_id)) {
        return false;
    }
    fold_env(doc, "ANTHROPIC_MODEL", model_id);
    if let Some(base_url) = base_url {
        fold_env(doc, "ANTHROPIC_BASE_URL", base_url);
    }
    if let Some(credential_ref_id) = credential_ref_id {
        let root = doc.as_object_mut().expect("document must be an object");
        root.insert(
            "apiKeyHelper".into(),
            Value::String(
                chm_harness_sdk::adapter::helpers::credential_helper_shell_command(
                    credential_ref_id,
                ),
            ),
        );
    }
    true
}

/// Remove the active model only when it still selects the requested id.
pub fn remove_gateway(doc: &mut Value, model_id: &str) -> bool {
    let Some(env) = doc.get_mut("env").and_then(|value| value.as_object_mut()) else {
        return false;
    };
    let matches = env
        .get("ANTHROPIC_MODEL")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(model_id));
    if !matches {
        return false;
    }
    env.remove("ANTHROPIC_MODEL");
    true
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
