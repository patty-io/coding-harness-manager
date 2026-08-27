//! OpenCode native config parser (opencode.json / opencode.jsonc).
//! Shape verified on 1.18.23 — see docs/harnesses/opencode.md.

use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use uuid::Uuid;

pub fn parse_config(raw: &str, config_dir: &std::path::Path) -> Result<ParsedState, AdapterError> {
    // JSONC tolerance: strip // and /* */ comments before parsing
    let clean = json_comments::StripComments::new(raw.as_bytes());
    let json: serde_json::Value =
        serde_json::from_reader(clean).map_err(|e| AdapterError::Parse {
            path: "opencode.jsonc".into(),
            detail: e.to_string(),
        })?;
    let mut state = ParsedState::default();

    // provider.<id>.models.<id> -> ModelRoute
    if let Some(providers) = json.get("provider").and_then(|p| p.as_object()) {
        for (provider_id, pv) in providers {
            let options = pv.get("options");
            let api_key = options
                .and_then(|o| o.get("apiKey"))
                .and_then(|v| v.as_str());
            state.providers.push(serde_json::json!({
                "native_provider_id": provider_id,
                "npm": pv.get("npm"),
                "base_url": options.and_then(|o| o.get("baseURL")),
                "api_key_template": api_key,
                "env_reference": api_key.and_then(|k| k.strip_prefix("{env:").and_then(|s| s.strip_suffix('}'))),
            }));
            if let Some(models) = pv.get("models").and_then(|m| m.as_object()) {
                for (model_id, meta) in models {
                    let display_name = meta
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(model_id)
                        .to_string();
                    // mirror the writer's subtraction: name/limit live in their own
                    // route fields, everything else is capability metadata
                    let mut caps = meta.clone();
                    if let Some(caps_obj) = caps.as_object_mut() {
                        caps_obj.remove("name");
                        caps_obj.remove("limit");
                    }
                    let capabilities = caps;
                    let mut route = ModelRoute::new(
                        model_id.clone(),
                        display_name,
                        meta.get("limit")
                            .and_then(|l| l.get("context"))
                            .and_then(|v| v.as_i64()),
                        capabilities,
                        serde_json::json!({
                            "native_provider_id": provider_id,
                            "native_config": pv.clone(),
                        }),
                    );
                    route.max_output = meta
                        .get("limit")
                        .and_then(|l| l.get("output"))
                        .and_then(|v| v.as_i64());
                    state.models.push(HarnessModel {
                        native_id: model_id.clone(),
                        route,
                    });
                }
            }
        }
    }

    // top-level mcp object (verified: all MCP lives inside opencode.jsonc in 1.18)
    if let Some(mcp) = json.get("mcp").and_then(|m| m.as_object()) {
        for (name, spec) in mcp {
            state.mcp.push(HarnessMcp {
                native_name: name.clone(),
                server: parse_mcp(name, spec),
            });
        }
    }
    let mcp_file = config_dir.join("opencode-mcp.json");
    if mcp_file.exists() {
        let raw_mcp = std::fs::read_to_string(&mcp_file)?;
        let json_mcp: serde_json::Value =
            serde_json::from_str(&raw_mcp).map_err(|e| AdapterError::Parse {
                path: mcp_file.display().to_string(),
                detail: e.to_string(),
            })?;
        if let Some(mcp) = json_mcp.as_object() {
            for (name, spec) in mcp {
                state.mcp.push(HarnessMcp {
                    native_name: name.clone(),
                    server: parse_mcp(name, spec),
                });
            }
        }
    }

    // skills dir
    let skills_dir = config_dir.join("skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let symlinked = entry
                    .path()
                    .symlink_metadata()
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                state
                    .skills
                    .push(chm_harness_sdk::adapter::types::HarnessSkill {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        path: entry.path().display().to_string(),
                        content_hash: None,
                        symlinked,
                    });
            }
        }
    }

    Ok(state)
}

fn parse_mcp(name: &str, spec: &serde_json::Value) -> McpServer {
    let transport = match spec.get("type").and_then(|t| t.as_str()) {
        Some("remote") => McpTransport::Http,
        _ => McpTransport::Stdio,
    };
    // command is an ARRAY in the real format: first element = command, rest = args
    let command_array: Vec<String> = spec
        .get("command")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let (command, args) = match command_array.split_first() {
        Some((first, rest)) => (Some(first.clone()), rest.to_vec()),
        None => (None, vec![]),
    };
    // environment (not env) holds local server env; remote headers preserved losslessly
    let mut env: serde_json::Map<String, serde_json::Value> = spec
        .get("environment")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if let Some(headers) = spec.get("headers").and_then(|v| v.as_object()) {
        env.insert("headers".into(), serde_json::Value::Object(headers.clone()));
    }
    McpServer {
        id: Uuid::new_v4(),
        name: name.to_string(),
        transport,
        command,
        args,
        url: spec.get("url").and_then(|v| v.as_str()).map(String::from),
        env,
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "opencode-native"}),
        enabled: spec
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
    }
}
