use chm_database::connect_test;

#[tokio::test]
async fn migration_creates_all_tables() {
    let pool = connect_test().await.expect("connect");
    for table in [
        "providers",
        "provider_endpoints",
        "credential_refs",
        "model_identities",
        "provider_catalog_models",
        "model_routes",
        "harness_installations",
        "harness_model_bindings",
        "mcp_servers",
        "harness_mcp_bindings",
        "skills",
        "harness_skill_bindings",
        "launch_profiles",
        "configuration_sets",
        "configuration_set_items",
        "sync_transactions",
        "config_snapshots",
    ] {
        let row: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("query");
        assert_eq!(row.0, 1, "table {table} missing");
    }
}