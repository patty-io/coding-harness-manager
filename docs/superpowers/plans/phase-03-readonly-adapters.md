# Phase 3 — Read-Only Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define the stable `HarnessAdapter` contract in `crates/harness-sdk` and implement read-only adapters for all five Tier-1 harnesses that parse real native configs into normalized state — no writes.

**Architecture:** The adapter trait is the version-aware contract the whole app programs against. Each adapter lives in `adapters/<harness>/`, parses the native config documented in Phase 0, and normalizes into domain types (`ModelRoute`, `McpServer`, `Skill`, `LaunchProfile` candidates). Golden tests run against the `fixtures/` corpus from Phase 0: fixture → parse → normalized state, compared with expected JSON committed alongside each test. Reading an unknown/unsupported version is never an error at this stage: it yields what it can parse plus a `parse_warnings` list (this feeds read-only mode in Phase 8).

**Tech Stack:** Rust edition 2024, `serde_json` (JSON/JSONC harnesses), `toml` 0.8 (TOML harnesses), `indexmap` for order-preserving maps, `sha2` (content hashes, Phase 3 skills). Fixture-based golden tests.

## Global Constraints

- READ-ONLY: no adapter in this phase may write, move, or create any file. Tests assert this by running against fixture copies only.
- The trait lives in `crates/harness-sdk`; adapter crates depend on `chm-harness-sdk` + `chm-core` only (never the database — adapters are pure parsers here).
- Normalized `models` output must use `ModelRoute` with `remote_model_id` from the native config's own model id (the native_id the harness actually uses).
- Unknown fields in native configs are preserved as opaque JSON inside the normalized `native_config` payloads — adapters never drop data they don't understand.
- Every adapter task requires the matching Phase 0 doc + fixtures to exist. If a fixture is missing, STOP and collect it (Phase 0 exit criteria) — do not invent config shapes.
- Phase exit: all five adapters parse their full fixtures with zero diff against golden expectations; `cargo test --workspace` green.

---

### Task 3.1: Harness Adapter Trait + Capabilities

**Files:**
- Create: `crates/harness-sdk/src/adapter/mod.rs`
- Create: `crates/harness-sdk/src/adapter/types.rs`
- Create: `crates/harness-sdk/tests/adapter_contract.rs`

**Interfaces:**
- Produces (the contract every later phase programs against):
  - `pub enum AdapterError { UnsupportedVersion { harness: String, version: Option<String> }, Io(std::io::Error), Parse { path: String, detail: String }, NotFound(String), Invalid(String) }` (thiserror)
  - `pub struct HarnessCapabilities { pub supports_custom_models: bool, pub supports_custom_providers: bool, pub supports_model_catalog: bool, pub supports_profiles: bool, pub supports_mcp_global: bool, pub supports_mcp_project: bool, pub supports_global_skills: bool, pub supports_project_skills: bool, pub supports_runtime_env: bool, pub supports_model_aliases: bool, pub supports_symlinked_skills: bool }` (all default true/false via builder)
  - `pub struct HarnessModel { pub native_id: String, pub route: ModelRoute }`
  - `pub struct HarnessMcp { pub native_name: String, pub server: McpServer }`
  - `pub struct HarnessSkill { pub name: String, pub path: String, pub content_hash: Option<String>, pub symlinked: bool }`
  - `pub struct ParsedState { pub models: Vec<HarnessModel>, pub providers: Vec<serde_json::Value>, pub mcp: Vec<HarnessMcp>, pub skills: Vec<HarnessSkill>, pub profiles: Vec<serde_json::Value>, pub warnings: Vec<String> }`
  - `pub trait HarnessAdapter: Send + Sync { fn id(&self) -> &'static str; fn detect(&self, home: &Path, path_env: Option<&str>) -> Option<HarnessInstallation>; fn capabilities(&self) -> HarnessCapabilities; fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError>; }`
  - `pub fn parse_version_supported(version: Option<&str>, supported: &[&str]) -> bool` — `None` (undetectable version) returns true with warning; exact `semver` prefix match against supported list.

- [ ] **Step 1: Write the failing test `tests/adapter_contract.rs`**

```rust
use chm_harness_sdk::adapter::types::{HarnessCapabilities, parse_version_supported};

#[test]
fn capabilities_default_to_safe_false_then_opt_in() {
    let caps = HarnessCapabilities::none();
    assert!(!caps.supports_custom_models);
    let caps = caps.with_models(true);
    assert!(caps.supports_custom_models);
}

#[test]
fn version_support_matches_prefix() {
    assert!(parse_version_supported(Some("0.30.2"), &["0.30"]));
    assert!(parse_version_supported(Some("0.31.0"), &["0.31"]));
    assert!(!parse_version_supported(Some("1.2.0"), &["0.31"]));
    // undetectable version: supported (read-only safety warns separately)
    assert!(parse_version_supported(None, &["0.31"]));
}
```

- [ ] **Step 2: Implement `adapter/mod.rs` and `adapter/types.rs`**

```rust
// adapter/mod.rs
pub mod types;

pub use types::*;
```

