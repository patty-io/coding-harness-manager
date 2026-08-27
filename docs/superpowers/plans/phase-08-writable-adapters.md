# Phase 8 — Writable Adapters + Sync Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make sync real: the filesystem safety layer (atomic writes, backups, directory links), write-capable adapters that translate `ReconciliationPlan` actions into minimal native-file edits, and the end-to-end sync flow with preview → apply → verify → rollback (project plan §15, §32, §33, §48, §49).

**Architecture:** `crates/filesystem` owns ALL disk mutation (atomic write, backup, link — the only crate allowed to touch native files). The adapter trait gains `plan()` (ReconciliationPlan → `NativePlan` of before/after file contents), `apply()` (executes with backups + atomic writes), `validate()`, and `rollback()`. The sync flow in `apps/desktop` orchestrates: desired state from DB → adapter `read_state` → `reconcile` → `filter_unsupported` → adapter `plan` → preview → apply with transaction + snapshots → verify → rollback on failure.

**Tech Stack:** Rust (existing crates + `sha2` for hashes), fsync via `std::fs::File::sync_all`. Frontend: PreviewChanges modal + Sync button wired through TanStack Query.

## Global Constraints

- ONLY `crates/filesystem` touches disks. Adapters call `atomic_write`/`backup_file`/`link_directory` — never `std::fs::write` on native configs.
- Writes are minimal-subtree: an adapter edits ONLY the managed section (e.g. `provider.<id>` block); everything else in the file is preserved byte-for-byte where the format allows (JSON: re-serialize full doc with unmanaged keys untouched; TOML: same via `toml_edit` document model).
- Every apply makes a backup first (`backup_file`), records `ConfigSnapshot` rows, and supports rollback by restoring backups.
- Unknown/unsupported harness versions → `plan()` returns all actions as `Unsupported` with reason (read-only mode per project plan §57); advanced override flag `force` on the command bypasses only with explicit UI warning.
- Adapter enablement order: OpenCode → Pi → Codex → Claude Code → Reasonix (project plan §66 Phase 8). Each adapter task is independent; the sync flow task (8.2) works with whatever adapters exist — skipped harnesses show "write support pending".
- Phase exit: full sync cycle works end-to-end against OpenCode on a real machine (preview shows diff, apply writes, re-read validates, rollback restores), all golden tests green.

---

### Task 8.1: Filesystem Safety Layer (crates/filesystem)

**Files:**
- Create: `crates/filesystem/src/lib.rs`
- Create: `crates/filesystem/tests/atomic.rs`

**Interfaces:**
- Produces (used by EVERY writable adapter task and Phase 10/12):
  - `pub enum FsError { Io(std::io::Error), InvalidPath(String), Unsupported(String) }` (thiserror)
  - `pub fn atomic_write(path: &Path, content: &str) -> Result<(), FsError>` — write to `<path>.chm-tmp-<pid>`, `sync_all`, rename over target (Unix rename replaces atomically; Windows: `std::fs::rename` fails if target exists → remove+rename with backup taken by caller first).
  - `pub fn backup_file(path: &Path) -> Result<PathBuf, FsError>` — copies to `<parent>/.chm-backups/<stamp>-<name>.<ext>.bak`, returns the backup path; creates the backups dir; no-op returning `None`-equivalent error-free path when the file doesn't exist (returns `Ok` with the would-be path, content empty).
  - `pub fn restore_backup(backup: &Path, target: &Path) -> Result<(), FsError>` — atomic_write of backup contents back to target.
  - `pub enum LinkOutcome { Symlink, Junction, Copy, AlreadyLinked, Unsupported(String) }`
  - `pub fn link_directory(source: &Path, target: &Path) -> Result<LinkOutcome, FsError>` — Unix: symlink (recreate if target exists and is a dangling symlink); Windows: junction via `mklink /J` (compile-gated); fallback copy (recursive) when symlink fails with `Unsupported` reason; `AlreadyLinked` when target already resolves to source.

- [ ] **Step 1: Write the failing tests `tests/atomic.rs`**

```rust
use chm_filesystem::{atomic_write, backup_file, restore_backup, link_directory, LinkOutcome};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn atomic_write_replaces_content() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("config.json");
    atomic_write(&f, "{\"a\":1}").unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "{\"a\":1}");
    atomic_write(&f, "{\"a\":2}").unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "{\"a\":2}");
    // no temp leftovers
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("chm-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp files must be cleaned up");
}

#[test]
fn atomic_write_creates_parent_dirs() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("nested/deeper/config.toml");
    atomic_write(&f, "[x]").unwrap();
    assert!(f.exists());
}

#[test]
fn backup_then_restore_roundtrip() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("config.toml");
    atomic_write(&f, "before").unwrap();
    let backup = backup_file(&f).unwrap();
    assert!(backup.exists());
    atomic_write(&f, "after").unwrap();
    restore_backup(&backup, &f).unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "before");
}

#[test]
fn link_directory_creates_symlink_or_reports_outcome() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("skills/brainstorming");
    std::fs::create_dir_all(&source).unwrap();
    let target = dir.path().join("linked-skills");
    let outcome = link_directory(&source, &target).unwrap();
    match outcome {
        LinkOutcome::Symlink => {
            assert!(std::fs::symlink_metadata(&target).unwrap().file_type().is_symlink());
            // idempotent
            let again = link_directory(&source, &target).unwrap();
            assert!(matches!(again, LinkOutcome::AlreadyLinked));
        }
        LinkOutcome::Copy => {
            assert!(target.join("").exists());
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn restore_backup_missing_backup_errors() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("config.toml");
    let res = restore_backup(&dir.path().join("nope.bak"), &f);
    assert!(res.is_err());
}
```

