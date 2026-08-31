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

    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/golden/0.150.0.json").expect("golden missing for 0.150.0"),
    )
    .unwrap();
    let mut models = state
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
    models.sort_by(|left, right| left["native_id"].as_str().cmp(&right["native_id"].as_str()));

    let mut providers = state.providers;
    providers.sort_by(|left, right| {
        left["native_provider_id"]
            .as_str()
            .cmp(&right["native_provider_id"].as_str())
    });

    let mut mcp = state
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
    mcp.sort_by(|left, right| {
        left["native_name"]
            .as_str()
            .cmp(&right["native_name"].as_str())
    });

    let actual = serde_json::json!({
        "models": models,
        "providers": providers,
        "mcp": mcp,
        "profiles": state.profiles,
        "skills": state.skills.iter().map(|s| serde_json::json!({
            "name": s.name,
            "symlinked": s.symlinked,
        })).collect::<Vec<_>>(),
    });
    assert_eq!(actual, expected);
}
