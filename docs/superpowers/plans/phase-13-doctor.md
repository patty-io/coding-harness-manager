# Phase 13 — Doctor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The diagnostics screen: one-click checks across harnesses, providers, MCP servers, and skills, plus a redacted diagnostic bundle export (project plan §35, §62, §63).

**Architecture:** Backend commands in `apps/desktop/src-tauri/src/commands/doctor.rs` returning typed check results. Checks reuse existing machinery: detection (Phase 2), health/discovery (Phase 5), MCP diagnostics (Phase 9), skill conflict scan (Phase 10). Export produces a JSON bundle with all checks + app/harness versions, with a redaction pass before writing.

**Tech Stack:** Rust (existing crates), `tracing` for logs. Frontend: Doctor screen with grouped check lists + Export button (file save via `@tauri-apps/plugin-dialog`).

## Global Constraints

- Doctor is READ-ONLY: it never modifies configs, runs inference, or starts persistent servers (probes time out; children are killed).
- Every check result: `{ check: String, passed: bool, detail: String }` — details must be actionable (project plan §63: bad `Error 500`, good "existing TOML contains a field this adapter version does not recognize; no files were modified").
- Export bundle redacts: API keys, bearer tokens, authorization headers, `sk-*` patterns, `ghp_*`, `$LP_*` values (project plan §62) — the redaction pass runs over the serialized JSON and the log excerpt.
- Phase exit: user runs Doctor on a machine with one broken provider + one healthy harness, sees clear pass/fail per group, exports a bundle, and confirms no secrets are in the exported file.

---

### Task 13.1: Harness + Provider Checks

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/doctor.rs`
- Create: `apps/desktop/src-tauri/src/redact.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Create: `apps/desktop/src-tauri/tests/doctor_tests.rs`

**Interfaces:**
- Produces:
  - `pub struct CheckResult { pub check: String, pub passed: bool, pub detail: String }` (camelCase)
  - `pub struct DoctorReport { pub generated_at: String, pub app_version: String, pub harness_checks: Vec<HarnessCheckGroup>, pub provider_checks: Vec<ProviderCheckGroup>, pub mcp_checks: Vec<CheckResult>, pub skill_checks: Vec<CheckResult>, pub summary: String }` (camelCase) where `HarnessCheckGroup { pub harness_type: String, pub version: Option<String>, pub checks: Vec<CheckResult> }` and `ProviderCheckGroup { pub provider_name: String, pub endpoint_name: String, pub checks: Vec<CheckResult> }`
  - `#[tauri::command] pub async fn run_doctor_cmd(state) -> Result<DoctorReport, String>`
  - `pub async fn run_doctor_core(pool, secrets, http) -> Result<DoctorReport, String>` (testable)
  - `pub fn redact(text: &str) -> String` — replaces secret patterns with `<REDACTED>`.

- [ ] **Step 1: Write the failing tests `tests/doctor_tests.rs`**

```rust
use coding_harness_manager_lib::redact::redact;

#[test]
fn redact_hides_secret_patterns() {
    let text = "key=sk-ant-abcdef1234567890 token=ghp_ABCDEF1234567890 Bearer abcdef0123456789";
    let out = redact(text);
    assert!(!out.contains("sk-ant-abcdef1234567890"));
    assert!(!out.contains("ghp_ABCDEF1234567890"));
    assert!(!out.contains("abcdef0123456789"));
    assert!(out.contains("<REDACTED>"));
}

#[test]
fn redact_leaves_plain_text_alone() {
    assert_eq!(redact("model glm-5 context 1048576"), "model glm-5 context 1048576");
}
```

- [ ] **Step 2: Implement `redact.rs`**

```rust
//! Secret redaction for logs and diagnostic bundles.

pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    // patterns: sk-*, sk-ant-*, ghp_*, github_pat_*, Bearer <hex>, api keys in quotes
    for pat in [
        r"sk-(ant-)?[A-Za-z0-9_\-]{8,}",
        r"ghp_[A-Za-z0-9]{20,}",
        r"github_pat_[A-Za-z0-9_]{20,}",
        r"Bearer [A-Za-z0-9._\-]{8,}",
        r"x-api-key[\"'\s:=]+[A-Za-z0-9._\-]{8,}",
    ] {
        let re = regex::Regex::new(pat).expect("valid regex");
        out = re.replace_all(&out, "<REDACTED>").into_owned();
    }
    out
}
```

Add `regex = "1"` to the desktop crate deps.

- [ ] **Step 3: Implement `doctor.rs` harness + provider checks**