```rust
// adapter/types.rs
//! The stable harness adapter contract.

use std::path::Path;

use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::mcp::McpServer;
use chm_core::domain::models::ModelRoute;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("harness {harness} version {version:?} is not supported by this adapter")]
    UnsupportedVersion { harness: String, version: Option<String> },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error in {path}: {detail}")]
    Parse { path: String, detail: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid state: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct HarnessCapabilities {
    pub supports_custom_models: bool,
    pub supports_custom_providers: bool,
    pub supports_model_catalog: bool,
    pub supports_profiles: bool,
    pub supports_mcp_global: bool,
    pub supports_mcp_project: bool,
    pub supports_global_skills: bool,
    pub supports_project_skills: bool,
    pub supports_runtime_env: bool,
    pub supports_model_aliases: bool,
    pub supports_symlinked_skills: bool,
}

impl HarnessCapabilities {
    pub fn none() -> Self {
        Self {
            supports_custom_models: false,
            supports_custom_providers: false,
            supports_model_catalog: false,
            supports_profiles: false,
            supports_mcp_global: false,
            supports_mcp_project: false,
            supports_global_skills: false,
            supports_project_skills: false,
            supports_runtime_env: false,
            supports_model_aliases: false,
            supports_symlinked_skills: false,
        }
    }

    pub fn with_models(mut self, v: bool) -> Self { self.supports_custom_models = v; self }
    pub fn with_providers(mut self, v: bool) -> Self { self.supports_custom_providers = v; self }
    pub fn with_mcp_global(mut self, v: bool) -> Self { self.supports_mcp_global = v; self }
    pub fn with_global_skills(mut self, v: bool) -> Self { self.supports_global_skills = v; self }
    pub fn with_profiles(mut self, v: bool) -> Self { self.supports_profiles = v; self }
    pub fn with_runtime_env(mut self, v: bool) -> Self { self.supports_runtime_env = v; self }
    pub fn with_model_aliases(mut self, v: bool) -> Self { self.supports_model_aliases = v; self }
    pub fn with_symlinked_skills(mut self, v: bool) -> Self { self.supports_symlinked_skills = v; self }
}

#[derive(Debug, Clone)]
pub struct HarnessModel {
    pub native_id: String,
    pub route: ModelRoute,
}

#[derive(Debug, Clone)]
pub struct HarnessMcp {
    pub native_name: String,
    pub server: McpServer,
}

#[derive(Debug, Clone)]
pub struct HarnessSkill {
    pub name: String,
    pub path: String,
    pub content_hash: Option<String>,
    pub symlinked: bool,
}

#[derive(Debug, Default, Clone)]
pub struct ParsedState {
    pub models: Vec<HarnessModel>,
    pub providers: Vec<serde_json::Value>,
    pub mcp: Vec<HarnessMcp>,
    pub skills: Vec<HarnessSkill>,
    pub profiles: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

pub trait HarnessAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, home: &Path, path_env: Option<&str>) -> Option<HarnessInstallation>;
    fn capabilities(&self) -> HarnessCapabilities;
    fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError>;
}

/// Version gate: supported list uses two-component prefixes ("0.30").
/// None (undetectable version) is treated as supported — the caller adds a
/// read-only-safety warning instead of failing.
pub fn parse_version_supported(version: Option<&str>, supported: &[&str]) -> bool {
    match version {
        None => true,
        Some(v) => {
            let prefix: String = v.split('.').take(2).collect::<Vec<_>>().join(".");
            supported.iter().any(|s| prefix == *s || v.starts_with(s))
        }
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p chm-harness-sdk
```

Expected: both pass.

- [ ] **Step 4: Commit**

```bash
git add crates/harness-sdk/
git commit -m "feat(phase3): adapter trait and capabilities"
```

---

### Task 3.2: OpenCode Read-Only Adapter (reference adapter)

**Files:**
- Create: `adapters/opencode/Cargo.toml`
- Create: `adapters/opencode/src/lib.rs`
- Create: `adapters/opencode/src/parser.rs`
- Create: `adapters/opencode/tests/read_fixtures.rs`
- Create: `adapters/opencode/tests/golden/*.json` (expected normalized state per fixture)

**Interfaces:**
- Consumes: `HarnessAdapter` trait, `HarnessDefinition` (opencode), domain types, Phase 0 `docs/harnesses/opencode.md` + `fixtures/opencode/`.
- Produces: `pub struct OpenCodeAdapter;` implementing `HarnessAdapter` — THE reference implementation later adapters mirror. Also produces `pub fn parse_config(raw: &str, config_dir: &Path) -> Result<ParsedState, AdapterError>` as a free function so the parser is testable without a fake install.

- [ ] **Step 1: Write the failing golden test `tests/read_fixtures.rs`**

