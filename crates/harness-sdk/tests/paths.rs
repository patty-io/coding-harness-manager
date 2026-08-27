use chm_harness_sdk::definition::{HarnessDefinition, Platform};
use chm_harness_sdk::detect::paths::{find_executable, resolve_config_path};
use std::path::PathBuf;
use tempfile::TempDir;

fn def_with_config(candidates: &'static [&'static str]) -> HarnessDefinition {
    HarnessDefinition {
        id: "test",
        name: "Test",
        executable_names: &["test-tool"],
        config_paths: candidates,
        skill_paths: &[],
        mcp_paths: &[],
        platforms: &[Platform::MacOs, Platform::Linux, Platform::Windows],
        detection_only: false,
    }
}

#[test]
fn find_executable_walks_path_in_order() {
    let dir = TempDir::new().unwrap();
    let first = dir.path().join("a");
    let second = dir.path().join("b");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("tool"), "#!/bin/sh\n").unwrap();
    std::fs::write(second.join("tool"), "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(first.join("tool"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        std::fs::set_permissions(second.join("tool"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    let path_env = format!("{}:{}", first.display(), second.display());
    assert_eq!(
        find_executable("tool", Some(&path_env)),
        Some(first.join("tool").display().to_string())
    );
}

#[test]
fn find_executable_returns_none_when_missing() {
    let dir = TempDir::new().unwrap();
    let path_env = dir.path().display().to_string();
    assert_eq!(find_executable("does-not-exist", Some(&path_env)), None);
}

#[test]
fn resolve_config_path_picks_first_existing_candidate() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".codex")).unwrap();
    std::fs::write(dir.path().join(".codex/config.toml"), "model = \"gpt-5\"\n").unwrap();
    let def = def_with_config(&[".codex/config.toml", ".codex/mcp.json"]);
    let resolved = resolve_config_path(&def, dir.path(), Platform::MacOs);
    assert_eq!(resolved, Some(dir.path().join(".codex/config.toml")));
}

#[test]
fn resolve_config_path_returns_none_when_nothing_exists() {
    let dir = TempDir::new().unwrap();
    let def = def_with_config(&[".codex/config.toml"]);
    assert_eq!(resolve_config_path(&def, dir.path(), Platform::MacOs), None);
}

#[test]
fn resolve_config_path_falls_back_to_appdata_on_windows() {
    let dir = TempDir::new().unwrap();
    let appdata = dir.path().join("appdata");
    std::fs::create_dir_all(&appdata).unwrap();
    std::fs::write(appdata.join("opencode.jsonc"), "{}").unwrap();
    // .config/opencode/opencode.jsonc does not exist under home; appdata/opencode does
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let def = def_with_config(&[".config/opencode/opencode.jsonc"]);
    let previous = std::env::var_os("APPDATA");
    unsafe {
        std::env::set_var("APPDATA", &appdata);
    }
    let resolved = resolve_config_path(&def, &home, Platform::Windows);
    unsafe {
        match previous {
            Some(v) => std::env::set_var("APPDATA", v),
            None => std::env::remove_var("APPDATA"),
        }
    }
    assert_eq!(resolved, Some(appdata.join("opencode.jsonc")));
}

#[test]
fn home_dir_uses_injection_over_env() {
    let dir = TempDir::new().unwrap();
    let home = chm_harness_sdk::detect::paths::home_dir(Platform::MacOs, Some(dir.path()));
    assert_eq!(home, PathBuf::from(dir.path()));
}
