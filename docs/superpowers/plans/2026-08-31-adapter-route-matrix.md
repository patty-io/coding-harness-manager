# Adapter Route Matrix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every registered harness adapter a truthful, tested route capability declaration and native provider/model/credential implementation where upstream supports it.

**Architecture:** Each adapter declares provider topology, wire protocols, credential targets, identity rules, and metadata support. A shared fixture matrix deploys the same provider bundle to compatible adapters and asserts exact blockers for incompatible adapters.

**Tech Stack:** Rust adapter crates, JSON/JSONC/TOML/YAML fixture writers, harness-specific env files, command-backed credentials.

**Spec:** `docs/superpowers/specs/2026-08-31-provider-route-portability-design.md`

## Global Constraints

- Implement documented native formats only; no undocumented secret-store mutation.
- Every adapter has positive fixture tests and negative protocol/topology tests.
- A provider credential is deployed once per endpoint, not once per model.
- Adapter validation proves provider/model/credential resolution, not only parsing.
- Unsupported route deployment is an explicit blocker and never a silent no-op.

---

## File structure

- Modify each adapter's `lib.rs`, parser, writer, and fixture tests.
- Modify `adapters/detection/src/lib.rs`, `parser.rs`, and `writer.rs` for Kimi, Qwen, Gemini, Cursor, Cline, Roo, Aider, Amp, Goose, and Continue.
- Create `adapters/tests/route_matrix.rs`: cross-adapter contract suite.
- Create `apps/desktop/src-tauri/src/bin/chm-credential-helper.rs`: fixed OS-secret helper for documented command-backed targets.

### Task 1: Pi, Kimi, and Qwen native bundles

**Files:**
- Modify: `adapters/pi/src/lib.rs`
- Modify: `adapters/pi/src/writer.rs`
- Modify: `adapters/detection/src/lib.rs`
- Modify: `adapters/detection/src/writer.rs`
- Test: `adapters/pi/tests/read_fixtures.rs`
- Test: `adapters/detection/tests/provider_model_fixtures.rs`

**Interfaces:**
- Consumes: route capabilities and protected targets from earlier plans.
- Produces: Pi command/protected credential target, Kimi protected-config target, and Qwen settings + `~/.qwen/.env` target.

- [ ] **Step 1: Add failing fixture tests for all three native formats**

```rust
#[test]
fn qwen_writes_custom_provider_protocol_model_and_isolated_env() {
    let out = plan_bundle("qwen-code", yolo_chat_bundle()).unwrap();
    assert_json(&out, ".qwen/settings.json", |v| {
        assert_eq!(v["providerProtocol"]["yolo-auto"], "openai");
        assert_eq!(v["modelProviders"]["yolo-auto"][0]["id"], "qwen3.8-27b");
        assert_eq!(v["modelProviders"]["yolo-auto"][0]["envKey"], "CHM_YOLO_AUTO_API_KEY");
    });
    assert_protected_target(&out, ".qwen/.env", "CHM_YOLO_AUTO_API_KEY");
}
```

Add equivalent assertions for Pi provider `baseUrl`/`api`/model grouping and Kimi `[providers.yolo-auto]` plus `[models."qwen3.8-27b"]`.

- [ ] **Step 2: Run and verify failures**

Run: `cargo test -p pi-adapter -p detection-adapters provider_model -- --nocapture`  
Expected: FAIL because credentials and current Qwen provider/model capabilities are absent.

- [ ] **Step 3: Implement native writers and capability declarations**

Pi must include `apiKey` as provider configuration and remove an empty provider only when its last CHM-managed model is removed. Kimi writes provider type/base URL and protected key plus model context. Qwen writes `modelProviders`, `providerProtocol`, `envKey`, and a protected home `.qwen/.env`; do not use deprecated `security.auth.apiKey`.

- [ ] **Step 4: Run adapter tests**