```rust
use chm_core::domain::harness::{HarnessInstallation, InstallationStatus};
use chm_harness_sdk::adapter::HarnessAdapter;
use opencode_adapter::OpenCodeAdapter;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); p.pop(); // adapters/opencode -> repo root
    p.push("fixtures/opencode");
    p
}

fn install(config_dir: PathBuf) -> HarnessInstallation {
    HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: chm_core::domain::harness::HarnessType::OpenCode,
        executable_path: Some("/fake/opencode".into()),
        version: Some("0.30.0".into()),
        config_path: Some(config_dir.join("opencode.json").display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    }
}

#[test]
fn opencode_full_config_parses_without_warnings() {
    let dir = fixture_dir();
    let versions: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert!(!versions.is_empty(), "fixtures/opencode is empty — run phase 0 first");

    for version_dir in versions {
        let golden_path = format!("tests/golden/{}.json", version_dir.file_name().unwrap().to_string_lossy());
        let adapter = OpenCodeAdapter;
        let inst = install(version_dir.clone());
        let state = adapter.read_state(&inst).expect("read_state ok");
        assert!(state.warnings.is_empty(), "warnings: {:?}", state.warnings);

        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(golden_path).unwrap_or_else(|_| panic!("golden missing for {golden_path}")),
        ).unwrap();
        let actual = serde_json::json!({
            "models": state.models.iter().map(|m| serde_json::json!({
                "native_id": m.native_id,
                "display_name": m.route.display_name,
                "remote_model_id": m.route.remote_model_id,
                "capabilities": m.route.capabilities,
            })).collect::<Vec<_>>(),
            "mcp": state.mcp.iter().map(|m| serde_json::json!({
                "native_name": m.native_name,
                "transport": m.server.transport.as_str(),
                "command": m.server.command,
                "args": m.server.args,
                "url": m.server.url,
                "env": m.server.env,
            })).collect::<Vec<_>>(),
            "skills": state.skills.iter().map(|s| serde_json::json!({
                "name": s.name,
                "symlinked": s.symlinked,
            })).collect::<Vec<_>>(),
        });
        assert_eq!(actual, expected, "golden mismatch for {}", version_dir.display());
    }
}
```

- [ ] **Step 2: Create the golden expectations**

For each fixture version dir, hand-write `tests/golden/<version>.json` from the fixture contents per the mapping rules below (this is the expected normalized state — the test then locks it). Example for a fixture with one model + one MCP:

```json
{
  "models": [
    {
      "native_id": "glm-5",
      "display_name": "GLM-5",
      "remote_model_id": "glm-5",
      "capabilities": { "limit": { "context": 1048576 } }
    }
  ],
  "mcp": [
    {
      "native_name": "playwright",
      "transport": "stdio",
      "command": "npx",
      "args": ["-y", "@playwright/mcp@latest"],
      "url": null,
      "env": {}
    }
  ],
  "skills": []
}
```

- [ ] **Step 3: Implement `adapters/opencode/src/parser.rs`**

Parsing rules (verify each against `docs/harnesses/opencode.md`; adjust to the documented shape):

```rust
//! OpenCode native config parser (opencode.json / opencode.jsonc).

use chm_core::domain::harness::HarnessType;
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use chrono::Utc;
use uuid::Uuid;

pub fn parse_config(raw: &str, config_dir: &std::path::Path) -> Result<ParsedState, AdapterError> {
    let json: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| AdapterError::Parse { path: "opencode.json".into(), detail: e.to_string() })?;
    let mut state = ParsedState::default();

    // provider.<id>.models.<id> -> ModelRoute
    if let Some(providers) = json.get("provider").and_then(|p| p.as_object()) {
        for (provider_id, pv) in providers {
            if let Some(models) = pv.get("models").and_then(|m| m.as_object()) {
                for (model_id, meta) in models {
                    let display_name = meta.get("name").and_then(|v| v.as_str()).unwrap_or(model_id).to_string();
                    let capabilities = meta.clone();
                    let route = ModelRoute {
                        id: Uuid::new_v4(),
                        endpoint_id: Uuid::new_v4(), // placeholder — real endpoint linking happens in Phase 5/6
                        model_identity_id: None,
                        remote_model_id: model_id.clone(),
                        display_name,
                        context_window: meta.get("limit").and_then(|l| l.get("context")).and_then(|v| v.as_i64()),
                        max_input: None,
                        max_output: meta.get("limit").and_then(|l| l.get("output")).and_then(|v| v.as_i64()),
                        capabilities,
                        overrides: serde_json::Value::Object(Default::default()),
                        enabled: true,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    };
                    state.providers.push(serde_json::json!({
                        "native_provider_id": provider_id,
                        "env": pv.get("env"),
                        "options": pv.get("options"),
                        "apiKey_env": pv.get("apiKey"),
                    }));
                    state.models.push(HarnessModel { native_id: model_id.clone(), route });
                }
            }
        }
    }

    // top-level mcp object OR separate opencode-mcp.json (passed as config_dir/opencode-mcp.json)
    if let Some(mcp) = json.get("mcp").and_then(|m| m.as_object()) {
        for (name, spec) in mcp {
            let mcp_server = parse_mcp(name, spec);
            state.mcp.push(HarnessMcp { native_name: name.clone(), server: mcp_server });
        }
    }
    let mcp_file = config_dir.join("opencode-mcp.json");
    if mcp_file.exists() {
        let raw_mcp = std::fs::read_to_string(&mcp_file)?;
        let json_mcp: serde_json::Value = serde_json::from_str(&raw_mcp)
            .map_err(|e| AdapterError::Parse { path: mcp_file.display().to_string(), detail: e.to_string() })?;
        if let Some(mcp) = json_mcp.as_object() {
            for (name, spec) in mcp {
                state.mcp.push(HarnessMcp { native_name: name.clone(), server: parse_mcp(name, spec) });
            }
        }
    }

    // skills dir
    let skills_dir = config_dir.join("skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let symlinked = entry.path().symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false);
                state.skills.push(chm_harness_sdk::adapter::types::HarnessSkill {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path().display().to_string(),
                    content_hash: None,
                    symlinked,
                });
            }
        }
    }

    Ok(state)
}

fn parse_mcp(name: &str, spec: &serde_json::Value) -> McpServer {
    let transport = match spec.get("type").and_then(|t| t.as_str()) {
        Some("remote") => McpTransport::Http,
        _ => McpTransport::Stdio,
    };
    McpServer {
        id: Uuid::new_v4(),
        name: name.to_string(),
        transport,
        command: spec.get("command").and_then(|v| v.as_str()).map(|s| s.to_string()),
        args: spec.get("args").and_then(|v| v.as_array()).map(|a| {
            a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        }).unwrap_or_default(),
        url: spec.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()),
        env: spec.get("env").and_then(|v| v.as_object()).cloned().unwrap_or_default(),
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "opencode-native"}),
        enabled: true,
    }
}
```