- [ ] **Step 2: Implement `crates/filesystem/src/lib.rs`**

```rust
//! Filesystem safety layer: atomic writes, backups, directory links.
//! THE ONLY module allowed to mutate native files.

use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub fn atomic_write(path: &Path, content: &str) -> Result<(), FsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("chm-tmp-{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists || std::fs::metadata(path).is_ok() && cfg!(windows) => {
            // Windows rename-over-existing: replace via remove+rename (caller already backed up)
            std::fs::remove_file(path)?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(FsError::Io(e))
        }
    }
}

pub fn backup_file(path: &Path) -> Result<PathBuf, FsError> {
    let parent = path.parent().ok_or_else(|| FsError::InvalidPath(path.display().to_string()))?;
    let backups_dir = parent.join(".chm-backups");
    std::fs::create_dir_all(&backups_dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%f");
    let file_name = path.file_name().ok_or_else(|| FsError::InvalidPath(path.display().to_string()))?;
    let backup = backups_dir.join(format!("{stamp}-{}", file_name.to_string_lossy()));
    if path.exists() {
        std::fs::copy(path, &backup)?;
    }
    Ok(backup)
}

pub fn restore_backup(backup: &Path, target: &Path) -> Result<(), FsError> {
    let content = std::fs::read_to_string(backup)?;
    atomic_write(target, &content)
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkOutcome {
    Symlink,
    Junction,
    Copy,
    AlreadyLinked,
    Unsupported(String),
}

#[cfg(unix)]
pub fn link_directory(source: &Path, target: &Path) -> Result<LinkOutcome, FsError> {
    if std::fs::symlink_metadata(target).is_ok() {
        // already exists: is it a symlink to our source? (dangling symlink included)
        if let Ok(link_target) = std::fs::read_link(target) {
            if link_target == source {
                return Ok(LinkOutcome::AlreadyLinked);
            }
            std::fs::remove_file(target)?; // broken/foreign symlink — replace
        } else {
            return Ok(LinkOutcome::Copy); // real dir exists — treat as copied/shared, do not clobber
        }
    }
    match std::os::unix::fs::symlink(source, target) {
        Ok(()) => Ok(LinkOutcome::Symlink),
        Err(e) => {
            if e.raw_os_error() == Some(libc::EPERM) || e.raw_os_error() == Some(libc::EACCES) {
                copy_tree(source, target)?;
                Ok(LinkOutcome::Copy)
            } else {
                Err(FsError::Io(e))
            }
        }
    }
}

#[cfg(windows)]
pub fn link_directory(source: &Path, target: &Path) -> Result<LinkOutcome, FsError> {
    // junction: mklink /J target source — requires cmd; fallback copy
    let out = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J", &target.display().to_string(), &source.display().to_string()])
        .output()?;
    if out.status.success() {
        return Ok(LinkOutcome::Junction);
    }
    copy_tree(source, target)?;
    Ok(LinkOutcome::Copy)
}

#[cfg(unix)]
fn copy_tree(source: &Path, target: &Path) -> Result<(), FsError> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
```

Note: the Windows branch of `atomic_write` is compile-gated by `cfg!(windows)` in the match arm — verify it compiles cleanly on macOS (it does: the condition is runtime, both arms compile). `libc` must be added as a dependency (`libc = "0.2"`).

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p chm-filesystem
git add crates/filesystem
git commit -m "feat(phase8): filesystem safety layer"
```

---

### Task 8.2: Adapter Write Contract + Sync Flow Backend

**Files:**
- Modify: `crates/harness-sdk/src/adapter/types.rs` (write methods)
- Create: `crates/harness-sdk/src/adapter/native_plan.rs`
- Create: `apps/desktop/src-tauri/src/commands/sync.rs`
- Create: `apps/desktop/src-tauri/tests/sync_flow_tests.rs`

**Interfaces:**
- Consumes: `ReconciliationPlan` (Phase 7), filesystem crate (8.1).
- Produces:
  - `pub struct NativeChange { pub file_path: String, pub before: Option<String>, pub after: Option<String> }` — before/after are FULL file contents (adapters always rewrite whole files atomically; minimal-subtree means the new content differs only in managed sections).
  - `pub struct NativeLink { pub kind: String, pub source: String, pub target: String }`
  - `pub struct NativePlan { pub changes: Vec<NativeChange>, pub links: Vec<NativeLink>, pub warnings: Vec<String> }`
  - `pub struct ApplyResult { pub files_written: Vec<String>, pub links_created: Vec<String> }`
  - `pub struct ValidationReport { pub ok: bool, pub errors: Vec<String> }`
  - Trait additions (default methods return `Err(Unsupported)`):
    - `fn plan(&self, plan: &ReconciliationPlan, install: &HarnessInstallation) -> Result<NativePlan, AdapterError>`
    - `fn apply(&self, install: &HarnessInstallation, native_plan: &NativePlan) -> Result<ApplyResult, AdapterError>`
    - `fn validate(&self, install: &HarnessInstallation) -> Result<ValidationReport, AdapterError>`
    - `fn rollback(&self, install: &HarnessInstallation, native_plan: &NativePlan) -> Result<(), AdapterError>`
  - Sync commands:
    - `#[tauri::command] pub async fn sync_preview(state, installation_id: String, mode: String) -> Result<PreviewReport, String>` — `mode` ∈ {"append", "replaceManaged"}; returns `PreviewReport { pub summary: String, pub actions: Vec<ActionView>, pub files: Vec<FilePreview> }` where `ActionView { pub kind: String, pub identity: String, pub action: String }`, `FilePreview { pub path: String, pub before: Option<String>, pub after: Option<String> }`.
    - `#[tauri::command] pub async fn sync_apply(state, installation_id: String, mode: String, force: bool) -> Result<ApplyReport, String>` — runs the full cycle with transaction + snapshots + rollback; returns `ApplyReport { pub summary: String, pub files_written: Vec<String>, pub links_created: Vec<String>, pub transaction_id: String, pub validation: ValidationReport }`.
  - Testable cores: `pub async fn build_native_plan(pool, installation_id, mode, force) -> Result<(ReconciliationPlan, NativePlan, PreviewReport), String>` and `pub async fn execute_sync(pool, secrets, installation_id, mode, force) -> Result<ApplyReport, String>` (no Tauri State).

