use crate::{ConfigFormat, DetectionSpec};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::helpers::{parse_mcp_json, read_optional, scan_skills_dir};
use chm_harness_sdk::adapter::types::{
    AdapterError, HarnessMcp, HarnessModel, ParsedState, ValidationReport,
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub(crate) fn read_state(
    spec: &DetectionSpec,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<ParsedState, AdapterError> {
    let home = home_for_install(spec, install);
    let config_path = resolve_config_path(spec, install, &home);
    let raw = match read_optional(&config_path)? {
        Some(raw) => Some(raw),
        None if spec.allow_missing_primary => None,
        None => return Err(AdapterError::NotFound(config_path.display().to_string())),
    };

    let mut state = match (spec.id, spec.format, raw.as_deref()) {
        ("kimi-cli", _, Some(raw))
            if matches!(
                config_path
                    .extension()
                    .and_then(|extension| extension.to_str()),
                Some("json" | "jsonc")
            ) =>
        {
            parse_kimi_json(raw, &config_path)?
        }
        ("kimi-cli", ConfigFormat::Toml, Some(raw)) => parse_kimi(raw, &config_path)?,
        ("continue", ConfigFormat::Yaml, Some(raw)) => parse_continue(raw, &config_path)?,
        ("aider", ConfigFormat::Yaml, Some(raw)) => parse_aider(raw, &config_path)?,
        ("goose", ConfigFormat::Yaml, Some(raw)) => parse_goose(raw, &config_path)?,
        (_, ConfigFormat::Json | ConfigFormat::Jsonc, Some(raw)) => {
            parse_json_main(spec, raw, &config_path)?
        }
        (_, _, None) => {
            let mut empty = ParsedState::default();
            empty.warnings.push(format!(
                "{} has no file-backed primary settings; provider profiles may be stored by the host editor",
                spec.id
            ));
            empty
        }
        (_, _, Some(_)) => ParsedState::default(),
    };

    // Some tools keep MCP in a sibling file rather than their primary
    // settings file. Read only known user-scope locations; project overlays
    // are intentionally left for the project-scope adapter pass.
    let mut mcp_paths = Vec::new();
    if config_path.file_name().and_then(|name| name.to_str()) != Some("mcp.json")
        && let Some(sibling) = config_path.parent().map(|parent| parent.join("mcp.json"))
    {
        mcp_paths.push(sibling);
    }
    mcp_paths.extend(
        spec.mcp_rels
            .iter()
            .map(|rel| path_for_rel(spec, &home, rel)),
    );
    let mut visited = std::collections::HashSet::new();
    for path in mcp_paths {
        if path == config_path || !visited.insert(path.clone()) {
            continue;
        }
        let Some(raw) = read_optional(&path)? else {
            continue;
        };
        match parse_json_document(&raw, ConfigFormat::Json, &path) {
            Ok(doc) => parse_mcp_root(&doc, &mut state, &path.display().to_string(), spec.id),
            Err(error) => state.warnings.push(error.to_string()),
        }
    }

    for rel in spec.skill_rels {
        state
            .skills
            .extend(scan_skills_dir(&path_for_rel(spec, &home, rel)));
    }

    // Cline stores its custom model catalog beside providers.json.
    if spec.id == "cline" {
        let models_path = path_for_rel(spec, &home, ".cline/data/settings/models.json");
        if models_path != config_path
            && let Some(raw) = read_optional(&models_path)?
        {
            let doc = parse_json_document(&raw, ConfigFormat::Json, &models_path)?;
            parse_cline_models(&doc, &mut state);
        }
    }

    // Goose custom providers are separate JSON documents and are the durable
    // model registry for Goose.
    if spec.id == "goose" {
        let custom_dir = path_for_rel(spec, &home, ".config/goose/custom_providers");
        if let Ok(entries) = std::fs::read_dir(custom_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let path = entry.path();
                let raw = read_optional(&path)?.unwrap_or_default();
                let doc = parse_json_document(&raw, ConfigFormat::Json, &path)?;
                parse_goose_custom_provider(&doc, &path, &mut state);
            }
        }
    }

    Ok(state)
}

pub(crate) fn validate(
    spec: &DetectionSpec,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Result<ValidationReport, AdapterError> {
    let home = home_for_install(spec, install);
    let config_path = resolve_config_path(spec, install, &home);
    let raw = match std::fs::read_to_string(&config_path) {
        Ok(raw) => raw,
        Err(error) => {
            return Ok(ValidationReport {
                ok: spec.allow_missing_primary,
                errors: if spec.allow_missing_primary {
                    vec![]
                } else {
                    vec![format!("cannot read {}: {error}", config_path.display())]
                },
            });
        }
    };
    match parse_by_format(spec.format, &raw, &config_path) {
        Ok(_) => Ok(ValidationReport {
            ok: true,
            errors: vec![],
        }),
        Err(error) => Ok(ValidationReport {
            ok: false,
            errors: vec![error.to_string()],
        }),
    }
}

pub(crate) fn home_for_install(
    spec: &DetectionSpec,
    install: &chm_core::domain::harness::HarnessInstallation,
) -> PathBuf {
    let Some(raw_path) = install.config_path.as_deref() else {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
    };
    let path = Path::new(raw_path);
    // Extension settings are stored below VS Code's globalStorage tree. The
    // dot-dir heuristic cannot derive a home from that path, so prefer the
    // process home (the scanner's home on normal installs) and fall back to a
    // platform-shaped ancestor only when no environment value is available.
    if raw_path.contains("globalStorage") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home);
        }
        for marker in ["Library", ".config", "AppData"] {
            if let Some(ancestor) = path.ancestors().find(|candidate| {
                candidate.file_name().and_then(|name| name.to_str()) == Some(marker)
            }) && let Some(parent) = ancestor.parent()
            {
                return parent.to_path_buf();
            }
        }
    }
    let dot = Path::new(spec.dot_dir);
    for ancestor in path.ancestors() {
        if ancestor.ends_with(dot)
            || (spec.id == "kimi-cli"
                && (ancestor.file_name().and_then(|name| name.to_str()) == Some(".kimi")
                    || ancestor.file_name().and_then(|name| name.to_str()) == Some(".kimi-code")))
        {
            return ancestor.parent().map(Path::to_path_buf).unwrap_or_default();
        }
    }
    path.parent()
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_default()
}