- [ ] **Step 4: Implement `adapters/opencode/src/lib.rs`**

```rust
//! OpenCode read-only adapter.

pub mod parser;

use std::path::Path;

use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::{AdapterError, HarnessAdapter, HarnessCapabilities, ParsedState};
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::paths::{find_executable, resolve_config_path};
use chm_harness_sdk::detect::scan;
use chm_harness_sdk::detect::version::detect_version;
use chrono::Utc;
use uuid::Uuid;

pub struct OpenCodeAdapter;

impl HarnessAdapter for OpenCodeAdapter {
    fn id(&self) -> &'static str { "opencode" }

    fn detect(&self, home: &Path, path_env: Option<&str>) -> Option<HarnessInstallation> {
        let def = chm_harness_sdk::definition::tier1_definitions()
            .into_iter().find(|d| d.id == "opencode")?;
        let exe = def.executable_names.iter().find_map(|n| find_executable(n, path_env));
        let config = resolve_config_path(&def, home, Platform::MacOs);
        if exe.is_none() && config.is_none() {
            return None;
        }
        let version = exe.as_ref().and_then(|e| detect_version(e, &["--version"]));
        Some(HarnessInstallation {
            id: Uuid::new_v4(),
            harness_type: HarnessType::OpenCode,
            executable_path: exe,
            version,
            config_path: config.clone().map(|c| c.display().to_string()),
            detected_at: Utc::now(),
            last_scanned_at: Some(Utc::now()),
            status: InstallationStatus::Installed,
        })
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::none()
            .with_models(true)
            .with_providers(true)
            .with_mcp_global(true)
            .with_global_skills(true)
            .with_runtime_env(true)
            .with_symlinked_skills(true)
    }

    fn read_state(&self, install: &HarnessInstallation) -> Result<ParsedState, AdapterError> {
        let config_path = install.config_path.as_ref().ok_or_else(|| AdapterError::NotFound("config_path".into()))?;
        let raw = std::fs::read_to_string(config_path)?;
        let config_dir = std::path::Path::new(config_path).parent().unwrap_or(Path::new(".")).to_path_buf();
        parser::parse_config(&raw, &config_dir)
    }
}
```

- [ ] **Step 5: Write `adapters/opencode/Cargo.toml`**

```toml
[package]
name = "opencode-adapter"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
chm-core.workspace = true
chm-harness-sdk = { path = "../../crates/harness-sdk" }
chrono.workspace = true
serde_json.workspace = true
uuid.workspace = true

[dev-dependencies]
serde_json.workspace = true
```

Add `"adapters/opencode"` to the workspace members in the root `Cargo.toml`.

- [ ] **Step 6: Run tests**

```bash
cargo test -p opencode-adapter
```

Expected: golden test passes against all fixture versions. If a fixture shape differs from the parser's assumption, the Phase 0 doc is authoritative — fix the parser, never the fixture.

- [ ] **Step 7: Commit**

```bash
git add adapters/opencode Cargo.toml
git commit -m "feat(phase3): opencode read-only adapter with golden tests"
```

---

### Task 3.3: Pi Read-Only Adapter

**Files:**
- Create: `adapters/pi/Cargo.toml`
- Create: `adapters/pi/src/lib.rs`
- Create: `adapters/pi/src/parser.rs`
- Create: `adapters/pi/tests/read_fixtures.rs`
- Create: `adapters/pi/tests/golden/*.json`

