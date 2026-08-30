use chm_core::domain::harness::*;
use chm_core::domain::history::*;
use chm_database::connect_test;
use chm_database::repos::harness::*;
use chm_database::repos::history::*;

#[tokio::test]
async fn installation_upsert_is_idempotent() {
    let pool = connect_test().await.unwrap();
    let now = chrono::Utc::now();
    let i = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: Some("/usr/local/bin/opencode".into()),
        version: Some("0.30.0".into()),
        config_path: Some("/Users/me/.config/opencode".into()),
        detected_at: now,
        last_scanned_at: Some(now),
        status: InstallationStatus::Installed,
    };
    upsert_installation(&pool, &i).await.unwrap();
    let i2 = HarnessInstallation {
        version: Some("0.31.0".into()),
        ..i.clone()
    };
    upsert_installation(&pool, &i2).await.unwrap();
    let all = list_installations(&pool).await.unwrap();
    assert_eq!(all.len(), 1, "upsert must replace, not duplicate");
    assert_eq!(all[0].version.as_deref(), Some("0.31.0"));
}

#[tokio::test]
async fn find_installation_uses_the_stable_id() {
    let pool = connect_test().await.unwrap();
    let installation = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Pi,
        executable_path: Some("/usr/local/bin/pi".into()),
        version: Some("0.84.3".into()),
        config_path: Some("/Users/me/.pi/agent/models.json".into()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    let stored = upsert_installation(&pool, &installation).await.unwrap();

    assert_eq!(
        find_installation(&pool, stored.id).await.unwrap().id,
        stored.id
    );
    let missing = find_installation(&pool, uuid::Uuid::new_v4())
        .await
        .unwrap_err();
    assert!(matches!(missing, chm_database::DbError::NotFound(_)));
}

#[tokio::test]
async fn upsert_returns_stable_stored_id_across_rescans() {
    let pool = connect_test().await.unwrap();
    let now = chrono::Utc::now();
    let i = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Pi,
        executable_path: Some("/usr/local/bin/pi".into()),
        version: Some("0.84.3".into()),
        config_path: Some("/Users/me/.pi/agent/models.json".into()),
        detected_at: now,
        last_scanned_at: Some(now),
        status: InstallationStatus::Installed,
    };
    let first = upsert_installation(&pool, &i).await.unwrap();
    // a rescan mints a fresh uuid — the stored row's id must win
    let rescan = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        version: Some("0.84.4".into()),
        ..i.clone()
    };
    let second = upsert_installation(&pool, &rescan).await.unwrap();
    assert_eq!(
        first.id, second.id,
        "installation id must be stable across rescans"
    );
    assert_eq!(second.version.as_deref(), Some("0.84.4"));
    // the scanned (fresh) id must NOT exist in the db
    let all = list_installations(&pool).await.unwrap();
    assert!(all.iter().all(|x| x.id == first.id));
}

#[tokio::test]
async fn transaction_and_snapshot_flow() {
    let pool = connect_test().await.unwrap();
    let tx = begin_transaction(
        &pool,
        TransactionType::Sync,
        serde_json::json!({"actions": []}),
    )
    .await
    .unwrap();
    finish_transaction(
        &pool,
        tx.id,
        TransactionStatus::Succeeded,
        Some("synced 5 models".into()),
        None,
    )
    .await
    .unwrap();
    let inst = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Pi,
        executable_path: None,
        version: Some("0.84.3".into()),
        config_path: Some("/tmp".into()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    upsert_installation(&pool, &inst).await.unwrap();
    let snap = ConfigSnapshot {
        id: uuid::Uuid::new_v4(),
        transaction_id: tx.id,
        harness_installation_id: inst.id,
        path: "/tmp/config.toml".into(),
        before_content: Some("a = 1".into()),
        after_content: Some("a = 2".into()),
        before_hash: Some("h1".into()),
        after_hash: Some("h2".into()),
    };
    add_snapshot(&pool, &snap).await.unwrap();
    let txs = list_transactions(&pool).await.unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].summary.as_deref(), Some("synced 5 models"));
    let snaps = list_snapshots(&pool, tx.id).await.unwrap();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].after_hash.as_deref(), Some("h2"));
    assert_eq!(
        latest_snapshot_content(&pool, inst.id, "/tmp/config.toml")
            .await
            .unwrap(),
        Some("a = 2".to_string())
    );
}
