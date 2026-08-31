# Protected Credential Deployment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve CHM credential references only during apply and transactionally deploy them to harness-native protected targets.

**Architecture:** Extend native plans with secret-free protected-change descriptors. A Tauri-side coordinator preflights credentials through `SecretStore`, delegates materialization to adapter credential writers, atomically applies ordinary and protected changes, validates the complete route, and restores both surfaces on failure.

**Tech Stack:** Rust, macOS Keychain/Windows Credential Manager/libsecret abstraction, serde_json, filesystem atomic writes, Unix permissions, sqlx.

**Spec:** `docs/superpowers/specs/2026-08-31-provider-route-portability-design.md`

## Global Constraints

- Resolved credentials are non-serializable and apply-scoped.
- Protected file contents never enter `NativePlan`, SQLite snapshots, logs, previews, or ordinary backups.
- All credentials are resolved before the first mutation.
- Ordinary and protected surfaces commit or roll back together.
- Protected files use owner-only permissions on Unix.

---

## File structure

- Create `crates/harness-sdk/src/adapter/protected.rs`: safe protected-change descriptors and apply-time materialization interface.
- Create `apps/desktop/src-tauri/src/services/credential_deployment.rs`: credential preflight, protected state capture, apply, validation, rollback.
- Modify `crates/harness-sdk/src/adapter/types.rs`: protected plan and route validation reports.
- Modify `apps/desktop/src-tauri/src/commands/sync.rs`: call the coordinator and keep audit records redacted.
- Modify `apps/desktop/src-tauri/src/services/transactions.rs`: restore protected state and clean snapshot credentials.
- Modify `crates/filesystem/src/lib.rs`: atomic protected writes with explicit permissions and change detection.
- Create `adapters/opencode/src/credentials.rs`: OpenCode native `auth.json` reader/writer/validator.

### Task 1: Add secret-free protected plans

**Files:**
- Create: `crates/harness-sdk/src/adapter/protected.rs`
- Modify: `crates/harness-sdk/src/adapter/mod.rs`
- Modify: `crates/harness-sdk/src/adapter/types.rs`
- Test: `crates/harness-sdk/src/adapter/protected.rs`

**Interfaces:**
- Produces: `ProtectedTarget`, `ProtectedOperation`, `ProtectedChangePlan`, `ResolvedCredential`, and `MaterializedProtectedChange`.

- [ ] **Step 1: Write failing serialization tests**

```rust
#[test]
fn protected_plan_serializes_reference_but_cannot_hold_value() {
    let plan = ProtectedChangePlan {
        target: ProtectedTarget::NativeSecretStore { provider_id: "yolo-auto".into() },
        credential_ref_id: Uuid::nil(),
        operation: ProtectedOperation::Upsert,
    };
    let json = serde_json::to_string(&plan).unwrap();
    assert!(json.contains("yolo-auto"));
    assert!(!json.contains("api_key"));
}

```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p chm-harness-sdk protected -- --nocapture`  
Expected: FAIL because protected plan types do not exist.

- [ ] **Step 3: Implement safe descriptors and extend `NativePlan`**

```rust
pub struct ResolvedCredential(Zeroizing<String>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedChangePlan {
    pub target: ProtectedTarget,
    pub credential_ref_id: Uuid,
    pub operation: ProtectedOperation,
}
```

Add `protected_changes: Vec<ProtectedChangePlan>` to `NativePlan` with `#[serde(default)]`. Implement only `ResolvedCredential::expose(&self) -> &str`; do not implement `Debug`, `Clone`, `Serialize`, or `Deserialize`. A compile-fail doctest demonstrates that `serde_json::to_string(&credential)` is rejected.

- [ ] **Step 4: Run SDK tests**

Run: `cargo test -p chm-harness-sdk`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/harness-sdk
git commit -m "feat(sync): add secret-free protected change plans"
```

### Task 2: Add protected atomic writes and concurrency checks

**Files:**
- Modify: `crates/filesystem/src/lib.rs`
- Test: `crates/filesystem/tests/atomic_write.rs`

**Interfaces:**
- Produces: `ProtectedWriteGuard::capture`, `ProtectedWriteGuard::replace`, and `ProtectedWriteGuard::restore`.

- [ ] **Step 1: Write failing filesystem tests**

```rust
#[cfg(unix)]
#[test]
fn protected_replace_is_atomic_owner_only_and_detects_concurrency() {
    let file = temp.path().join("auth.json");
    std::fs::write(&file, b"before").unwrap();
    let guard = ProtectedWriteGuard::capture(&file).unwrap();
    guard.replace(b"after", 0o600).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), b"after");
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);

    let stale = ProtectedWriteGuard::capture(&file).unwrap();
    std::fs::write(&file, b"external").unwrap();
    assert!(matches!(stale.replace(b"ours", 0o600), Err(FsError::ConcurrentChange(_))));
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p chm-filesystem protected_replace -- --nocapture`  
Expected: FAIL because `ProtectedWriteGuard` does not exist.

- [ ] **Step 3: Implement capture/hash/atomic replace/restore**

Capture path existence, bytes, SHA-256, and permissions in memory. Immediately before rename, re-hash the target and reject a mismatch. Write a same-directory temporary file, set permissions before rename, fsync the file, rename, and fsync the parent directory. Restore the captured bytes or remove a newly created target.

- [ ] **Step 4: Run filesystem tests on supported platforms**

Run: `cargo test -p chm-filesystem`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/filesystem
git commit -m "feat(filesystem): add protected atomic transaction guard"
```