- [ ] **Step 1: Add the write contract to the trait**

```rust
// in types.rs — additions

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeChange {
    pub file_path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeLink {
    pub kind: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NativePlan {
    pub changes: Vec<NativeChange>,
    pub links: Vec<NativeLink>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub files_written: Vec<String>,
    pub links_created: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub ok: bool,
    pub errors: Vec<String>,
}

// trait additions (defaults reject):
fn plan(&self, _plan: &ReconciliationPlan, _install: &HarnessInstallation) -> Result<NativePlan, AdapterError> {
    Err(AdapterError::UnsupportedVersion { harness: self.id().into(), version: None })
}
fn apply(&self, _install: &HarnessInstallation, _native_plan: &NativePlan) -> Result<ApplyResult, AdapterError> {
    Err(AdapterError::UnsupportedVersion { harness: self.id().into(), version: None })
}
fn validate(&self, _install: &HarnessInstallation) -> Result<ValidationReport, AdapterError> {
    Err(AdapterError::UnsupportedVersion { harness: self.id().into(), version: None })
}
fn rollback(&self, _install: &HarnessInstallation, _native_plan: &NativePlan) -> Result<(), AdapterError> {
    Err(AdapterError::UnsupportedVersion { harness: self.id().into(), version: None })
}
```

- [ ] **Step 2: Write the failing sync flow test**

`tests/sync_flow_tests.rs` — uses a fake "sync harness" whose config file lives in a temp dir and whose adapter is injected (the sync core takes `&dyn HarnessAdapter`):

```rust
#[tokio::test]
async fn execute_sync_applies_and_records_snapshots() {
    let pool = connect_test().await.unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    // seed: provider + endpoint + one route (desired)
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = endpoint(&p.id);
    create_endpoint(&pool, &e).await.unwrap();
    create_route(&pool, &route(&e.id, "glm-5", Some(1_048_576))).await.unwrap();
    // seed: installation pointing at the temp config file
    let inst = HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: None,
        version: Some("0.30.0".into()),
        config_path: Some(dir.path().join("opencode.json").display().to_string()),
        detected_at: Utc::now(),
        last_scanned_at: Some(Utc::now()),
        status: InstallationStatus::Installed,
    };
    upsert_installation(&pool, &inst).await.unwrap();
    // seed actual: empty config file
    chm_filesystem::atomic_write(&dir.path().join("opencode.json"), "{}").unwrap();

    let report = execute_sync(&pool, &MockSecrets, &inst.id.to_string(), &Mode::Append, false).await.unwrap();
    assert_eq!(report.files_written.len(), 1);
    // file now contains the model
    let content = std::fs::read_to_string(dir.path().join("opencode.json")).unwrap();
    assert!(content.contains("glm-5"));
    // transaction + snapshot recorded
    let txs = list_transactions(&pool).await.unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].status, TransactionStatus::Succeeded);
    let snaps = list_snapshots(&pool, txs[0].id).await.unwrap();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].before_content.as_deref(), Some("{}"));
}
```

The test uses the REAL `OpenCodeAdapter` (Task 8.3) — sequence: write Task 8.2 with the flow + task 8.3 adapter, then this test passes. `MockSecrets` implements `SecretStore` returning `None`.

- [ ] **Step 3: Implement `commands/sync.rs` (core flow)**

