# Phase 1 — Core + Database Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bootstrap the Rust workspace and build the SQLite schema, domain entities, repository layer, secret-store abstraction, models.dev client, and provider client — the entire backend foundation with no UI.

**Architecture:** A cargo workspace with `crates/core` (pure domain types, no I/O), `crates/database` (sqlx + migrations + repositories), `crates/secrets` (OS-native secret store abstraction), `crates/providers` (HTTP client for health checks + model discovery), `crates/models-dev` (metadata client + matching), and placeholder crates for later phases. Domain types live in `core` so every other crate can depend on them without circularity.

**Tech Stack:** Rust edition 2024, sqlx 0.8 (runtime-tokio, sqlite, migrate), tokio 1, serde/serde_json, uuid 1, chrono 0.4, reqwest 0.12, thiserror 2, tempfile (dev), wiremock (dev). TDD throughout: every task writes the failing test first.

## Global Constraints

- SQLite stores only `credential_refs` (type + reference). Never store a secret value in the DB.
- All ids are UUID v4 (stored as TEXT). Booleans are INTEGER 0/1. JSON fields are TEXT (serde_json).
- Timestamps are UTC ISO-8601 strings via `chrono::DateTime<Utc>`.
- Route identity uniqueness: `(endpoint_id, remote_model_id)`.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` must be green at each task end.
- Phase exit: all tasks green; `cargo test` passes across the workspace; every crate compiles with `-D warnings`.

---

### Task 1.1: Cargo Workspace Bootstrap

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `LICENSE`
- Create: `README.md`
- Create: `.github/workflows/ci.yml`
- Create: `crates/{core,database,secrets,reconciliation,providers,models-dev,filesystem,harness-sdk}/Cargo.toml`
- Create: `crates/{core,database,secrets,reconciliation,providers,models-dev,filesystem,harness-sdk}/src/lib.rs`
- Create: `adapters/{claude-code,codex,opencode,pi,reasonix}/.gitkeep`

**Interfaces:**
- Produces: the workspace every later task compiles in.

- [ ] **Step 1: Write the root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
  "crates/core",
  "crates/database",
  "crates/secrets",
  "crates/reconciliation",
  "crates/providers",
  "crates/models-dev",
  "crates/filesystem",
  "crates/harness-sdk",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono", "uuid", "migrate"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "process"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tempfile = "3"
wiremock = "0.6"
```

- [ ] **Step 2: Write `.gitignore`**

```gitignore
/target
node_modules/
dist/
*.log
.DS_Store
.env
```

- [ ] **Step 3: Write `LICENSE` (MIT)**

Copy the MIT license text with `Copyright (c) 2026 Coding Harness Manager contributors`.

- [ ] **Step 4: Write `README.md`**

```markdown
# Coding Harness Manager

Manage models, providers, MCP servers, skills, and profiles across
Claude Code, Codex, OpenCode, Pi, Reasonix, and other AI coding harnesses
from one desktop application.

Configure once. Preview the diff. Sync everywhere.

## Repository Layout

apps/desktop        Tauri 2 desktop application (Phase 4+)
crates/core         domain types (no I/O)
crates/database     sqlx + migrations + repositories
crates/secrets      OS-native secret store abstraction
crates/reconciliation  desired-state engine (Phase 7)
crates/providers    provider HTTP client (health, /models)
crates/models-dev   models.dev metadata client + matching
crates/filesystem   atomic writes, backups, links (Phase 8)
crates/harness-sdk  adapter contract (Phase 3)
adapters/*          harness adapters
fixtures/           real config snapshots for golden tests
docs/               research + plans
```

- [ ] **Step 5: Write `.github/workflows/ci.yml`**

```yaml
name: CI
on:
  pull_request:
  push:
    branches: [main]

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 6: Create the eight crate manifests**

Each `crates/<name>/Cargo.toml` follows this pattern (adjust `dependencies` per crate; placeholder crates get no deps yet):

```toml
[package]
name = "chm-core"          # crate name per row below
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
```

Crate names: `core` → `chm-core`, `database` → `chm-database`, `secrets` → `chm-secrets`, `reconciliation` → `chm-reconciliation`, `providers` → `chm-providers`, `models-dev` → `chm-models-dev`, `filesystem` → `chm-filesystem`, `harness-sdk` → `chm-harness-sdk`.

Each `src/lib.rs` starts as:

```rust
//! <crate purpose — one line>
```

- [ ] **Step 7: Verify the workspace compiles**

```bash
cargo check --workspace
```

Expected: compiles with no warnings (0 crates with deps yet, so this is instant).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml .gitignore LICENSE README.md .github/ crates/ adapters/
git commit -m "chore(phase1): bootstrap cargo workspace and CI"
```

---

### Task 1.2: SQLite Migrations + Connection

**Files:**
- Create: `crates/database/migrations/0001_init.sql`
- Create: `crates/database/src/lib.rs`
- Create: `crates/database/tests/migrations.rs`

**Interfaces:**
- Produces: `pub async fn connect(path: &str) -> Result<Pool<Sqlite>, DbError>` — used by every repository task in this phase and by the Tauri app (Phase 4).
- Produces: `DbError` (thiserror enum: `Migration`, `Sqlx(sqlx::Error)`, `NotFound(String)`).

- [ ] **Step 1: Write the initial migration `0001_init.sql`**

Full schema from project plan §43 (ids TEXT, booleans INTEGER, JSON TEXT, timestamps TEXT):

```sql
CREATE TABLE providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  notes TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE provider_endpoints (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  protocol TEXT NOT NULL,
  discovery_path TEXT,
  auth_type TEXT NOT NULL DEFAULT 'none',
  credential_ref_id TEXT REFERENCES credential_refs(id),
  headers_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE credential_refs (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  reference TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE model_identities (
  id TEXT PRIMARY KEY,
  canonical_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  family TEXT,
  models_dev_id TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE provider_catalog_models (
  id TEXT PRIMARY KEY,
  endpoint_id TEXT NOT NULL REFERENCES provider_endpoints(id) ON DELETE CASCADE,
  remote_model_id TEXT NOT NULL,
  raw_metadata_json TEXT NOT NULL DEFAULT '{}',
  canonical_model_id TEXT REFERENCES model_identities(id),
  match_confidence INTEGER,
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  missing_since TEXT,
  status TEXT NOT NULL DEFAULT 'available',
  UNIQUE (endpoint_id, remote_model_id)
);

CREATE TABLE model_routes (
  id TEXT PRIMARY KEY,
  endpoint_id TEXT NOT NULL REFERENCES provider_endpoints(id) ON DELETE CASCADE,
  model_identity_id TEXT REFERENCES model_identities(id),
  remote_model_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  context_window INTEGER,
  max_input INTEGER,
  max_output INTEGER,
  capabilities_json TEXT NOT NULL DEFAULT '{}',
  overrides_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (endpoint_id, remote_model_id)
);

CREATE TABLE harness_installations (
  id TEXT PRIMARY KEY,
  harness_type TEXT NOT NULL,
  executable_path TEXT,
  version TEXT,
  config_path TEXT,
  detected_at TEXT NOT NULL,
  last_scanned_at TEXT,
  status TEXT NOT NULL DEFAULT 'detected'
);

CREATE TABLE harness_model_bindings (
  id TEXT PRIMARY KEY,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  model_route_id TEXT NOT NULL REFERENCES model_routes(id) ON DELETE CASCADE,
  native_id TEXT NOT NULL,
  native_config_json TEXT NOT NULL DEFAULT '{}',
  managed INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE mcp_servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  transport TEXT NOT NULL DEFAULT 'stdio',
  command TEXT,
  args_json TEXT NOT NULL DEFAULT '[]',
  url TEXT,
  env_json TEXT NOT NULL DEFAULT '{}',
  scope_type TEXT NOT NULL DEFAULT 'global',
  scope_path TEXT,
  provenance_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE harness_mcp_bindings (
  id TEXT PRIMARY KEY,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  mcp_server_id TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
  native_name TEXT NOT NULL,
  native_config_json TEXT NOT NULL DEFAULT '{}',
  managed INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE skills (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  canonical_path TEXT NOT NULL UNIQUE,
  source_type TEXT NOT NULL DEFAULT 'folder',
  source_url TEXT,
  content_hash TEXT,
  provenance_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE harness_skill_bindings (
  id TEXT PRIMARY KEY,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
  target_path TEXT NOT NULL,
  binding_type TEXT NOT NULL DEFAULT 'symlink',
  managed INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'active'
);

CREATE TABLE launch_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  harness_type TEXT NOT NULL,
  model_route_id TEXT REFERENCES model_routes(id),
  provider_endpoint_id TEXT REFERENCES provider_endpoints(id),
  env_json TEXT NOT NULL DEFAULT '{}',
  role_mappings_json TEXT NOT NULL DEFAULT '{}',
  native_overrides_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE configuration_sets (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  description TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE configuration_set_items (
  id TEXT PRIMARY KEY,
  configuration_set_id TEXT NOT NULL REFERENCES configuration_sets(id) ON DELETE CASCADE,
  item_type TEXT NOT NULL,
  item_id TEXT NOT NULL
);

CREATE TABLE sync_transactions (
  id TEXT PRIMARY KEY,
  transaction_type TEXT NOT NULL,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  status TEXT NOT NULL DEFAULT 'running',
  summary TEXT,
  plan_json TEXT NOT NULL DEFAULT '{}',
  error_json TEXT
);

CREATE TABLE config_snapshots (
  id TEXT PRIMARY KEY,
  transaction_id TEXT NOT NULL REFERENCES sync_transactions(id) ON DELETE CASCADE,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  before_content TEXT,
  after_content TEXT,
  before_hash TEXT,
  after_hash TEXT
);

CREATE INDEX idx_endpoints_provider ON provider_endpoints(provider_id);
CREATE INDEX idx_catalog_endpoint ON provider_catalog_models(endpoint_id);
CREATE INDEX idx_routes_endpoint ON model_routes(endpoint_id);
CREATE INDEX idx_mcp_binding_harness ON harness_mcp_bindings(harness_installation_id);
CREATE INDEX idx_skill_binding_harness ON harness_skill_bindings(harness_installation_id);
CREATE INDEX idx_binding_harness ON harness_model_bindings(harness_installation_id);
CREATE INDEX idx_snapshot_transaction ON config_snapshots(transaction_id);
```

