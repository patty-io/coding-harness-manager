//! OpenCode writer: minimal-subtree edits to opencode.jsonc.
//! plan() folds ALL actions into ONE cumulative change per file so multiple
//! model adds never clobber each other.

use chm_harness_sdk::adapter::types::{NativeChange, ValidationReport};
use serde_json::{Map, Value};

/// Folds a model entry into a running document (comments stripped via
/// json_comments; unparseable input is an error, never silently `{}`).
pub fn fold_model(
    doc: &mut Value,
    provider_id: &str,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
    capabilities: &Value,
) {
    let providers = doc
        .as_object_mut()
        .expect("document must be an object")
        .entry("provider")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("provider must be an object");
    let pv = providers
        .entry(provider_id)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("provider entry must be an object");
    let models = pv
        .entry("models")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("models must be an object");
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
}

/// Folds an MCP server into `mcp.<name>` (local: command array + environment;
/// remote: type/url/headers).
pub fn fold_mcp(doc: &mut Value, name: &str, spec: &chm_core::domain::mcp::McpServer) {
    let mcp = doc
        .as_object_mut()
        .expect("document must be an object")
        .entry("mcp")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("mcp must be an object");
    let mut entry = Map::new();
    let remote = !matches!(spec.transport, chm_core::domain::mcp::McpTransport::Stdio);
    entry.insert(
        "type".into(),
        Value::String(if remote {
            "remote".into()
        } else {
            "local".into()
        }),
    );
    if remote {
        if let Some(url) = &spec.url {
            entry.insert("url".into(), Value::String(url.clone()));
        }
    } else if let Some(cmd) = &spec.command {
        let mut arr = vec![Value::String(cmd.clone())];
        arr.extend(spec.args.iter().map(|a| Value::String(a.clone())));
        entry.insert("command".into(), Value::Array(arr));
    }
    if !spec.env.is_empty() {
        let mut environment = Map::new();
        for (k, v) in &spec.env {
            if k == "headers" {
                entry.insert("headers".into(), v.clone());
            } else if k == "_direct_tools" {
                continue;
            } else {
                environment.insert(k.clone(), v.clone());
            }
        }
        if !environment.is_empty() {
            entry.insert("environment".into(), Value::Object(environment));
        }
    }
    entry.insert("enabled".into(), Value::Bool(true));
    mcp.insert(name.to_string(), Value::Object(entry));
}

/// Parses an opencode config into a mutable document (JSONC-tolerant).
pub fn parse_document(raw: &str) -> Result<Value, String> {
    let clean = json_comments::StripComments::new(raw.as_bytes());
    serde_json::from_reader(clean).map_err(|e| format!("opencode config is not valid JSON: {e}"))
}

/// Serializes the final document.
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
            errors: vec![format!("cannot read opencode.jsonc: {e}")],
        },
    }
}

#[allow(dead_code)]
fn _type_check(_: NativeChange) {}