```rust
//! Sync flow: desired -> actual -> plan -> native plan -> preview/apply -> verify.

use adapters::all_adapters;
use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::history::{ConfigSnapshot, SyncTransaction, TransactionStatus, TransactionType};
use chm_database::repos::harness::list_installations;
use chm_database::repos::history::{add_snapshot, begin_transaction, finish_transaction};
use chm_database::repos::mcp::list_mcp_servers;
use chm_database::repos::models::list_routes;
use chm_database::repos::skills::list_skills;
use chm_filesystem::{atomic_write, backup_file};
use chm_harness_sdk::adapter::types::{
    ApplyResult, HarnessAdapter, NativePlan, ValidationReport,
};
use chm_reconciliation::engine::{filter_unsupported, reconcile};
use chm_reconciliation::plan::{DesiredState, ActualState, Mode, PlanAction, ReconciliationPlan};
use chm_secrets::SecretStore;
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionView {
    pub kind: String,
    pub identity: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    pub summary: String,
    pub actions: Vec<ActionView>,
    pub files: Vec<FilePreview>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub summary: String,
    pub files_written: Vec<String>,
    pub links_created: Vec<String>,
    pub transaction_id: String,
    pub validation: ValidationReport,
}

pub fn adapter_for(harness_type: &str) -> Option<Box<dyn HarnessAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == harness_type)
}

pub fn parse_mode(s: &str) -> Mode {
    match s {
        "replaceManaged" => Mode::ReplaceManaged,
        _ => Mode::Append,
    }
}

async fn desired_state(pool: &Pool<Sqlite>) -> Result<DesiredState, String> {
    Ok(DesiredState {
        routes: list_routes(pool).await.map_err(|e| e.to_string())?
            .into_iter().filter(|r| r.enabled).collect(),
        mcp_servers: list_mcp_servers(pool).await.map_err(|e| e.to_string())?
            .into_iter().filter(|m| m.enabled).collect(),
        skills: list_skills(pool).await.map_err(|e| e.to_string())?
            .into_iter().filter(|s| s.enabled).collect(),
    })
}

fn actual_state(parsed: &chm_harness_sdk::adapter::types::ParsedState, managed: std::collections::HashMap<String, bool>) -> ActualState {
    ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: managed,
    }
}

/// managed_flags: rows from harness_model_bindings / harness_mcp_bindings /
/// harness_skill_bindings for this installation (Phase 12 fills persistence;
/// V1 flags every actual item as unmanaged — replace-managed then only removes
/// what CHM itself added, tracked via bindings in later phases).
fn managed_flags_for(install: &HarnessInstallation, parsed: &chm_harness_sdk::adapter::types::ParsedState) -> std::collections::HashMap<String, bool> {
    let mut flags = std::collections::HashMap::new();
    for m in &parsed.models {
        flags.insert(format!("route:{}:{}", m.route.endpoint_id, m.native_id), false);
    }
    for m in &parsed.mcp {
        flags.insert(format!("mcp:{}", m.native_name), false);
    }
    for s in &parsed.skills {
        flags.insert(format!("skill:{}", s.path), false);
    }
    let _ = install;
    flags
}

pub async fn build_native_plan(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
) -> Result<(HarnessInstallation, Box<dyn HarnessAdapter>, ReconciliationPlan, NativePlan), String> {
    let inst = list_installations(pool).await.map_err(|e| e.to_string())?
        .into_iter().find(|i| i.id.to_string() == installation_id)
        .ok_or("installation not found")?;
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    let desired = desired_state(pool).await?;
    let actual = actual_state(&parsed, managed_flags_for(&inst, &parsed));
    let plan = reconcile(&desired, &actual, *mode).map_err(|e| e.to_string())?;
    let caps = adapter.capabilities();
    let plan = filter_unsupported(plan, &caps);
    let native_plan = adapter.plan(&plan, &inst).map_err(|e| e.to_string())?;
    Ok((inst, adapter, plan, native_plan))
}

#[tauri::command]
pub async fn sync_preview(state: State<'_, AppState>, installation_id: String, mode: String) -> Result<PreviewReport, String> {
    let m = parse_mode(&mode);
    let (_, _, plan, native_plan) = build_native_plan(&state.pool, &installation_id, &m).await?;
    let actions = plan.actions.iter().map(|a| match a {
        PlanAction::Add(x) => ActionView { kind: x.kind.clone(), identity: x.identity.clone(), action: "add".into() },
        PlanAction::Update(x) => ActionView { kind: x.kind.clone(), identity: x.identity.clone(), action: "update".into() },
        PlanAction::Remove(x) => ActionView { kind: x.kind.clone(), identity: x.identity.clone(), action: "remove".into() },
        PlanAction::Unchanged(x) => ActionView { kind: x.kind.clone(), identity: x.identity.clone(), action: "unchanged".into() },
        PlanAction::Conflict(x) => ActionView { kind: x.kind.clone(), identity: x.identity.clone(), action: "conflict".into() },
        PlanAction::Unsupported(x) => ActionView { kind: x.kind.clone(), identity: x.identity.clone(), action: "unsupported".into() },
        PlanAction::NoOp(x) => ActionView { kind: "noop".into(), identity: x.clone(), action: "noop".into() },
    }).collect();
    let files = native_plan.changes.iter().map(|c| FilePreview {
        path: c.file_path.clone(),
        before: c.before.clone(),
        after: c.after.clone(),
    }).collect();
    Ok(PreviewReport {
        summary: plan.summary(),
        actions,
        files,
    })
}

pub async fn execute_sync(
    pool: &Pool<Sqlite>,
    secrets: &dyn SecretStore,
    installation_id: &str,
    mode: &Mode,
    _force: bool,
) -> Result<ApplyReport, String> {
    let (inst, adapter, _plan, native_plan) = build_native_plan(pool, installation_id, mode).await?;
    let tx = begin_transaction(pool, TransactionType::Sync, serde_json::json!(native_plan)).await.map_err(|e| e.to_string())?;
    let mut backups = Vec::new();
    let mut result = ApplyReport {
        summary: String::new(),
        files_written: Vec::new(),
        links_created: Vec::new(),
        transaction_id: tx.id.to_string(),
        validation: ValidationReport { ok: true, errors: vec![] },
    };
    let _ = secrets;

    // backups first (all-or-nothing before any mutation)
    for change in &native_plan.changes {
        let backup = backup_file(std::path::Path::new(&change.file_path)).map_err(|e| e.to_string())?;
        backups.push((change.file_path.clone(), backup));
    }

    let apply_outcome = (|| -> Result<ApplyResult, String> {
        let apply_result = adapter.apply(&inst, &native_plan).map_err(|e| e.to_string())?;
        // snapshots per file
        for (file, backup) in &backups {
            let before = std::fs::read_to_string(backup).ok();
            let after = std::fs::read_to_string(file).ok();
            let hash = |s: &str| -> String { format!("{:x}", sha2::Sha256::digest(s.as_bytes())) };
            add_snapshot(pool, &ConfigSnapshot {
                id: Uuid::new_v4(),
                transaction_id: tx.id,
                harness_installation_id: inst.id,
                path: file.clone(),
                before_content: before.clone(),
                after_content: after.clone(),
                before_hash: before.as_deref().map(hash),
                after_hash: after.as_deref().map(hash),
            }).await.map_err(|e| e.to_string())?;
        }
        Ok(apply_result)
    })();

    match apply_outcome {
        Ok(apply_result) => {
            result.files_written = apply_result.files_written;
            result.links_created = apply_result.links_created;
            // verify
            match adapter.validate(&inst) {
                Ok(v) => {
                    result.validation = v;
                    if v.ok {
                        finish_transaction(pool, tx.id, TransactionStatus::Succeeded, Some(format!("synced {}", result.files_written.len())), None).await.map_err(|e| e.to_string())?;
                    } else {
                        let _ = adapter.rollback(&inst, &native_plan);
                        for (file, backup) in &backups {
                            let _ = chm_filesystem::restore_backup(backup, std::path::Path::new(file));
                        }
                        finish_transaction(pool, tx.id, TransactionStatus::Failed, None, Some(format!("validation failed: {:?}", v.errors))).await.map_err(|e| e.to_string())?;
                        return Err(format!("validation failed after apply; rolled back: {:?}", v.errors));
                    }
                }
                Err(e) => {
                    let _ = adapter.rollback(&inst, &native_plan);
                    for (file, backup) in &backups {
                        let _ = chm_filesystem::restore_backup(backup, std::path::Path::new(file));
                    }
                    finish_transaction(pool, tx.id, TransactionStatus::Failed, None, Some(e.to_string())).await.map_err(|e| e.to_string())?;
                    return Err(format!("apply failed; rolled back: {e}"));
                }
            }
        }
        Err(e) => {
            for (file, backup) in &backups {
                let _ = chm_filesystem::restore_backup(&backup, std::path::Path::new(&file));
            }
            finish_transaction(pool, tx.id, TransactionStatus::Failed, None, Some(e.clone())).await.map_err(|e| e.to_string())?;
            return Err(e);
        }
    }

    result.summary = format!("{} files written, {} links created", result.files_written.len(), result.links_created.len());
    Ok(result)
}

#[tauri::command]
pub async fn sync_apply(
    state: State<'_, AppState>,
    installation_id: String,
    mode: String,
    force: bool,
) -> Result<ApplyReport, String> {
    execute_sync(&state.pool, state.secrets.as_ref(), &installation_id, &parse_mode(&mode), force).await
}
```