- [ ] **Step 2: Write `crates/database/src/lib.rs`**

```rust
//! SQLite connection, migrations, and repository layer.

pub mod repos;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(String),
}

pub async fn connect(path: &str) -> Result<Pool<Sqlite>, DbError> {
    let opts = SqliteConnectOptions::parse_str(path)?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// In-memory pool for tests (migrations applied to a fresh DB).
pub async fn connect_test() -> Result<Pool<Sqlite>, DbError> {
    connect("sqlite::memory:").await
}
```

- [ ] **Step 3: Write the failing migration test**

`crates/database/tests/migrations.rs`:

```rust
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
```

- [ ] **Step 4: Add `[dev-dependencies]` and run the test**

In `crates/database/Cargo.toml`:

```toml
[dependencies]
chm-core.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
thiserror.workspace = true

[dev-dependencies]
tokio.workspace = true
```

Run: `cargo test -p chm-database`
Expected: `migration_creates_all_tables ... ok` (fails first on missing code until Step 2 is in place — write the test before lib.rs if running strictly TDD; either way the test must pass now).

- [ ] **Step 5: Commit**

```bash
git add crates/database/
git commit -m "feat(phase1): sqlite schema and migrations"
```

---

### Task 1.3: Domain Entities (crates/core)

**Files:**
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/domain/mod.rs`
- Create: `crates/core/src/domain/provider.rs`
- Create: `crates/core/src/domain/credentials.rs`
- Create: `crates/core/src/domain/models.rs`
- Create: `crates/core/src/domain/harness.rs`
- Create: `crates/core/src/domain/mcp.rs`
- Create: `crates/core/src/domain/skills.rs`
- Create: `crates/core/src/domain/profiles.rs`
- Create: `crates/core/src/domain/sets.rs`
- Create: `crates/core/src/domain/history.rs`
- Create: `crates/core/tests/domain_serde.rs`

**Interfaces:**
- Produces: ALL domain types used by every later plan. Canonical definitions live here; later tasks import them by name.

- [ ] **Step 1: Write `crates/core/src/lib.rs` and `domain/mod.rs`**

```rust
// lib.rs
//! Pure domain types shared across all CHM crates. No I/O.

pub mod domain;