pub(crate) fn resolve_config_path(
    spec: &DetectionSpec,
    install: &chm_core::domain::harness::HarnessInstallation,
    home: &Path,
) -> PathBuf {
    if let Some(raw_path) = install.config_path.as_deref() {
        let path = PathBuf::from(raw_path);
        if path.is_file() {
            return path;
        }
        if path.is_dir() {
            let rel = Path::new(spec.config_rel);
            if let Ok(within_home) = rel.strip_prefix(spec.dot_dir) {
                let candidate = path.join(within_home);
                if candidate.exists() {
                    return candidate;
                }
            }
            if let Some(file) = rel.file_name() {
                let candidate = path.join(file);
                if candidate.exists() {
                    return candidate;
                }
            }
        } else if path.extension().is_some() {
            return path;
        }
    }

    let mut candidates = vec![spec.config_rel];
    candidates.extend(spec.alternate_config_rels.iter().copied());
    candidates
        .into_iter()
        .map(|rel| path_for_rel(spec, home, rel))
        .find(|path| path.is_file())
        .unwrap_or_else(|| path_for_rel(spec, home, spec.config_rel))
}

/// Resolve a documented relative location, honoring the harness-specific
/// environment overrides used by Kimi Code and Cline, plus `%APPDATA%` for
/// VS Code extension globalStorage on Windows.
pub(crate) fn path_for_rel(spec: &DetectionSpec, home: &Path, rel: &str) -> PathBuf {
    if spec.id == "kimi-cli"
        && let Some(base) = std::env::var_os("KIMI_CODE_HOME")
    {
        let base = PathBuf::from(base);
        if let Some(stripped) = rel.strip_prefix(".kimi-code/") {
            return base.join(stripped);
        }
        if let Some(stripped) = rel.strip_prefix(".kimi/") {
            return base.join(stripped);
        }
    }
    if spec.id == "cline"
        && rel == ".cline/data/settings/providers.json"
        && let Some(path) = std::env::var_os("CLINE_PROVIDER_SETTINGS_PATH")
    {
        return PathBuf::from(path);
    }
    if spec.id == "cline"
        && rel == ".cline/data/settings/global-settings.json"
        && let Some(path) = std::env::var_os("CLINE_GLOBAL_SETTINGS_PATH")
    {
        return PathBuf::from(path);
    }
    if spec.id == "cline"
        && rel == ".cline/data/settings/models.json"
        && let Some(provider_path) = std::env::var_os("CLINE_PROVIDER_SETTINGS_PATH")
        && let Some(parent) = Path::new(&provider_path).parent()
    {
        return parent.join("models.json");
    }
    if spec.id == "cline"
        && (rel == ".cline/data/settings/cline_mcp_settings.json" || rel == ".cline/mcp.json")
        && let Some(path) = std::env::var_os("CLINE_MCP_SETTINGS_PATH")
    {
        return PathBuf::from(path);
    }
    if spec.id == "cline"
        && let Some(base) = std::env::var_os("CLINE_DATA_DIR")
    {
        let base = PathBuf::from(base);
        if let Some(stripped) = rel.strip_prefix(".cline/data/") {
            return base.join(stripped);
        }
    }
    if spec.id == "cursor"
        && let Some(base) = std::env::var_os("CURSOR_CONFIG_DIR")
    {
        let base = PathBuf::from(base);
        if let Some(stripped) = rel.strip_prefix(".cursor/") {
            return base.join(stripped);
        }
    }
    if spec.id == "cursor"
        && let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        && let Some(stripped) = rel.strip_prefix(".cursor/")
    {
        return PathBuf::from(xdg).join("cursor").join(stripped);
    }
    if spec.id == "goose"
        && let Some(appdata) = std::env::var_os("APPDATA")
        && let Some(stripped) = rel.strip_prefix(".config/goose/")
    {
        return PathBuf::from(appdata)
            .join("Block/goose/config")
            .join(stripped);
    }
    if spec.id == "goose"
        && let Some(appdata) = std::env::var_os("APPDATA")
        && let Some(stripped) = rel.strip_prefix("APPDATA/")
    {
        return PathBuf::from(appdata).join(stripped);
    }
    if (spec.id == "roo-code" || spec.id == "cline")
        && rel.starts_with("Code/User/")
        && let Some(appdata) = std::env::var_os("APPDATA")
    {
        return PathBuf::from(appdata).join(rel);
    }
    home.join(rel)
}

