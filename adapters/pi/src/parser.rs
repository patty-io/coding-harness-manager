//! Pi native config parser (~/.pi/agent/*.json).
//! Shape verified on 0.84.3 — see docs/harnesses/pi.md. JSON, not TOML.

use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use uuid::Uuid;

pub fn parse_config(
    models_raw: Option<&str>,
    mcp_raw: Option<&str>,
    settings_raw: Option<&str>,
    home: &std::path::Path,
) -> Result<ParsedState, AdapterError> {
    let mut state = ParsedState::default();

    // models.json -> providers + model routes
    if let Some(raw) = models_raw {
        let json: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| AdapterError::Parse {
                path: "models.json".into(),
                detail: e.to_string(),
            })?;
        if let Some(providers) = json.get("providers").and_then(|p| p.as_object()) {
            for (provider_id, pv) in providers {
                state.providers.push(serde_json::json!({
                    "native_provider_id": provider_id,
                    "base_url": pv.get("baseUrl"),
                    "api": pv.get("api"),
                    "api_key_inline": pv.get("apiKey").is_some(), // boolean only — never the value
                    // Pi supports explicit `$VAR`/`${VAR}` references in
                    // apiKey. Preserve only the variable name so an import
                    // can create a credential reference without copying a
                    // secret or executing a command.
                    "env_key": pv
                        .get("apiKey")
                        .and_then(|value| value.as_str())
                        .and_then(api_key_env_reference),
                    "compat": pv.get("compat"),
                }));
                if let Some(models) = pv.get("models").and_then(|m| m.as_array()) {
                    for meta in models {
                        let model_id = meta
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        if model_id.is_empty() {
                            continue;
                        }
                        let capabilities = meta.clone();
                        let api = meta
                            .get("api")
                            .and_then(|value| value.as_str())
                            .or_else(|| pv.get("api").and_then(|value| value.as_str()));
                        let route = ModelRoute::new(
                            model_id.clone(),
                            meta.get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&model_id)
                                .to_string(),
                            meta.get("contextWindow").and_then(|v| v.as_i64()),
                            capabilities,
                            serde_json::json!({
                                "native_provider_id": provider_id,
                                "base_url": pv.get("baseUrl"),
                                "protocol": pi_protocol(api),
                                "wire_model": model_id,
                            }),
                        );
                        state.models.push(HarnessModel {
                            native_id: model_id.clone(),
                            route,
                        });
                    }
                }
            }
        }
    }

    // mcp.json -> mcpServers + imports
    if let Some(raw) = mcp_raw {
        let json: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| AdapterError::Parse {
                path: "mcp.json".into(),
                detail: e.to_string(),
            })?;
        if let Some(imports) = json.get("imports").and_then(|i| i.as_array()) {
            state.providers.push(serde_json::json!({
                "native_provider_id": "__mcp_imports__",
                "imports": imports.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>(),
            }));
        }
        if let Some(servers) = json.get("mcpServers").and_then(|s| s.as_object()) {
            for (name, spec) in servers {
                state.mcp.push(HarnessMcp {
                    native_name: name.clone(),
                    server: parse_mcp(name, spec),
                });
            }
        }
    }

    // settings.json -> selection + external skill paths
    if let Some(raw) = settings_raw {
        let json: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| AdapterError::Parse {
                path: "settings.json".into(),
                detail: e.to_string(),
            })?;
        state.profiles.push(serde_json::json!({
            "default_provider": json.get("defaultProvider"),
            "default_model": json.get("defaultModel"),
            "default_thinking_level": json.get("defaultThinkingLevel"),
        }));
        if let Some(skills) = json.get("skills").and_then(|s| s.as_array()) {
            for path in skills.iter().filter_map(|v| v.as_str()) {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string());
                state
                    .skills
                    .push(chm_harness_sdk::adapter::types::HarnessSkill {
                        name,
                        path: path.to_string(),
                        content_hash: None,
                        symlinked: false, // external reference — resolved by the harness itself
                    });
            }
        }
    }

    // own skills dir: ~/.pi/agent/skills
    let skills_dir = home.join(".pi/agent/skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                state
                    .skills
                    .push(chm_harness_sdk::adapter::types::HarnessSkill {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        path: entry.path().display().to_string(),
                        content_hash: None,
                        symlinked: entry
                            .path()
                            .symlink_metadata()
                            .map(|m| m.file_type().is_symlink())
                            .unwrap_or(false),
                    });
            }
        }
    }

    Ok(state)
}

/// Return the environment variable name from Pi's explicit interpolation
/// syntax. Plain strings (including uppercase-looking placeholders) remain
/// opaque because Pi treats them as literal keys; command-backed values are
/// intentionally not executed or imported.
fn api_key_env_reference(value: &str) -> Option<String> {
    let value = value.trim();
    let name = value
        .strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
        .or_else(|| value.strip_prefix('$'))?;
    if name.is_empty()
        || !name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    Some(name.to_string())
}

fn pi_protocol(api: Option<&str>) -> &'static str {
    match api.unwrap_or("") {
        "anthropic-messages" => "anthropic-messages",
        "openai-responses" => "openai-responses",
        "openrouter" | "openrouter-openai" => "openrouter-openai",
        _ => "openai-chat",
    }
}

fn parse_mcp(name: &str, spec: &serde_json::Value) -> McpServer {
    let transport = match spec.get("type").and_then(|t| t.as_str()) {
        Some("http") | Some("sse") => McpTransport::Http,
        _ => McpTransport::Stdio,
    };
    let mut env: serde_json::Map<String, serde_json::Value> = Default::default();
    if let Some(headers) = spec.get("headers").and_then(|v| v.as_object()) {
        env.insert("headers".into(), serde_json::Value::Object(headers.clone()));
    }
    if let Some(dt) = spec.get("directTools") {
        env.insert("_direct_tools".into(), dt.clone());
    }
    McpServer {
        id: Uuid::new_v4(),
        name: name.to_string(),
        transport,
        command: spec
            .get("command")
            .and_then(|v| v.as_str())
            .map(String::from),
        args: spec
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        url: spec.get("url").and_then(|v| v.as_str()).map(String::from),
        env,
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "pi-native"}),
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::api_key_env_reference;

    #[test]
    fn only_explicit_env_references_are_imported() {
        assert_eq!(
            api_key_env_reference("$MY_API_KEY"),
            Some("MY_API_KEY".into())
        );
        assert_eq!(
            api_key_env_reference("${MY_API_KEY}"),
            Some("MY_API_KEY".into())
        );
        assert_eq!(api_key_env_reference("MY_API_KEY"), None);
        assert_eq!(api_key_env_reference("!security vault read key"), None);
        assert_eq!(api_key_env_reference("${KEY_PREFIX}_${KEY_SUFFIX}"), None);
    }
}
