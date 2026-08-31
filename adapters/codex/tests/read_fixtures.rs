use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::HarnessAdapter;
use codex_adapter::CodexAdapter;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_home() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.join("fixtures/codex/0.150.0/home")
}

fn sort_entries(value: &mut Value, key: &str) {
    let Some(entries) = value.as_array_mut() else {
        return;
    };
    entries.sort_by(|left, right| {
        let left_key = left
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let right_key = right
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        left_key.cmp(&right_key).then_with(|| {
            serde_json::to_string(left)
                .unwrap_or_default()
                .cmp(&serde_json::to_string(right).unwrap_or_default())
        })
    });
}

fn normalize_fixture_order(value: &mut Value) {
    for (key, sort_key) in [("models", "native_id"), ("providers", "native_provider_id")] {
        if let Some(entries) = value.get_mut(key) {
            sort_entries(entries, sort_key);
        }
    }
    if let Some(entries) = value.get_mut("mcp") {
        sort_entries(entries, "native_name");
    }
}

#[test]
fn codex_full_config_parses_without_warnings() {
    let home = fixture_home();
    let config_file = home.join(".codex/config.toml");
    assert!(config_file.exists(), "fixture missing — run phase 0 first");
    let inst = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Codex,
        executable_path: Some("/fake/codex".into()),
        version: Some("0.150.0".into()),
        config_path: Some(config_file.display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    let state = CodexAdapter.read_state(&inst).expect("read_state ok");
    assert!(state.warnings.is_empty(), "warnings: {:?}", state.warnings);

    let mut expected: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/golden/0.150.0.json").expect("golden missing for 0.150.0"),
    )
    .unwrap();
    let models = state
        .models
        .iter()
        .map(|m| {
            serde_json::json!({
                "native_id": m.native_id,
                "remote_model_id": m.route.remote_model_id,
                "display_name": m.route.display_name,
                "context_window": m.route.context_window,
                "capabilities": m.route.capabilities,
                "overrides": m.route.overrides,
            })
        })
        .collect::<Vec<_>>();
    let providers = state.providers;

    let mcp = state
        .mcp
        .iter()
        .map(|m| {
            serde_json::json!({
                "native_name": m.native_name,
                "command": m.server.command,
                "args": m.server.args,
            })
        })
        .collect::<Vec<_>>();
    let mut actual = serde_json::json!({
        "models": models,
        "providers": providers,
        "mcp": mcp,
        "profiles": state.profiles,
        "skills": state.skills.iter().map(|s| serde_json::json!({
            "name": s.name,
            "symlinked": s.symlinked,
        })).collect::<Vec<_>>(),
    });
    normalize_fixture_order(&mut expected);
    normalize_fixture_order(&mut actual);
    assert_eq!(actual, expected);
}
