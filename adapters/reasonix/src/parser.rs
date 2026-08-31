//! Reasonix native config parser (~/.reasonix/config.toml).
//! Shape verified on 1.31.4 — config_version = 7, [[providers]] array.
//! See docs/harnesses/reasonix.md.

use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessModel, ParsedState};

pub fn parse_config(raw: &str, home: &std::path::Path) -> Result<ParsedState, AdapterError> {
    let toml: toml::Value = toml::from_str(raw).map_err(|e| AdapterError::Parse {
        path: "~/.reasonix/config.toml".into(),
        detail: e.to_string(),
    })?;
    let mut state = ParsedState::default();

    let config_version = toml.get("config_version").and_then(|v| v.as_integer());
    state.providers.push(serde_json::json!({
        "native_provider_id": "__schema__",
        "config_version": config_version,
    }));

    // [[providers]] array: name, kind (openai|anthropic), base_url, models[], default,
    // api_key_env, context_window, max_output_tokens, price
    if let Some(providers) = toml.get("providers").and_then(|p| p.as_array()) {
        for pv in providers {
            let name = pv.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed");
            state.providers.push(serde_json::json!({
                "native_provider_id": name,
                "kind": pv.get("kind"),
                "base_url": pv.get("base_url"),
                "api_key_env": pv.get("api_key_env"),
                "context_window": pv.get("context_window"),
                "max_output_tokens": pv.get("max_output_tokens"),
                "default": pv.get("default"),
            }));
            if let Some(models) = pv.get("models").and_then(|m| m.as_array()) {
                for model_id in models.iter().filter_map(|v| v.as_str()) {
                    let mut route = ModelRoute::new(
                        model_id.to_string(),
                        format!("{name}/{model_id}"),
                        pv.get("context_window").and_then(|v| v.as_integer()),
                        serde_json::json!({
                            "kind": pv.get("kind"),
                            "price": pv.get("price"),
                        }),
                        serde_json::json!({
                            "native_provider_id": name,
                            "api_key_env": pv.get("api_key_env"),
                            "base_url": pv.get("base_url"),
                            "protocol": reasonix_protocol(pv.get("kind").and_then(|v| v.as_str())),
                        }),
                    );
                    route.max_output = pv.get("max_output_tokens").and_then(|v| v.as_integer());
                    state.models.push(HarnessModel {
                        native_id: model_id.to_string(),
                        route,
                    });
                }
            }
        }
    }

    // selection + skills dir
    state.profiles.push(serde_json::json!({
        "default_model": toml.get("default_model"),
        "credentials_store": toml.get("credentials_store"),
    }));
    let skills_dir = home.join(".reasonix/skills");
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

fn reasonix_protocol(kind: Option<&str>) -> &'static str {
    match kind {
        Some("anthropic") => "anthropic-messages",
        _ => "openai-chat",
    }
}
