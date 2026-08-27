# Phase 12 — Drift Detection + History + Rollback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect external config changes, persist sync history with full snapshots, and provide one-click rollback (project plan §33, §34, §40, §42, §51).

**Architecture:** File watching via the `notify` crate on each installation's config files + `~/.agents/skills`; debounced re-read → compare against the last known state (stored in `config_snapshots` after each sync) → mark harness drifted. History screen reads `sync_transactions` + `config_snapshots`; rollback restores before-content snapshots via `restore_backup`/atomic write and records a `Rollback` transaction. Managed/unmanaged tracking is completed: bindings tables (model/mcp/skill) now feed `managed_flags` in the sync flow, replacing the V1 all-false stub (project plan §18).

**Tech Stack:** Rust `notify` 6 + `tokio::sync::mpsc` debouncer. Frontend: Changes/History screens with diff view.

## Global Constraints

- Watch statuses (project plan §34): `In Sync`, `Pending`, `Externally Modified`, `Conflict`, `Error`. Never auto-overwrite external edits — a drifted harness shows status and requires explicit user action to reconcile.
- Debounce: coalesce file events for 2s before acting; only mark status, never auto-sync (project plan §51).
- History retention: full snapshots, 90-day rotation (auto-purge `config_snapshots` older than 90 days on app start).
- Rollback restores `before_content` for every snapshot in the transaction, in reverse order, then marks the transaction `rolled_back`.
- Phase exit: user edits a harness config externally → status flips to Externally Modified within ~3s; a sync from before can be rolled back restoring the exact prior file contents.

---

### Task 12.1: Snapshot-Based State Tracking

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs` (`execute_sync`: write managed_flags + last-known-state)
- Modify: `crates/database/src/repos/history.rs` (`latest_snapshot_content`)
- Create: `apps/desktop/src-tauri/src/drift.rs` (pure compare logic)
- Create: `apps/desktop/src-tauri/tests/drift_tests.rs`

**Interfaces:**
- Produces:
  - `pub fn normalize_config(raw: &str) -> Result<String, String>` — canonical JSON/TOML normalization (parse + re-serialize) so formatting-only changes don't count as drift.
  - `pub fn diff_state(last_known: &str, current: &str) -> DriftKind` where `pub enum DriftKind { InSync, ExternallyModified, Conflict, Error }` — `InSync` when normalized equal; `ExternallyModified` when the file changed but no pending CHM transaction touches it; `Conflict` when the change overlaps a managed subtree (a managed field value differs from the last known state); `Error` when current fails to parse.
  - `pub async fn latest_snapshot_content(pool, installation_id, path) -> Result<Option<String>, DbError>` — the `after_content` of the newest snapshot for that file.
  - Sync flow: after successful apply, record `managed_flags` from bindings tables (model/mcp/skill bindings for the installation → true) instead of the all-false stub.

- [ ] **Step 1: Write the failing tests `tests/drift_tests.rs`**

```rust
use coding_harness_manager_lib::drift::{diff_state, normalize_config, DriftKind};

#[test]
fn formatting_only_change_is_in_sync() {
    let a = r#"{"provider":{"zai":{"models":{}}}}"#;
    let b = "{\n  \"provider\": {\n    \"zai\": {\n      \"models\": {}\n    }\n  }\n}";
    assert_eq!(diff_state(a, b), DriftKind::InSync);
}

#[test]
fn unrelated_change_is_externally_modified() {
    let a = r#"{"provider":{"zai":{}},"theme":"dark"}"#;
    let b = r#"{"provider":{"zai":{}},"theme":"light"}"#;
    assert_eq!(diff_state(a, b), DriftKind::ExternallyModified);
}

#[test]
fn managed_subtree_change_is_conflict() {
    // last-known contains managed marker: provider.zai.models.glm-5
    let a = r#"{"provider":{"zai":{"models":{"glm-5":{"name":"GLM-5"}}}}}"#;
    let b = r#"{"provider":{"zai":{"models":{"glm-5":{"name":"GLM-5 CHANGED"}}}}}"#;
    // diff_state checks overlap against the managed subtree passed in
    let managed_subtrees = vec!["provider.zai.models.glm-5"];
    assert_eq!(diff_state_with_managed(a, b, &managed_subtrees), DriftKind::Conflict);
}