Note: `sha2::Sha256::digest` needs `sha2` dependency with `Digest` trait in scope — add `use sha2::Digest;` and `sha2 = "0.10"` to the desktop crate. The transaction/snapshot rows use Phase 1 repo functions.

- [ ] **Step 3: Commit the contract + flow**

```bash
cd apps/desktop/src-tauri && cargo check
git add crates/harness-sdk apps/desktop/src-tauri
git commit -m "feat(phase8): adapter write contract and sync flow"
```

(The failing test `execute_sync_applies_and_records_snapshots` stays red until Task 8.3 implements the OpenCode adapter — that is expected at this commit.)

---

### Task 8.3: OpenCode Writable Adapter

**Files:**
- Modify: `adapters/opencode/src/lib.rs`
- Create: `adapters/opencode/src/writer.rs`
- Create: `adapters/opencode/tests/write_fixtures.rs`

**Interfaces:**
- Consumes: `NativePlan`/`NativeChange`, filesystem crate, `ReconciliationPlan` actions.
- Produces: OpenCode `plan()`/`apply()`/`validate()`/`rollback()` — the reference implementation all other writable adapters mirror.

- [ ] **Step 1: Write the failing write test `tests/write_fixtures.rs`**

Golden serialize test (project plan §55): fixture → parse → modify desired → serialize → expected native config.

```rust
use chm_filesystem::atomic_write;
use chm_harness_sdk::adapter::types::{NativeChange, NativePlan};
use opencode_adapter::writer::{apply_native_plan, plan_model_add};
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn adding_a_model_merges_only_the_provider_subtree() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("opencode.json");
    let original = r#"{
      "model": "glm-5",
      "theme": "dark",
      "provider": {
        "zai": {
          "models": { "glm-5": { "name": "GLM-5" } }
        }
      }
    }"#;
    atomic_write(&config, original).unwrap();

    let change = NativeChange {
        file_path: config.display().to_string(),
        before: Some(original.to_string()),
        after: None, // computed by plan
    };
    let plan = NativePlan {
        changes: vec![plan_model_add(&config.display().to_string(), "zai", "glm-5-air", "GLM-5 Air"),
                      change],
        links: vec![],
        warnings: vec![],
    };
    apply_native_plan(&plan).unwrap();

    let content = std::fs::read_to_string(&config).unwrap();
    let json: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["theme"], "dark", "unmanaged key must survive");
    assert_eq!(json["provider"]["zai"]["models"]["glm-5-air"]["name"], "GLM-5 Air");
    assert_eq!(json["model"], "glm-5", "top-level model untouched");
}

#[test]
fn plan_model_add_marks_after_content() {
    let original = r#"{"provider": {}}"#;
    let change = plan_model_add("/tmp/opencode.json", "zai", "glm-5", "GLM-5");
    assert_eq!(change.before.as_deref(), Some(original));
    assert!(change.after.as_deref().unwrap_or("").contains("glm-5"));
}
```