**Interfaces:**
- Consumes: trait + fixtures from `fixtures/pi/` + `docs/harnesses/pi.md`.
- Produces: `pub struct PiAdapter;` + `pub fn parse_config(raw: &str, home: &Path) -> Result<ParsedState, AdapterError>`.

- [ ] **Step 1: Write the failing golden test**

Mirror Task 3.2 Step 1 exactly, substituting `pi` paths, `PiAdapter`, `HarnessType::Pi`, and fixture dir `fixtures/pi`. The golden JSON uses the same shape (models/mcp/skills).

- [ ] **Step 2: Create golden expectations from fixtures**

Same procedure as Task 3.2 Step 2.

- [ ] **Step 3: Implement `parser.rs` (TOML)**

```rust
//! Pi native config parser (~/.pi/agent/config.toml).

use chm_core::domain::harness::HarnessType;
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use chrono::Utc;
use uuid::Uuid;

pub fn parse_config(raw: &str, home: &std::path::Path) -> Result<ParsedState, AdapterError> {
    let toml: toml::Value = toml::from_str(raw)
        .map_err(|e| AdapterError::Parse { path: "~/.pi/agent/config.toml".into(), detail: e.to_string() })?;
    let mut state = ParsedState::default();

    // provider/model sections — shape per docs/harnesses/pi.md.
    // Typical: [provider.<id>] base_url + api_key_env; [model.<id>] fields.
    if let Some(provider) = toml.get("provider").and_then(|p| p.as_table()) {
        for (pid, pv) in provider {
            state.providers.push(serde_json::json!({
                "native_provider_id": pid,
                "base_url": pv.get("base_url"),
                "api_key_env": pv.get("api_key_env").or_else(|| pv.get("api_key")),
            }));
        }
    }
    if let Some(model) = toml.get("model").and_then(|m| m.as_table()) {
        for (mid, mv) in model {
            // each entry maps to a ModelRoute; role fields (e.g. role = "opus")
            // become capability metadata and, where the harness supports role
            // mapping, a profile candidate in state.profiles.
            let display_name = mv.get("display_name").and_then(|v| v.as_str()).unwrap_or(mid).to_string();
            let mut route = ModelRoute {
                id: Uuid::new_v4(),
                endpoint_id: Uuid::new_v4(),
                model_identity_id: None,
                remote_model_id: mid.clone(),
                display_name,
                context_window: mv.get("context_window").and_then(|v| v.as_integer()),
                max_input: None,
                max_output: None,
                capabilities: serde_json::to_value(mv).unwrap_or_default(),
                overrides: serde_json::Value::Object(Default::default()),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            if let Some(role) = mv.get("role").and_then(|v| v.as_str()) {
                state.profiles.push(serde_json::json!({
                    "native_role": role,
                    "model": mid,
                }));
            }
            state.models.push(HarnessModel { native_id: mid.clone(), route });
        }
    }

    // MCP per docs/harnesses/pi.md — parse the section and map to McpServer.
    if let Some(mcp) = toml.get("mcp").and_then(|m| m.as_table()) {
        for (name, spec) in mcp {
            let server = McpServer {
                id: Uuid::new_v4(),
                name: name.clone(),
                transport: McpTransport::Stdio,
                command: spec.get("command").and_then(|v| v.as_str()).map(|s| s.to_string()),
                args: spec.get("args").and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                url: spec.get("url").and_then(|v| v.as_str()).map(String::from),
                env: spec.get("env").and_then(|v| v.as_table())
                    .map(|t| t.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.to_string()))).collect())
                    .unwrap_or_default(),
                scope_type: ScopeType::Global,
                scope_path: None,
                provenance: serde_json::json!({"source": "pi-native"}),
                enabled: true,
            };
            state.mcp.push(HarnessMcp { native_name: name.clone(), server });
        }
    }

    // skills: ~/.pi/agent/skills (symlinks followed per doc)
    let skills_dir = home.join(".pi/agent/skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                state.skills.push(chm_harness_sdk::adapter::types::HarnessSkill {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path().display().to_string(),
                    content_hash: None,
                    symlinked: entry.path().symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false),
                });
            }
        }
    }

    Ok(state)
}
```

Note: if `docs/harnesses/pi.md` shows a different config layout, rewrite this parser to the documented shape BEFORE filling golden files — the doc is authoritative.

- [ ] **Step 4: Implement `lib.rs` + manifest, run tests**

Mirror Task 3.2 Steps 4–5 (`PiAdapter`, `HarnessType::Pi`, config path `~/.pi/agent/config.toml`). Add `toml = "0.8"` to dependencies.

```bash
cargo test -p pi-adapter
```

Expected: golden test passes.

- [ ] **Step 5: Commit**

```bash
git add adapters/pi Cargo.toml
git commit -m "feat(phase3): pi read-only adapter with golden tests"
```

---

### Task 3.4: Codex Read-Only Adapter

**Files:**
- Create: `adapters/codex/Cargo.toml`
- Create: `adapters/codex/src/lib.rs`
- Create: `adapters/codex/src/parser.rs`
- Create: `adapters/codex/tests/read_fixtures.rs`
- Create: `adapters/codex/tests/golden/*.json`