Run: `cargo test -p pi-adapter -p detection-adapters`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add adapters/pi adapters/detection
git commit -m "feat(adapters): deploy complete routes to Pi Kimi and Qwen"
```

### Task 2: Codex, Claude Code, and Reasonix credential strategies

**Files:**
- Create: `apps/desktop/src-tauri/src/bin/chm-credential-helper.rs`
- Modify: `adapters/codex/src/lib.rs`
- Modify: `adapters/codex/src/writer.rs`
- Modify: `adapters/claude-code/src/lib.rs`
- Modify: `adapters/claude-code/src/writer.rs`
- Modify: `adapters/reasonix/src/lib.rs`
- Modify: `adapters/reasonix/src/writer.rs`
- Test: `apps/desktop/src-tauri/tests/credential_helper.rs`
- Test: adapter writer test modules.

**Interfaces:**
- Produces: `chm-credential-helper read --binding <uuid>`; Codex `auth.command`; Claude `apiKeyHelper`; Reasonix provider plus protected home `.env`.

- [ ] **Step 1: Write failing helper security and adapter tests**

```rust
#[test]
fn helper_rejects_unknown_binding_and_writes_only_secret_to_stdout() {
    let denied = run_helper(["read", "--binding", "unknown"]);
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&denied.stderr).contains("credential reference"));

    let allowed = run_helper_with_binding("binding-id", "sk-value");
    assert_eq!(allowed.stdout, b"sk-value\n");
}
```

Add Codex test asserting `[model_providers.yolo-auto.auth] command = <helper>` only for compatible protocol; Claude test asserting `apiKeyHelper`; Reasonix test asserting `api_key_env` and protected `.env`.

- [ ] **Step 2: Run and verify failures**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test credential_helper && cargo test -p codex-adapter -p claude-code-adapter -p reasonix-adapter`
Expected: FAIL because the helper and protected strategies are undefined.

- [ ] **Step 3: Implement helper authorization and writers**

The helper accepts a binding UUID, loads only its bound credential reference, resolves via the platform store, and emits the value. It rejects arbitrary references and commands. Codex uses the helper only for supported Responses routes; Claude only for gateway-compatible Anthropic routes. Reasonix writes a collision-resistant env name to its own `.env`.

- [ ] **Step 4: Run tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test credential_helper && cargo test -p codex-adapter -p claude-code-adapter -p reasonix-adapter`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/bin adapters/codex adapters/claude-code adapters/reasonix
git commit -m "feat(adapters): add native credentials for Codex Claude and Reasonix"
```

### Task 3: Continue, Aider, and Goose route writers

**Files:**
- Modify: `adapters/detection/src/lib.rs`
- Modify: `adapters/detection/src/parser.rs`
- Modify: `adapters/detection/src/writer.rs`
- Test: `adapters/detection/tests/provider_model_fixtures.rs`

**Interfaces:**
- Produces: Continue model + secret reference, Aider model/provider metadata + `.env`, and Goose provider/model credential configuration.

- [ ] **Step 1: Write failing YAML fixture tests**

```rust
#[test]
fn continue_route_has_api_base_model_and_secret_reference() {
    let plan = plan_bundle("continue", yolo_chat_bundle()).unwrap();
    let yaml = ordinary_after(&plan, ".continue/config.yaml");
    assert!(yaml.contains("provider: openai"));
    assert!(yaml.contains("model: qwen3.8-27b"));
    assert!(yaml.contains("apiBase: https://yolo-auto.com/v1"));
    assert!(yaml.contains("${{ secrets.CHM_YOLO_AUTO_API_KEY }}"));
    assert_protected_target(&plan, ".continue/.env", "CHM_YOLO_AUTO_API_KEY");
}
```

Add Aider and Goose fixtures that preserve unrelated YAML keys and reject unsupported protocols.

- [ ] **Step 2: Run and verify failures**

Run: `cargo test -p detection-adapters provider_model_fixtures -- --nocapture`  
Expected: FAIL for incomplete Aider/Goose capabilities and missing protected credential targets.

