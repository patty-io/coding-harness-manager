use chm_database::connect_test;
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;

#[tokio::test]
async fn scan_writes_inventory_to_db() {
    let pool = connect_test().await.unwrap();
    // fake machine: opencode installed under temp dir
    let dir = tempfile::TempDir::new().unwrap();
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
    let inventory = scan(
        Platform::MacOs,
        Some(&homedir),
        Some(&bindir.display().to_string()),
    );
    assert_eq!(inventory.installations.len(), 1);
    for inst in &inventory.installations {
        chm_database::repos::harness::upsert_installation(&pool, inst)
            .await
            .unwrap();
    }
    let all = chm_database::repos::harness::list_installations(&pool)
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].harness_type.as_str(), "opencode");
}
