use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::HarnessAdapter;
use reasonix_adapter::ReasonixAdapter;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_home() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.join("fixtures/reasonix/1.31.4/home")
}

#[test]
fn reasonix_full_config_parses_without_warnings() {
    let home = fixture_home();
    let config_file = home.join(".reasonix/config.toml");
    assert!(config_file.exists(), "fixture missing — run phase 0 first");
    let inst = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Reasonix,
        executable_path: Some("/fake/reasonix".into()),
        version: Some("1.31.4".into()),
        config_path: Some(config_file.display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    let state = ReasonixAdapter.read_state(&inst).expect("read_state ok");
    assert!(state.warnings.is_empty(), "warnings: {:?}", state.warnings);

    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/golden/1.31.4.json").expect("golden missing"),
    )
    .unwrap();
    let actual = serde_json::json!({
        "models": state.models.iter().map(|m| serde_json::json!({
            "native_id": m.native_id,
            "remote_model_id": m.route.remote_model_id,
            "display_name": m.route.display_name,
            "context_window": m.route.context_window,
            "max_output": m.route.max_output,
            "overrides": m.route.overrides,
        })).collect::<Vec<_>>(),
        "providers": state.providers,
        "profiles": state.profiles,
        "skills": state.skills.iter().map(|s| serde_json::json!({
            "name": s.name,
            "symlinked": s.symlinked,
        })).collect::<Vec<_>>(),
    });
    // Endpoint metadata is intentionally richer than the original fixture:
    // the adapter now preserves base_url/protocol so cross-adapter sync can
    // match a registry route even when native provider ids differ. Keep the
    // historical golden focused on the public model/provider shape while
    // asserting the new metadata explicitly below.
    for model in &state.models {
        assert!(
            model
                .route
                .overrides
                .get("base_url")
                .and_then(Value::as_str)
                .is_some()
        );
        assert!(
            model
                .route
                .overrides
                .get("protocol")
                .and_then(Value::as_str)
                .is_some()
        );
    }
    let mut actual_without_endpoint_metadata = actual.clone();
    if let Some(models) = actual_without_endpoint_metadata
        .get_mut("models")
        .and_then(Value::as_array_mut)
    {
        for model in models {
            if let Some(overrides) = model.get_mut("overrides").and_then(Value::as_object_mut) {
                overrides.remove("base_url");
                overrides.remove("protocol");
            }
        }
    }
    assert_eq!(actual_without_endpoint_metadata, expected);
}