- [ ] **Step 3: Implement format-preserving writers**

Use existing YAML AST conventions. Continue writes documented `apiBase` and secret syntax. Aider uses fully qualified provider/model identity, model metadata, and its supported `.env`. Goose maps only installed native provider backends and blocks unknown protocols before writing.

- [ ] **Step 4: Run detection adapter tests**

Run: `cargo test -p detection-adapters`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add adapters/detection
git commit -m "feat(adapters): deploy routes to Continue Aider and Goose"
```

### Task 4: Truthful constrained capabilities for Gemini, Cursor, Amp, Cline, and Roo

**Files:**
- Modify: `adapters/detection/src/lib.rs`
- Modify: `adapters/detection/src/parser.rs`
- Test: `adapters/detection/tests/capabilities.rs`

**Interfaces:**
- Produces: precise compatibility blockers; no provider/model writer until a documented safe target exists.

- [ ] **Step 1: Write failing capability tests**

```rust
#[test]
fn constrained_adapters_explain_their_native_limits() {
    assert_blocked("gemini-cli", yolo_chat_bundle(), "Gemini-compatible routes only");
    assert_blocked("cursor", second_openai_provider_bundle(), "single global OpenAI override");
    assert_blocked("amp", arbitrary_endpoint_bundle(), "no arbitrary endpoint deployment API");
    assert_blocked("cline", yolo_chat_bundle(), "no documented writable credential profile integration");
    assert_blocked("roo-code", yolo_chat_bundle(), "no documented writable credential profile integration");
}
```

- [ ] **Step 2: Run and verify current generic unsupported errors fail**

Run: `cargo test -p detection-adapters constrained_adapters -- --nocapture`  
Expected: FAIL because current booleans cannot describe route-specific limitations.

- [ ] **Step 3: Implement exact capability declarations**

Declare Gemini protocol/fixed-provider rules, Cursor single-global-override topology, Amp managed-only topology, and Cline/Roo protected profile target as unavailable. Continue to support their other implemented surfaces (MCP, skills, profiles); do not label the adapter detection-only.

- [ ] **Step 4: Run tests**

Run: `cargo test -p detection-adapters`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add adapters/detection
git commit -m "fix(adapters): report native route limitations precisely"
```

### Task 5: Cross-adapter contract matrix

**Files:**
- Create: `adapters/tests/route_matrix.rs`
- Modify: `adapters/Cargo.toml`

**Interfaces:**
- Consumes: every registered adapter and shared Yolo/OpenAI Chat, Responses, Anthropic, and Gemini fixtures.
- Produces: one release-gating matrix for route deployment behavior.

- [ ] **Step 1: Write the failing matrix test**

```rust
#[test]
fn every_registered_adapter_is_ready_or_precisely_blocked() {
    for adapter in all_adapters() {
        let result = exercise_route_contract(&*adapter, yolo_chat_bundle());
        match result {
            MatrixResult::Ready(report) => {
                assert!(report.provider_present);
                assert!(report.model_present);
                assert!(report.credential_resolves);
            }
            MatrixResult::Blocked(reason) => {
                assert!(!reason.trim().is_empty());
                assert_ne!(reason, "harness does not support this resource");
            }
            MatrixResult::Partial(_) => panic!("partial route success is forbidden"),
        }
    }
}
```

- [ ] **Step 2: Run and expose remaining partial adapters**

Run: `cargo test -p adapters --test route_matrix -- --nocapture`  
Expected: FAIL naming every adapter that still writes a partial route or generic blocker.

- [ ] **Step 3: Fix each reported adapter without weakening assertions**

For a compatible adapter, complete its native writer and validator. For a native limitation, add the exact capability blocker. Do not add adapter-name exceptions inside the matrix test.

- [ ] **Step 4: Run all adapter tests**

Run: `cargo test -p adapters --test route_matrix && cargo test --workspace`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add adapters
git commit -m "test(adapters): enforce provider route contract across harnesses"
```
