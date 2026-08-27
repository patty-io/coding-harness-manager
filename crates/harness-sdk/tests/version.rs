use chm_harness_sdk::detect::version::detect_version;

#[test]
fn parses_semver_from_standard_output() {
    // `cargo --version` prints "cargo 1.97.1 (...)" — guaranteed available on any toolchain machine.
    let v = detect_version("cargo", &["--version"]).expect("cargo must be on PATH");
    assert!(v.split('.').count() >= 2, "expected semver, got {v}");
    assert!(v.chars().next().unwrap().is_ascii_digit());
}

#[test]
fn missing_binary_returns_none() {
    assert_eq!(
        detect_version("/nonexistent/definitely-not-a-binary", &["--version"]),
        None
    );
}

#[test]
fn unknown_output_returns_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let script = dir.path().join("no-version");
    std::fs::write(&script, "#!/bin/sh\nprintf 'hello world'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(detect_version(&script.display().to_string(), &[]), None);
}

#[test]
fn parses_reasonix_v_prefixed_version() {
    // reasonix prints "reasonix v1.31.4" — token after v is 1.31.4
    let dir = tempfile::TempDir::new().unwrap();
    let script = dir.path().join("fake-reasonix");
    std::fs::write(&script, "#!/bin/sh\nprintf 'reasonix v1.31.4\\n'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(
        detect_version(&script.display().to_string(), &[]),
        Some("1.31.4".to_string())
    );
}