- [ ] **Step 2: Implement `adapters/opencode/src/writer.rs`**

```rust
//! OpenCode writer: minimal-subtree edits to opencode.json.

use chm_filesystem::{atomic_write, backup_file, restore_backup};
use chm_harness_sdk::adapter::types::{ApplyResult, NativeChange, NativePlan, ValidationReport};
use serde_json::{Map, Value};

/// One change per managed subtree. `after` is computed by merging desired
/// model entries into the `provider.<id>.models` object, preserving all other keys.
pub fn plan_model_add(file_path: &str, provider_id: &str, model_id: &str, display_name: &str) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| "{}".into());
    let after = merge_model(&raw, provider_id, model_id, display_name);
    NativeChange {
        file_path: file_path.to_string(),
        before: Some(raw),
        after: Some(after),
    }
}

pub fn merge_model(raw: &str, provider_id: &str, model_id: &str, display_name: &str) -> String {
    let mut doc: Value = serde_json::from_str(raw).unwrap_or(Value::Object(Map::new()));
    let providers = doc
        .as_object_mut()
        .unwrap()
        .entry("provider")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let pv = providers
        .entry(provider_id)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let models = pv.entry("models").or_insert_with(|| Value::Object(Map::new())).as_object_mut().unwrap();
    models.insert(
        model_id.to_string(),
        Value::Object(Map::from_iter([("name".to_string(), Value::String(display_name.to_string()))])),
    );
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| raw.to_string())
}

pub fn apply_native_plan(plan: &NativePlan) -> Result<ApplyResult, String> {
    let mut result = ApplyResult { files_written: vec![], links_created: vec![] };
    for change in &plan.changes {
        let after = change.after.clone().ok_or("change without after content")?;
        let backup = backup_file(std::path::Path::new(&change.file_path)).map_err(|e| e.to_string())?;
        let _ = backup;
        atomic_write(std::path::Path::new(&change.file_path), &after).map_err(|e| e.to_string())?;
        result.files_written.push(change.file_path.clone());
    }
    Ok(result)
}

pub fn validate_config(file_path: &str) -> ValidationReport {
    match std::fs::read_to_string(file_path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(_) => ValidationReport { ok: true, errors: vec![] },
            Err(e) => ValidationReport { ok: false, errors: vec![format!("opencode.json no longer valid JSON: {e}")] },
        },
        Err(e) => ValidationReport { ok: false, errors: vec![format!("cannot read opencode.json: {e}")] },
    }
}

pub fn restore(file_path: &str, backup: &str) -> Result<(), String> {
    restore_backup(std::path::Path::new(backup), std::path::Path::new(file_path)).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Implement the adapter's write methods in `lib.rs`**

```rust
use chm_harness_sdk::adapter::types::{
    ApplyResult, NativeLink, NativePlan, ValidationReport,
};
use chm_reconciliation::plan::{PlanAction, ReconciliationPlan};

impl HarnessAdapter for OpenCodeAdapter {
    fn plan(&self, plan: &ReconciliationPlan, _install: &HarnessInstallation) -> Result<NativePlan, AdapterError> {
        // version gate: read-only for unknown versions (project plan §57)
        let version = _install.version.as_deref();
        if !parse_version_supported(version, &["0.30", "0.31", "0.32"]) {
            return Ok(NativePlan {
                changes: vec![],
                links: vec![],
                warnings: vec![format!("OpenCode {version:?} untested — read-only mode")],
            });
        }
        let mut changes: Vec<NativeChange> = vec![];
        let mut links: Vec<NativeLink> = vec![];
        let mut warnings = vec![];
        let config_path = _install.config_path.as_deref().unwrap_or("").to_string();

        for action in &plan.actions {
            match action {
                PlanAction::Add(a) if a.kind == "model" => {
                    let provider_id = a.payload.get("native_provider_id")
                        .and_then(|v| v.as_str()).unwrap_or("custom");
                    let model_id = a.payload.get("remote_model_id").and_then(|v| v.as_str()).unwrap_or("");
                    let display = a.payload.get("display_name").and_then(|v| v.as_str()).unwrap_or(model_id);
                    changes.push(plan_model_add(&config_path, provider_id, model_id, display));
                }
                PlanAction::Remove(r) if r.kind == "model" => {
                    warnings.push(format!("model removal for {} deferred to phase 12 (bindings)", r.identity));
                }
                PlanAction::Unsupported(u) => warnings.push(format!("unsupported: {}", u.reason)),
                PlanAction::Conflict(c) => warnings.push(format!("conflict on {}: {}", c.identity, c.reason)),
                _ => {}
            }
        }
        Ok(NativePlan { changes, links, warnings })
    }

    fn apply(&self, _install: &HarnessInstallation, native_plan: &NativePlan) -> Result<ApplyResult, AdapterError> {
        writer::apply_native_plan(native_plan).map_err(|e| AdapterError::Invalid(e))
    }

    fn validate(&self, install: &HarnessInstallation) -> Result<ValidationReport, AdapterError> {
        let path = install.config_path.as_deref().unwrap_or("").to_string();
        Ok(writer::validate_config(&path))
    }

