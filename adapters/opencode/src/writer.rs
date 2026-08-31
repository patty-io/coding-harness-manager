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
    fold_model_with_provider(
        doc,
        provider_id,
        model_id,
        display_name,
        context_window,
        capabilities,
        None,
    );
}

/// Folds a model and, when the route carries endpoint metadata, materializes
/// the provider settings OpenCode needs for a custom provider. Existing
/// provider settings are preserved; only missing values are filled.
pub fn fold_model_with_provider(
    doc: &mut Value,
    provider_id: &str,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
    capabilities: &Value,
    provider_config: Option<&Value>,
) {
    // Older CHM versions wrote providerless routes under a synthetic
    // `custom` provider. When the route now carries its canonical provider,
    // move that legacy model instead of leaving two copies in the config.
    if !provider_id.eq_ignore_ascii_case("custom") {
        take_legacy_custom_model(doc, model_id);
    }
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
    configure_provider(pv, provider_config);
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

/// Update the metadata of a model already present in an OpenCode provider.
/// Returns false when the provider/model pair cannot be found. Limits are
/// merged so provider-specific fields (for example reasoning or modalities)
/// are preserved while CHM-owned context/output values are refreshed.
pub fn update_model_in_provider(
    doc: &mut Value,
    provider_id: Option<&str>,
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
    max_output: Option<i64>,
) -> bool {
    let Some(providers) = doc
        .as_object_mut()
        .and_then(|object| object.get_mut("provider"))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    for (name, provider) in providers.iter_mut() {
        if provider_id.is_some_and(|wanted| !name.eq_ignore_ascii_case(wanted)) {
            continue;
        }
        let Some(models) = provider.get_mut("models").and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(model) = models.get_mut(model_id).and_then(Value::as_object_mut) else {
            continue;
        };
        model.insert("name".into(), Value::String(display_name.to_string()));
        update_limits(model, context_window, max_output);
        return true;
    }
    false
}

fn update_limits(
    model: &mut Map<String, Value>,
    context_window: Option<i64>,
    max_output: Option<i64>,
) {
    if context_window.is_none() && max_output.is_none() {
        // Do not disturb an existing provider-specific limit object when the
        // route carries no limits.
        return;
    }
    let limits = model
        .entry("limit")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("model limit must be an object");
    if let Some(context) = context_window {
        limits.insert("context".into(), Value::Number(context.into()));
    } else {
        limits.remove("context");
    }
    if let Some(output) = max_output {
        limits.insert("output".into(), Value::Number(output.into()));
    } else {
        limits.remove("output");
    }
    if limits.is_empty() {
        model.remove("limit");
    }
}

/// Remove one model from an OpenCode provider. The provider itself is kept so
/// its endpoint/auth settings remain available for future models.
pub fn remove_model_in_provider(
    doc: &mut Value,
    provider_id: Option<&str>,
    model_id: &str,
) -> bool {
    let Some(providers) = doc
        .as_object_mut()
        .and_then(|object| object.get_mut("provider"))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    for (name, provider) in providers.iter_mut() {
        if provider_id.is_some_and(|wanted| !name.eq_ignore_ascii_case(wanted)) {
            continue;
        }
        let Some(models) = provider.get_mut("models").and_then(Value::as_object_mut) else {
            continue;
        };
        if models.remove(model_id).is_some() {
            return true;
        }
    }
    false
}

fn configure_provider(provider: &mut Map<String, Value>, config: Option<&Value>) {
    let Some(config) = config.and_then(Value::as_object) else {
        return;
    };

    if let Some(name) = config
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        provider
            .entry("name")
            .or_insert_with(|| Value::String(name.to_string()));
    }

    if let Some(protocol) = config.get("protocol").and_then(Value::as_str) {
        provider.entry("npm").or_insert_with(|| {
            Value::String(
                match protocol {
                    "openai-responses" => "@ai-sdk/openai",
                    "anthropic-messages" => "@ai-sdk/anthropic",
                    _ => "@ai-sdk/openai-compatible",
                }
                .to_string(),
            )
        });
    }

    let base_url = config
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base_url| !base_url.is_empty());
    let api_key_env = config
        .get("api_key_env")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty());
    if base_url.is_none() && api_key_env.is_none() {
        return;
    }
    let options = provider
        .entry("options")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("provider options must be an object");
    if let Some(base_url) = base_url {
        options
            .entry("baseURL")
            .or_insert_with(|| Value::String(base_url.to_string()));
    }
    if let Some(api_key_env) = api_key_env {
        options
            .entry("apiKey")
            .or_insert_with(|| Value::String(format!("{{env:{api_key_env}}}")));
    }
}

/// Remove a model written by the old providerless fallback. We only touch a
/// `custom` provider that has no provider-level configuration (`npm` or
/// `options`), which distinguishes CHM's legacy stub from a user-managed
/// custom provider. The returned value is intentionally unused today; the
/// target provider is rewritten from the canonical route metadata below.
fn take_legacy_custom_model(doc: &mut Value, model_id: &str) -> Option<Value> {
    let providers = doc
        .as_object_mut()
        .and_then(|object| object.get_mut("provider"))
        .and_then(|provider| provider.as_object_mut())?;
    let custom_id = providers
        .keys()
        .find(|id| id.eq_ignore_ascii_case("custom"))
        .cloned()?;
    let value = {
        let custom = providers.get_mut(&custom_id)?.as_object_mut()?;
        if custom.contains_key("npm") || custom.contains_key("options") {
            return None;
        }
        let models = custom.get_mut("models")?.as_object_mut()?;
        let value = models.remove(model_id);
        if models.is_empty() {
            custom.remove("models");
        }
        value
    };
    if value.is_some()
        && providers
            .get(&custom_id)
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
    {
        providers.remove(&custom_id);
    }
    value
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