### Task 3: Build the apply-time credential coordinator

**Files:**
- Create: `apps/desktop/src-tauri/src/services/credential_deployment.rs`
- Modify: `apps/desktop/src-tauri/src/services/mod.rs`
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs`
- Modify: `apps/desktop/src-tauri/src/services/transactions.rs`
- Test: `apps/desktop/src-tauri/src/services/credential_deployment.rs`

**Interfaces:**
- Consumes: `NativePlan::protected_changes`, endpoint `CredentialRef` rows, and `&dyn SecretStore`.
- Produces: `CredentialDeploymentTransaction::preflight`, `apply`, `validate`, `commit`, and `rollback`.

- [ ] **Step 1: Write failing transaction tests using an in-memory secret store**

```rust
#[test]
fn missing_second_secret_prevents_first_write() {
    let fs = FakeProtectedFs::new();
    let secrets = FakeSecrets::with([("first", "secret-one")]);
    let result = CredentialDeploymentTransaction::preflight(
        &two_provider_plan("first", "missing"),
        &secrets,
        &fs,
    );
    assert!(matches!(result, Err(DeploymentError::MissingCredential { .. })));
    assert!(fs.writes().is_empty());
}

#[test]
fn protected_failure_restores_ordinary_and_protected_state() {
    let mut tx = successful_preflight();
    tx.inject_failure_after_protected_write();
    assert!(tx.apply().is_err());
    assert_eq!(read("config.json"), ORIGINAL_CONFIG);
    assert_eq!(read("auth.json"), ORIGINAL_AUTH);
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p coding-harness-manager credential_deployment -- --nocapture`  
Expected: FAIL because the coordinator does not exist and sync begins ordinary backups before credential preflight.

- [ ] **Step 3: Implement preflight-first coordinated apply**

Resolve every `CredentialRef` through `state.secrets` before `begin_transaction` mutates files. Keep values in `ResolvedCredential`. Capture protected targets, apply ordinary adapter changes, materialize protected changes, run native and route validation, then commit. Any error restores protected guards and ordinary backups in reverse order.

Serialize only `RedactedNativePlan::from(&native_plan)` into `begin_transaction`; reject attempts to serialize materialized changes.

- [ ] **Step 4: Run sync transaction tests**

Run: `cargo test -p coding-harness-manager credential_deployment commands::sync services::transactions -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/services apps/desktop/src-tauri/src/commands/sync.rs
git commit -m "feat(sync): coordinate protected credential deployment"
```

### Task 4: Implement OpenCode native credential deployment through the general contract

**Files:**
- Create: `adapters/opencode/src/credentials.rs`
- Modify: `adapters/opencode/src/lib.rs`
- Modify: `adapters/opencode/src/writer.rs`
- Test: `adapters/opencode/tests/credential_sync.rs`

**Interfaces:**
- Consumes: provider-keyed `ProtectedChangePlan` and `ResolvedCredential` through the SDK materialization interface.
- Produces: OpenCode `auth.json` merge and route-level validation.

- [ ] **Step 1: Write the Pi/Yolo/OpenCode failing regression**

```rust
#[test]
fn deploys_provider_model_and_one_native_credential() {
    let home = TempDir::new().unwrap();
    seed_opencode_config(&home, r#"{"provider":{"existing":{}}}"#);
    seed_opencode_auth(&home, r#"{"existing":{"type":"api","key":"keep"}}"#);
    let result = apply_yolo_bundle(&home, "sk-yolo").unwrap();

    let config = read_jsonc(home.path().join(".config/opencode/opencode.jsonc"));
    assert_eq!(config["provider"]["yolo-auto"]["options"]["baseURL"], "https://yolo-auto.com/v1");
    assert!(config["provider"]["yolo-auto"]["models"].get("qwen3.8-27b").is_some());

    let auth = read_json(home.path().join(".local/share/opencode/auth.json"));
    assert_eq!(auth["yolo-auto"]["type"], "api");
    assert_eq!(auth["yolo-auto"]["key"], "sk-yolo");
    assert_eq!(auth["existing"]["key"], "keep");
    assert_eq!(result.validation.status, RouteStatus::Ready);
}
```

- [ ] **Step 2: Run and verify current model-only sync fails**

Run: `cargo test -p opencode-adapter --test credential_sync -- --nocapture`  
Expected: FAIL because no OpenCode auth writer exists.

- [ ] **Step 3: Implement strict auth merge and validation**

Read an absent file as `{}` and reject malformed/non-object JSON. Merge exactly `{ provider_id: { "type": "api", "key": resolved } }`, preserving other entries. Use `ProtectedWriteGuard` with `0600`. Validate config provider ID, base URL, selected models, and matching auth entry. Never include auth bytes in `NativeChange`.

- [ ] **Step 4: Run OpenCode and sync tests**

Run: `cargo test -p opencode-adapter && cargo test -p coding-harness-manager credential_deployment -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add adapters/opencode apps/desktop/src-tauri/src/services/credential_deployment.rs
git commit -m "fix(opencode): deploy provider credentials through native auth store"
```

### Task 5: Add protected history lifecycle and security documentation

**Files:**
- Modify: `crates/core/src/domain/history.rs`
- Modify: `crates/database/src/repos/history.rs`
- Modify: `apps/desktop/src-tauri/src/commands/history.rs`
- Modify: `SECURITY.md`
- Test: `apps/desktop/src-tauri/src/commands/history.rs`

**Interfaces:**
- Produces: redacted protected snapshot metadata and OS-store cleanup on rollback/deletion/expiry.

- [ ] **Step 1: Write failing lifecycle tests**

```rust
#[tokio::test]
async fn protected_snapshot_never_stores_secret_and_is_deleted_with_history() {
    let (pool, secrets) = fixture_history().await;
    let id = add_protected_snapshot(&pool, &secrets, "sk-secret").await.unwrap();
    let db = load_snapshot(&pool, id).await.unwrap();
    assert!(!serde_json::to_string(&db).unwrap().contains("sk-secret"));
    delete_transaction(&pool, &secrets, db.transaction_id).await.unwrap();
    assert!(secrets.get(&db.secret_snapshot_ref.unwrap()).unwrap().is_none());
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p coding-harness-manager protected_snapshot -- --nocapture`  
Expected: FAIL because protected snapshot metadata and cleanup do not exist.

- [ ] **Step 3: Implement opaque snapshot references and cleanup**

Store only path, hashes of redacted structure, operation, and opaque secret snapshot reference in SQLite. Store rollback material in `SecretStore`, delete it on successful rollback, explicit history deletion, provider deletion, and retention cleanup. Update `SECURITY.md` to distinguish CHM persistence from explicit protected native deployment.

- [ ] **Step 4: Run phase gates and secret scan**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && git grep -n "sk-yolo\|sk-secret" -- ':!**/tests/**' ':!docs/**'`  
Expected: tests and clippy PASS; secret scan returns no production matches.

- [ ] **Step 5: Commit**

```bash
git add crates/core crates/database apps/desktop/src-tauri/src/commands/history.rs SECURITY.md
git commit -m "feat(history): protect credential-bearing native snapshots"
```