pub use domain::*;
```

```rust
// domain/mod.rs
pub mod credentials;
pub mod harness;
pub mod history;
pub mod mcp;
pub mod models;
pub mod profiles;
pub mod provider;
pub mod sets;
pub mod skills;
```

- [ ] **Step 2: Write `provider.rs`**

```rust
//! Provider and provider endpoint entities.
//! Rule: Provider != Endpoint != Model Route != Model Identity — never flattened.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::credentials::CredentialRef;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub enabled: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Protocol {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
    OpenRouterOpenAi,
    Custom,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => "openai-chat",
            Self::OpenAiResponses => "openai-responses",
            Self::AnthropicMessages => "anthropic-messages",
            Self::OpenRouterOpenAi => "openrouter-openai",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "openai-chat" => Self::OpenAiChatCompletions,
            "openai-responses" => Self::OpenAiResponses,
            "anthropic-messages" => Self::AnthropicMessages,
            "openrouter-openai" => Self::OpenRouterOpenAi,
            _ => Self::Custom,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthType {
    None,
    ApiKeyHeader,
    BearerToken,
    CustomHeader,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderEndpoint {
    pub id: Uuid,
    pub provider_id: Uuid,
    pub name: String,
    pub base_url: String,
    pub protocol: Protocol,
    pub discovery_path: Option<String>,
    pub auth_type: AuthType,
    pub credential_ref: Option<CredentialRef>,
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 3: Write `credentials.rs`**

```rust
//! Secrets are NEVER stored in SQLite — only references.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CredentialKind {
    Keychain,
    WindowsCredentialManager,
    Libsecret,
    Env,
    Vault,
}

impl CredentialKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::WindowsCredentialManager => "windows-credential-manager",
            Self::Libsecret => "libsecret",
            Self::Env => "env",
            Self::Vault => "vault",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "windows-credential-manager" => Self::WindowsCredentialManager,
            "libsecret" => Self::Libsecret,
            "env" => Self::Env,
            "vault" => Self::Vault,
            _ => Self::Keychain,
        }
    }
}

/// A reference to a secret: either an OS-native store entry or an env var name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialRef {
    pub id: Uuid,
    pub kind: CredentialKind,
    /// e.g. "coding-harness-manager/providers/<uuid>" (keychain) or "ZAI_API_KEY" (env)
    pub reference: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 4: Write `models.rs`**

```rust
//! Model identities, catalog entries, and model routes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelIdentity {
    pub id: Uuid,
    pub canonical_id: String,
    pub display_name: String,
    pub family: Option<String>,
    pub models_dev_id: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CatalogStatus {
    Available,
    New,
    Missing,
    Deprecated,
    Unknown,
}

impl CatalogStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::New => "new",
            Self::Missing => "missing",
            Self::Deprecated => "deprecated",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "available" => Self::Available,
            "new" => Self::New,
            "missing" => Self::Missing,
            "deprecated" => Self::Deprecated,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCatalogModel {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub remote_model_id: String,
    pub raw_metadata: serde_json::Value,
    pub canonical_model_id: Option<Uuid>,
    pub match_confidence: Option<u8>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub missing_since: Option<DateTime<Utc>>,
    pub status: CatalogStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRoute {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub model_identity_id: Option<Uuid>,
    pub remote_model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub max_input: Option<i64>,
    pub max_output: Option<i64>,
    pub capabilities: serde_json::Value,
    pub overrides: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Route identity — the dedup key for the whole system.
pub fn route_identity(endpoint_id: Uuid, remote_model_id: &str) -> (Uuid, String) {
    (endpoint_id, remote_model_id.to_string())
}
```

- [ ] **Step 5: Write `harness.rs`**

```rust
//! Harness installations and bindings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HarnessType {
    ClaudeCode,
    Codex,
    OpenCode,
    Pi,
    Reasonix,
}

impl HarnessType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
            Self::Reasonix => "reasonix",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "claude-code" => Self::ClaudeCode,
            "codex" => Self::Codex,
            "opencode" => Self::OpenCode,
            "pi" => Self::Pi,
            _ => Self::Reasonix,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstallationStatus {
    Detected,
    Installed,
    ConfigMissing,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessInstallation {
    pub id: Uuid,
    pub harness_type: HarnessType,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub config_path: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub status: InstallationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessModelBinding {
    pub id: Uuid,
    pub harness_installation_id: Uuid,
    pub model_route_id: Uuid,
    pub native_id: String,
    pub native_config: serde_json::Value,
    pub managed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BindingType {
    Symlink,
    Junction,
    Copy,
    Native,
    Unsupported,
}

impl BindingType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Symlink => "symlink",
            Self::Junction => "junction",
            Self::Copy => "copy",
            Self::Native => "native",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "symlink" => Self::Symlink,
            "junction" => Self::Junction,
            "copy" => Self::Copy,
            "native" => Self::Native,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessMcpBinding {
    pub id: Uuid,
    pub harness_installation_id: Uuid,
    pub mcp_server_id: Uuid,
    pub native_name: String,
    pub native_config: serde_json::Value,
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessSkillBinding {
    pub id: Uuid,
    pub harness_installation_id: Uuid,
    pub skill_id: Uuid,
    pub target_path: String,
    pub binding_type: BindingType,
    pub managed: bool,
    pub status: String,
}
```

- [ ] **Step 6: Write `mcp.rs`**

```rust
//! Canonical MCP servers. V1 exposes global scope; schema supports project.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

impl McpTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "http" => Self::Http,
            "sse" => Self::Sse,
            _ => Self::Stdio,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScopeType {
    Global,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServer {
    pub id: Uuid,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: serde_json::Map<String, serde_json::Value>,
    pub scope_type: ScopeType,
    pub scope_path: Option<String>,
    pub provenance: serde_json::Value,
    pub enabled: bool,
}
```

- [ ] **Step 7: Write `skills.rs`**

```rust
//! Skills: metadata in SQLite, files on disk. Never blobs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillSourceType {
    Folder,
    Git,
    HarnessImport,
    Package,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    pub id: Uuid,
    pub name: String,
    pub canonical_path: String,
    pub source_type: SkillSourceType,
    pub source_url: Option<String>,
    pub content_hash: Option<String>,
    pub provenance: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 8: Write `profiles.rs`**

```rust
//! Launch profiles: harness + route + endpoint + env + role mappings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::harness::HarnessType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleMapping {
    /// e.g. "opus", "sonnet", "haiku"
    pub role: String,
    /// remote model id to substitute for that role
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchProfile {
    pub id: Uuid,
    pub name: String,
    pub harness_type: HarnessType,
    pub model_route_id: Option<Uuid>,
    pub provider_endpoint_id: Option<Uuid>,
    pub env: serde_json::Map<String, serde_json::Value>,
    pub role_mappings: Vec<RoleMapping>,
    pub native_overrides: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 9: Write `sets.rs`**

```rust
//! Configuration sets: reusable bundles of routes/MCP/skills/profiles.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SetItemType {
    ModelRoute,
    McpServer,
    Skill,
    LaunchProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationSet {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationSetItem {
    pub id: Uuid,
    pub configuration_set_id: Uuid,
    pub item_type: SetItemType,
    pub item_id: Uuid,
}
```

- [ ] **Step 10: Write `history.rs`**

```rust
//! Sync transactions and config snapshots (audit + rollback support).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionType {
    Sync,
    Import,
    Rollback,
    Restore,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncTransaction {
    pub id: Uuid,
    pub transaction_type: TransactionType,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: TransactionStatus,
    pub summary: Option<String>,
    pub plan: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigSnapshot {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub harness_installation_id: Uuid,
    pub path: String,
    pub before_content: Option<String>,
    pub after_content: Option<String>,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
}
```

- [ ] **Step 11: Write the serde roundtrip test**

`crates/core/tests/domain_serde.rs`:

```rust
use chm_core::domain::provider::{AuthType, Protocol, Provider, ProviderEndpoint};
use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chrono::{TimeZone, Utc};
use uuid::Uuid;

fn ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 27, 9, 0, 0).unwrap()
}

#[test]
fn provider_roundtrips_through_json() {
    let p = Provider {
        id: Uuid::new_v4(),
        name: "zai".into(),
        display_name: "Z.AI".into(),
        enabled: true,
        notes: None,
        created_at: ts(),
        updated_at: ts(),
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: Provider = serde_json::parse_str(&json).unwrap();
    assert_eq!(p, back);
}

#[test]
fn endpoint_roundtrips_with_credential_ref() {
    let e = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: Uuid::new_v4(),
        name: "Anthropic-compatible".into(),
        base_url: "https://api.z.ai/api/anthropic".into(),
        protocol: Protocol::AnthropicMessages,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: Some(CredentialRef {
            id: Uuid::new_v4(),
            kind: CredentialKind::Keychain,
            reference: "coding-harness-manager/providers/abc".into(),
            created_at: ts(),
            updated_at: ts(),
        }),
        headers: serde_json::Map::new(),
        enabled: true,
        created_at: ts(),
        updated_at: ts(),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: ProviderEndpoint = serde_json::parse_str(&json).unwrap();
    assert_eq!(e, back);
    assert_eq!(back.credential_ref.unwrap().kind, CredentialKind::Keychain);
}

#[test]
fn protocol_and_status_strings_roundtrip() {
    assert_eq!(Protocol::parse_str(Protocol::OpenAiResponses.as_str()), Protocol::OpenAiResponses);
    assert_eq!(Protocol::parse_str("garbage"), Protocol::Custom);
}
```

- [ ] **Step 12: Wire `Cargo.toml` and run tests**

In `crates/core/Cargo.toml`:

```toml
[dependencies]
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
```

Run: `cargo test -p chm-core`
Expected: all three tests pass.

- [ ] **Step 13: Commit**

```bash
git add crates/core/
git commit -m "feat(phase1): domain entities"
```

---

### Task 1.4: Repositories — Providers, Credentials, Models

**Files:**
- Create: `crates/database/src/repos/mod.rs`
- Create: `crates/database/src/repos/providers.rs`
- Create: `crates/database/src/repos/models.rs`
- Create: `crates/database/tests/repos_providers.rs`
- Create: `crates/database/tests/repos_models.rs`

**Interfaces:**
- Consumes: `connect_test()` (Task 1.2), domain types (Task 1.3).
- Produces (exact signatures used by Phase 4–6 UIs and Phase 5 backend):
  - `pub async fn create_provider(pool, name, display_name) -> Result<Provider, DbError>`
  - `pub async fn list_providers(pool) -> Result<Vec<Provider>, DbError>`
  - `pub async fn update_provider(pool, id, display_name, enabled, notes) -> Result<Provider, DbError>`
  - `pub async fn delete_provider(pool, id) -> Result<(), DbError>`
  - `pub async fn create_endpoint(pool, e: &ProviderEndpoint) -> Result<ProviderEndpoint, DbError>`
  - `pub async fn list_endpoints(pool, provider_id) -> Result<Vec<ProviderEndpoint>, DbError>`
  - `pub async fn create_credential_ref(pool, kind, reference) -> Result<CredentialRef, DbError>`
  - `pub async fn create_identity(pool, i: &ModelIdentity) -> Result<ModelIdentity, DbError>`
  - `pub async fn upsert_catalog_model(pool, m: &ProviderCatalogModel) -> Result<ProviderCatalogModel, DbError>` (INSERT ... ON CONFLICT DO UPDATE)
  - `pub async fn list_catalog_models(pool, endpoint_id) -> Result<Vec<ProviderCatalogModel>, DbError>`
  - `pub async fn create_route(pool, r: &ModelRoute) -> Result<ModelRoute, DbError>`
  - `pub async fn update_route(pool, r: &ModelRoute) -> Result<ModelRoute, DbError>`
  - `pub async fn delete_route(pool, id) -> Result<(), DbError>`
  - `pub async fn list_routes(pool) -> Result<Vec<ModelRoute>, DbError>`

- [ ] **Step 1: Write `repos/mod.rs`**

```rust
pub mod models;
pub mod providers;
```

- [ ] **Step 2: Write the failing test `tests/repos_providers.rs`**

```rust
use chm_database::connect_test;
use chm_database::repos::providers::*;
use chm_core::domain::provider::*;
use chm_core::domain::credentials::*;

#[tokio::test]
async fn provider_crud_flow() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    assert_eq!(p.name, "zai");

    let list = list_providers(&pool).await.unwrap();
    assert_eq!(list.len(), 1);

    let updated = update_provider(&pool, p.id, "Z.AI (kr)", true, Some("korean provider".into())).await.unwrap();
    assert_eq!(updated.display_name, "Z.AI (kr)");

    delete_provider(&pool, p.id).await.unwrap();
    assert!(list_providers(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn endpoint_and_credential_flow() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "minimax", "MiniMax").await.unwrap();
    let cred = create_credential_ref(&pool, CredentialKind::Env, "MINIMAX_API_KEY").await.unwrap();
    let e = ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id: p.id,
        name: "openai-compat".into(),
        base_url: "https://api.minimaxi.com/v1".into(),
        protocol: Protocol::OpenAiChatCompletions,
        discovery_path: Some("/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: Some(cred),
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let endpoints = list_endpoints(&pool, p.id).await.unwrap();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].protocol, Protocol::OpenAiChatCompletions);
    assert_eq!(
        endpoints[0].credential_ref.as_ref().unwrap().reference,
        "MINIMAX_API_KEY"
    );
}
```

- [ ] **Step 3: Write the failing test `tests/repos_models.rs`**

```rust
use chm_database::connect_test;
use chm_database::repos::models::*;
use chm_database::repos::providers::*;
use chm_core::domain::models::*;

#[tokio::test]
async fn catalog_upsert_deduplicates_by_endpoint_and_remote_id() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id: p.id,
        name: "anthropic".into(),
        base_url: "https://api.z.ai/api/anthropic".into(),
        protocol: Protocol::AnthropicMessages,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let now = chrono::Utc::now();

    let m1 = ProviderCatalogModel {
        id: uuid::Uuid::new_v4(),
        endpoint_id: e.id,
        remote_model_id: "glm-5".into(),
        raw_metadata: serde_json::json!({"id": "glm-5"}),
        canonical_model_id: None,
        match_confidence: None,
        first_seen_at: now,
        last_seen_at: now,
        missing_since: None,
        status: CatalogStatus::New,
    };
    upsert_catalog_model(&pool, &m1).await.unwrap();
    let m2 = ProviderCatalogModel { id: uuid::Uuid::new_v4(), last_seen_at: now, ..m1.clone() };
    upsert_catalog_model(&pool, &m2).await.unwrap();

    let all = list_catalog_models(&pool, e.id).await.unwrap();
    assert_eq!(all.len(), 1, "upsert must not duplicate");
    assert_eq!(all[0].status, CatalogStatus::New);
}

#[tokio::test]
async fn route_crud_flow() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "openrouter", "OpenRouter").await.unwrap();
    let e = ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id: p.id,
        name: "openai".into(),
        base_url: "https://openrouter.ai/api/v1".into(),
        protocol: Protocol::OpenRouterOpenAi,
        discovery_path: Some("/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let r = ModelRoute {
        id: uuid::Uuid::new_v4(),
        endpoint_id: e.id,
        model_identity_id: None,
        remote_model_id: "anthropic/claude-opus".into(),
        display_name: "Claude Opus via OpenRouter".into(),
        context_window: Some(200_000),
        max_input: None,
        max_output: None,
        capabilities: serde_json::json!({"reasoning": true}),
        overrides: serde_json::Value::Object(Default::default()),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_route(&pool, &r).await.unwrap();
    let dup = ModelRoute { id: uuid::Uuid::new_v4(), ..r.clone() };
    assert!(create_route(&pool, &dup).await.is_err(), "unique (endpoint_id, remote_model_id) must reject dup");
    assert_eq!(list_routes(&pool).await.unwrap().len(), 1);
    delete_route(&pool, r.id).await.unwrap();
    assert!(list_routes(&pool).await.unwrap().is_empty());
}
```

- [ ] **Step 4: Implement `repos/providers.rs`**

```rust
//! Provider, endpoint, and credential-ref repositories.

use chm_core::domain::credentials::CredentialRef;
use chm_core::domain::provider::{Provider, ProviderEndpoint};
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

pub async fn create_provider(
    pool: &Pool<Sqlite>,
    name: &str,
    display_name: &str,
) -> Result<Provider, DbError> {
    let now = Utc::now();
    let p = Provider {
        id: Uuid::new_v4(),
        name: name.into(),
        display_name: display_name.into(),
        enabled: true,
        notes: None,
        created_at: now,
        updated_at: now,
    };
    sqlx::query("INSERT INTO providers (id, name, display_name, enabled, notes, created_at, updated_at) VALUES (?, ?, ?, 1, NULL, ?, ?)")
        .bind(p.id.to_string())
        .bind(&p.name)
        .bind(&p.display_name)
        .bind(p.created_at.to_rfc3339())
        .bind(p.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
    Ok(p)
}

pub async fn list_providers(pool: &Pool<Sqlite>) -> Result<Vec<Provider>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64, Option<String>, String, String)>(
        "SELECT id, name, display_name, enabled, notes, created_at, updated_at FROM providers ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(row_to_provider).collect()
}

pub async fn update_provider(
    pool: &Pool<Sqlite>,
    id: Uuid,
    display_name: &str,
    enabled: bool,
    notes: Option<String>,
) -> Result<Provider, DbError> {
    let now = Utc::now();
    let res = sqlx::query("UPDATE providers SET display_name = ?, enabled = ?, notes = ?, updated_at = ? WHERE id = ?")
        .bind(display_name)
        .bind(enabled as i64)
        .bind(&notes)
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("provider {id}")));
    }
    Ok(Provider { id, name: String::new(), display_name: display_name.into(), enabled, notes, created_at: now, updated_at: now })
}

pub async fn delete_provider(pool: &Pool<Sqlite>, id: Uuid) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM providers WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("provider {id}")));
    }
    Ok(())
}