**Interfaces:**
- Consumes: trait + fixtures from `fixtures/codex/` + `docs/harnesses/codex.md`.
- Produces: `pub struct CodexAdapter;` + `pub fn parse_config(raw: &str, home: &Path) -> Result<ParsedState, AdapterError>`.

- [ ] **Step 1: Write the failing golden test**

Mirror Task 3.2 Step 1 (`CodexAdapter`, `HarnessType::Codex`, `fixtures/codex`), plus an additional assertion: models carry `model_route.overrides` containing the native `model_providers.<id>.wire_api` value so Phase 6/8 can enforce protocol compatibility.

- [ ] **Step 2: Create golden expectations**

Same procedure. Include the `wire_api` override in the models golden entries.

- [ ] **Step 3: Implement `parser.rs` (TOML)**

Parsing rules (verify against `docs/harnesses/codex.md`):

```rust
//! Codex native config parser (~/.codex/config.toml).

use chm_core::domain::harness::HarnessType;
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use chrono::Utc;
use uuid::Uuid;

pub fn parse_config(raw: &str, _home: &std::path::Path) -> Result<ParsedState, AdapterError> {
    let toml: toml::Value = toml::from_str(raw)
        .map_err(|e| AdapterError::Parse { path: "~/.codex/config.toml".into(), detail: e.to_string() })?;
    let mut state = ParsedState::default();

    // model_providers.<id> -> providers (base_url, env_key, wire_api)
    if let Some(mps) = toml.get("model_providers").and_then(|m| m.as_table()) {
        for (pid, pv) in mps {
            state.providers.push(serde_json::json!({
                "native_provider_id": pid,
                "base_url": pv.get("base_url"),
                "env_key": pv.get("env_key"),
                "wire_api": pv.get("wire_api"),
            }));
        }
    }
    // legacy layout [providers.<id>] — same normalization, flag as legacy
    if toml.get("model_providers").is_none() {
        if let Some(ps) = toml.get("providers").and_then(|m| m.as_table()) {
            for (pid, pv) in ps {
                state.providers.push(serde_json::json!({
                    "native_provider_id": pid,
                    "base_url": pv.get("base_url"),
                    "env_key": pv.get("env_key"),
                    "wire_api": pv.get("wire_api"),
                    "legacy_layout": true,
                }));
            }
            state.warnings.push("legacy [providers] layout detected".into());
        }
    }

    // models.<id> -> ModelRoute; resolve provider wire_api into route.overrides
    if let Some(models) = toml.get("models").and_then(|m| m.as_table()) {
        for (mid, mv) in models {
            let provider_id = mv.get("provider").and_then(|v| v.as_str()).unwrap_or_default();
            let wire_api = toml.get("model_providers")
                .and_then(|mps| mps.get(provider_id))
                .and_then(|pv| pv.get("wire_api"))
                .and_then(|v| v.as_str())
                .unwrap_or("chat");
            let remote_model_id = mv.get("model").and_then(|v| v.as_str()).unwrap_or(mid);
            let route = ModelRoute {
                id: Uuid::new_v4(),
                endpoint_id: Uuid::new_v4(),
                model_identity_id: None,
                remote_model_id: remote_model_id.to_string(),
                display_name: mid.to_string(),
                context_window: None,
                max_input: None,
                max_output: None,
                capabilities: serde_json::to_value(mv).unwrap_or_default(),
                overrides: serde_json::json!({
                    "native_model_id": mid,
                    "native_provider_id": provider_id,
                    "wire_api": wire_api,
                }),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            state.models.push(HarnessModel { native_id: mid.to_string(), route });
        }
    }

    // MCP: [mcp_servers.<name>] or separate mcp.json handled by lib.rs
    if let Some(mcps) = toml.get("mcp_servers").and_then(|m| m.as_table()) {
        for (name, spec) in mcps {
            let server = McpServer {
                id: Uuid::new_v4(),
                name: name.clone(),
                transport: McpTransport::Stdio,
                command: spec.get("command").and_then(|v| v.as_str()).map(String::from),
                args: spec.get("args").and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                url: None,
                env: spec.get("env").and_then(|v| v.as_table())
                    .map(|t| t.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.to_string()))).collect())
                    .unwrap_or_default(),
                scope_type: ScopeType::Global,
                scope_path: None,
                provenance: serde_json::json!({"source": "codex-native"}),
                enabled: true,
            };
            state.mcp.push(HarnessMcp { native_name: name.clone(), server });
        }
    }

    // skills: ~/.codex/skills
    let skills_dir = _home.join(".codex/skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                state.skills.push(chm_harness_sdk::adapter::types::HarnessSkill {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path().display().to_string(),
                    content_hash: None,
                    symlinked: entry.path().symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false),
                });
            }
        }
    }

    Ok(state)
}
```

- [ ] **Step 4: Handle `~/.codex/mcp.json` in `lib.rs`**

If the fixture set includes `mcp.json`, `read_state` reads it additionally and merges its entries into `state.mcp` (same `parse_mcp` mapping as OpenCode: `{command, args, env}` → stdio server), marking provenance `"codex-mcp-json"`.

