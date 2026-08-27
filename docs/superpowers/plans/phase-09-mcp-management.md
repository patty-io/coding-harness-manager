# Phase 9 — MCP Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Full MCP lifecycle: canonical global MCP registry (CRUD), per-harness bindings with native overrides, native translation into each harness's MCP config through the sync engine, and diagnostics (project plan §22, §23, §24).

**Architecture:** Backend commands in `apps/desktop/src-tauri/src/commands/mcp.rs` wrap the Phase 1 MCP repos. Bindings are written when sync applies MCP actions — the `harness_mcp_bindings` table records what CHM owns per harness (this feeds `managed_flags` in Phase 8.2's `managed_flags_for`, replacing the V1 all-false stub). Diagnostics run MCP validation checks per project plan §24 (command exists, executable launches, env available, HTTP reachable, init works, duplicate names/tools).

**Tech Stack:** As Phase 4 + `npx`/`node` process probing via tokio `process::Command` for stdio servers; `reqwest` for HTTP servers (already in `AppState`).

## Global Constraints

- Canonical MCP definitions are independent from harness bindings (project plan §23): one `McpServer` row, N `HarnessMcpBinding` rows.
- V1 exposes `global` scope only; `project` scope exists in the schema but is hidden in UI (project plan §22).
- Binding creation happens ONLY through the sync flow (adapter `plan()` for MCP actions → native config edit + binding row). Direct binding writes require an explicit "bind" command that also syncs the harness.
- Duplicate MCP names in the canonical registry are impossible (name UNIQUE); duplicates detected against a harness are reported as Conflicts by the reconciliation engine.
- Diagnostics are read-only and never start long-running servers: stdio probes use a 5s timeout and kill the child; HTTP probes use a 3s timeout.
- Phase exit: user can add GitHub-style MCP server, bind it to two harnesses via sync, and run diagnostics showing per-check pass/fail.

---

### Task 9.1: MCP CRUD Commands + Registry Screen

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/mcp.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Create: `apps/desktop/src/hooks/useMcp.ts`
- Create: `apps/desktop/src/screens/McpScreen.tsx` (replaces placeholder)

**Interfaces:**
- Consumes: `create_mcp_server`, `list_mcp_servers`, `delete_mcp_server` (Phase 1 Task 1.5).
- Produces:
  - `#[tauri::command] pub async fn create_mcp_cmd(state, input: McpInput) -> Result<McpServer, String>`
  - `pub struct McpInput { pub name: String, pub transport: String, pub command: Option<String>, pub args: Vec<String>, pub url: Option<String>, pub env: serde_json::Map<String, serde_json::Value> }`
  - `#[tauri::command] pub async fn list_mcp_cmd(state) -> Result<Vec<McpServer>, String>`
  - `#[tauri::command] pub async fn delete_mcp_cmd(state, id: String) -> Result<(), String>`
  - JS: `createMcp`, `listMcp`, `deleteMcp`.

- [ ] **Step 1: Write the failing backend test**

`apps/desktop/src-tauri/tests/mcp_commands.rs`:

```rust
#[tokio::test]
async fn mcp_create_rejects_duplicate_name() {
    let pool = connect_test().await.unwrap();
    let server = mcp_server("github");
    chm_database::repos::mcp::create_mcp_server(&pool, &server).await.unwrap();
    let dup = chm_database::repos::mcp::create_mcp_server(&pool, &mcp_server("github")).await;
    assert!(dup.is_err(), "duplicate canonical name must be rejected");
}

#[tokio::test]
async fn mcp_env_roundtrips_as_object_not_string() {
    let pool = connect_test().await.unwrap();
    let server = mcp_server("github"); // env contains {"TOKEN": "$LP_GITHUB_TOKEN"}
    chm_database::repos::mcp::create_mcp_server(&pool, &server).await.unwrap();
    let list = chm_database::repos::mcp::list_mcp_servers(&pool).await.unwrap();
    assert_eq!(list[0].env.get("TOKEN").and_then(|v| v.as_str()), Some("$LP_GITHUB_TOKEN"));
}
```

- [ ] **Step 2: Implement `commands/mcp.rs`** — thin wrappers over the repos; `create_mcp_cmd` maps `McpInput` into `McpServer` with `scope_type: Global`, `provenance: {"source": "manual"}`, `enabled: true`.

- [ ] **Step 3: Frontend**

`useMcp.ts`: `useMcpServers()`, `useCreateMcp()`, `useDeleteMcp()` (all invalidating `["mcp"]`).

`McpScreen.tsx` (project plan §22 field list): table of servers (name, transport badge, command/url, env keys count, enabled toggle, bindings count placeholder `—`, actions: Edit, Delete with confirm). "Add MCP Server" modal form (React Hook Form + Zod): name, transport select (stdio/http/sse), conditional fields (command + args textarea line-per-arg, or url), env key/value rows (dynamic list, value uses `$LP_<NAME>` placeholder convention when it references an env var).

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase9): mcp registry CRUD and screen"
```

---

### Task 9.2: MCP Bindings + Sync Integration

**Files:**
- Modify: `crates/database/src/repos/mcp.rs` (binding methods)
- Modify: `apps/desktop/src-tauri/src/commands/mcp.rs` (`bind_mcp_cmd`)
- Modify: `adapters/opencode/src/lib.rs`, `adapters/claude-code/src/lib.rs`, `adapters/pi/src/lib.rs`, `adapters/codex/src/lib.rs` (`plan()` MCP arms)
- Modify: `apps/desktop/src-tauri/src/commands/sync.rs` (persist bindings after apply)

**Interfaces:**
- Consumes: `HarnessMcpBinding` domain type, `harness_mcp_bindings` table.
- Produces:
  - `pub async fn create_mcp_binding(pool, b: &HarnessMcpBinding) -> Result<(), DbError>` and `pub async fn list_mcp_bindings(pool, installation_id) -> Result<Vec<HarnessMcpBinding>, DbError>` in `repos/mcp.rs`.
  - `#[tauri::command] pub async fn bind_mcp_cmd(state, installation_id: String, mcp_id: String) -> Result<(), String>` — creates the binding row AND triggers a sync apply of the MCP action to that harness (preview+apply), so binding never diverges from native config.