fn parse_by_format(format: ConfigFormat, raw: &str, path: &Path) -> Result<Value, AdapterError> {
    let format = match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => ConfigFormat::Json,
        Some("jsonc") => ConfigFormat::Jsonc,
        Some("toml") => ConfigFormat::Toml,
        Some("yml") | Some("yaml") => ConfigFormat::Yaml,
        _ => format,
    };
    match format {
        ConfigFormat::Json | ConfigFormat::Jsonc => parse_json_document(raw, format, path),
        ConfigFormat::Toml => {
            let value: toml::Value = toml::from_str(raw).map_err(|e| AdapterError::Parse {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?;
            serde_json::to_value(value).map_err(|e| AdapterError::Parse {
                path: path.display().to_string(),
                detail: e.to_string(),
            })
        }
        ConfigFormat::Yaml => {
            let value: serde_yaml::Value =
                serde_yaml::from_str(raw).map_err(|e| AdapterError::Parse {
                    path: path.display().to_string(),
                    detail: e.to_string(),
                })?;
            serde_json::to_value(value).map_err(|e| AdapterError::Parse {
                path: path.display().to_string(),
                detail: e.to_string(),
            })
        }
    }
}

fn parse_json_document(
    raw: &str,
    format: ConfigFormat,
    path: &Path,
) -> Result<Value, AdapterError> {
    if format == ConfigFormat::Jsonc || path.extension().and_then(|e| e.to_str()) == Some("jsonc") {
        return serde_json::from_reader(json_comments::StripComments::new(raw.as_bytes())).map_err(
            |e| AdapterError::Parse {
                path: path.display().to_string(),
                detail: e.to_string(),
            },
        );
    }
    serde_json::from_str(raw).map_err(|e| AdapterError::Parse {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

fn parse_kimi_json(raw: &str, path: &Path) -> Result<ParsedState, AdapterError> {
    let doc = parse_json_document(raw, ConfigFormat::Json, path)?;
    let mut state = ParsedState::default();
    if let Some(providers) = doc.get("providers").and_then(Value::as_object) {
        for (provider_id, provider) in providers {
            let Some(object) = provider.as_object() else {
                state
                    .warnings
                    .push(format!("Kimi provider {provider_id} is not an object"));
                continue;
            };
            let mut safe = Map::new();
            safe.insert(
                "native_provider_id".into(),
                Value::String(provider_id.clone()),
            );
            for key in ["type", "base_url", "baseUrl", "protocol"] {
                if let Some(value) = object.get(key) {
                    safe.insert(key.into(), redact_value(value, Some(key)));
                }
            }
            if let Some(env) = object.get("env").and_then(Value::as_object) {
                safe.insert(
                    "env_keys".into(),
                    Value::Array(env.keys().cloned().map(Value::String).collect()),
                );
            }
            if contains_sensitive_key(provider) {
                safe.insert("credential_configured".into(), Value::Bool(true));
            }
            state.providers.push(Value::Object(safe));
        }
    }
    if let Some(models) = doc.get("models").and_then(Value::as_object) {
        for (alias, model) in models {
            let Some(object) = model.as_object() else {
                state
                    .warnings
                    .push(format!("Kimi model {alias} is not an object"));
                continue;
            };
            let provider = object.get("provider").and_then(Value::as_str);
            let wire_model = object.get("model").and_then(Value::as_str);
            if provider.is_none() || wire_model.is_none() {
                state.warnings.push(format!(
                    "Kimi model {alias} is missing provider or model and was skipped"
                ));
                continue;
            }
            let display = object
                .get("display_name")
                .or_else(|| object.get("displayName"))
                .and_then(Value::as_str)
                .unwrap_or(alias);
            let mut route = ModelRoute::new(
                alias.clone(),
                display.to_string(),
                object
                    .get("max_context_size")
                    .or_else(|| object.get("maxContextSize"))
                    .and_then(Value::as_i64),
                serde_json::json!({"wire_model": wire_model, "provider": provider}),
                serde_json::json!({
                    "native_provider_id": provider,
                    "wire_model": wire_model,
                    "config_format": "kimi-json",
                }),
            );
            route.max_input = object
                .get("max_input_size")
                .or_else(|| object.get("maxInputSize"))
                .and_then(Value::as_i64);
            route.max_output = object
                .get("max_output_size")
                .or_else(|| object.get("maxOutputSize"))
                .and_then(Value::as_i64);
            state.models.push(HarnessModel {
                native_id: alias.clone(),
                route,
            });
        }
    }
    if let Some(default_model) = doc.get("default_model").or_else(|| doc.get("defaultModel")) {
        state.profiles.push(serde_json::json!({
            "default_model": default_model,
            "source": "config.json"
        }));
    }
    Ok(state)
}

fn parse_kimi(raw: &str, path: &Path) -> Result<ParsedState, AdapterError> {
    let doc: toml::Value = toml::from_str(raw).map_err(|e| AdapterError::Parse {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    let mut state = ParsedState::default();
    if let Some(providers) = doc.get("providers").and_then(toml::Value::as_table) {
        for (provider_id, provider) in providers {
            let Some(table) = provider.as_table() else {
                state
                    .warnings
                    .push(format!("Kimi provider {provider_id} is not a table"));
                continue;
            };
            let mut safe = Map::new();
            safe.insert(
                "native_provider_id".into(),
                Value::String(provider_id.clone()),
            );
            for key in ["type", "base_url", "protocol"] {
                if let Some(value) = table.get(key) {
                    safe.insert(key.into(), toml_to_json(value));
                }
            }
            if let Some(env) = table.get("env").and_then(toml::Value::as_table) {
                safe.insert(
                    "env_keys".into(),
                    Value::Array(env.keys().cloned().map(Value::String).collect()),
                );
            }
            if table.contains_key("api_key") || table.contains_key("oauth") {
                safe.insert("credential_configured".into(), Value::Bool(true));
            }
            state.providers.push(Value::Object(safe));
        }
    }
    if let Some(models) = doc.get("models").and_then(toml::Value::as_table) {
        for (alias, model) in models {
            let Some(table) = model.as_table() else {
                state
                    .warnings
                    .push(format!("Kimi model {alias} is not a table"));
                continue;
            };
            let provider = table.get("provider").and_then(toml::Value::as_str);
            let wire_model = table.get("model").and_then(toml::Value::as_str);
            if provider.is_none() || wire_model.is_none() {
                state.warnings.push(format!(
                    "Kimi model {alias} is missing provider or model and was skipped"
                ));
                continue;
            }
            let display = table
                .get("display_name")
                .and_then(toml::Value::as_str)
                .unwrap_or(alias);
            let mut route = ModelRoute::new(
                alias.clone(),
                display.to_string(),
                table
                    .get("max_context_size")
                    .and_then(toml::Value::as_integer),
                serde_json::json!({
                    "wire_model": wire_model,
                    "provider": provider,
                }),
                serde_json::json!({
                    "native_provider_id": provider,
                    "wire_model": wire_model,
                    "config_format": "kimi-toml",
                }),
            );
            route.max_input = table
                .get("max_input_size")
                .and_then(toml::Value::as_integer);
            route.max_output = table
                .get("max_output_size")
                .and_then(toml::Value::as_integer);
            state.models.push(HarnessModel {
                native_id: alias.clone(),
                route,
            });
        }
    }
    if let Some(default_model) = doc.get("default_model") {
        state.profiles.push(serde_json::json!({
            "default_model": default_model,
            "source": "config.toml"
        }));
    }
    if let Some(mcp) = doc.get("mcp_servers") {
        parse_mcp_toml(mcp, &mut state, "kimi-cli");
    }
    Ok(state)
}

fn parse_continue(raw: &str, path: &Path) -> Result<ParsedState, AdapterError> {
    let doc = parse_by_format(ConfigFormat::Yaml, raw, path)?;
    let mut state = ParsedState::default();
    if let Some(models) = doc.get("models").and_then(Value::as_array) {
        for model in models {
            let Some(object) = model.as_object() else {
                continue;
            };
            let native_id = object
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| object.get("model").and_then(Value::as_str));
            let Some(native_id) = native_id else {
                state
                    .warnings
                    .push("Continue model without name/model skipped".into());
                continue;
            };
            let wire_model = object
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(native_id);
            let protocol_provider = object
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("openai");
            let display = object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(native_id);
            let context = object
                .get("defaultCompletionOptions")
                .and_then(Value::as_object)
                .and_then(|o| o.get("contextLength"))
                .and_then(Value::as_i64);
            let mut route = ModelRoute::new(
                wire_model.to_string(),
                display.to_string(),
                context,
                serde_json::json!({
                    "provider": protocol_provider,
                    "api_base": object.get("apiBase"),
                    "wire_model": wire_model,
                }),
                serde_json::json!({
                    "native_provider_id": protocol_provider,
                    "provider": protocol_provider,
                    "native_alias": native_id,
                    "wire_model": wire_model,
                    "base_url": object.get("apiBase"),
                    "config_format": "continue-yaml",
                }),
            );
            route.max_output = object
                .get("defaultCompletionOptions")
                .and_then(Value::as_object)
                .and_then(|o| o.get("maxTokens"))
                .and_then(Value::as_i64);
            state.models.push(HarnessModel {
                native_id: native_id.to_string(),
                route,
            });
        }
    }
    if let Some(mcp) = doc.get("mcpServers").and_then(Value::as_array) {
        parse_continue_mcp(mcp, &mut state);
    }
    state.profiles.push(serde_json::json!({
        "name": doc.get("name"),
        "version": doc.get("version"),
        "schema": doc.get("schema"),
    }));
    Ok(state)
}

fn parse_aider(raw: &str, path: &Path) -> Result<ParsedState, AdapterError> {
    let doc = parse_by_format(ConfigFormat::Yaml, raw, path)?;
    let mut state = ParsedState::default();
    let Some(object) = doc.as_object() else {
        return Ok(state);
    };
    for (key, role) in [
        ("model", "main"),
        ("weak-model", "weak"),
        ("editor-model", "editor"),
    ] {
        if let Some(model) = object.get(key).and_then(Value::as_str) {
            state.profiles.push(serde_json::json!({
                "role": role,
                "model": model,
                "source": ".aider.conf.yml",
            }));
        }
    }

    // Aider has one active model (the `model` option) and uses a provider
    // prefix to select the LiteLLM implementation. Treat that selection as a
    // real harness model so CHM can round-trip a custom OpenAI-compatible
    // endpoint instead of exposing it as profile-only metadata.
    if let Some(model_value) = object.get("model").and_then(Value::as_str) {
        let (provider, wire_model) = aider_provider_and_model(model_value);
        let base_url = object
            .get("openai-api-base")
            .and_then(Value::as_str)
            .or_else(|| object.get("openai_api_base").and_then(Value::as_str));
        let metadata = aider_model_metadata(path, model_value, wire_model);
        let mut overrides = serde_json::json!({
            "native_provider_id": provider,
            "wire_model": wire_model,
            "config_format": "aider-yaml",
        });
        if let Some(base_url) = base_url {
            overrides["base_url"] = Value::String(base_url.into());
        }
        let mut route = ModelRoute::new(
            wire_model.to_string(),
            wire_model.to_string(),
            metadata
                .as_ref()
                .and_then(|value| value.get("max_input_tokens"))
                .and_then(Value::as_i64),
            serde_json::json!({
                "provider": provider,
                "wire_model": wire_model,
                "base_url": base_url,
            }),
            overrides,
        );
        route.max_input = metadata
            .as_ref()
            .and_then(|value| value.get("max_input_tokens"))
            .and_then(Value::as_i64);
        route.max_output = metadata
            .as_ref()
            .and_then(|value| {
                value
                    .get("max_output_tokens")
                    .or_else(|| value.get("max_tokens"))
            })
            .and_then(Value::as_i64);
        if let Some(metadata) = metadata {
            route.capabilities = metadata;
        }
        state.models.push(HarnessModel {
            native_id: wire_model.to_string(),
            route,
        });
    }
    if let Some(aliases) = object.get("alias") {
        state.profiles.push(serde_json::json!({
            "aliases": redact_value(aliases, None),
            "source": ".aider.conf.yml",
        }));
    }
    Ok(state)
}

fn aider_provider_and_model(model: &str) -> (&str, &str) {
    if let Some((provider, wire)) = model.split_once('/') {
        match provider.to_ascii_lowercase().as_str() {
            "anthropic" => ("anthropic", wire),
            "openrouter" => ("openrouter", wire),
            "openai" => ("openai", wire),
            _ => ("openai", model),
        }
    } else {
        ("openai", model)
    }
}

fn aider_model_metadata(path: &Path, model: &str, wire_model: &str) -> Option<Value> {
    let metadata_path = path
        .parent()
        .map(|parent| parent.join(".aider.model.metadata.json"))?;
    let raw = std::fs::read_to_string(metadata_path).ok()?;
    let doc: Value = serde_json::from_str(&raw).ok()?;
    doc.get(model)
        .or_else(|| doc.get(wire_model))
        .filter(|value| value.is_object())
        .cloned()
}

fn parse_goose(raw: &str, path: &Path) -> Result<ParsedState, AdapterError> {
    let doc = parse_by_format(ConfigFormat::Yaml, raw, path)?;
    let mut state = ParsedState::default();
    let legacy_provider = doc
        .get("provider")
        .and_then(Value::as_str)
        .or_else(|| doc.get("GOOSE_PROVIDER").and_then(Value::as_str));
    let legacy_model = doc
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| doc.get("GOOSE_MODEL").and_then(Value::as_str));
    let active_provider = doc
        .get("active_provider")
        .and_then(Value::as_str)
        .or(legacy_provider);

    // Current Goose config keeps provider-specific settings under a map and
    // selects one with `active_provider`.  Keep the provider metadata safe
    // (never copy API keys) and expose the selected model as profile metadata;
    // the durable model registry, when present, is parsed from custom
    // provider JSON files below rather than invented from this selection.
    if let Some(providers) = doc.get("providers").and_then(Value::as_object) {
        for (provider_id, provider) in providers {
            let Some(object) = provider.as_object() else {
                continue;
            };
            let mut safe = Map::new();
            safe.insert(
                "native_provider_id".into(),
                Value::String(provider_id.clone()),
            );
            for key in [
                "name",
                "display_name",
                "engine",
                "provider",
                "base_url",
                "api_key_env",
                "model",
            ] {
                if let Some(value) = object.get(key) {
                    safe.insert(key.into(), redact_value(value, Some(key)));
                }
            }
            if contains_sensitive_key(provider) {
                safe.insert("credential_configured".into(), Value::Bool(true));
            }
            state.providers.push(Value::Object(safe));
        }
    }

    let selected_model = active_provider
        .and_then(|provider_id| {
            doc.get("providers")
                .and_then(Value::as_object)
                .and_then(|providers| providers.get(provider_id))
                .and_then(|provider| provider.get("model"))
                .and_then(Value::as_str)
        })
        .or(legacy_model);
    if active_provider.is_some() || selected_model.is_some() {
        state.profiles.push(serde_json::json!({
            "provider": active_provider,
            "model": selected_model,
            "source": "config.yaml",
        }));
    }
    if let Some(extensions) = doc.get("extensions") {
        state.profiles.push(serde_json::json!({
            "extensions": redact_value(extensions, None),
            "source": "config.yaml",
        }));
    }
    Ok(state)
}

fn parse_json_main(
    spec: &DetectionSpec,
    raw: &str,
    path: &Path,
) -> Result<ParsedState, AdapterError> {
    let doc = parse_by_format(spec.format, raw, path)?;
    let mut state = ParsedState::default();
    parse_mcp_root(&doc, &mut state, &path.display().to_string(), spec.id);
    match spec.id {
        "gemini-cli" => {
            if let Some(configs) = doc.get("modelConfigs").and_then(Value::as_object) {
                for (alias, config) in configs {
                    let model = config
                        .get("modelConfig")
                        .and_then(Value::as_object)
                        .and_then(|o| o.get("model"))
                        .and_then(Value::as_str)
                        .or_else(|| config.get("model").and_then(Value::as_str));
                    let Some(model) = model else { continue };
                    let route = ModelRoute::new(
                        alias.clone(),
                        alias.clone(),
                        None,
                        serde_json::json!({"wire_model": model, "source": "modelConfigs"}),
                        serde_json::json!({"config_format": "gemini-json", "wire_model": model}),
                    );
                    state.models.push(HarnessModel {
                        native_id: alias.clone(),
                        route,
                    });
                }
            }
            if let Some(model) = doc
                .get("model")
                .and_then(Value::as_object)
                .and_then(|o| o.get("name"))
                .and_then(Value::as_str)
            {
                state.profiles.push(serde_json::json!({"model": model}));
            }
        }
        "qwen-code" => {
            // Current Qwen Code exposes a real model registry under
            // modelProviders. Custom provider ids are mapped to a wire
            // protocol through providerProtocol.
            if let Some(provider_models) = doc.get("modelProviders").and_then(Value::as_object) {
                let protocols = doc.get("providerProtocol").and_then(Value::as_object);
                for (provider_id, models) in provider_models {
                    let protocol = protocols
                        .and_then(|entries| entries.get(provider_id))
                        .and_then(Value::as_str);
                    let Some(models) = models.as_array() else {
                        continue;
                    };
                    for model in models {
                        let Some(object) = model.as_object() else {
                            continue;
                        };
                        let Some(id) = object.get("id").and_then(Value::as_str) else {
                            continue;
                        };
                        let display = object.get("name").and_then(Value::as_str).unwrap_or(id);
                        let context = object
                            .get("generationConfig")
                            .and_then(Value::as_object)
                            .and_then(|config| config.get("contextWindowSize"))
                            .and_then(Value::as_i64);
                        let max_output = object
                            .get("generationConfig")
                            .and_then(Value::as_object)
                            .and_then(|config| config.get("samplingParams"))
                            .and_then(Value::as_object)
                            .and_then(|sampling| sampling.get("max_tokens"))
                            .and_then(Value::as_i64);
                        let mut overrides = serde_json::json!({
                            "native_provider_id": provider_id,
                            "wire_model": id,
                            "config_format": "qwen-json",
                        });
                        if let Some(base_url) = object.get("baseUrl") {
                            overrides["base_url"] = base_url.clone();
                        }
                        if let Some(env_key) = object.get("envKey") {
                            overrides["env_key"] = env_key.clone();
                        }
                        if let Some(protocol) = protocol {
                            overrides["provider_protocol"] = Value::String(protocol.into());
                        }
                        let mut route = ModelRoute::new(
                            id.to_string(),
                            display.to_string(),
                            context,
                            serde_json::json!({
                                "provider": provider_id,
                                "wire_model": id,
                                "base_url": object.get("baseUrl"),
                                "env_key": object.get("envKey"),
                                "provider_protocol": protocol,
                            }),
                            overrides,
                        );
                        route.max_output = max_output;
                        state.models.push(HarnessModel {
                            native_id: id.to_string(),
                            route,
                        });
                    }
                }
            }
            if let Some(model) = doc.get("model").and_then(Value::as_str) {
                state.profiles.push(serde_json::json!({"model": model}));
            }
        }
        "cursor" => {
            if let Some(model) = model_setting(&doc) {
                state.profiles.push(serde_json::json!({"model": model}));
            }
        }
        "cline" => {
            let filename = path.file_name().and_then(|name| name.to_str());
            if !matches!(filename, Some("mcp.json" | "cline_mcp_settings.json")) {
                parse_cline_providers(&doc, &mut state);
                parse_cline_models(&doc, &mut state);
            }
        }
        "roo-code" => {
            if let Some(configs) = doc.get("apiConfigs").and_then(Value::as_object) {
                for (name, config) in configs {
                    state.profiles.push(serde_json::json!({
                        "name": name,
                        "config": redact_value(config, None),
                    }));
                }
            } else if !is_mcp_config_path(path) {
                state.warnings.push(
                    "Roo Code provider profiles are normally stored in VS Code SecretStorage; only exported/project JSON is readable".into(),
                );
            }
        }
        "amp" => {
            if let Some(mcp) = doc.get("amp.mcpServers") {
                parse_mcp_map(mcp, &mut state, &path.display().to_string(), spec.id);
            }
            if let Some(amp) = doc.get("amp").and_then(Value::as_object)
                && let Some(mcp) = amp.get("mcpServers")
            {
                parse_mcp_map(mcp, &mut state, &path.display().to_string(), spec.id);
            }
            state.profiles.push(serde_json::json!({
                "settings_file": path.display().to_string(),
            }));
        }
        _ => {}
    }
    Ok(state)
}

fn is_mcp_config_path(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("mcp.json") | Some("mcp_settings.json") | Some("cline_mcp_settings.json")
    )
}

fn model_setting(doc: &Value) -> Option<&str> {
    doc.get("model").and_then(Value::as_str).or_else(|| {
        doc.get("model")
            .and_then(Value::as_object)
            .and_then(|o| o.get("name"))
            .and_then(Value::as_str)
    })
}

fn parse_cline_providers(doc: &Value, state: &mut ParsedState) {
    let Some(providers) = cline_provider_object(doc) else {
        return;
    };
    for (id, value) in providers {
        let Some(object) = value.as_object() else {
            continue;
        };
        let settings = cline_settings_object(value).unwrap_or(object);
        // global-settings.json and other host files are occasionally selected
        // as the only available Cline document. Do not turn every arbitrary
        // nested settings object into a provider row.
        if !settings.keys().any(|key| {
            matches!(
                key.as_str(),
                "apiKey" | "apiProvider" | "provider" | "baseUrl" | "model" | "models" | "protocol"
            )
        }) {
            continue;
        }
        let mut safe = Map::new();
        safe.insert("native_provider_id".into(), Value::String(id.clone()));
        for key in [
            "name",
            "provider",
            "apiProvider",
            "baseUrl",
            "openAiBaseUrl",
            "openAiModelId",
            "protocol",
            "model",
        ] {
            if let Some(value) = settings.get(key) {
                safe.insert(key.into(), redact_value(value, Some(key)));
            }
        }
        if contains_sensitive_key(value) {
            safe.insert("credential_configured".into(), Value::Bool(true));
        }
        state.providers.push(Value::Object(safe));
    }
    if let Some(last_provider) = doc.get("lastUsedProvider").and_then(Value::as_str) {
        let model = providers
            .get(last_provider)
            .and_then(cline_settings_object)
            .and_then(|settings| settings.get("model"))
            .and_then(Value::as_str);
        state.profiles.push(serde_json::json!({
            "provider": last_provider,
            "model": model,
            "source": "providers.json",
        }));
    }
}

fn parse_cline_models(doc: &Value, state: &mut ParsedState) {
    let Some(providers) = cline_provider_object(doc) else {
        return;
    };
    for (provider_id, value) in providers {
        let models = value
            .as_object()
            .and_then(|object| object.get("models"))
            .or_else(|| value.as_array().map(|_| value));
        let Some(models) = models else { continue };
        match models {
            Value::Array(models) => {
                for model in models {
                    let (id, display, context, max_output) = match model {
                        Value::String(id) => (id.as_str(), id.as_str(), None, None),
                        Value::Object(obj) => {
                            let id = obj
                                .get("id")
                                .or_else(|| obj.get("name"))
                                .or_else(|| obj.get("model"))
                                .and_then(Value::as_str);
                            let Some(id) = id else { continue };
                            let display = obj.get("name").and_then(Value::as_str).unwrap_or(id);
                            let context = obj
                                .get("contextWindow")
                                .or_else(|| obj.get("context_window"))
                                .and_then(Value::as_i64);
                            let max_output = obj
                                .get("maxTokens")
                                .or_else(|| obj.get("max_tokens"))
                                .and_then(Value::as_i64);
                            (id, display, context, max_output)
                        }
                        _ => continue,
                    };
                    push_cline_model(state, provider_id, id, display, context, max_output);
                }
            }
            Value::Object(models) => {
                // Current Cline models.json stores a map keyed by model id;
                // metadata entries do not repeat the id in their object.
                for (id, model) in models {
                    let (display, context, max_output) = model
                        .as_object()
                        .map(|object| {
                            (
                                object
                                    .get("name")
                                    .or_else(|| object.get("displayName"))
                                    .and_then(Value::as_str)
                                    .unwrap_or(id)
                                    .to_string(),
                                object
                                    .get("contextWindow")
                                    .or_else(|| object.get("context_window"))
                                    .and_then(Value::as_i64),
                                object
                                    .get("maxTokens")
                                    .or_else(|| object.get("max_tokens"))
                                    .and_then(Value::as_i64),
                            )
                        })
                        .unwrap_or_else(|| (id.clone(), None, None));
                    push_cline_model(state, provider_id, id, &display, context, max_output);
                }
            }
            _ => {}
        }
    }
}

fn cline_provider_object(doc: &Value) -> Option<&serde_json::Map<String, Value>> {
    let root = doc.as_object()?;
    root.get("providers")
        .and_then(Value::as_object)
        .or(Some(root))
}

/// Current Cline provider settings are wrapped as
/// `{ "settings": { ... }, "updatedAt": ..., "tokenSource": ... }`.
/// Older exports put provider fields directly on the entry; accept both
/// shapes while keeping timestamps/token sources out of the normalized row.
fn cline_settings_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    let object = value.as_object()?;
    object
        .get("settings")
        .and_then(Value::as_object)
        .or(Some(object))
}

fn push_cline_model(
    state: &mut ParsedState,
    provider_id: &str,
    id: &str,
    display: &str,
    context: Option<i64>,
    max_output: Option<i64>,
) {
    let mut route = ModelRoute::new(
        id.to_string(),
        display.to_string(),
        context,
        serde_json::json!({"provider": provider_id}),
        serde_json::json!({
            "native_provider_id": provider_id,
            "config_format": "cline-json",
        }),
    );
    route.max_output = max_output;
    state.models.push(HarnessModel {
        native_id: id.to_string(),
        route,
    });
}

fn parse_goose_custom_provider(doc: &Value, path: &Path, state: &mut ParsedState) {
    let Some(object) = doc.as_object() else {
        return;
    };
    let Some(id) = object
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| path.file_stem().and_then(|s| s.to_str()))
    else {
        state.warnings.push(format!(
            "Goose custom provider {} has no name and was skipped",
            path.display()
        ));
        return;
    };
    let mut provider = Map::new();
    provider.insert("native_provider_id".into(), Value::String(id.into()));
    for key in ["name", "engine", "display_name", "base_url", "api_key_env"] {
        if let Some(value) = object.get(key) {
            provider.insert(key.into(), redact_value(value, Some(key)));
        }
    }
    state.providers.push(Value::Object(provider));
    if let Some(models) = object.get("models").and_then(Value::as_array) {
        for model in models {
            let (model_id, context) = match model {
                Value::String(id) => (id.to_string(), None),
                Value::Object(obj) => {
                    let Some(id) = obj.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    (
                        id.to_string(),
                        obj.get("context_limit").and_then(Value::as_i64),
                    )
                }
                _ => continue,
            };
            let display_name = model
                .as_object()
                .and_then(|obj| obj.get("alias").or_else(|| obj.get("subtext")))
                .and_then(Value::as_str)
                .filter(|display| !display.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("{id}/{model_id}"));
            let route = ModelRoute::new(
                model_id.clone(),
                display_name,
                context,
                serde_json::json!({"provider": id}),
                serde_json::json!({
                    "native_provider_id": id,
                    "config_file": path.display().to_string(),
                    "config_format": "goose-provider-json",
                }),
            );
            state.models.push(HarnessModel {
                native_id: model_id,
                route,
            });
        }
    }
}

fn parse_mcp_root(doc: &Value, state: &mut ParsedState, source: &str, harness_id: &str) {
    if let Some(mcp) = doc.get("mcpServers") {
        parse_mcp_map(mcp, state, source, harness_id);
    }
}

fn parse_mcp_map(value: &Value, state: &mut ParsedState, source: &str, harness_id: &str) {
    let Some(mcp) = value.as_object() else {
        return;
    };
    for (name, raw_spec) in mcp {
        let mut spec = sanitize_mcp(raw_spec);
        // Kimi's documented `url` shape is HTTP by default; only an explicit
        // `transport: "sse"` denotes the legacy SSE protocol. Other clients
        // (Gemini, Continue, etc.) historically use bare `url` for SSE.
        if matches!(harness_id, "kimi-cli" | "amp")
            && spec.get("url").is_some()
            && spec.get("command").is_none()
            && spec.get("type").is_none()
            && spec.get("transport").is_none()
            && let Some(object) = spec.as_object_mut()
        {
            // Kimi documents URL-only entries as HTTP. Amp likewise uses
            // URL-only entries and negotiates modern Streamable HTTP first;
            // explicit `transport: "sse"` remains available for legacy URLs.
            object.insert("transport".into(), Value::String("http".into()));
        }
        state.mcp.push(HarnessMcp {
            native_name: name.clone(),
            server: parse_mcp_json(name, &spec, serde_json::json!({"source": source})),
        });
    }
}

fn parse_mcp_toml(value: &toml::Value, state: &mut ParsedState, harness_id: &str) {
    let Some(mcp) = value.as_table() else {
        return;
    };
    for (name, raw_spec) in mcp {
        let mut spec = toml_to_json(raw_spec);
        if matches!(harness_id, "kimi-cli" | "amp")
            && spec.get("url").is_some()
            && spec.get("command").is_none()
            && spec.get("type").is_none()
            && spec.get("transport").is_none()
            && let Some(object) = spec.as_object_mut()
        {
            object.insert("transport".into(), Value::String("http".into()));
        }
        state.mcp.push(HarnessMcp {
            native_name: name.clone(),
            server: parse_mcp_json(
                name,
                &sanitize_mcp(&spec),
                serde_json::json!({"source": "kimi-config.toml"}),
            ),
        });
    }
}

fn parse_continue_mcp(values: &[Value], state: &mut ParsedState) {
    for value in values {
        let Some(obj) = value.as_object() else {
            continue;
        };
        let Some(name) = obj.get("name").and_then(Value::as_str) else {
            continue;
        };
        state.mcp.push(HarnessMcp {
            native_name: name.to_string(),
            server: parse_mcp_json(
                name,
                &sanitize_mcp(value),
                serde_json::json!({"source": "continue-config.yaml"}),
            ),
        });
    }
}

fn sanitize_mcp(value: &Value) -> Value {
    redact_value(value, None)
}

fn redact_value(value: &Value, key: Option<&str>) -> Value {
    if key.is_some_and(is_sensitive_key) {
        return match value {
            Value::Null => Value::Null,
            Value::Bool(_) => Value::Bool(true),
            _ => Value::String("<redacted>".into()),
        };
    }
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), redact_value(value, Some(key))))
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.iter().map(|v| redact_value(v, None)).collect())
        }
        _ => value.clone(),
    }
}

fn contains_sensitive_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| is_sensitive_key(key) || contains_sensitive_key(value)),
        Value::Array(values) => values.iter().any(contains_sensitive_key),
        _ => false,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "token",
        "secret",
        "password",
        "authorization",
        "access_key",
        "private_key",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn toml_to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(v) => Value::String(v.clone()),
        toml::Value::Integer(v) => Value::Number((*v).into()),
        toml::Value::Float(v) => serde_json::Number::from_f64(*v)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml::Value::Boolean(v) => Value::Bool(*v),
        toml::Value::Datetime(v) => Value::String(v.to_string()),
        toml::Value::Array(v) => Value::Array(v.iter().map(toml_to_json).collect()),
        toml::Value::Table(v) => Value::Object(
            v.iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect(),
        ),
    }
}