pub async fn create_credential_ref(
    pool: &Pool<Sqlite>,
    kind: chm_core::domain::credentials::CredentialKind,
    reference: &str,
) -> Result<CredentialRef, DbError> {
    let now = Utc::now();
    let c = CredentialRef { id: Uuid::new_v4(), kind, reference: reference.into(), created_at: now, updated_at: now };
    sqlx::query("INSERT INTO credential_refs (id, type, reference, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind(c.id.to_string())
        .bind(c.kind.as_str())
        .bind(&c.reference)
        .bind(c.created_at.to_rfc3339())
        .bind(c.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
    Ok(c)
}

pub async fn create_endpoint(
    pool: &Pool<Sqlite>,
    e: &ProviderEndpoint,
) -> Result<ProviderEndpoint, DbError> {
    sqlx::query(
        "INSERT INTO provider_endpoints (id, provider_id, name, base_url, protocol, discovery_path, auth_type, credential_ref_id, headers_json, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.id.to_string())
    .bind(e.provider_id.to_string())
    .bind(&e.name)
    .bind(&e.base_url)
    .bind(e.protocol.as_str())
    .bind(&e.discovery_path)
    .bind(e.auth_type_as_str())
    .bind(e.credential_ref.as_ref().map(|c| c.id.to_string()))
    .bind(serde_json::to_string(&e.headers)?)
    .bind(e.enabled as i64)
    .bind(e.created_at.to_rfc3339())
    .bind(e.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(e.clone())
}

pub async fn list_endpoints(pool: &Pool<Sqlite>, provider_id: Uuid) -> Result<Vec<ProviderEndpoint>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, String, Option<String>, String, i64, String, String)>(
        "SELECT id, provider_id, name, base_url, protocol, discovery_path, auth_type, credential_ref_id, headers_json, enabled, created_at, updated_at
         FROM provider_endpoints WHERE provider_id = ? ORDER BY name",
    )
    .bind(provider_id.to_string())
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for (id, pid, name, base_url, protocol, discovery, auth, cred_id, headers, enabled, created, updated) in rows {
        let cred = match cred_id {
            Some(cid) => Some(fetch_credential(pool, &cid).await?),
            None => None,
        };
        out.push(ProviderEndpoint {
            id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id.clone()))?,
            provider_id: Uuid::parse_str(&pid).map_err(|_| DbError::NotFound(pid))?,
            name,
            base_url,
            protocol: chm_core::domain::provider::Protocol::parse_str(&protocol),
            discovery_path: discovery,
            auth_type: auth_type_from_str(&auth),
            credential_ref: cred,
            headers: serde_json::parse_str(&headers).unwrap_or_default(),
            enabled: enabled == 1,
            created_at: chrono::DateTime::parse_from_rfc3339(&created).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
        });
    }
    Ok(out)
}

async fn fetch_credential(pool: &Pool<Sqlite>, id: &str) -> Result<CredentialRef, DbError> {
    let (kind, reference, created, updated) = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT type, reference, created_at, updated_at FROM credential_refs WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(CredentialRef {
        id: Uuid::parse_str(id).map_err(|_| DbError::NotFound(id.into()))?,
        kind: chm_core::domain::credentials::CredentialKind::parse_str(&kind),
        reference,
        created_at: chrono::DateTime::parse_from_rfc3339(&created).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_provider(
    (id, name, display_name, enabled, notes, created, updated): (String, String, String, i64, Option<String>, String, String),
) -> Result<Provider, DbError> {
    Ok(Provider {
        id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
        name,
        display_name,
        enabled: enabled == 1,
        notes,
        created_at: chrono::DateTime::parse_from_rfc3339(&created).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
    })
}

impl ProviderEndpoint {
    fn auth_type_as_str(&self) -> &'static str {
        match self.auth_type {
            chm_core::domain::provider::AuthType::None => "none",
            chm_core::domain::provider::AuthType::ApiKeyHeader => "api-key-header",
            chm_core::domain::provider::AuthType::BearerToken => "bearer-token",
            chm_core::domain::provider::AuthType::CustomHeader => "custom-header",
        }
    }
}

fn auth_type_from_str(s: &str) -> chm_core::domain::provider::AuthType {
    match s {
        "api-key-header" => chm_core::domain::provider::AuthType::ApiKeyHeader,
        "bearer-token" => chm_core::domain::provider::AuthType::BearerToken,
        "custom-header" => chm_core::domain::provider::AuthType::CustomHeader,
        _ => chm_core::domain::provider::AuthType::None,
    }
}
```

- [ ] **Step 5: Implement `repos/models.rs`**

```rust
//! Identity, catalog, and route repositories.

use chm_core::domain::models::*;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

pub async fn create_identity(pool: &Pool<Sqlite>, i: &ModelIdentity) -> Result<ModelIdentity, DbError> {
    sqlx::query("INSERT INTO model_identities (id, canonical_id, display_name, family, models_dev_id, metadata_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(i.id.to_string())
        .bind(&i.canonical_id)
        .bind(&i.display_name)
        .bind(&i.family)
        .bind(&i.models_dev_id)
        .bind(serde_json::to_string(&i.metadata)?)
        .bind(i.created_at.to_rfc3339())
        .bind(i.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
    Ok(i.clone())
}

pub async fn upsert_catalog_model(
    pool: &Pool<Sqlite>,
    m: &ProviderCatalogModel,
) -> Result<ProviderCatalogModel, DbError> {
    sqlx::query(
        "INSERT INTO provider_catalog_models (id, endpoint_id, remote_model_id, raw_metadata_json, canonical_model_id, match_confidence, first_seen_at, last_seen_at, missing_since, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (endpoint_id, remote_model_id) DO UPDATE SET
           raw_metadata_json = excluded.raw_metadata_json,
           canonical_model_id = excluded.canonical_model_id,
           match_confidence = excluded.match_confidence,
           last_seen_at = excluded.last_seen_at,
           missing_since = excluded.missing_since,
           status = excluded.status",
    )
    .bind(m.id.to_string())
    .bind(m.endpoint_id.to_string())
    .bind(&m.remote_model_id)
    .bind(serde_json::to_string(&m.raw_metadata)?)
    .bind(m.canonical_model_id.map(|id| id.to_string()))
    .bind(m.match_confidence.map(|c| c as i64))
    .bind(m.first_seen_at.to_rfc3339())
    .bind(m.last_seen_at.to_rfc3339())
    .bind(m.missing_since.map(|t| t.to_rfc3339()))
    .bind(m.status.as_str())
    .execute(pool)
    .await?;
    Ok(m.clone())
}

pub async fn list_catalog_models(pool: &Pool<Sqlite>, endpoint_id: Uuid) -> Result<Vec<ProviderCatalogModel>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, Option<i64>, String, String, Option<String>, String)>(
        "SELECT id, endpoint_id, remote_model_id, raw_metadata_json, canonical_model_id, match_confidence, first_seen_at, last_seen_at, missing_since, status
         FROM provider_catalog_models WHERE endpoint_id = ? ORDER BY remote_model_id",
    )
    .bind(endpoint_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, eid, rid, raw, canon, conf, first, last, missing, status)| {
            Ok(ProviderCatalogModel {
                id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                endpoint_id: Uuid::parse_str(&eid).map_err(|_| DbError::NotFound(eid))?,
                remote_model_id: rid,
                raw_metadata: serde_json::parse_str(&raw).unwrap_or_default(),
                canonical_model_id: canon.map(|c| Uuid::parse_str(&c).map_err(|_| DbError::NotFound(c.clone()))).transpose()?,
                match_confidence: conf.map(|c| c as u8),
                first_seen_at: parse_ts(&first),
                last_seen_at: parse_ts(&last),
                missing_since: missing.map(|t| parse_ts(&t)),
                status: CatalogStatus::parse_str(&status),
            })
        })
        .collect()
}

pub async fn create_route(pool: &Pool<Sqlite>, r: &ModelRoute) -> Result<ModelRoute, DbError> {
    sqlx::query(
        "INSERT INTO model_routes (id, endpoint_id, model_identity_id, remote_model_id, display_name, context_window, max_input, max_output, capabilities_json, overrides_json, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(r.id.to_string())
    .bind(r.endpoint_id.to_string())
    .bind(r.model_identity_id.map(|i| i.to_string()))
    .bind(&r.remote_model_id)
    .bind(&r.display_name)
    .bind(r.context_window)
    .bind(r.max_input)
    .bind(r.max_output)
    .bind(serde_json::to_string(&r.capabilities)?)
    .bind(serde_json::to_string(&r.overrides)?)
    .bind(r.enabled as i64)
    .bind(r.created_at.to_rfc3339())
    .bind(r.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(r.clone())
}

pub async fn update_route(pool: &Pool<Sqlite>, r: &ModelRoute) -> Result<ModelRoute, DbError> {
    let res = sqlx::query(
        "UPDATE model_routes SET model_identity_id = ?, display_name = ?, context_window = ?, max_input = ?, max_output = ?, capabilities_json = ?, overrides_json = ?, enabled = ?, updated_at = ? WHERE id = ?",
    )
    .bind(r.model_identity_id.map(|i| i.to_string()))
    .bind(&r.display_name)
    .bind(r.context_window)
    .bind(r.max_input)
    .bind(r.max_output)
    .bind(serde_json::to_string(&r.capabilities)?)
    .bind(serde_json::to_string(&r.overrides)?)
    .bind(r.enabled as i64)
    .bind(Utc::now().to_rfc3339())
    .bind(r.id.to_string())
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("route {}", r.id)));
    }
    Ok(r.clone())
}