- [ ] **Step 1: Write the failing tests**

Repo test: create binding + list by installation. Sync test: extend `execute_sync_applies_and_records_snapshots` scenario with a canonical MCP server in desired state → after `execute_sync`, a `harness_mcp_bindings` row exists for the installation and the OpenCode config file contains the MCP entry.

- [ ] **Step 2: Implement the repo methods + `bind_mcp_cmd`**

`bind_mcp_cmd`: load installation + McpServer; call `execute_sync` with `Mode::Append` filtered to MCP only (add `include_mcp: bool` to `execute_sync`'s options or a dedicated `execute_mcp_sync` that builds desired with only the one server); on success, `create_mcp_binding` with `native_name` = server name, `managed: true`, `native_config` = the after-state of the server entry.

- [ ] **Step 3: Adapter MCP plan arms**

Each writable adapter's `plan()` gains an `PlanAction::Add(a) if a.kind == "mcp"` arm that appends the server to its native config file (OpenCode: `mcp.<name>` object in opencode.json or opencode-mcp.json per Phase 0 doc; Claude Code: `mcpServers` in `~/.claude.json`; Pi/Codex: `[mcp]`/`[mcp_servers]` tables). Follow each adapter's existing writer module with a `merge_mcp_*` helper + golden test (extend the write fixtures from Phase 8).

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cargo test -p opencode-adapter -p claude-code-adapter -p pi-adapter -p codex-adapter
git add crates/database apps/desktop/src-tauri adapters/
git commit -m "feat(phase9): mcp bindings through sync"
```

---

### Task 9.3: MCP Diagnostics

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/mcp_diagnostics.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (register)
- Modify: `apps/desktop/src/screens/McpScreen.tsx` (diagnostics panel)

**Interfaces:**
- Consumes: `list_mcp_servers`, `list_mcp_bindings`; `AppState.http`.
- Produces:
  - `#[tauri::command] pub async fn run_mcp_diagnostics(state, mcp_id: String) -> Result<Vec<CheckResult>, String>`
  - `pub struct CheckResult { pub check: String, pub passed: bool, pub detail: String }` (camelCase)
  - Checks (project plan §24):
    1. `command exists` — `which`-style lookup of `McpServer.command`.
    2. `env available` — every env key's value resolves (or is `$LP_*`-style placeholder that the OS will resolve at launch).
    3. `executable launches` — spawn `command args --help` with 5s timeout, kill on timeout; passed if spawn succeeds (exit code ignored).
    4. `http reachable` — for http/sse transport: `GET url` with 3s timeout; passed on any 2xx/4xx (server answered), failed on timeout/conn refused.
    5. `duplicate names` — passed if no other canonical server shares the name (schema already enforces; kept for harness-side names via bindings in Phase 13).
  - `#[tauri::command] pub async fn run_all_mcp_diagnostics(state) -> Result<Vec<McpDiagSummary>, String>` — one summary per server (passed checks / total + first failure).

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn diagnostics_detect_missing_command() {
    let pool = connect_test().await.unwrap();
    let server = McpServer { command: Some("/nonexistent/bin/tool".into()), ..mcp_server("broken") };
    chm_database::repos::mcp::create_mcp_server(&pool, &server).await.unwrap();
    let checks = run_mcp_diagnostics_core(&pool, &server.id.to_string()).await.unwrap();
    let cmd_check = checks.iter().find(|c| c.check == "command exists").unwrap();
    assert!(!cmd_check.passed);
}

#[tokio::test]
async fn diagnostics_pass_for_http_server() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .mount(&server).await;
    // McpServer with url = server.uri() → http reachable passes
}
```

- [ ] **Step 2: Implement** — core function `run_mcp_diagnostics_core(pool, mcp_id) -> Result<Vec<CheckResult>, String>` (testable), command wrapper adds State. Timeouts via `tokio::time::timeout`; stdio spawn via `tokio::process::Command`.

- [ ] **Step 3: UI** — "Run Diagnostics" button per row and "Run All" on the screen; results rendered as green/red check list with detail text.

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase9): mcp diagnostics"
```

---

### Task 9.4: MCP Screen Bindings View + Phase Exit

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mcp.rs` (`mcp_detail_cmd`)
- Modify: `apps/desktop/src/screens/McpScreen.tsx`

**Interfaces:**
- Produces: `#[tauri::command] pub async fn mcp_detail_cmd(state, mcp_id: String) -> Result<McpDetail, String>` where `McpDetail { pub server: McpServer, pub bindings: Vec<BindingView> }` and `BindingView { pub installation_id: String, pub harness_type: String, pub native_name: String, pub managed: bool }`.

- [ ] **Step 1: Implement + UI** — detail drawer on row click showing bindings with harness names and a "Bind to harness…" multi-select calling `bind_mcp_cmd`.

- [ ] **Step 2: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase9): mcp detail and binding UI"
```

Phase complete when all steps green.