- [ ] **Step 5: Run tests + commit**

```bash
cargo test -p codex-adapter
git add adapters/codex Cargo.toml
git commit -m "feat(phase3): codex read-only adapter with golden tests"
```

---

### Task 3.5: Claude Code Read-Only Adapter

**Files:**
- Create: `adapters/claude-code/Cargo.toml`
- Create: `adapters/claude-code/src/lib.rs`
- Create: `adapters/claude-code/src/parser.rs`
- Create: `adapters/claude-code/tests/read_fixtures.rs`
- Create: `adapters/claude-code/tests/golden/*.json`

**Interfaces:**
- Consumes: trait + fixtures from `fixtures/claude-code/` + `docs/harnesses/claude-code.md`.
- Produces: `pub struct ClaudeCodeAdapter;` + `pub fn parse_config(settings_raw: Option<&str>, claude_json_raw: Option<&str>, home: &Path) -> Result<ParsedState, AdapterError>`.

- [ ] **Step 1: Write the failing golden test**

Mirror Task 3.2 Step 1 (`ClaudeCodeAdapter`, `HarnessType::ClaudeCode`, `fixtures/claude-code`). The fixture dir may contain `settings-full.json` and `claude-json-mcp.json`; `read_state` passes both files (whichever exist) into `parse_config`.

- [ ] **Step 2: Create golden expectations**

Include models from `settings.json`'s env/model overrides (e.g. `ANTHROPIC_DEFAULT_*_MODEL` entries become `state.models` with `native_id` = the role, `remote_model_id` = the model value, capabilities `{"role": "opus"}`) and MCP from `claude-json-mcp.json`'s `mcpServers` object. Exact mapping per `docs/harnesses/claude-code.md`.

- [ ] **Step 3: Implement `parser.rs` (JSON)**

```rust
//! Claude Code native config parser (settings.json + ~/.claude.json mcpServers).

use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use chrono::Utc;
use uuid::Uuid;

const ROLE_ENV_VARS: &[(&str, &str)] = &[
    ("ANTHROPIC_DEFAULT_OPUS_MODEL", "opus"),
    ("ANTHROPIC_DEFAULT_SONNET_MODEL", "sonnet"),
    ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "haiku"),
];

pub fn parse_config(
    settings_raw: Option<&str>,
    claude_json_raw: Option<&str>,
    _home: &std::path::Path,
) -> Result<ParsedState, AdapterError> {
    let mut state = ParsedState::default();

    // settings.json: model role overrides + env/base-url overrides
    if let Some(raw) = settings_raw {
        let json: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| AdapterError::Parse { path: "settings.json".into(), detail: e.to_string() })?;
        if let Some(env) = json.get("env").and_then(|e| e.as_object()) {
            for (key, value) in env {
                if let Some((_, role)) = ROLE_ENV_VARS.iter().find(|(k, _)| *k == key) {
                    if let Some(model) = value.as_str() {
                        let route = ModelRoute {
                            id: Uuid::new_v4(),
                            endpoint_id: Uuid::new_v4(),
                            model_identity_id: None,
                            remote_model_id: model.to_string(),
                            display_name: format!("{role} role"),
                            context_window: None,
                            max_input: None,
                            max_output: None,
                            capabilities: serde_json::json!({"role": role}),
                            overrides: serde_json::json!({"env_key": key}),
                            enabled: true,
                            created_at: Utc::now(),
                            updated_at: Utc::now(),
                        };
                        state.models.push(HarnessModel { native_id: role.to_string(), route });
                    }
                } else {
                    state.providers.push(serde_json::json!({
                        "env_override": key,
                        "value": value,
                        "source": "settings.json",
                    }));
                }
            }
        }
        let _ = &mut state; // full settings.json preserved via provenance in lib.rs
    }

    // ~/.claude.json: global mcpServers
    if let Some(raw) = claude_json_raw {
        let json: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| AdapterError::Parse { path: "~/.claude.json".into(), detail: e.to_string() })?;
        if let Some(mcp) = json.get("mcpServers").and_then(|m| m.as_object()) {
            for (name, spec) in mcp {
                let transport = match spec.get("type").and_then(|t| t.as_str()) {
                    Some("http") | Some("sse") => McpTransport::Http,
                    _ => McpTransport::Stdio,
                };
                let server = McpServer {
                    id: Uuid::new_v4(),
                    name: name.clone(),
                    transport,
                    command: spec.get("command").and_then(|v| v.as_str()).map(String::from),
                    args: spec.get("args").and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    url: spec.get("url").and_then(|v| v.as_str()).map(String::from),
                    env: spec.get("env").and_then(|v| v.as_object()).cloned().unwrap_or_default(),
                    scope_type: ScopeType::Global,
                    scope_path: None,
                    provenance: serde_json::json!({"source": "claude-json-mcpServers"}),
                    enabled: true,
                };
                state.mcp.push(HarnessMcp { native_name: name.clone(), server });
            }
        }
    }

    // skills: ~/.claude/skills
    let skills_dir = _home.join(".claude/skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                state.skills.push(chm_harness_sdk::adapter::types::HarnessSkill {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path().display().to_string(),
                    content_hash: None,
                    symlinked: entry.path().symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false),
                });
            }
        }
    }

    Ok(state)
}
```

