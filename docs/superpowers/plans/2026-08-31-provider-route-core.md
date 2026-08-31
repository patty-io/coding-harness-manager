# Provider Route Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make provider, endpoint, protocol, credential reference, and models one secret-free deployment unit throughout CHM planning.

**Architecture:** Add normalized provider-route bundle and structured adapter capability types to the harness SDK. Build desired state from endpoint-backed bundles, reject incompatible bundles before native planning, and remove all partial-success/`custom` fallback paths.

**Tech Stack:** Rust, serde, sqlx, Tauri, React/TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-08-31-provider-route-portability-design.md`

## Global Constraints

- A model sync succeeds only when provider, protocol, endpoint, credential, and model are usable together.
- Plans, previews, hashes, logs, diagnostics, and SQLite never contain resolved credential values.
- CHM never edits `.zshrc`, `.bashrc`, or a general project `.env`.
- Incompatible routes are blockers before mutation; force cannot bypass them.
- Adapters never invent `custom` when CHM knows the endpoint's provider identity.

---

## File structure

- Create `crates/harness-sdk/src/adapter/route.rs`: normalized route bundle, protocol, capability, credential-target, and compatibility types.
- Modify `crates/harness-sdk/src/adapter/mod.rs`: export route contract.
- Modify `crates/harness-sdk/src/adapter/types.rs`: attach structured route capabilities to adapters.
- Modify `crates/harness-sdk/src/adapter/plan.rs`: carry bundles in desired state and safe blocker details.
- Modify `apps/desktop/src-tauri/src/commands/sync.rs`: group routes by endpoint and build secret-free bundles.
- Modify `crates/reconciliation/src/engine.rs`: run route compatibility before resource reconciliation.
- Modify `apps/desktop/src/lib/api.ts` and `apps/desktop/src/components/SyncDialog.tsx`: render route blockers and prohibit forced partial apply.

### Task 1: Define the route deployment contract

**Files:**
- Create: `crates/harness-sdk/src/adapter/route.rs`
- Modify: `crates/harness-sdk/src/adapter/mod.rs`
- Modify: `crates/harness-sdk/src/adapter/types.rs`
- Test: `crates/harness-sdk/src/adapter/route.rs`

**Interfaces:**
- Produces: `CredentialTarget`, `ProviderTopology`, `CredentialRequirement`, `ProviderRouteBundle`, `RouteDeploymentCapabilities`, and `RouteCompatibility`; reuses core `Protocol`, `AuthType`, and `CredentialRef`.

- [ ] **Step 1: Write failing unit tests for protocol and topology compatibility**

```rust
#[test]
fn rejects_protocol_and_topology_mismatches() {
    let bundle = fixture_bundle(Protocol::OpenAiChatCompletions);
    let responses_only = fixture_caps(
        ProviderTopology::Multiple,
        [Protocol::OpenAiResponses],
    );
    assert_eq!(
        responses_only.check(&bundle),
        RouteCompatibility::Blocked {
            reason: "OpenAI Chat is not supported by this harness".into()
        }
    );

    let fixed = fixture_caps(ProviderTopology::FixedProvider, [Protocol::OpenAiChatCompletions]);
    assert!(matches!(fixed.check(&bundle), RouteCompatibility::Blocked { .. }));
}
```

- [ ] **Step 2: Run the tests and verify the contract does not exist**

Run: `cargo test -p chm-harness-sdk adapter::route -- --nocapture`  
Expected: FAIL because `adapter::route` and the route types are undefined.

- [ ] **Step 3: Implement the serializable-safe route types**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialRequirement {
    None,
    Secret {
        credential_ref: CredentialRef,
        auth_type: AuthType,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRouteBundle {
    pub provider_id: String,
    pub display_name: String,
    pub endpoint_id: Uuid,
    pub base_url: String,
    pub protocol: Protocol,
    pub credential: CredentialRequirement,
    pub models: Vec<ModelRoute>,
}
```