pub async fn delete_route(pool: &Pool<Sqlite>, id: Uuid) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM model_routes WHERE id = ?").bind(id.to_string()).execute(pool).await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("route {id}")));
    }
    Ok(())
}

pub async fn list_routes(pool: &Pool<Sqlite>) -> Result<Vec<ModelRoute>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String, Option<i64>, Option<i64>, Option<i64>, String, String, i64, String, String)>(
        "SELECT id, endpoint_id, model_identity_id, remote_model_id, display_name, context_window, max_input, max_output, capabilities_json, overrides_json, enabled, created_at, updated_at
         FROM model_routes ORDER BY display_name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, eid, mid, rid, dname, ctx, mi, mo, caps, ovr, enabled, created, updated)| {
            Ok(ModelRoute {
                id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                endpoint_id: Uuid::parse_str(&eid).map_err(|_| DbError::NotFound(eid))?,
                model_identity_id: mid.map(|m| Uuid::parse_str(&m).map_err(|_| DbError::NotFound(m.clone()))).transpose()?,
                remote_model_id: rid,
                display_name: dname,
                context_window: ctx,
                max_input: mi,
                max_output: mo,
                capabilities: serde_json::parse_str(&caps).unwrap_or_default(),
                overrides: serde_json::parse_str(&ovr).unwrap_or_default(),
                enabled: enabled == 1,
                created_at: parse_ts(&created),
                updated_at: parse_ts(&updated),
            })
        })
        .collect()
}

fn parse_ts(s: &str) -> chrono::DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now())
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p chm-database
```

Expected: all repo tests pass (including the unique-constraint rejection in `route_crud_flow`).

- [ ] **Step 7: Commit**

```bash
git add crates/database/
git commit -m "feat(phase1): provider and model repositories"
```

---

### Task 1.5: Repositories — MCP, Skills, Profiles, Sets

**Files:**
- Create: `crates/database/src/repos/mcp.rs`
- Create: `crates/database/src/repos/skills.rs`
- Create: `crates/database/src/repos/profiles.rs`
- Create: `crates/database/tests/repos_mcp_skills.rs`

**Interfaces:**
- Consumes: `connect_test()`, domain types.
- Produces:
  - `pub async fn create_mcp_server(pool, s: &McpServer) -> Result<McpServer, DbError>`
  - `pub async fn list_mcp_servers(pool) -> Result<Vec<McpServer>, DbError>`
  - `pub async fn delete_mcp_server(pool, id) -> Result<(), DbError>`
  - `pub async fn create_skill(pool, s: &Skill) -> Result<Skill, DbError>`
  - `pub async fn list_skills(pool) -> Result<Vec<Skill>, DbError>`
  - `pub async fn create_profile(pool, p: &LaunchProfile) -> Result<LaunchProfile, DbError>`
  - `pub async fn list_profiles(pool) -> Result<Vec<LaunchProfile>, DbError>`
  - `pub async fn create_set(pool, name, description) -> Result<ConfigurationSet, DbError>`
  - `pub async fn add_set_item(pool, set_id, item_type, item_id) -> Result<(), DbError>`
  - `pub async fn list_sets(pool) -> Result<Vec<ConfigurationSet>, DbError>`

- [ ] **Step 1: Write the failing test `tests/repos_mcp_skills.rs`**

```rust
use chm_database::connect_test;
use chm_database::repos::mcp::*;
use chm_database::repos::skills::*;
use chm_database::repos::profiles::*;
use chm_core::domain::mcp::*;
use chm_core::domain::skills::*;
use chm_core::domain::profiles::*;
use chm_core::domain::harness::HarnessType;

