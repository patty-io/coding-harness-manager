use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::HarnessAdapter;
use pi_adapter::PiAdapter;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("fixtures/pi");
    p
}

fn install(config_file: PathBuf, _home: PathBuf) -> HarnessInstallation {
    HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Pi,
        executable_path: Some("/fake/pi".into()),
        version: Some("0.84.3".into()),
        config_path: Some(config_file.display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    }
}

#[test]
fn pi_full_config_parses_without_warnings() {
    let dir = fixture_dir();
    let versions: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert!(
        !versions.is_empty(),
        "fixtures/pi is empty — run phase 0 first"
    );

    for version_dir in versions {
        let golden_path = format!(
            "tests/golden/{}.json",
            version_dir.file_name().unwrap().to_string_lossy()
        );
        let home = version_dir.join("home");
        let config_file = home.join(".pi/agent/models.json");
        let adapter = PiAdapter;
        let inst = install(config_file, home.clone());
        let state = adapter.read_state(&inst).expect("read_state ok");
        assert!(state.warnings.is_empty(), "warnings: {:?}", state.warnings);

        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(&golden_path)
                .unwrap_or_else(|_| panic!("golden missing for {golden_path}")),
        )
        .unwrap();
        let actual = serde_json::json!({
            "models": state.models.iter().map(|m| serde_json::json!({
                "native_id": m.native_id,
                "display_name": m.route.display_name,
                "remote_model_id": m.route.remote_model_id,
                "context_window": m.route.context_window,
                "capabilities": m.route.capabilities,
            })).collect::<Vec<_>>(),
            "providers": state.providers,
            "mcp": state.mcp.iter().map(|m| serde_json::json!({
                "native_name": m.native_name,
                "transport": m.server.transport.as_str(),
                "command": m.server.command,
                "args": m.server.args,
                "url": m.server.url,
                "env": m.server.env,
            })).collect::<Vec<_>>(),
            "profiles": state.profiles,
            "skills": state.skills.iter().map(|s| serde_json::json!({
                "name": s.name,
                "path": s.path,
                "symlinked": s.symlinked,
            })).collect::<Vec<_>>(),
        });
        assert_eq!(
            actual,
            expected,
            "golden mismatch for {}",
            version_dir.display()
        );
    }
}