- [ ] **Step 4: Implement `lib.rs`**

`read_state` resolves `settings.json` (from `install.config_path` if it points there, else `home/.claude/settings.json`), reads `home/.claude.json` if present, and calls `parse_config`. Capabilities: models via env overrides (true), mcp global true, global skills true, symlinked skills per Phase 0 doc, profiles true (role mapping), runtime env true.

- [ ] **Step 5: Run tests + commit**

```bash
cargo test -p claude-code-adapter
git add adapters/claude-code Cargo.toml
git commit -m "feat(phase3): claude-code read-only adapter with golden tests"
```

---

### Task 3.6: Reasonix Read-Only Adapter

**Files:**
- Create: `adapters/reasonix/Cargo.toml`
- Create: `adapters/reasonix/src/lib.rs`
- Create: `adapters/reasonix/src/parser.rs`
- Create: `adapters/reasonix/tests/read_fixtures.rs`
- Create: `adapters/reasonix/tests/golden/*.json`

**Interfaces:**
- Consumes: trait + fixtures from `fixtures/reasonix/` + `docs/harnesses/reasonix.md`.
- Produces: `pub struct ReasonixAdapter;` + `pub fn parse_config(raw: &str, home: &Path) -> Result<ParsedState, AdapterError>`.

- [ ] **Step 1: Write the failing golden test**

Mirror Task 3.2 Step 1 (`ReasonixAdapter`, `HarnessType::Reasonix`, `fixtures/reasonix`). If `fixtures/reasonix` has no version dir (pending from Phase 0), this task is BLOCKED — do not write a parser against invented shapes. Instead, commit the adapter skeleton with the test marked `#[ignore = "blocked: fixtures/reasonix pending from phase 0"]` and log the blocker.

- [ ] **Step 2: Implement parser per `docs/harnesses/reasonix.md`**

Same pattern as Task 3.4 (TOML if the doc shows TOML, JSON otherwise — follow the doc). Map the documented provider/model/MCP/skill layout into `ParsedState` with the same field semantics used by the other adapters.

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p reasonix-adapter
git add adapters/reasonix Cargo.toml
git commit -m "feat(phase3): reasonix read-only adapter with golden tests"
```

---

### Task 3.7: Adapter Registry + Phase Exit

**Files:**
- Create: `adapters/src/lib.rs`
- Create: `adapters/Cargo.toml` (thin facade crate)
- Create: `adapters/tests/registry.rs`

**Interfaces:**
- Consumes: all five adapters.
- Produces: `pub fn all_adapters() -> Vec<Box<dyn HarnessAdapter>>` — the app's adapter registry (used by Phase 4 import wizard and Phase 8 sync).

- [ ] **Step 1: Write the failing test `adapters/tests/registry.rs`**

```rust
use adapters::all_adapters;

#[test]
fn registry_contains_all_five_tier1_adapters() {
    let adapters = all_adapters();
    let ids: Vec<&str> = adapters.iter().map(|a| a.id()).collect();
    for expected in ["claude-code", "codex", "opencode", "pi", "reasonix"] {
        assert!(ids.contains(&expected), "missing {expected}");
    }
}

#[test]
fn every_adapter_capabilities_are_sane() {
    for a in all_adapters() {
        let caps = a.capabilities();
        // at least one integration surface must be supported
        assert!(
            caps.supports_custom_models || caps.supports_mcp_global || caps.supports_global_skills,
            "{} declares no supported surface",
            a.id()
        );
    }
}
```

- [ ] **Step 2: Implement `adapters/src/lib.rs`**

```rust
//! Adapter facade: compiles all Tier-1 adapters into one registry.

use chm_harness_sdk::adapter::types::HarnessAdapter;

pub mod claude_code { pub use claude_code_adapter::*; }
pub mod codex { pub use codex_adapter::*; }
pub mod opencode { pub use opencode_adapter::*; }
pub mod pi { pub use pi_adapter::*; }
pub mod reasonix { pub use reasonix_adapter::*; }

pub fn all_adapters() -> Vec<Box<dyn HarnessAdapter>> {
    vec![
        Box::new(claude_code::ClaudeCodeAdapter),
        Box::new(codex::CodexAdapter),
        Box::new(opencode::OpenCodeAdapter),
        Box::new(pi::PiAdapter),
        Box::new(reasonix::ReasonixAdapter),
    ]
}
```

`adapters/Cargo.toml` depends on `chm-core`, `chm-harness-sdk`, and all five `*-adapter` crates (path deps). Add `"adapters"` to workspace members.

- [ ] **Step 3: Run full gate**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: green, including all golden tests.

- [ ] **Step 4: Commit**

```bash
git add adapters/
git commit -m "feat(phase3): adapter registry facade"
```

Phase complete when all steps green (reasonix adapter may be `#[ignore]`d pending fixtures — disclose in the commit message).