`harness_checks(pool)` (project plan §35 Harness): per installation — `executable exists` (path is_file), `version supported` (`parse_version_supported` against the adapter's known list — obtainable via a new trait method `supported_versions() -> Vec<&str>` added to `HarnessAdapter` with default `vec![]` meaning "unknown"), `config readable` (read_to_string ok), `config parse valid` (adapter `read_state` succeeds), `config writable` (parent dir metadata writable — check via `std::fs::metadata().permissions()` on Unix, best-effort), `backup dir writable` (`<config parent>/.chm-backups` create+remove probe — CREATE only when missing; remove after).

`provider_checks(pool, secrets, http)` (project plan §35 Providers): per endpoint — `endpoint reachable`, `authentication works` (health_check mapping: Healthy → both pass; AuthFailed → auth fails; Unreachable → reachable fails), `discovery works` (`discover_models` ok → pass with count; DiscoveryUnsupported → pass with note "discovery unsupported"), `sample inference` NOT run (documented skip).

- [ ] **Step 4: Run tests + commit**

```bash
cd apps/desktop/src-tauri && cargo test
git add apps/desktop crates/harness-sdk
git commit -m "feat(phase13): harness and provider doctor checks"
```

---

### Task 13.2: MCP + Skill Checks

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/doctor.rs`

**Interfaces:**
- Consumes: `run_mcp_diagnostics_core` (Phase 9.3), `detect_conflicts` (Phase 10.3), adapter `read_state`.
- Produces: MCP checks per server (reuse Phase 9 checks verbatim) and skill checks per harness: `canonical path exists`, `links resolve` (binding targets exist), `duplicates` (registry name collisions — count), `broken links` (dangling symlinks), `permissions` (SKILL.md readable).

- [ ] **Step 1: Write the failing test** — seed a skill binding with a broken symlink → `broken links` check fails with detail naming the target.

- [ ] **Step 2: Implement** — `mcp_checks(pool)` loops `list_mcp_servers` → `run_mcp_diagnostics_core` per server (map to `CheckResult`s with `check` prefixed by server name). `skill_checks(pool)` scans registry skills + bindings; for each harness, run `detect_conflicts` and map kinds to check rows.

- [ ] **Step 3: Run tests + commit**

```bash
cd apps/desktop/src-tauri && cargo test
git add apps/desktop
git commit -m "feat(phase13): mcp and skill doctor checks"
```

---

### Task 13.3: Doctor Screen + Export

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/doctor.rs` (`export_diagnostics_cmd`)
- Create: `apps/desktop/src/hooks/useDoctor.ts`
- Create: `apps/desktop/src/screens/DoctorScreen.tsx` (replaces placeholder)
- Modify: `apps/desktop/src/lib/api.ts`

**Interfaces:**
- Produces:
  - `#[tauri::command] pub async fn export_diagnostics_cmd(state, dest_dir: String) -> Result<String, String>` — runs `run_doctor_core`, serializes to JSON, applies `redact()` to the serialized string, writes `chm-diagnostics-<timestamp>.json` into `dest_dir`; returns the file path.
  - JS: `runDoctor()`, `exportDiagnostics(destDir)`.

- [ ] **Step 1: Write the test** — `export_diagnostics_core(pool, secrets, http, dest_dir)` writes a file whose contents contain no `sk-` patterns and parse as JSON.

- [ ] **Step 2: Implement export** (folder picked via `@tauri-apps/plugin-dialog` `open({directory: true})` from the frontend; path passed to the command).

- [ ] **Step 3: Doctor screen UI** — grouped sections per §35 with expandable details; summary banner ("12/15 checks passed — 3 issues: …"); "Run Doctor" button; "Export Diagnostics" button; per-check detail tooltips.

- [ ] **Step 4: Verify + commit**

Manual: break a provider key (wrong value), run Doctor, confirm Authentication failed shows with actionable detail; export; grep the export for the wrong key → absent.

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase13): doctor screen and redacted export"
```

---

### Task 13.4: Logging + Phase Exit

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs` (tracing init)
- Modify: `apps/desktop/src-tauri/src/commands/doctor.rs` (include log excerpt in export)

**Interfaces:**
- Produces: log redaction guaranteed at the source — a `redact` wrapper on log writes.

- [ ] **Step 1: Logging setup** — `tracing_subscriber::fmt()` with an env filter (`RUST_LOG`), writing to `~/.coding-harness-manager/logs/chm.log` (via `tracing_appender`); add `tracing-appender = "0.2"`. The export includes the last 200 log lines through `redact()`.

- [ ] **Step 2: Wire key events** — `tracing::info!` on: scan, sync apply (files written), rollback, import, doctor run.

- [ ] **Step 3: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase13): structured logging with redaction"
```

Phase complete when all steps green.