#[test]
fn unparseable_current_is_error() {
    assert_eq!(diff_state("{}", "not json at all"), DriftKind::Error);
}
```

- [ ] **Step 2: Implement `drift.rs`**

`normalize_config`: try JSON parse → `serde_json::to_string(&value)`; else try TOML parse (`toml_edit`) → re-serialize; else return original trimmed (so `Error` determination is left to the caller). `diff_state` + `diff_state_with_managed` (managed-subtree paths like `"provider.zai.models.glm-5"` — a change under any managed path is a `Conflict`; otherwise `ExternallyModified`). Managed-subtree detection: JSON pointer-ish walk comparing values between last-known and current.

- [ ] **Step 3: Update `execute_sync`** — after apply, load binding tables for the installation and build real `managed_flags`; also write the post-apply file contents as the new "last known state" (already in `config_snapshots.after_content` — `latest_snapshot_content` reads it).

- [ ] **Step 4: Run tests + commit**

```bash
cd apps/desktop/src-tauri && cargo test
git add apps/desktop crates/database
git commit -m "feat(phase12): snapshot state tracking and drift classification"
```

---

### Task 12.2: File Watcher + Drift Status

**Files:**
- Create: `apps/desktop/src-tauri/src/watcher.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (spawn watcher on setup)
- Modify: `apps/desktop/src-tauri/src/commands/scan.rs` (`installation_drift_cmd`)
- Create: `apps/desktop/src-tauri/tests/watcher_tests.rs`

**Interfaces:**
- Produces:
  - `pub async fn spawn_watcher(pool: Pool<Sqlite>) -> tokio::task::JoinHandle<()>` — watches all installations' config files + `~/.agents/skills`; debounces 2s; on event: for each watched file, `diff_state(latest_snapshot_content, current)`; persists drift status to a new table (below); broadcasts via `tauri::Emitter` event `drift-updated` with `{harness: <id>, status: <kind>}`.
  - New migration `0002_drift_status.sql`: `CREATE TABLE drift_status (id TEXT PRIMARY KEY, harness_installation_id TEXT NOT NULL UNIQUE REFERENCES harness_installations(id) ON DELETE CASCADE, status TEXT NOT NULL, detail TEXT, updated_at TEXT NOT NULL);` (run via existing migration runner — new file in `crates/database/migrations/`).
  - `pub async fn set_drift_status(pool, installation_id, status, detail) -> Result<(), DbError>` + `pub async fn list_drift_status(pool) -> Result<Vec<DriftStatusRow>, DbError>` in `repos/harness.rs`.
  - `#[tauri::command] pub async fn installation_drift_cmd(state) -> Result<Vec<DriftStatusRow>, String>`

- [ ] **Step 1: Write the failing test** — unit-test the debounce + classify pipeline with a synthetic file:

```rust
#[tokio::test]
async fn watcher_flags_external_change_after_debounce() {
    let pool = connect_test().await.unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("opencode.json");
    chm_filesystem::atomic_write(&file, r#"{"theme":"dark"}"#).unwrap();
    // seed installation + snapshot (after_content = original)
    let inst = install_with_config(file.display().to_string());
    chm_database::repos::harness::upsert_installation(&pool, &inst).await.unwrap();
    add_snapshot(&pool, &ConfigSnapshot {
        id: uuid::Uuid::new_v4(),
        transaction_id: uuid::Uuid::new_v4(),
        harness_installation_id: inst.id,
        path: file.display().to_string(),
        before_content: None,
        after_content: Some(r#"{"theme":"dark"}"#.into()),
        before_hash: None,
        after_hash: None,
    }).await.unwrap();

    // external edit
    chm_filesystem::atomic_write(&file, r#"{"theme":"light"}"#).unwrap();
    // run the classify-once entry point (no real notify needed for the unit)
    classify_installation(&pool, &inst.id).await.unwrap();
    let rows = chm_database::repos::harness::list_drift_status(&pool).await.unwrap();
    assert_eq!(rows[0].status, "externally-modified");
}
```

- [ ] **Step 2: Implement `watcher.rs`** — `spawn_watcher` uses `notify::RecommendedWatcher` with `tokio` channel; maps watched paths → installations; on debounced batch, calls `classify_installation(pool, install_id)` for each affected; emits the Tauri event. Spawn it in `setup()` after `app.manage(...)` (needs the pool; use `app.state::<AppState>()` clone). Add `notify = "6"` to the desktop crate.

- [ ] **Step 3: UI** — harness rows show drift badge (In Sync green / Externally Modified amber / Conflict red / Error gray) from `installation_drift_cmd`; Harness detail Overview gains a status line + "Reconcile…" button that opens the sync dialog.

- [ ] **Step 4: Verify + commit**

