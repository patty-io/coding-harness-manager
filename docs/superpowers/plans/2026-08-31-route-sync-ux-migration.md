# Route Sync UX and Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repair existing partial bindings, present complete route outcomes in the UI, and document one truthful supported-harness philosophy.

**Architecture:** Audit existing bindings against route-level validation, mark incomplete rows for repair, and surface Ready/Blocked/Rolled back outcomes in the sync preview. Finish with English/Korean documentation, security policy updates, and a temporary-home end-to-end release matrix.

**Tech Stack:** Rust/sqlx migrations, Tauri IPC, React/TypeScript, Vitest, Markdown.

**Spec:** `docs/superpowers/specs/2026-08-31-provider-route-portability-design.md`

## Global Constraints

- Existing user configs are never rewritten during migration without Apply.
- `needsRepair` is derived from safe route validation and contains no secret.
- The UI shows provider, protocol, credential destination, model, and target harness together.
- Unsupported and missing-credential routes cannot be force-applied.
- English and Korean README files use one supported-harness list without primary/additional tiers.

---

## File structure

- Create `crates/database/migrations/*_route_binding_state.sql`: safe route binding metadata.
- Modify `crates/core/src/domain/harness.rs` and database repositories: route status and repair reason.
- Modify `apps/desktop/src-tauri/src/commands/sync.rs`, `dashboard.rs`, and `harness_detail.rs`: audit and expose route readiness.
- Modify `apps/desktop/src/lib/api.ts` and `apps/desktop/src/components/SyncDialog.tsx`: route-oriented preview.
- Modify `README.md`, `README.ko.md`, and `SECURITY.md`: public behavior and security contract.
- Create `apps/desktop/src-tauri/tests/route_portability_e2e.rs`: temporary-home release matrix.

### Task 1: Migrate and audit existing partial bindings