Implement `RouteDeploymentCapabilities::check(&ProviderRouteBundle)` with exact protocol, topology, credential-target, and model-identity checks. Do not add a catch-all protocol.

- [ ] **Step 4: Run SDK tests**

Run: `cargo test -p chm-harness-sdk adapter::route -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/harness-sdk/src/adapter
git commit -m "feat(sync): define portable provider route contract"
```

### Task 2: Build endpoint-backed route bundles

**Files:**
- Modify: `crates/harness-sdk/src/adapter/plan.rs`
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs`
- Test: `apps/desktop/src-tauri/src/commands/sync.rs`

**Interfaces:**
- Consumes: `ProviderRouteBundle` and `CredentialRequirement` from Task 1.
- Produces: `DesiredState::provider_routes: Vec<ProviderRouteBundle>` and `group_provider_routes(...)`.

- [ ] **Step 1: Replace the current route decoration test with failing bundle tests**

```rust
#[test]
fn groups_models_by_endpoint_without_resolving_credentials() {
    let endpoint = fixture_endpoint(CredentialKind::Keychain, "coding-harness-manager/providers/yolo");
    let routes = vec![fixture_route(endpoint.id, "qwen-a"), fixture_route(endpoint.id, "qwen-b")];
    let bundles = group_provider_routes(&routes, &[fixture_provider()], &[endpoint]).unwrap();
    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].provider_id, "yolo-auto");
    assert_eq!(bundles[0].models.len(), 2);
    assert_eq!(bundles[0].credential.kind, CredentialKind::Keychain);
}

#[test]
fn endpoint_without_provider_or_credential_is_an_error() {
    assert!(group_provider_routes(&[fixture_route(Uuid::nil(), "qwen")], &[], &[]).is_err());
}
```

- [ ] **Step 2: Run the failing sync tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands::sync::tests::groups_models -- --nocapture`
Expected: FAIL because desired state still decorates flat routes and drops non-env credentials.

- [ ] **Step 3: Implement `group_provider_routes` and change desired state**

Delete `EndpointProviderInfo::api_key_env` and `route_with_endpoint_provider`. Load providers, endpoints, and credential references once; group enabled selected routes by endpoint ID; require canonical provider identity and a credential reference unless the endpoint explicitly declares `AuthType::None`.

```rust
pub struct DesiredState {
    pub provider_routes: Vec<ProviderRouteBundle>,
    pub mcp_servers: Vec<McpServer>,
    pub skills: Vec<Skill>,
}
```

Retain `routes()` as a flattening iterator only where model reconciliation still needs it. The iterator must borrow bundle models and must not clone or mutate provider metadata.

- [ ] **Step 4: Run sync and reconciliation tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands::sync -- --nocapture && cargo test -p chm-reconciliation`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/harness-sdk/src/adapter/plan.rs apps/desktop/src-tauri/src/commands/sync.rs crates/reconciliation
git commit -m "refactor(sync): plan endpoint-backed provider bundles"
```

### Task 3: Make compatibility blockers non-bypassable

**Files:**
- Modify: `crates/reconciliation/src/engine.rs`
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Modify: `apps/desktop/src/components/SyncDialog.tsx`
- Test: `crates/reconciliation/src/engine.rs`
- Test: `apps/desktop/src/components/SyncDialog.test.tsx`

**Interfaces:**
- Consumes: `HarnessAdapter::route_capabilities()` and `DesiredState::provider_routes`.
- Produces: `RouteBlockerView { provider_id, model_ids, reason }` on `PreviewReport`.

- [ ] **Step 1: Write failing reconciliation and UI tests**

```rust
#[test]
fn incompatible_bundle_is_a_route_blocker_not_a_model_action() {
    let plan = reconcile_with_capabilities(&desired_chat_bundle(), &actual_empty(), &responses_caps()).unwrap();
    assert!(matches!(&plan.actions[0], PlanAction::Unsupported(x)
        if x.kind == "provider-route" && x.identity == "yolo-auto"));
    assert_eq!(plan.count("model"), 0);
}
```