Manual: sync a harness, edit its config in an editor, watch the badge flip within ~3s.

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop crates/database
git commit -m "feat(phase12): drift watcher and status"
```

---

### Task 12.3: History Screen + Rollback

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/history.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Create: `apps/desktop/src/hooks/useHistory.ts`
- Create: `apps/desktop/src/screens/HistoryScreen.tsx` (replaces placeholder)
- Create: `apps/desktop/src/screens/ChangesScreen.tsx` (replaces placeholder)

**Interfaces:**
- Consumes: `list_transactions`, `list_snapshots` (Phase 1 Task 1.6).
- Produces:
  - `#[tauri::command] pub async fn list_history_cmd(state, limit: Option<u32>) -> Result<Vec<HistoryEntry>, String>`
  - `pub struct HistoryEntry { pub transaction_id: String, pub transaction_type: String, pub status: String, pub started_at: String, pub summary: Option<String>, pub snapshots: Vec<SnapshotEntry> }` (camelCase) where `SnapshotEntry { pub path: String, pub before: Option<String>, pub after: Option<String> }`
  - `#[tauri::command] pub async fn rollback_transaction_cmd(state, transaction_id: String) -> Result<RollbackReport, String>` where `RollbackReport { pub files_restored: Vec<String>, pub new_transaction_id: String }`
  - `#[tauri::command] pub async fn purge_old_snapshots_cmd(state) -> Result<usize, String>` — deletes snapshots/transactions older than 90 days; returns count.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn rollback_restores_before_content() {
    let pool = connect_test().await.unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("opencode.json");
    chm_filesystem::atomic_write(&file, "after").unwrap();
    // seed install + tx + snapshot with before="before", after="after"
    let inst = install_with_config(file.display().to_string());
    chm_database::repos::harness::upsert_installation(&pool, &inst).await.unwrap();
    let tx = begin_transaction(&pool, TransactionType::Sync, serde_json::json!({})).await.unwrap();
    finish_transaction(&pool, tx.id, TransactionStatus::Succeeded, Some("test".into()), None).await.unwrap();
    add_snapshot(&pool, &ConfigSnapshot {
        id: uuid::Uuid::new_v4(),
        transaction_id: tx.id,
        harness_installation_id: inst.id,
        path: file.display().to_string(),
        before_content: Some("before".into()),
        after_content: Some("after".into()),
        before_hash: None,
        after_hash: None,
    }).await.unwrap();

    let report = rollback_transaction_core(&pool, &tx.id.to_string()).await.unwrap();
    assert_eq!(report.files_restored.len(), 1);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
    let txs = list_transactions(&pool).await.unwrap();
    assert_eq!(txs.len(), 2, "rollback recorded as new transaction");
    assert_eq!(txs[0].transaction_type, TransactionType::Rollback);
}
```

- [ ] **Step 2: Implement `commands/history.rs`**

`list_history_cmd` joins transactions + snapshots (newest first, limit default 100). `rollback_transaction_core(pool, tx_id)`: load snapshots for the tx; for each, `atomic_write(path, before_content)` (or `restore_backup` when before is None but a backup exists — Phase 8 keeps backups on disk under `.chm-backups`, so fall back to the newest matching backup); then `begin_transaction(Rollback, {rolled_back: <tx_id>})` + `finish_transaction(Succeeded)`. `purge_old_snapshots_cmd`: `DELETE FROM config_snapshots WHERE ... < now - 90 days` via `transaction_id IN (SELECT id FROM sync_transactions WHERE completed_at < ...)`; count rows.

- [ ] **Step 3: UI**

`HistoryScreen.tsx` (project plan §33 layout): timeline list — timestamp, summary, status badge, [View Diff] (modal: file-by-file before/after `<pre>` blocks), [Rollback] (confirm + run). `ChangesScreen.tsx`: pending changes view = the latest `sync_preview` result cached per harness (reuse Phase 8 dialog), plus drift rows with "Reconcile" shortcut.

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase12): history screen and rollback"
```

---

### Task 12.4: Retention + Phase Exit

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs` (purge on startup)

**Interfaces:**
- Consumes: `purge_old_snapshots_cmd` core.

- [ ] **Step 1: Startup purge** — in `setup()`, after connect: `tauri::async_runtime::block_on(purge_old_snapshots_core(&pool))` (log count via `tracing`).

- [ ] **Step 2: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase12): snapshot retention purge"
```

Phase complete when all steps green.

---

### Task 12.5: Backup/Restore + Import/Export

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/backup.rs`
- Create: `apps/desktop/src-tauri/src/commands/export_import.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Create: `apps/desktop/src/hooks/useBackup.ts`
- Create: `apps/desktop/src/screens/SettingsScreen.tsx` (replaces placeholder; backup + import/export section)
- Modify: `apps/desktop/src/lib/api.ts`