**Files:**
- Create: `crates/database/migrations/<next>_route_binding_state.sql`
- Modify: `crates/core/src/domain/harness.rs`
- Modify: `crates/database/src/repos/harness.rs`
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs`
- Test: `apps/desktop/src-tauri/tests/sync_flow_tests.rs`

**Interfaces:**
- Produces: `RouteBindingStatus::{Ready, NeedsRepair, Blocked}` and `repair_reason`.

- [ ] **Step 1: Write a failing migration/audit test**

```rust
#[tokio::test]
async fn legacy_model_only_binding_is_marked_needs_repair_without_writing_harness() {
    let fixture = legacy_yolo_opencode_binding().await;
    let before = std::fs::read(&fixture.opencode_config).unwrap();
    audit_route_bindings(&fixture.pool, &fixture.adapter).await.unwrap();
    let binding = load_binding(&fixture.pool, fixture.binding_id).await.unwrap();
    assert_eq!(binding.route_status, RouteBindingStatus::NeedsRepair);
    assert_eq!(binding.repair_reason.as_deref(), Some("provider credential is not configured in OpenCode"));
    assert_eq!(std::fs::read(&fixture.opencode_config).unwrap(), before);
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p coding-harness-manager legacy_model_only_binding -- --nocapture`  
Expected: FAIL because binding readiness is not stored or audited.

- [ ] **Step 3: Add safe status columns and audit logic**

Add non-null status with a conservative `needs_repair` default for legacy rows and nullable safe reason. Audit installed bindings through adapter route validation after scan; never resolve a secret merely to render the dashboard and never mutate native files.

- [ ] **Step 4: Run migration and sync tests**

Run: `cargo test -p chm-database && cargo test -p coding-harness-manager sync_flow_tests -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/database crates/core apps/desktop/src-tauri/src/commands/sync.rs apps/desktop/src-tauri/tests/sync_flow_tests.rs
git commit -m "feat(sync): audit legacy bindings for route repair"
```

### Task 2: Replace file-centric sync feedback with route outcomes

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Modify: `apps/desktop/src/components/SyncDialog.tsx`
- Test: `apps/desktop/src/components/SyncDialog.test.tsx`

**Interfaces:**
- Produces: `RoutePreviewView` and `RouteApplyOutcome` wire types.

- [ ] **Step 1: Write failing UI tests**

```tsx
it("shows the complete deployment route without exposing the key", async () => {
  mockPreview({
    routes: [{
      providerName: "Yolo-Auto",
      modelIds: ["qwen3.8-27b"],
      protocol: "OpenAI Chat",
      credentialTarget: "OpenCode native auth store",
      targetHarness: "OpenCode",
      status: "ready",
      reason: null,
    }],
  });
  render(<SyncDialog installationId="i" harnessType="opencode" onClose={() => {}} />);
  expect(await screen.findByText("Yolo-Auto")).toBeVisible();
  expect(screen.getByText("qwen3.8-27b")).toBeVisible();
  expect(screen.getByText("OpenCode native auth store")).toBeVisible();
  expect(document.body.textContent).not.toMatch(/sk-[A-Za-z0-9]/);
});
```

- [ ] **Step 2: Run and verify failure**

Run: `cd apps/desktop && npm test -- SyncDialog.test.tsx`  
Expected: FAIL because the dialog currently renders flat actions and file counts.

- [ ] **Step 3: Add route cards and exact outcome copy**

Render one card per provider bundle with provider, model list, protocol, credential destination, and target. Use Ready, Blocked, or Failed and rolled back. Keep ordinary file diffs in a secondary expandable section. Remove the unsupported-force checkbox entirely; conflicts may retain a separate advanced resolution flow only when no route blocker exists.

- [ ] **Step 4: Run frontend tests and accessibility checks**

Run: `cd apps/desktop && npm test -- SyncDialog.test.tsx && npm run lint && npm run build`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/commands/sync.rs apps/desktop/src/lib/api.ts apps/desktop/src/components/SyncDialog.tsx apps/desktop/src/components/SyncDialog.test.tsx
git commit -m "feat(ui): show complete provider route sync outcomes"
```

### Task 3: Add repair actions to harness and dashboard views

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/dashboard.rs`
- Modify: `apps/desktop/src-tauri/src/commands/harness_detail.rs`
- Modify: `apps/desktop/src/screens/DashboardScreen.tsx`
- Modify: `apps/desktop/src/screens/HarnessDetailScreen.tsx`
- Test: screen test files.

**Interfaces:**
- Consumes: `RouteBindingStatus` from Task 1.
- Produces: “Repair route…” action that opens selection-scoped sync for the affected bundle.

- [ ] **Step 1: Write failing screen test**

```tsx
it("offers route repair instead of showing a misleading installed model", async () => {
  mockHarnessModel({ nativeId: "qwen3.8-27b", providerName: "Yolo-Auto", routeStatus: "needsRepair", repairReason: "credential missing" });
  render(<HarnessDetailScreen />);
  expect(await screen.findByText("Credential missing")).toBeVisible();
  await user.click(screen.getByRole("button", { name: /repair route/i }));
  expect(screen.getByRole("dialog", { name: /sync opencode/i })).toBeVisible();
});
```

- [ ] **Step 2: Run and verify failure**

Run: `cd apps/desktop && npm test -- HarnessDetailScreen`  
Expected: FAIL because route readiness is not exposed.

- [ ] **Step 3: Implement repair status and action**

Show concise readiness beside provider attribution. “Repair route…” opens `SyncDialog` with all model IDs in the provider bundle. Do not add another per-row Import action. Dashboard aggregates incomplete bundles as a single harness-level warning.

- [ ] **Step 4: Run frontend suite**

Run: `cd apps/desktop && npm test && npm run lint && npm run build`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/commands/dashboard.rs apps/desktop/src-tauri/src/commands/harness_detail.rs apps/desktop/src/screens
git commit -m "feat(ui): surface and repair incomplete provider routes"
```

### Task 4: Update public documentation and security promises

**Files:**
- Modify: `README.md`
- Modify: `README.ko.md`
- Modify: `SECURITY.md`
- Modify: `adapters/detection/src/lib.rs`
- Modify: `crates/harness-sdk/src/definition.rs`

**Interfaces:**
- Produces: one supported-harness list and capability-scoped portability explanation in both languages.

- [ ] **Step 1: Add a documentation assertion test or script check**

```bash
for file in README.md README.ko.md; do
  ! rg -n "Primary adapters|Additional format-aware|first-class set|detection-only" "$file"
  rg -n "Claude Code.*Codex.*OpenCode.*Pi.*Reasonix" "$file"
done
```

- [ ] **Step 2: Run and capture current tiered wording**

Run the script above.  
Expected: FAIL while README or source comments retain primary/additional language.

- [ ] **Step 3: Rewrite capability and security sections**

List all registered harnesses once. Explain that support is native-surface and route-specific: CHM either deploys the complete compatible route or blocks before writing. Document protected native credential deployment and clarify that CHM persistence remains OS-secret-store-only. Mirror meaning, examples, and limitations in Korean.

- [ ] **Step 4: Run doc checks and link validation**

Run: `for file in README.md README.ko.md; do ! rg -n "Primary adapters|Additional format-aware|first-class set|detection-only" "$file"; done && git diff --check`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add README.md README.ko.md SECURITY.md adapters/detection/src/lib.rs crates/harness-sdk/src/definition.rs
git commit -m "docs: describe complete route portability across harnesses"
```

### Task 5: Run the temporary-home end-to-end release matrix

**Files:**
- Create: `apps/desktop/src-tauri/tests/route_portability_e2e.rs`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: complete adapter matrix and protected credential coordinator.
- Produces: release gate proving no real user configuration is touched.

- [ ] **Step 1: Write the failing end-to-end test**

```rust
#[tokio::test]
async fn yolo_route_is_ready_or_precisely_blocked_for_every_adapter() {
    let home = TempDir::new().unwrap();
    let app = TestApp::new(home.path()).await.with_keychain("yolo", "sk-test");
    app.import_yolo_provider_and_qwen_model().await;

    for adapter in all_adapters() {
        seed_minimal_installation(home.path(), adapter.id());
        match app.sync_bundle(adapter.id(), "yolo-auto").await {
            RouteApplyOutcome::Ready(result) => assert_complete_native_route(adapter.id(), &result),
            RouteApplyOutcome::Blocked(blocker) => assert!(!blocker.reason.trim().is_empty()),
            RouteApplyOutcome::Failed(error) => panic!("{} failed instead of ready/blocked: {error}", adapter.id()),
        }
    }
    assert_no_secret_in_tree(home.path(), "sk-test");
}
```

- [ ] **Step 2: Run and expose remaining integration failures**

Run: `cargo test -p coding-harness-manager --test route_portability_e2e -- --nocapture`  
Expected: FAIL until every adapter returns Ready or a precise pre-write Blocked result.

- [ ] **Step 3: Fix integration boundaries and add CI gate**

Fix production code rather than weakening the matrix. Add the test to CI on macOS, Linux, and Windows with platform fake secret stores. Assert all paths remain inside the temporary home.

- [ ] **Step 4: Run full release verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/desktop && npm test && npm run lint && npm run build
```

Expected: all commands PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/tests/route_portability_e2e.rs .github/workflows/ci.yml
git commit -m "test(sync): gate complete route portability across adapters"
```
