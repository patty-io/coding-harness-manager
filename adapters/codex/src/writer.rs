//! Codex writer: folds model adds into the per-provider config file.

use chm_harness_sdk::adapter::types::ValidationReport;
use toml_edit::{DocumentMut, Item, Table, Value};

/// Folds one model add into the provider file document.
///
/// This compatibility wrapper is kept for callers that only have the legacy
/// provider fields. New route deployments should use
/// `fold_provider_with_metadata`, which also preserves model limits and the
/// credential strategy.
pub fn fold_provider(
    doc: &mut DocumentMut,
    provider_id: &str,
    model_id: &str,
    base_url: &str,
    env_key: Option<&str>,
    wire_api: &str,
) {
    fold_provider_with_metadata(
        doc,
        provider_id,
        model_id,
        base_url,
        env_key,
        wire_api,
        ProviderMetadata::default(),
    );
}

#[derive(Debug, Clone, Default)]
pub struct ProviderMetadata {
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    pub credential_ref_id: Option<uuid::Uuid>,
}

/// Folds a complete portable route into Codex's documented provider config.
/// Codex stores the selected model and provider metadata in TOML; API keys
/// remain either an existing environment variable or a command-backed token
/// helper, never in this document.
pub fn fold_provider_with_metadata(
    doc: &mut DocumentMut,
    provider_id: &str,
    model_id: &str,
    base_url: &str,
    env_key: Option<&str>,
    wire_api: &str,
    metadata: ProviderMetadata,
) {
    doc["model"] = Item::Value(Value::from(format!("{provider_id}/{model_id}")));
    doc["model_provider"] = Item::Value(Value::from(provider_id));
    if let Some(value) = metadata.context_window {
        doc["model_context_window"] = Item::Value(Value::from(value));
    }
    let mps_key = format!("model_providers.{provider_id}");
    let table = doc
        .entry(&mps_key)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .expect("model_providers.<id> must be a table");
    table["name"] = Item::Value(Value::from(provider_id));
    table["base_url"] = Item::Value(Value::from(base_url));
    table["wire_api"] = Item::Value(Value::from(wire_api));
    if let Some(key) = env_key {
        table["env_key"] = Item::Value(Value::from(key));
        // An existing command hook can otherwise win over the selected env
        // variable. It is safe to remove only the provider's auth selector;
        // unrelated provider fields remain untouched.
        table.remove("auth");
    } else if let Some(credential_ref_id) = metadata.credential_ref_id {
        let auth = table
            .entry("auth")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .expect("model_providers.<id>.auth must be a table");
        let (command, args) =
            chm_harness_sdk::adapter::helpers::credential_helper_invocation(credential_ref_id);
        auth["command"] = Item::Value(Value::from(command));
        let mut values = toml_edit::Array::new();
        for arg in args {
            values.push(arg);
        }
        auth["args"] = Item::Value(Value::Array(values));
    }
}

/// Update the selected model and metadata in one existing provider file.
/// Returns false when the file does not select the requested provider/model.
pub fn update_provider(
    doc: &mut DocumentMut,
    provider_id: &str,
    model_id: &str,
    context_window: Option<i64>,
) -> bool {
    let selected = doc
        .get("model")
        .and_then(Item::as_value)
        .and_then(Value::as_str);
    let selected_provider = doc
        .get("model_provider")
        .and_then(Item::as_value)
        .and_then(Value::as_str);
    let matches = selected_provider.is_some_and(|provider| {
        provider.eq_ignore_ascii_case(provider_id)
            && selected.is_some_and(|value| selected_model_matches(value, provider_id, model_id))
    });
    if !matches {
        return false;
    }
    doc["model_provider"] = Item::Value(Value::from(provider_id));
    if let Some(value) = context_window {
        doc["model_context_window"] = Item::Value(Value::from(value));
    } else {
        doc.remove("model_context_window");
    }
    true
}

/// Whether a profile selects the requested provider/model pair. Codex profile
/// files are also used for named profiles, so callers must inspect every
/// `*.config.toml` rather than assuming the filename is the provider id.
pub fn contains_provider_model(doc: &DocumentMut, provider_id: &str, model_id: &str) -> bool {
    let selected = doc
        .get("model")
        .and_then(Item::as_value)
        .and_then(Value::as_str);
    let provider = doc
        .get("model_provider")
        .and_then(Item::as_value)
        .and_then(Value::as_str);
    selected.is_some_and(|value| {
        provider.is_some_and(|actual| actual.eq_ignore_ascii_case(provider_id))
            && selected_model_matches(value, provider_id, model_id)
    })
}

/// Clear a selected model from a provider file while preserving provider and
/// unrelated Codex settings. Codex falls back to its built-in model when the
/// optional selection keys are absent, so deleting a model does not require
/// deleting the user's provider credentials or file.
pub fn remove_provider(doc: &mut DocumentMut, provider_id: Option<&str>, model_id: &str) -> bool {
    let selected = doc
        .get("model")
        .and_then(Item::as_value)
        .and_then(Value::as_str);
    let selected_provider = doc
        .get("model_provider")
        .and_then(Item::as_value)
        .and_then(Value::as_str);
    let matches = selected.is_some_and(|value| {
        let provider_from_model = value.split('/').next().unwrap_or_default();
        selected_model_matches(value, provider_id.unwrap_or(provider_from_model), model_id)
            && provider_id.is_none_or(|id| {
                selected_provider
                    .map(|provider| provider.eq_ignore_ascii_case(id))
                    .unwrap_or_else(|| provider_from_model.eq_ignore_ascii_case(id))
            })
    });
    if !matches {
        return false;
    }
    doc.remove("model");
    doc.remove("model_provider");
    doc.remove("model_context_window");
    true
}

fn selected_model_matches(value: &str, provider_id: &str, model_id: &str) -> bool {
    value.eq_ignore_ascii_case(&format!("{provider_id}/{model_id}"))
        || (!model_id.contains('/')
            && value
                .rsplit('/')
                .next()
                .is_some_and(|id| id.eq_ignore_ascii_case(model_id)))
}

pub fn validate_config(file_path: &str) -> ValidationReport {
    match std::fs::read_to_string(file_path) {
        Ok(raw) => match raw.parse::<DocumentMut>() {
            Ok(_) => ValidationReport {
                ok: true,
                errors: vec![],
            },
            Err(e) => ValidationReport {
                ok: false,
                errors: vec![format!("config.toml invalid: {e}")],
            },
        },
        Err(e) => ValidationReport {
            ok: false,
            errors: vec![format!("cannot read: {e}")],
        },
    }
}

/// Validates every changed file in a native plan (the files apply will touch).
pub fn validate_changed_files(
    plan: &chm_harness_sdk::adapter::types::NativePlan,
) -> ValidationReport {
    let mut errors = Vec::new();
    for change in &plan.changes {
        let path = &change.file_path;
        if let Some(after) = &change.after
            && after.parse::<DocumentMut>().is_err()
        {
            errors.push(format!("{path}: serialized TOML invalid"));
        }
    }
    ValidationReport {
        ok: errors.is_empty(),
        errors,
    }
}
