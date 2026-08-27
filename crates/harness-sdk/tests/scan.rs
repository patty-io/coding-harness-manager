use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn scan_finds_tier1_harness_with_executable_and_config() {
    let dir = TempDir::new().unwrap();
    let bindir = dir.path().join("bin");
    let homedir = dir.path().join("home");
    std::fs::create_dir_all(&bindir).unwrap();
    std::fs::create_dir_all(homedir.join(".config/opencode")).unwrap();
    std::fs::write(
        bindir.join("opencode"),
        "#!/bin/sh\nprintf 'opencode 0.30.0\n'\n",
    )
    .unwrap();
    std::fs::write(homedir.join(".config/opencode/opencode.jsonc"), "{}").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            bindir.join("opencode"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let inv = scan(
        Platform::MacOs,
        Some(Path::new(&homedir.display().to_string())),
        Some(&bindir.display().to_string()),
    );
    let oc = inv
        .installations
        .iter()
        .find(|i| i.harness_type.as_str() == "opencode")
        .expect("opencode detected");
    assert_eq!(
        oc.status,
        chm_core::domain::harness::InstallationStatus::Installed
    );
    assert_eq!(oc.version.as_deref(), Some("0.30.0"));
    assert!(
        oc.config_path
            .as_deref()
            .unwrap_or("")
            .contains("opencode.jsonc")
    );
}

#[test]
fn scan_marks_config_missing_when_only_executable_absent() {
    let dir = TempDir::new().unwrap();
    let homedir = dir.path().join("home");
    std::fs::create_dir_all(homedir.join(".claude")).unwrap();
    std::fs::write(homedir.join(".claude/settings.json"), "{}").unwrap();
    let inv = scan(
        Platform::MacOs,
        Some(Path::new(&homedir.display().to_string())),
        Some("/nonexistent"),
    );
    let cc = inv
        .installations
        .iter()
        .find(|i| i.harness_type.as_str() == "claude-code")
        .expect("claude-code present");
    assert_eq!(
        cc.status,
        chm_core::domain::harness::InstallationStatus::ConfigMissing
    );
}

#[test]
fn scan_detects_detection_only_harness_by_executable() {
    let dir = TempDir::new().unwrap();
    let bindir = dir.path().join("bin");
    std::fs::create_dir_all(&bindir).unwrap();
    std::fs::write(bindir.join("gemini"), "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            bindir.join("gemini"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let inv = scan(
        Platform::MacOs,
        Some(Path::new(&dir.path().join("home").display().to_string())),
        Some(&bindir.display().to_string()),
    );
    let gem = inv
        .installations
        .iter()
        .find(|i| i.harness_type.as_str() == "gemini-cli");
    assert!(gem.is_some(), "detection-only harness must be reported");
    assert_eq!(gem.unwrap().status_v(), "detected");
    assert!(
        gem.unwrap().version.is_none(),
        "no version work for detection-only"
    );
}