**Interfaces:**
- Consumes: all Phase 1 repos.
- Produces:
  - `#[tauri::command] pub async fn backup_now_cmd(state, dest_dir: String) -> Result<String, String>` — `VACUUM INTO '<dest>/chm-backup-<timestamp>.sqlite'` — returns the file path.
  - `#[tauri::command] pub async fn restore_backup_cmd(state, backup_path: String) -> Result<(), String>` — V1: swap the live DB file and prompt an app restart.
  - `#[tauri::command] pub async fn export_config_cmd(state, dest_dir: String) -> Result<String, String>` — writes `chm-export-<timestamp>.json` with `{app_version, exported_at, providers, endpoints (credential_ref as {kind, reference} only), model_identities, model_routes, mcp_servers, skills, launch_profiles, configuration_sets, preferences}` (project plan §41 — secrets excluded by construction).
  - `#[tauri::command] pub async fn preview_import_cmd(state, file_path: String) -> Result<serde_json::Value, String>` — `{additions: [...], conflicts: [...], unchanged: [...]}` per entity kind (identity: provider name, endpoint name+base_url, route (endpoint_id, remote_model_id), mcp name, skill canonical_path, profile name, set name).
  - `#[tauri::command] pub async fn import_config_cmd(state, file_path: String, mode: String) -> Result<serde_json::Value, String>` — `mode` ∈ {"merge", "replaceManaged"}; returns `{applied: usize, skipped: Vec<String>}`; always preceded by preview in the UI (project plan §41: "Always show diff before import").
  - `pub async fn export_config_core(pool, dest_dir) -> Result<String, String>`, `pub async fn preview_import_core(pool, file_path) -> Result<serde_json::Value, String>`, `pub async fn import_config_core(pool, file_path, mode) -> Result<serde_json::Value, String>` (testable, no State).

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn export_contains_references_never_values() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let cred = create_credential_ref(&pool, CredentialKind::Keychain, "coding-harness-manager/providers/zai").await.unwrap();
    let e = ProviderEndpoint { credential_ref: Some(cred), ..endpoint(&p.id) };
    create_endpoint(&pool, &e).await.unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = export_config_core(&pool, &dir.path().display().to_string()).await.unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("coding-harness-manager/providers/zai"), "reference kept");
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["providers"].as_array().unwrap().len(), 1);
    // no secret VALUES anywhere: every credential object is {kind, reference}
    let endpoints = json["endpoints"].as_array().unwrap();
    for ep in endpoints {
        if let Some(cred) = ep.get("credential_ref") {
            assert!(cred.get("value").is_none(), "credential value must never be exported");
            assert!(cred.get("kind").is_some() && cred.get("reference").is_some());
        }
    }
}

#[tokio::test]
async fn export_import_roundtrip_preserves_entities() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = endpoint(&p.id);
    create_endpoint(&pool, &e).await.unwrap();
    let route = route(&e.id, "glm-5", Some(1_048_576));
    create_route(&pool, &route).await.unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = export_config_core(&pool, &dir.path().display().to_string()).await.unwrap();

    // fresh DB: import merge
    let pool2 = connect_test().await.unwrap();
    let report = import_config_core(&pool2, &path, "merge").await.unwrap();
    assert_eq!(report["applied"].as_u64().unwrap(), 3, "provider + endpoint + route");
    let providers = chm_database::repos::providers::list_providers(&pool2).await.unwrap();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].name, "zai");
    // idempotent: re-import → all skipped
    let report2 = import_config_core(&pool2, &path, "merge").await.unwrap();
    assert_eq!(report2["applied"].as_u64().unwrap(), 0);
}
```

- [ ] **Step 2: Implement**

`export_config_core`: collect all entities via repos, serialize with a versioned envelope; endpoints map `credential_ref` → `{kind: <kind string>, reference: <reference>}` (never fetch values — the secret store is not even consulted). `preview_import_core`: parse file, compare against live rows by the identity rules above. `import_config_core`: merge = insert additions only; replaceManaged = insert additions + update colliding rows (matched by identity) with file values; return `{applied, skipped}`.

- [ ] **Step 3: Settings UI**

`SettingsScreen.tsx`: Backup section (Backup Now → save dialog via `@tauri-apps/plugin-dialog`; Restore → file picker + warning modal "the app will restart"), Export section (Export configuration → save dialog → success path), Import section (file picker → preview modal with additions/conflicts lists → merge/replace radio → apply → result summary).

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase12): backup, restore, import, export"
```