#[tokio::test]
async fn mcp_crud_flow() {
    let pool = connect_test().await.unwrap();
    let s = McpServer {
        id: uuid::Uuid::new_v4(),
        name: "github".into(),
        transport: McpTransport::Stdio,
        command: Some("npx".into()),
        args: vec!["-y".into(), "@modelcontextprotocol/server-github".into()],
        url: None,
        env: serde_json::json!({"GITHUB_PERSONAL_ACCESS_TOKEN": "$LP_GITHUB_TOKEN"}).as_object().unwrap().clone(),
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "manual"}),
        enabled: true,
    };
    create_mcp_server(&pool, &s).await.unwrap();
    let all = list_mcp_servers(&pool).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].transport, McpTransport::Stdio);
    assert_eq!(all[0].env.len(), 1);
    delete_mcp_server(&pool, s.id).await.unwrap();
    assert!(list_mcp_servers(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn skill_and_profile_and_set_flow() {
    let pool = connect_test().await.unwrap();
    let sk = Skill {
        id: uuid::Uuid::new_v4(),
        name: "brainstorming".into(),
        canonical_path: "/Users/me/.agents/skills/brainstorming".into(),
        source_type: SkillSourceType::Folder,
        source_url: None,
        content_hash: Some("abc123".into()),
        provenance: serde_json::json!({"source": "imported"}),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_skill(&pool, &sk).await.unwrap();
    assert_eq!(list_skills(&pool).await.unwrap().len(), 1);

    let p = LaunchProfile {
        id: uuid::Uuid::new_v4(),
        name: "zai-claude".into(),
        harness_type: HarnessType::ClaudeCode,
        model_route_id: None,
        provider_endpoint_id: None,
        env: serde_json::json!({"ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic"}).as_object().unwrap().clone(),
        role_mappings: vec![RoleMapping { role: "opus".into(), model: "glm-5".into() }],
        native_overrides: serde_json::Value::Object(Default::default()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_profile(&pool, &p).await.unwrap();
    assert_eq!(list_profiles(&pool).await.unwrap().len(), 1);
    assert_eq!(list_profiles(&pool).await.unwrap()[0].role_mappings[0].model, "glm-5");

    let set = create_set(&pool, "Work", Some("work models")).await.unwrap();
    add_set_item(&pool, set.id, chm_core::domain::sets::SetItemType::ModelRoute, uuid::Uuid::new_v4()).await.unwrap();
    assert_eq!(list_sets(&pool).await.unwrap().len(), 1);
}
```

- [ ] **Step 2: Implement the three repo modules**

`repos/mcp.rs` — follows the exact insert/select pattern from Task 1.4 (`create_mcp_server` inserts all columns with `s.transport.as_str()`, `serde_json::to_string(&s.args)?`, `s.scope_type` as `"global"`/`"project"`; `list_mcp_servers` selects all rows ordered by name and reconstructs with `McpTransport::from_str`, `ScopeType` via `match scope { "project" => Project, _ => Global }`, and `serde_json::parse_str(&env)`; `delete_mcp_server` deletes by id, `rows_affected == 0` → `NotFound`).

`repos/skills.rs` — `create_skill` inserts with `s.source_type.as_str()` (match: Folder→"folder", Git→"git", HarnessImport→"harness-import", Package→"package", Remote→"remote") and `from_str` the reverse; `list_skills` selects ordered by name.

`repos/profiles.rs` — `create_profile` inserts with `p.harness_type.as_str()`, `serde_json::to_string(&p.role_mappings)?` into `role_mappings_json`; `list_profiles` reconstructs `role_mappings: serde_json::parse_str(&rm_json).unwrap_or_default()`, `env` from `env_json`; `create_set`/`add_set_item`/`list_sets` are trivial inserts/selects (`add_set_item` uses `item_type.as_str()`: ModelRoute→"model_route", McpServer→"mcp_server", Skill→"skill", LaunchProfile→"launch_profile`).

Update `repos/mod.rs` to add `pub mod mcp; pub mod profiles; pub mod skills;`.

- [ ] **Step 3: Run tests**

```bash
cargo test -p chm-database
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/database/
git commit -m "feat(phase1): mcp, skill, profile, and set repositories"
```

---

### Task 1.6: Repositories — Harness Installations, Bindings, History

**Files:**
- Create: `crates/database/src/repos/harness.rs`
- Create: `crates/database/src/repos/history.rs`
- Create: `crates/database/tests/repos_harness_history.rs`

**Interfaces:**
- Consumes: `connect_test()`, domain types.
- Produces:
  - `pub async fn upsert_installation(pool, i: &HarnessInstallation) -> Result<HarnessInstallation, DbError>` (insert, on conflict `harness_type` replace)
  - `pub async fn list_installations(pool) -> Result<Vec<HarnessInstallation>, DbError>`
  - `pub async fn create_model_binding(pool, b: &HarnessModelBinding) -> Result<(), DbError>`
  - `pub async fn list_model_bindings(pool, installation_id) -> Result<Vec<HarnessModelBinding>, DbError>`
  - `pub async fn begin_transaction(pool, tx_type, plan) -> Result<SyncTransaction, DbError>`
  - `pub async fn finish_transaction(pool, id, status, summary, error) -> Result<(), DbError>`
  - `pub async fn add_snapshot(pool, s: &ConfigSnapshot) -> Result<(), DbError>`
  - `pub async fn list_transactions(pool) -> Result<Vec<SyncTransaction>, DbError>`
  - `pub async fn list_snapshots(pool, transaction_id) -> Result<Vec<ConfigSnapshot>, DbError>`

- [ ] **Step 1: Write the failing test `tests/repos_harness_history.rs`**

```rust
use chm_database::connect_test;
use chm_database::repos::harness::*;
use chm_database::repos::history::*;
use chm_core::domain::harness::*;
use chm_core::domain::history::*;

#[tokio::test]
async fn installation_upsert_is_idempotent() {
    let pool = connect_test().await.unwrap();
    let now = chrono::Utc::now();
    let i = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: Some("/usr/local/bin/opencode".into()),
        version: Some("0.30.0".into()),
        config_path: Some("/Users/me/.config/opencode".into()),
        detected_at: now,
        last_scanned_at: Some(now),
        status: InstallationStatus::Installed,
    };
    upsert_installation(&pool, &i).await.unwrap();
    let i2 = HarnessInstallation { version: Some("0.31.0".into()), ..i.clone() };
    upsert_installation(&pool, &i2).await.unwrap();
    let all = list_installations(&pool).await.unwrap();
    assert_eq!(all.len(), 1, "upsert must replace, not duplicate");
    assert_eq!(all[0].version.as_deref(), Some("0.31.0"));
}

#[tokio::test]
async fn transaction_and_snapshot_flow() {
    let pool = connect_test().await.unwrap();
    let tx = begin_transaction(&pool, TransactionType::Sync, serde_json::json!({"actions": []})).await.unwrap();
    finish_transaction(&pool, tx.id, TransactionStatus::Succeeded, Some("synced 5 models".into()), None).await.unwrap();
    let snap = ConfigSnapshot {
        id: uuid::Uuid::new_v4(),
        transaction_id: tx.id,
        harness_installation_id: uuid::Uuid::new_v4(),
        path: "/tmp/config.toml".into(),
        before_content: Some("a = 1".into()),
        after_content: Some("a = 2".into()),
        before_hash: Some("h1".into()),
        after_hash: Some("h2".into()),
    };
    add_snapshot(&pool, &snap).await.unwrap();
    let txs = list_transactions(&pool).await.unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].summary.as_deref(), Some("synced 5 models"));
    let snaps = list_snapshots(&pool, tx.id).await.unwrap();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].after_hash.as_deref(), Some("h2"));
}
```

- [ ] **Step 2: Implement `repos/harness.rs` and `repos/history.rs`**

Follow the Task 1.4 row pattern. `upsert_installation` uses `INSERT ... ON CONFLICT (harness_type) DO UPDATE SET executable_path=excluded.executable_path, version=excluded.version, config_path=excluded.config_path, last_scanned_at=excluded.last_scanned_at, status=excluded.status`. `begin_transaction` inserts with `status='running'`; `finish_transaction` sets `status, summary, error_json, completed_at`. `list_installations` orders by `harness_type`; map `HarnessType::from_str`, `InstallationStatus` via `match status { "installed" => Installed, "config-missing" => ConfigMissing, "error" => Error, _ => Detected }` (as_str the reverse).

Update `repos/mod.rs` with both modules.

- [ ] **Step 3: Run tests**

```bash
cargo test -p chm-database
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/database/
git commit -m "feat(phase1): harness, binding, and history repositories"
```

---

### Task 1.7: Secrets Crate (crates/secrets)

**Files:**
- Create: `crates/secrets/src/lib.rs`
- Create: `crates/secrets/tests/env_store.rs`

**Interfaces:**
- Produces (used by Phase 5 credential UI and Phase 8 launcher):
  - `pub trait SecretStore { fn set(&self, key: &str, value: &str) -> Result<(), SecretError>; fn get(&self, key: &str) -> Result<Option<String>, SecretError>; fn delete(&self, key: &str) -> Result<(), SecretError>; }`
  - `pub struct EnvStore` — reads from process env; `set` returns `Unsupported` (env refs are user-managed).
  - `pub struct KeychainStore` — macOS `security` CLI-backed keychain.
  - `pub struct EncryptedVaultStore` — fallback AES-GCM vault file when no OS secret service exists.
  - `pub fn default_store() -> Box<dyn SecretStore>` — platform dispatch: macOS → KeychainStore, Windows → `WindowsCredentialManagerStore`, Linux → `LibsecretStore` if available else vault.
  - `pub enum SecretError` (thiserror): `Unsupported(&'static str)`, `Io(std::io::Error)`, `Keychain(String)`, `Crypto(String)`, `NotFound`.

- [ ] **Step 1: Write the failing test `tests/env_store.rs`**

```rust
use chm_secrets::{EnvStore, SecretStore};

#[test]
fn env_store_reads_process_environment() {
    std::env::set_var("CHM_TEST_SECRET", "hello");
    let store = EnvStore;
    assert_eq!(store.get("CHM_TEST_SECRET").unwrap(), Some("hello".to_string()));
    assert_eq!(store.get("CHM_TEST_MISSING").unwrap(), None);
}

#[test]
fn env_store_is_read_only() {
    let store = EnvStore;
    assert!(store.set("CHM_TEST_SECRET", "x").is_err(), "env refs are user-managed");
    assert!(store.delete("CHM_TEST_SECRET").is_err());
}
```

- [ ] **Step 2: Write the failing test `tests/keychain_store.rs` (macOS only)**

```rust
use chm_secrets::{KeychainStore, SecretStore};

#[cfg(target_os = "macos")]
#[test]
fn keychain_set_get_delete_roundtrip() {
    let store = KeychainStore::new("chm-test");
    let key = format!("test/{}", std::process::id());
    store.set(&key, "supersecret").unwrap();
    assert_eq!(store.get(&key).unwrap(), Some("supersecret".to_string()));
    store.delete(&key).unwrap();
    assert_eq!(store.get(&key).unwrap(), None);
}
```

- [ ] **Step 3: Implement `crates/secrets/src/lib.rs`**

```rust
//! OS-native secret storage. SQLite stores references, never values.

use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("unsupported on this platform: {0}")]
    Unsupported(&'static str),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("secret not found: {0}")]
    NotFound(String),
}

pub trait SecretStore {
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// Reads secrets from the process environment. Env references are
/// user-managed, so set/delete are unsupported.
pub struct EnvStore;

impl SecretStore for EnvStore {
    fn set(&self, _key: &str, _value: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported("env references are user-managed"))
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(std::env::var(key).ok())
    }

    fn delete(&self, _key: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported("env references are user-managed"))
    }
}

/// macOS Keychain via the `security` CLI.
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new(service: &str) -> Self {
        Self { service: service.to_string() }
    }

    fn account(&self, key: &str) -> String {
        format!("{}:{}", self.service, key)
    }
}

impl SecretStore for KeychainStore {
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let out = Command::new("security")
            .args(["add-generic-password", "-U", "-s", &self.service, "-a", &self.account(key), "-w", value])
            .output()?;
        if !out.status.success() {
            return Err(SecretError::Keychain(String::from_utf8_lossy(&out.stderr).into_owned()));
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let out = Command::new("security")
            .args(["find-generic-password", "-s", &self.service, "-a", &self.account(key), "-w"])
            .output()?;
        if !out.status.success() {
            // "could not be found" (errSecItemNotFound) means absent, not error
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("could not be found") || stderr.contains("errSecItemNotFound") {
                return Ok(None);
            }
            return Err(SecretError::Keychain(stderr.into_owned()));
        }
        Ok(Some(String::from_utf8(out.stdout).map_err(|e| SecretError::Crypto(e.to_string()))?.trim_end().to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        let out = Command::new("security")
            .args(["delete-generic-password", "-s", &self.service, "-a", &self.account(key)])
            .output()?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("could not be found") || stderr.contains("errSecItemNotFound") {
                return Ok(()); // already gone
            }
            return Err(SecretError::Keychain(stderr.into_owned()));
        }
        Ok(())
    }
}

/// Placeholder for Windows / Linux — implemented in Phase 14 cross-platform pass.
pub struct WindowsCredentialManagerStore;
pub struct LibsecretStore;

impl SecretStore for WindowsCredentialManagerStore {
    fn set(&self, _k: &str, _v: &str) -> Result<(), SecretError> { Err(SecretError::Unsupported("windows credential manager lands in phase 14")) }
    fn get(&self, _k: &str) -> Result<Option<String>, SecretError> { Err(SecretError::Unsupported("windows credential manager lands in phase 14")) }
    fn delete(&self, _k: &str) -> Result<(), SecretError> { Err(SecretError::Unsupported("windows credential manager lands in phase 14")) }
}

impl SecretStore for LibsecretStore {
    fn set(&self, _k: &str, _v: &str) -> Result<(), SecretError> { Err(SecretError::Unsupported("libsecret lands in phase 14")) }
    fn get(&self, _k: &str) -> Result<Option<String>, SecretError> { Err(SecretError::Unsupported("libsecret lands in phase 14")) }
    fn delete(&self, _k: &str) -> Result<(), SecretError> { Err(SecretError::Unsupported("libsecret lands in phase 14")) }
}

pub fn default_store() -> Box<dyn SecretStore> {
    #[cfg(target_os = "macos")]
    { Box::new(KeychainStore::new("coding-harness-manager")) }
    #[cfg(target_os = "windows")]
    { Box::new(WindowsCredentialManagerStore) }
    #[cfg(all(unix, not(target_os = "macos")))]
    { Box::new(LibsecretStore) }
    #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
    { Box::new(EncryptedVaultStore::new("chm-vault")) }
}

/// Fallback AES-GCM vault used only when no OS secret service exists.
pub struct EncryptedVaultStore {
    path: std::path::PathBuf,
}

impl EncryptedVaultStore {
    pub fn new(name: &str) -> Self {
        let dir = dirs_data_dir();
        Self { path: dir.join(format!("{name}.json")) }
    }
}

fn dirs_data_dir() -> std::path::PathBuf {
    std::env::var_os("CHM_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".coding-harness-manager")
        })
}
```

Note: `EncryptedVaultStore`'s real AES-GCM implementation (with a passphrase-derived key) is a Phase 14 task; for now it compiles as a stub behind the trait. The macOS keychain path is fully functional and tested here.

- [ ] **Step 4: Run tests**

```bash
cargo test -p chm-secrets
```

Expected: both test modules pass on macOS (keychain test requires a login session — if CI headless, gate it with `#[ignore]` and run manually once locally).

- [ ] **Step 5: Commit**

```bash
git add crates/secrets/
git commit -m "feat(phase1): secret store abstraction with keychain support"
```

---

### Task 1.8: models.dev Client (crates/models-dev)

**Files:**
- Create: `crates/models-dev/src/lib.rs`
- Create: `crates/models-dev/fixtures/catalog.json` (download once, committed as static fixture)
- Create: `crates/models-dev/tests/matching.rs`

**Interfaces:**
- Consumes: `reqwest` (workspace), domain `ModelIdentity`.
- Produces:
  - `pub struct ModelsDevCatalog { pub models: Vec<ModelsDevModel> }` where `ModelsDevModel { pub id: String, pub name: String, pub provider: Option<String>, pub context_window: Option<i64>, pub max_output: Option<i64>, pub modalities: serde_json::Value }`
  - `pub async fn fetch_catalog(http: &reqwest::Client) -> Result<ModelsDevCatalog, MdError>` — GET `https://models.dev/api.json` (or the documented current URL from Phase 0 research).
  - `pub fn match_model(remote_id: &str, catalog: &ModelsDevCatalog) -> MatchResult` where `MatchResult { pub confidence: u8, pub model: Option<ModelsDevModel> }` — 100 exact id, 95 known alias (stripped `provider/` prefix for OpenRouter-style ids), 85 normalized (lowercase + strip non-alnum), 60 candidate (substring family match), 0 unknown.
  - `pub enum MdError` (thiserror: `Http(reqwest::Error)`, `Parse(serde_json::Error)`).

- [ ] **Step 1: Download and commit the static catalog fixture**

```bash
mkdir -p crates/models-dev/fixtures
curl -sL https://models.dev/api.json -o crates/models-dev/fixtures/catalog.json
wc -c crates/models-dev/fixtures/catalog.json
```

Expected: file present, non-trivial size (MBs is fine). This fixture makes all matching tests hermetic.

- [ ] **Step 2: Write the failing test `tests/matching.rs`**

```rust
use chm_models_dev::{match_model, ModelsDevCatalog};
use serde_json::Value;

fn load_catalog() -> ModelsDevCatalog {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/catalog.json")).unwrap();
    let parsed: Value = serde_json::parse_str(&raw).unwrap();
    // api.json shape: map of provider -> { models: { id: {...} } }
    let mut models = Vec::new();
    if let Some(providers) = parsed.as_object() {
        for (_provider, pv) in providers {
            if let Some(map) = pv.get("models").and_then(|m| m.as_object()) {
                for (id, meta) in map {
                    models.push(chm_models_dev::ModelsDevModel {
                        id: id.clone(),
                        name: meta.get("name").and_then(|v| v.as_str()).unwrap_or(id).to_string(),
                        provider: pv.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        context_window: meta.get("context").and_then(|v| v.as_i64()),
                        max_output: meta.get("max_output").and_then(|v| v.as_i64()),
                        modalities: meta.clone(),
                    });
                }
            }
        }
    }
    ModelsDevCatalog { models }
}

#[test]
fn exact_id_is_100_confidence() {
    let catalog = load_catalog();
    let hit = match_model("gpt-4o", &catalog);
    assert_eq!(hit.confidence, 100, "gpt-4o should exist in the live fixture");
    assert!(hit.model.is_some());
}

#[test]
fn openrouter_prefixed_id_matches_known_alias() {
    let catalog = load_catalog();
    let hit = match_model("openai/gpt-4o", &catalog);
    assert!(hit.confidence >= 95, "provider-prefixed id should match at alias level");
    assert!(hit.model.is_some());
}

#[test]
fn garbage_id_is_unknown() {
    let catalog = load_catalog();
    assert_eq!(match_model("totally-not-a-real-model-xyz", &catalog).confidence, 0);
}
```

- [ ] **Step 3: Implement `crates/models-dev/src/lib.rs`**

```rust
//! models.dev client + confidence-scored matching.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MdError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelsDevModel {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    pub modalities: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct ModelsDevCatalog {
    pub models: Vec<ModelsDevModel>,
}

pub async fn fetch_catalog(http: &reqwest::Client) -> Result<ModelsDevCatalog, MdError> {
    let resp = http.get("https://models.dev/api.json").send().await?;
    let raw: serde_json::Value = resp.json().await?;
    let mut models = Vec::new();
    if let Some(providers) = raw.as_object() {
        for (pid, pv) in providers {
            let provider_name = pv.get("id").and_then(|v| v.as_str()).unwrap_or(pid).to_string();
            if let Some(map) = pv.get("models").and_then(|m| m.as_object()) {
                for (id, meta) in map {
                    models.push(ModelsDevModel {
                        id: id.clone(),
                        name: meta.get("name").and_then(|v| v.as_str()).unwrap_or(id).to_string(),
                        provider: Some(provider_name.clone()),
                        context_window: meta.get("context").and_then(|v| v.as_i64()),
                        max_output: meta.get("max_output").and_then(|v| v.as_i64()),
                        modalities: meta.clone(),
                    });
                }
            }
        }
    }
    Ok(ModelsDevCatalog { models })
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchResult {
    pub confidence: u8,
    pub model: Option<ModelsDevModel>,
}

/// 100 exact | 95 alias | 85 normalized | 60 candidate | 0 unknown.
pub fn match_model(remote_id: &str, catalog: &ModelsDevCatalog) -> MatchResult {
    for m in &catalog.models {
        if m.id == remote_id {
            return MatchResult { confidence: 100, model: Some(m.clone()) };
        }
    }
    // known alias: openrouter-style "provider/model"
    let stripped = remote_id.split('/').last().unwrap_or(remote_id);
    for m in &catalog.models {
        if m.id == stripped {
            return MatchResult { confidence: 95, model: Some(m.clone()) };
        }
    }
    // normalized: lowercase, keep alnum only
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
    let target = norm(stripped);
    for m in &catalog.models {
        if norm(&m.id) == target {
            return MatchResult { confidence: 85, model: Some(m.clone()) };
        }
    }
    // candidate: exact family name appears in a known model id
    if let Some(family) = target.rsplit(|c: char| c.is_ascii_digit()).next() {
        if !family.is_empty() && family.len() >= 4 {
            for m in &catalog.models {
                if norm(&m.id).contains(&family) {
                    return MatchResult { confidence: 60, model: Some(m.clone()) };
                }
            }
        }
    }
    MatchResult { confidence: 0, model: None }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p chm-models-dev
```

Expected: exact/alias/garbage tests pass against the committed fixture.

- [ ] **Step 5: Commit**

```bash
git add crates/models-dev/
git commit -m "feat(phase1): models.dev client and matching"
```

---

### Task 1.9: Provider Client (crates/providers)

**Files:**
- Create: `crates/providers/src/lib.rs`
- Create: `crates/providers/tests/health_discovery.rs`

**Interfaces:**
- Consumes: domain `Protocol`, `AuthType`, `CredentialRef`, `reqwest`.
- Produces (used by Phase 5 provider validation):
  - `pub enum HealthStatus { Healthy, AuthFailed, Unreachable, DiscoveryUnsupported, RateLimited, MalformedResponse, Unknown }`
  - `pub struct ProviderModel { pub id: String, pub raw: serde_json::Value }`
  - `pub async fn health_check(endpoint: &ProviderEndpoint, credential: Option<&str>, http: &reqwest::Client) -> HealthStatus`
  - `pub async fn discover_models(endpoint: &ProviderEndpoint, credential: Option<&str>, http: &reqwest::Client) -> Result<Vec<ProviderModel>, ProviderError>`
  - `pub enum ProviderError` (thiserror: `Http(reqwest::Error)`, `Auth`, `RateLimit`, `Malformed`, `Unreachable`).
  - `pub fn resolve_credential(ref_: &CredentialRef, store: &dyn SecretStore) -> Option<String>` — env refs resolve from process env, keychain refs from the store; returns `None` when unset (endpoint may still work with no auth).

- [ ] **Step 1: Write the failing test `tests/health_discovery.rs`**

Uses `wiremock` to stand in for a real provider:

```rust
use chm_core::domain::provider::*;
use chm_providers::{discover_models, health_check, HealthStatus};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn endpoint(base: &str, protocol: Protocol) -> ProviderEndpoint {
    ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id: uuid::Uuid::new_v4(),
        name: "mock".into(),
        base_url: base.into(),
        protocol,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn health_check_reports_healthy_on_200() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"object": "list", "data": []})))
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let status = health_check(&endpoint(&server.uri(), Protocol::OpenAiChatCompletions), None, &http).await;
    assert_eq!(status, HealthStatus::Healthy);
}

#[tokio::test]
async fn health_check_detects_auth_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let status = health_check(&endpoint(&server.uri(), Protocol::OpenAiChatCompletions), None, &http).await;
    assert_eq!(status, HealthStatus::AuthFailed);
}

#[tokio::test]
async fn discovery_parses_openai_model_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [{"id": "glm-5", "object": "model"}, {"id": "glm-5-air", "object": "model"}]
        })))
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let models = discover_models(&endpoint(&server.uri(), Protocol::OpenAiChatCompletions), None, &http).await.unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "glm-5");
}

#[tokio::test]
async fn unreachable_endpoint_reports_unreachable() {
    let http = reqwest::Client::new();
    let e = endpoint("http://127.0.0.1:1", Protocol::OpenAiChatCompletions); // port 1: connection refused
    let status = health_check(&e, None, &http).await;
    assert_eq!(status, HealthStatus::Unreachable);
}
```

- [ ] **Step 2: Implement `crates/providers/src/lib.rs`**

```rust
//! Provider HTTP client: health checks and /v1/models discovery.

use chm_core::domain::credentials::CredentialRef;
use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};
use chm_secrets::SecretStore;
use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("authentication failed")]
    Auth,
    #[error("rate limited")]
    RateLimit,
    #[error("malformed response")]
    Malformed,
    #[error("endpoint unreachable")]
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    AuthFailed,
    Unreachable,
    DiscoveryUnsupported,
    RateLimited,
    MalformedResponse,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ProviderModel {
    pub id: String,
    pub raw: serde_json::Value,
}

pub fn resolve_credential(ref_: &CredentialRef, store: &dyn SecretStore) -> Option<String> {
    store.get(&ref_.reference).ok().flatten()
}

fn discovery_url(endpoint: &ProviderEndpoint) -> String {
    let path = endpoint.discovery_path.as_deref().unwrap_or("/v1/models");
    format!("{}{}", endpoint.base_url.trim_end_matches('/'), path)
}

fn request_builder(
    http: &reqwest::Client,
    endpoint: &ProviderEndpoint,
    credential: Option<&str>,
    url: &str,
) -> reqwest::RequestBuilder {
    let mut req = http.get(url);
    match endpoint.auth_type {
        AuthType::BearerToken => {
            if let Some(c) = credential {
                req = req.bearer_auth(c);
            }
        }
        AuthType::ApiKeyHeader => {
            if let Some(c) = credential {
                req = req.header("x-api-key", c);
            }
        }
        AuthType::CustomHeader => {
            if let Some(c) = credential {
                req = req.header("authorization", c);
            }
        }
        AuthType::None => {}
    }
    for (k, v) in &endpoint.headers {
        if let Some(s) = v.as_str() {
            req = req.header(k, s);
        }
    }
    req
}

pub async fn health_check(
    endpoint: &ProviderEndpoint,
    credential: Option<&str>,
    http: &reqwest::Client,
) -> HealthStatus {
    if endpoint.discovery_path.is_none() && matches!(endpoint.protocol, Protocol::Custom) {
        return HealthStatus::DiscoveryUnsupported;
    }
    let url = discovery_url(endpoint);
    let resp = match request_builder(http, endpoint, credential, &url).send().await {
        Ok(r) => r,
        Err(_) => return HealthStatus::Unreachable,
    };
    match resp.status() {
        StatusCode::OK => HealthStatus::Healthy,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => HealthStatus::AuthFailed,
        StatusCode::TOO_MANY_REQUESTS => HealthStatus::RateLimited,
        s if s.is_client_error() || s.is_server_error() => HealthStatus::Unknown,
        _ => HealthStatus::Unknown,
    }
}

pub async fn discover_models(
    endpoint: &ProviderEndpoint,
    credential: Option<&str>,
    http: &reqwest::Client,
) -> Result<Vec<ProviderModel>, ProviderError> {
    let url = discovery_url(endpoint);
    let resp = request_builder(http, endpoint, credential, &url).send().await.map_err(|_| ProviderError::Unreachable)?;
    match resp.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(ProviderError::Auth),
        StatusCode::TOO_MANY_REQUESTS => return Err(ProviderError::RateLimit),
        _ => return Err(ProviderError::Unreachable),
    }
    let body: serde_json::Value = resp.json().await.map_err(|_| ProviderError::Malformed)?;
    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or(ProviderError::Malformed)?;
    let mut out = Vec::new();
    for item in data {
        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
            out.push(ProviderModel { id: id.to_string(), raw: item.clone() });
        }
    }
    Ok(out)
}
```

- [ ] **Step 3: Run tests**

In `crates/providers/Cargo.toml` add `[dependencies] chm-core, chm-secrets, reqwest, thiserror, serde_json` and `[dev-dependencies] tokio, wiremock, uuid, chrono, serde_json, chm-core` (dev-dep duplicate ok). Then:

```bash
cargo test -p chm-providers
```

Expected: all four tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/providers/
git commit -m "feat(phase1): provider health and model discovery client"
```

---

### Task 1.10: Phase Exit Verification

**Files:**
- Modify: `.github/workflows/ci.yml` (unchanged — verify only)

**Interfaces:**
- Consumes: Tasks 1.1–1.9.

- [ ] **Step 1: Full workspace gate**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: all three commands succeed with zero warnings.

- [ ] **Step 2: Verify crate dependency sanity**

```bash
cargo tree --workspace --depth 1 | head -40
```

Expected: no crate depends on another crate's private module; `core` has no dependencies beyond serde/chrono/uuid (pure domain).

- [ ] **Step 3: Commit any drift fixes**

```bash
git add -A
git commit -m "chore(phase1): phase exit cleanup"  # only if changes exist
```

Phase complete when all steps green.