    fn rollback(&self, _install: &HarnessInstallation, _native_plan: &NativePlan) -> Result<(), AdapterError> {
        // execute_sync restores from filesystem backups; adapter-level rollback
        // is a no-op here because apply_native_plan never partially mutates
        // (each atomic_write is complete). Kept for adapters with link steps.
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests + commit**

```bash
cargo test -p opencode-adapter
cargo test -p coding-harness-manager --test sync_flow_tests   # in apps/desktop/src-tauri
git add adapters/opencode apps/desktop/src-tauri
git commit -m "feat(phase8): opencode writable adapter + green sync flow"
```

Expected: write tests + `execute_sync_applies_and_records_snapshots` now pass.

---

### Task 8.4: Pi Writable Adapter

**Files:**
- Modify: `adapters/pi/src/lib.rs`
- Create: `adapters/pi/src/writer.rs`
- Create: `adapters/pi/tests/write_fixtures.rs`

**Interfaces:**
- Consumes: contract from 8.2.
- Produces: Pi `plan/apply/validate/rollback` editing `~/.pi/agent/config.toml` with `toml_edit` (preserves comments + unmanaged sections).

- [ ] **Step 1: Write the failing golden test**

Fixture-based: load `fixtures/pi/<version>/config-toml-full.toml`, plan an Add for model `glm-5-air`, serialize, assert: (a) output parses back as TOML, (b) the new model entry exists, (c) a known unmanaged key (e.g. `[theme]` or whatever the Phase 0 doc shows) is unchanged. Add `toml_edit = "0.22"` to deps.

- [ ] **Step 2: Implement `writer.rs`**

```rust
//! Pi writer: toml_edit-based minimal-subtree edits.

use chm_filesystem::atomic_write;
use chm_harness_sdk::adapter::types::{ApplyResult, NativeChange, NativePlan, ValidationReport};
use toml_edit::{DocumentMut, Item, Table, Value};

pub fn plan_model_add(file_path: &str, model_id: &str, display_name: &str) -> NativeChange {
    let raw = std::fs::read_to_string(file_path).unwrap_or_else(|_| String::new());
    let after = merge_model(&raw, model_id, display_name);
    NativeChange { file_path: file_path.into(), before: Some(raw), after: Some(after) }
}

pub fn merge_model(raw: &str, model_id: &str, display_name: &str) -> String {
    let mut doc: DocumentMut = raw.parse().unwrap_or_default();
    let models = doc.entry("model").or_insert(Item::Table(Table::new()));
    let table = models.as_table_mut().expect("model must be a table");
    table.insert(model_id, Item::Value(Value::from_iter([("display_name", Value::from(display_name))])));
    doc.to_string()
}

pub fn apply_native_plan(plan: &NativePlan) -> Result<ApplyResult, String> {
    // same pattern as opencode writer::apply_native_plan
    let mut result = ApplyResult { files_written: vec![], links_created: vec![] };
    for change in &plan.changes {
        let after = change.after.clone().ok_or("change without after")?;
        atomic_write(std::path::Path::new(&change.file_path), &after).map_err(|e| e.to_string())?;
        result.files_written.push(change.file_path.clone());
    }
    Ok(result)
}

pub fn validate_config(file_path: &str) -> ValidationReport {
    match std::fs::read_to_string(file_path) {
        Ok(raw) => match raw.parse::<DocumentMut>() {
            Ok(_) => ValidationReport { ok: true, errors: vec![] },
            Err(e) => ValidationReport { ok: false, errors: vec![format!("config.toml invalid: {e}")] },
        },
        Err(e) => ValidationReport { ok: false, errors: vec![format!("cannot read: {e}")] },
    }
}
```

- [ ] **Step 3: Implement `plan/apply/validate/rollback` in `lib.rs`** — mirror OpenCode (version gate list from `docs/harnesses/pi.md`), mapping `PlanAction::Add` model → `plan_model_add`, warnings for unsupported/conflict.

- [ ] **Step 4: Run tests + commit**

```bash
cargo test -p pi-adapter
git add adapters/pi
git commit -m "feat(phase8): pi writable adapter"
```

---

### Task 8.5: Codex Writable Adapter

**Files:**
- Modify: `adapters/codex/src/lib.rs`
- Create: `adapters/codex/src/writer.rs`
- Create: `adapters/codex/tests/write_fixtures.rs`

**Interfaces:**
- Consumes: contract from 8.2.
- Produces: Codex `plan/apply/validate/rollback` editing `~/.codex/config.toml` — the `[models.<id>]` table (name, provider, model, temperature, reasoning_effort per Phase 0 doc) and, when the desired route's endpoint uses a non-standard base URL, the `[model_providers.<id>]` table (base_url, env_key, wire_api).

- [ ] **Step 1: Write the failing golden test**

Fixture: load `fixtures/codex/<version>/config-toml-full.toml`; plan: Add model `glm-5` with provider `zai`, wire_api `responses`, base_url `https://api.z.ai/v1`; assert output has `models.glm-5.provider == "zai"`, `model_providers.zai.base_url` set, and the pre-existing `[model]` selection key unchanged. Note: planning writes BOTH model and provider tables in one `NativeChange` (one file, one rewrite).

- [ ] **Step 2: Implement `writer.rs`** (same structure as Pi's, with the two-table merge + `wire_api` mapping from `route.overrides.wire_api`).

- [ ] **Step 3: Implement the adapter write methods** — version gate list from `docs/harnesses/codex.md`; on Add actions, read `payload.native_provider_id` (default "custom"), `payload.overrides.wire_api`; produce the single-file change.

- [ ] **Step 4: Run tests + commit**

```bash
cargo test -p codex-adapter
git add adapters/codex
git commit -m "feat(phase8): codex writable adapter"
```

---

### Task 8.6: Claude Code Writable Adapter

**Files:**
- Modify: `adapters/claude-code/src/lib.rs`
- Create: `adapters/claude-code/src/writer.rs`
- Create: `adapters/claude-code/tests/write_fixtures.rs`

**Interfaces:**
- Consumes: contract from 8.2.
- Produces: Claude Code `plan/apply/validate/rollback` editing `~/.claude/settings.json` `env` block: for each Add model with `capabilities.role` (opus/sonnet/haiku), set `env.ANTHROPIC_DEFAULT_<ROLE>_MODEL`; base URL overrides via `env.ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` placeholder per Phase 0 doc. MCP adds target `~/.claude.json` `mcpServers` (second file — plan produces one change per file).

- [ ] **Step 1: Write the failing golden test**

Fixture: load `fixtures/claude-code/<version>/settings-full.json`; plan Add model with role "sonnet" → assert `env["ANTHROPIC_DEFAULT_SONNET_MODEL"]` set, unmanaged `env` keys and all other settings keys preserved. Second test: MCP add writes into `claude-json-mcp.json`'s `mcpServers` with command/args/env from the desired `McpServer`.

- [ ] **Step 2: Implement `writer.rs`** — JSON merge helpers `merge_env_setting(raw, key, value)` and `merge_mcp_server(raw_claude_json, name, spec)` (both preserve all other keys), plus `apply_native_plan`/`validate_config` (validate: JSON parse of both files).

- [ ] **Step 3: Implement adapter write methods** — `plan()` maps Add model (role from capabilities) → settings.json change; Add mcp → claude.json change; collects warnings for the rest.

- [ ] **Step 4: Run tests + commit**

```bash
cargo test -p claude-code-adapter
git add adapters/claude-code
git commit -m "feat(phase8): claude-code writable adapter"
```

---

### Task 8.7: Reasonix Writable Adapter

**Files:**
- Modify: `adapters/reasonix/src/lib.rs`
- Create: `adapters/reasonix/src/writer.rs`
- Create: `adapters/reasonix/tests/write_fixtures.rs`

**Interfaces:**
- Consumes: contract from 8.2 + Phase 0 Reasonix doc.
- Produces: Reasonix write methods per its documented native format (mirror Pi if TOML, OpenCode if JSON). If fixtures are still pending, mark write tests `#[ignore]` with the blocker noted and implement `plan()` returning `warnings: ["reasonix write support pending phase-0 fixtures"]` with empty changes.

- [ ] **Step 1–4: Mirror Task 8.4 or 8.3 per the doc; test + commit**

```bash
cargo test -p reasonix-adapter
git add adapters/reasonix
git commit -m "feat(phase8): reasonix writable adapter"
```

---

### Task 8.8: Sync UI + Phase Exit

**Files:**
- Create: `apps/desktop/src/screens/HarnessesScreen.tsx` (replaces `/scan` placeholder content — per project plan §17 harness detail tabs)
- Create: `apps/desktop/src/components/SyncDialog.tsx`
- Create: `apps/desktop/src/hooks/useSync.ts`
- Modify: `apps/desktop/src/App.tsx`, `api.ts`

**Interfaces:**
- Consumes: `sync_preview`, `sync_apply` commands; `useInstallations`.
- Produces: the push workflow (project plan §48): select models → select harnesses → mode → preview → apply.

- [ ] **Step 1: Extend `api.ts` + `useSync.ts`**

Types `PreviewReport`, `ApplyReport`, `ActionView`, `FilePreview`; functions `syncPreview(installationId, mode)`, `syncApply(installationId, mode, force)`; hooks `useSyncPreview(installationId, mode)` (query, enabled when both set), `useSyncApply()` (mutation → invalidate `["installations"]` + `["routes"]`).

- [ ] **Step 2: Write `SyncDialog.tsx`**

Modal: harness name, mode radio (Append/update vs Replace managed), summary line from preview, action table (kind/identity/action with color badges: add green, update amber, remove red, conflict orange, unsupported gray), expandable file diff (before → after side by side in `<pre>`), [Apply] button (disabled when any Conflict/Unsupported without `force` checkbox "Apply despite conflicts (advanced)"), success → report summary + validation errors.

- [ ] **Step 3: `HarnessesScreen.tsx`**

List of installations (from `useInstallations`) each with sync status badge ("In Sync"/"Pending"/"Unsupported" placeholder until Phase 12) and a "Sync…" button opening `SyncDialog`. Detail view tabs (Overview/Models/MCP/Skills per §17) showing adapter `read_state` results via `read_harness_state` — Models/MCP/Skills tabs become real in Phase 9–10; here they show counts + warning lists.

- [ ] **Step 4: End-to-end manual verification**

With a real OpenCode config: select models in `/models`, open harness → Sync… → Append → Preview shows the file diff → Apply → config file updated → re-scan shows models → repeat sync → "0 items will change". Then modify the config file by hand (add a random model) → Replace Managed with that model unmanaged → confirm nothing unrelated is removed.

- [ ] **Step 5: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase8): sync UI with preview and apply"
```

Phase complete when all steps green.