```tsx
it("does not offer force or Apply when a route is blocked", async () => {
  mockPreview({ hasBlockers: true, routeBlockers: [{ providerId: "yolo-auto", modelIds: ["qwen"], reason: "protocol mismatch" }] });
  render(<SyncDialog installationId="i" harnessType="codex" onClose={() => {}} />);
  expect(await screen.findByText(/protocol mismatch/)).toBeVisible();
  expect(screen.queryByText(/apply despite/i)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /apply/i })).toBeDisabled();
});
```

- [ ] **Step 2: Run tests and verify current force behavior fails them**

Run: `cargo test -p chm-reconciliation incompatible_bundle -- --nocapture && npm test -- SyncDialog.test.tsx`  
Expected: FAIL because unsupported actions are generated after flat reconciliation and the dialog exposes force.

- [ ] **Step 3: Implement pre-reconciliation compatibility and blocker UI**

Run `capabilities.check(bundle)` before `reconcile_models`. Add one safe `UnsupportedAction` per blocked provider route. Remove the force checkbox for unsupported actions; retain force only for resolvable unmanaged conflicts if the existing conflict policy still requires it. `validate_apply_request` must reject any `provider-route` blocker regardless of `force`.

- [ ] **Step 4: Run targeted tests**

Run: `cargo test -p chm-reconciliation && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands::sync && npm test -- SyncDialog.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reconciliation apps/desktop/src-tauri/src/commands/sync.rs apps/desktop/src/lib/api.ts apps/desktop/src/components/SyncDialog.tsx apps/desktop/src/components/SyncDialog.test.tsx
git commit -m "fix(sync): block incompatible provider routes before mutation"
```

### Task 4: Remove provider fallback and bind complete routes

**Files:**
- Modify: `crates/reconciliation/src/models.rs`
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs`
- Modify: `adapters/detection/src/writer.rs`
- Test: `apps/desktop/src-tauri/src/commands/sync.rs`

**Interfaces:**
- Consumes: canonical `provider_id` on every `ProviderRouteBundle`.
- Produces: bindings with `endpoint_id`, `provider_id`, `protocol`, and safe `credential_ref_id` metadata.

- [ ] **Step 1: Write failing regression tests**

```rust
#[test]
fn known_provider_is_never_rewritten_as_custom() {
    let action = first_model_add(&reconcile_bundle(yolo_bundle(), actual_empty()));
    assert_eq!(action.native_provider_id.as_deref(), Some("yolo-auto"));
    assert!(!serde_json::to_string(action).unwrap().contains("\"custom\""));
}

#[test]
fn binding_records_complete_safe_route_identity() {
    let config = binding_config(&yolo_bundle());
    assert_eq!(config["provider_id"], "yolo-auto");
    assert_eq!(config["endpoint_id"], yolo_endpoint_id().to_string());
    assert!(config.get("credential_value").is_none());
}
```

- [ ] **Step 2: Run tests and observe the detection writer's `custom` fallback**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml known_provider -- --nocapture`
Expected: FAIL while `adapters/detection/src/writer.rs` still uses `unwrap_or("custom")`.

- [ ] **Step 3: Require provider identity and persist complete safe bindings**

Replace every provider fallback with an `AdapterError::Invalid` or a pre-plan blocker. Extend binding metadata with endpoint/provider/protocol/credential-reference IDs; never store a resolved credential.

- [ ] **Step 4: Run the core phase gates**

Run: `cargo test -p chm-harness-sdk -p chm-reconciliation -p coding-harness-manager && npm test -- SyncDialog.test.tsx && npm run lint`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reconciliation apps/desktop/src-tauri/src/commands/sync.rs adapters/detection/src/writer.rs
git commit -m "fix(sync): preserve canonical provider identity in bindings"
```
