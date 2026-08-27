//! OpenCode writer: minimal-subtree edits to opencode.jsonc.

use chm_filesystem::{atomic_write, backup_file};
use chm_harness_sdk::adapter::types::{ApplyResult, NativeChange, NativePlan, ValidationReport};
use serde_json::{Map, Value};

/// Mirrors the full desired model entry so a re-read produces an identical
/// normalized state (idempotent sync).
pub fn plan_model_add(
    file_path: &str,
    provider_id: &str,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
    capabilities: &serde_json::Value,
) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| "{}".into());
    let after = merge_model(
        &raw,
        provider_id,
        model_id,
        display_name,
        context_window,
        capabilities,
    );
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
    capabilities: &serde_json::Value,
) -> String {
    let mut doc: Value = serde_json::from_str(raw).unwrap_or(Value::Object(Map::new()));
    let providers = doc
        .as_object_mut()
        .unwrap()
        .entry("provider")
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
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let mut entry = Map::new();
    entry.insert("name".to_string(), Value::String(display_name.to_string()));
    if let Some(ctx) = context_window {
        entry.insert(
            "limit".to_string(),
            Value::Object(Map::from_iter([(
                "context".to_string(),
                Value::Number(ctx.into()),
            )])),
        );
    }
    if let Some(caps) = capabilities.as_object() {
        for (k, v) in caps {
            if k == "name" || k == "limit" {
                continue;
            }
            entry.insert(k.clone(), v.clone());
        }
    }
    models.insert(model_id.to_string(), Value::Object(entry));
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| raw.to_string())
}

/// MCP merge: `mcp.<name>` object; local servers keep the command array.
pub fn plan_mcp_add(file_path: &str, name: &str, command: &str, args: &[String]) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| "{}".into());
    let mut doc: Value = serde_json::from_str(&raw).unwrap_or(Value::Object(Map::new()));
    let mcp = doc
        .as_object_mut()
        .unwrap()
        .entry("mcp")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let mut entry = Map::new();
    entry.insert("type".into(), Value::String("local".into()));
    let mut cmd = vec![Value::String(command.to_string())];
    cmd.extend(args.iter().map(|a| Value::String(a.clone())));
    entry.insert("command".into(), Value::Array(cmd));
    entry.insert("enabled".into(), Value::Bool(true));
    mcp.insert(name.to_string(), Value::Object(entry));
    let after = serde_json::to_string_pretty(&doc).unwrap_or_else(|_| raw.to_string());
    NativeChange {
        file_path: file_path.to_string(),
        before: Some(raw),
        after: Some(after),
    }
}

pub fn apply_native_plan(plan: &NativePlan) -> Result<ApplyResult, String> {
    let mut result = ApplyResult {
        files_written: vec![],
        links_created: vec![],
    };
    for change in &plan.changes {
        let after = change.after.clone().ok_or("change without after content")?;
        let backup =
            backup_file(std::path::Path::new(&change.file_path)).map_err(|e| e.to_string())?;
        let _ = backup;
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
                errors: vec![format!("opencode.jsonc no longer valid JSON: {e}")],
            },
        },
        Err(e) => ValidationReport {
            ok: false,
            errors: vec![format!("cannot read opencode.jsonc: {e}")],
        },
    }
}
