# Phase 11 — Profiles + Launcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Launch profiles and the harness launcher: reusable harness/provider/model combos with role mappings and environment injection, launched with a constructed process environment — never touching shell startup files (project plan §29).

**Architecture:** Backend commands in `apps/desktop/src-tauri/src/commands/profiles.rs` (CRUD) and `launcher.rs` (resolve profile → resolve secrets → construct env → spawn via tokio process, or emit a copyable command). `launch_profiles` rows already exist (Phase 1 Task 1.5). The launcher works with the app running OR headless: `launch` builds `Command` with `envs()` and spawns detached; `copy_command` returns the shell command string with env exports for terminals.

**Tech Stack:** Rust (tokio `process::Command`), `shell-words` for safe command construction. Frontend: Profiles screen + Launch button.

## Global Constraints

- NEVER edit `.zshrc`/`.bashrc`/PowerShell profiles (project plan §29). Environment is injected per-process only.
- Secret resolution: profile env values that are `$LP_<NAME>` or `$<NAME>`-style references resolve through `SecretStore.get`; plain values pass through. `$LP_*` placeholders in env values are resolved at launch time, never persisted in plaintext (env_json stores the reference form).
- Role mappings (opus/sonnet/haiku → remote model ids) are resolved per harness: the profile's `role_mappings` override the harness's own defaults by setting the harness's role env vars (e.g. `ANTHROPIC_DEFAULT_SONNET_MODEL`) — adapter-neutral: the map is `{role → model}`, each adapter's `launch_env(profile)` translates.
- Launch is detached: the child keeps running after the app closes (macOS: spawn via `Command` with `process_group(0)` where available; the CLI `harnessctl run` is the canonical detached path).
- Phase exit: user creates a Z.AI Claude profile, launches Claude Code from the app with `ANTHROPIC_BASE_URL` + role mappings active (verified by `claude` echoing its env), and copies an equivalent command for the terminal.

---

### Task 11.1: Profile CRUD Commands + Screen

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/profiles.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Create: `apps/desktop/src/hooks/useProfiles.ts`
- Create: `apps/desktop/src/screens/ProfilesScreen.tsx` (replaces placeholder)

**Interfaces:**
- Consumes: `create_profile`, `list_profiles` (Phase 1 Task 1.5), `list_routes`, `list_endpoints`, `list_providers`.
- Produces:
  - `#[tauri::command] pub async fn list_profiles_cmd(state) -> Result<Vec<ProfileView>, String>`
  - `pub struct ProfileView { pub id: String, pub name: String, pub harness_type: String, pub model_route_id: Option<String>, pub provider_endpoint_id: Option<String>, pub provider_name: Option<String>, pub model_display: Option<String>, pub env: serde_json::Value, pub role_mappings: Vec<RoleMappingView> }` (camelCase)
  - `#[tauri::command] pub async fn create_profile_cmd(state, input: ProfileInput) -> Result<String, String>`
  - `pub struct ProfileInput { pub name: String, pub harness_type: String, pub model_route_id: Option<String>, pub provider_endpoint_id: Option<String>, pub env: serde_json::Map<String, serde_json::Value>, pub role_mappings: Vec<RoleMappingInput> }` where `RoleMappingInput { pub role: String, pub model: String }`
  - `#[tauri::command] pub async fn delete_profile_cmd(state, id: String) -> Result<(), String>`

- [ ] **Step 1: Write the failing test** — profile CRUD through the repo layer + view resolution (env with `$LP_` reference stored as-is; view shows provider name from endpoint).

- [ ] **Step 2: Implement commands** (thin wrappers + view join with providers/endpoints/routes).

- [ ] **Step 3: Frontend**

`useProfiles.ts`: queries + mutations. `ProfilesScreen.tsx`: card list (name, harness badge, model display, provider, role mapping chips `opus → glm-5`, Launch button (Task 11.3), Copy Command (Task 11.3), Edit/Delete). "New Profile" modal: harness select, model route select (from `useRoutes`), role mappings dynamic list (role select from harness's known roles + model input), env key/value dynamic list.

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase11): launch profile CRUD and screen"
```

---

### Task 11.2: Environment Resolution

**Files:**
- Create: `apps/desktop/src-tauri/src/launcher.rs` (pure env construction)
- Create: `apps/desktop/src-tauri/tests/launcher_env.rs`

**Interfaces:**
- Produces:
  - `pub fn resolve_profile_env(env: &serde_json::Map<String, serde_json::Value>, secrets: &dyn SecretStore, inherited: &HashMap<String, String>) -> HashMap<String, String>` — for each key: value is `$LP_<NAME>` or `${LP_<NAME>}` → `secrets.get(NAME)` (fallback: inherited[NAME]); `$<NAME>` (non-LP) → inherited lookup; plain → as-is.
  - `pub fn role_env_for(harness_type: &str, mappings: &[RoleMapping]) -> Vec<(String, String)>` — harness-specific env var names per role: Claude Code → `ANTHROPIC_DEFAULT_OPUS/SONNET/HAIKU_MODEL`; Codex → `CODEX_MODEL` when a single model mapping exists (role "default"); Pi → per Phase 0 doc; OpenCode → `OPENCODE_MODEL` (verify in doc).
  - `pub fn full_launch_env(profile_env: HashMap<String, String>, role_env: Vec<(String, String)>, inherited: &HashMap<String, String>) -> HashMap<String, String>` — merged, profile wins, missing keys from inherited.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn lp_placeholders_resolve_through_secret_store() {
    let env = serde_json::json!({
        "ANTHROPIC_AUTH_TOKEN": "$LP_ZAI_TOKEN",
        "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic",
        "PLAIN": "value",
    }).as_object().unwrap().clone();
    let secrets = FakeSecrets::with("ZAI_TOKEN", "sekret");
    let inherited = std::collections::HashMap::new();
    let out = resolve_profile_env(&env, &secrets, &inherited);
    assert_eq!(out.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str), Some("sekret"));
    assert_eq!(out.get("PLAIN").map(String::as_str), Some("value"));
}

#[test]
fn claude_role_mapping_env_vars() {
    let mappings = vec![
        RoleMapping { role: "opus".into(), model: "glm-5".into() },
        RoleMapping { role: "sonnet".into(), model: "glm-5-air".into() },
    ];
    let out = role_env_for("claude-code", &mappings);
    assert!(out.contains(&("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), "glm-5".to_string())));
    assert!(out.contains(&("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), "glm-5-air".to_string())));
}
```

- [ ] **Step 2: Implement `launcher.rs`** per the interfaces above (pure functions; `FakeSecrets` is a test helper implementing `SecretStore`).

- [ ] **Step 3: Run tests + commit**

```bash
cd apps/desktop/src-tauri && cargo test
git add apps/desktop
git commit -m "feat(phase11): launch environment resolution"
```

---

### Task 11.3: Launcher (spawn + copy command)

**Files:**
- Modify: `apps/desktop/src-tauri/src/launcher.rs` (`launch`, `copy_command`)
- Modify: `apps/desktop/src-tauri/src/commands/launcher.rs`
- Create: `apps/desktop/src-tauri/tests/launcher_tests.rs`

**Interfaces:**
- Produces:
  - `pub async fn launch(profile: &ProfileView, install: &HarnessInstallation, secrets: &dyn SecretStore, inherited: &HashMap<String, String>) -> Result<LaunchResult, String>` where `LaunchResult { pub pid: Option<u32>, pub executable: String }` — finds executable from install, resolves env, spawns detached with env, returns pid when captured.
  - `pub fn copy_command(profile: &ProfileView, install: &HarnessInstallation, env: &HashMap<String, String>) -> String` — shell snippet: `export KEY='value'; exec <exe>` (POSIX) or `$env:KEY='value'; <exe>` (Windows, compile-gated).
  - `#[tauri::command] pub async fn launch_profile_cmd(state, profile_id: String) -> Result<LaunchResult, String>`
  - `#[tauri::command] pub async fn copy_profile_command_cmd(state, profile_id: String) -> Result<String, String>`

- [ ] **Step 1: Write the failing test** — launch against a fake executable script that writes its env to a file:

```rust
#[tokio::test]
async fn launch_injects_resolved_env() {
    let tmp = tempfile::TempDir::new().unwrap();
    let script = tmp.path().join("fake-claude");
    std::fs::write(&script, "#!/bin/sh\nprintf '%s' \"$ANTHROPIC_BASE_URL\" > \"$CHM_ENV_OUT\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let env_out = tmp.path().join("env.txt");
    let profile = profile_view_with_env(serde_json::json!({"ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic"}));
    let install = install_with_exe(script.display().to_string());
    let mut inherited = std::collections::HashMap::new();
    inherited.insert("CHM_ENV_OUT".to_string(), env_out.display().to_string());
    launch(&profile, &install, &FakeSecrets::empty(), &inherited).await.unwrap();
    // wait briefly for the child to write
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(std::fs::read_to_string(&env_out).unwrap(), "https://api.z.ai/api/anthropic");
}
```

- [ ] **Step 2: Implement** — `launch` uses `tokio::process::Command` with `.envs(resolved)`, `.kill_on_drop(false)` (detached), `.stdout/Stderr(null)`; spawn failure → error with actionable message. `copy_command` uses `shell_words::quote` (add `shell-words = "1"`).

- [ ] **Step 3: Wire commands + UI** — Launch button per profile card (disabled when no installation for that harness); Copy Command button → clipboard via `@tauri-apps/plugin-clipboard-manager` (add plugin) + toast.

- [ ] **Step 4: Manual verify + commit**

Launch a real Claude Code from a Z.AI profile; in the harness run `env | grep ANTHROPIC` to confirm overrides; kill it. 

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase11): harness launcher"
```

---

### Task 11.4: CLI Skeleton (harnessctl) + Phase Exit

**Files:**
- Create: `cli/Cargo.toml`
- Create: `cli/src/main.rs`
- Modify: `Cargo.toml` (workspace member)

**Interfaces:**
- Produces: `harnessctl` binary (V1 scope: `list`, `scan`, `status`, `run <harness> --profile <name>` sharing the same DB + launcher core).

- [ ] **Step 1: Write the CLI**

```rust
// cli/src/main.rs
//! harnessctl — companion CLI sharing the CHM core library.

use clap::Parser;
use chm_database::connect;
use chm_secrets::KeychainStore;
use std::collections::HashMap;

#[derive(Parser)]
#[command(name = "harnessctl", version, about = "Coding Harness Manager CLI")]
enum Cli {
    /// List detected harnesses
    List,
    /// Rescan and persist harness inventory
    Scan,
    /// Show sync status per harness
    Status,
    /// Launch a harness with a profile
    Run {
        harness: String,
        #[arg(long)]
        profile: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let db = format!("{}/chm.sqlite", std::env::var("HOME").unwrap_or_default());
    let pool = connect(&db).await.expect("db");
    #[cfg(target_os = "macos")]
    let secrets: Box<dyn chm_secrets::SecretStore> = Box::new(KeychainStore::new("coding-harness-manager"));
    #[cfg(not(target_os = "macos"))]
    let secrets: Box<dyn chm_secrets::SecretStore> = Box::new(chm_secrets::EnvStore);
    match cli {
        Cli::List => {
            for i in chm_database::repos::harness::list_installations(&pool).await.unwrap() {
                println!("{}\t{:?}\t{}", i.harness_type.as_str(), i.status, i.version.unwrap_or_default());
            }
        }
        Cli::Scan => {
            let inventory = chm_harness_sdk::detect::scan::scan(platform(), None, None);
            for i in &inventory.installations {
                chm_database::repos::harness::upsert_installation(&pool, i).await.unwrap();
            }
            println!("scanned {} harnesses", inventory.installations.len());
        }
        Cli::Status => {
            for i in chm_database::repos::harness::list_installations(&pool).await.unwrap() {
                println!("{}: {}", i.harness_type.as_str(), i.status_v());
            }
        }
        Cli::Run { harness, profile } => {
            // resolve profile + installation + launch (reuse launcher core)
            let profiles = chm_database::repos::profiles::list_profiles(&pool).await.unwrap();
            let p = profiles.into_iter().find(|p| p.name == profile && p.harness_type.as_str() == harness)
                .expect("profile not found");
            let installs = chm_database::repos::harness::list_installations(&pool).await.unwrap();
            let install = installs.into_iter().find(|i| i.harness_type.as_str() == harness)
                .expect("harness not installed");
            // env resolution: profile env values referencing $LP_* go through secrets
            let env = resolve_profile_env(&p.env, secrets.as_ref(), &std::env::vars().collect::<HashMap<_, _>>());
            let role_env = role_env_for(harness.as_str(), &p.role_mappings);
            let mut all = env;
            for (k, v) in role_env { all.insert(k, v); }
            let child = tokio::process::Command::new(install.executable_path.clone().unwrap())
                .envs(all)
                .kill_on_drop(false)
                .spawn()
                .expect("spawn");
            println!("launched pid {}", child.id().unwrap_or(0));
        }
    }
}

fn platform() -> chm_harness_sdk::definition::Platform {
    #[cfg(target_os = "macos")]
    { chm_harness_sdk::definition::Platform::MacOs }
    #[cfg(target_os = "windows")]
    { chm_harness_sdk::definition::Platform::Windows }
    #[cfg(all(unix, not(target_os = "macos")))]
    { chm_harness_sdk::definition::Platform::Linux }
}
```

Add `cli` to workspace members; `clap = { version = "4", features = ["derive"] }` dependency. The `launcher` module must be extracted from the Tauri crate into a shared location — move `launcher.rs` logic into a new crate `crates/launcher` (depends on `chm-core`, `chm-secrets`, `chm-database`) so both the desktop app and `harnessctl` use the same code (project plan §30: same Rust core library). Move `resolve_profile_env`, `role_env_for`, `full_launch_env`, `launch`, `copy_command` there; the desktop crate re-exports from `chm-launcher`.

- [ ] **Step 2: Verify + commit**

```bash
cargo test -p chm-launcher -p cli
cargo run -p cli -- scan && cargo run -p cli -- list
git add cli crates/launcher Cargo.toml
git commit -m "feat(phase11): harnessctl CLI on shared core"
```

- [ ] **Step 3: Full gate**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
```

- [ ] **Step 4: Commit any cleanup**

```bash
git add -A
git commit -m "chore(phase11): phase exit cleanup"  # only if changes exist
```

Phase complete when all steps green.

---

### Task 11.5: Configuration Sets (apply bundles)

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/sets.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Modify: `crates/database/src/repos/profiles.rs` (`list_set_items`)
- Create: `apps/desktop/src/hooks/useSets.ts`
- Create: `apps/desktop/src/screens/SetsScreen.tsx` (replaces placeholder)
- Modify: `apps/desktop/src/lib/api.ts`

**Interfaces:**
- Consumes: `create_set`, `add_set_item`, `list_sets` (Phase 1 Task 1.5), `build_native_plan`/`execute_sync` (Phase 8.2).
- Produces:
  - `pub async fn list_set_items(pool, set_id) -> Result<Vec<ConfigurationSetItem>, DbError>` (new repo method)
  - `#[tauri::command] pub async fn list_sets_cmd(state) -> Result<Vec<SetView>, String>` where `SetView { pub id: String, pub name: String, pub description: Option<String>, pub items: Vec<SetItemView> }` and `SetItemView { pub item_type: String, pub item_id: String }`
  - `#[tauri::command] pub async fn create_set_cmd(state, name: String, description: Option<String>) -> Result<String, String>`
  - `#[tauri::command] pub async fn add_set_item_cmd(state, set_id: String, item_type: String, item_id: String) -> Result<(), String>`
  - `#[tauri::command] pub async fn remove_set_item_cmd(state, set_id: String, item_type: String, item_id: String) -> Result<(), String>`
  - `#[tauri::command] pub async fn apply_set_preview_cmd(state, set_id: String, installation_id: String) -> Result<PreviewReport, String>` — like `sync_preview` but desired state is FILTERED to the set's items (routes/MCP/skills by ids).
  - `#[tauri::command] pub async fn apply_set_cmd(state, set_id: String, installation_id: String, mode: String) -> Result<ApplyReport, String>` — `execute_sync` with set-filtered desired state.
  - `pub async fn set_filtered_desired(pool, set_id) -> Result<DesiredState, String>` — testable core.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn set_filter_limits_desired_state() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = endpoint(&p.id);
    create_endpoint(&pool, &e).await.unwrap();
    let ra = route(&e.id, "glm-5", Some(1_048_576));
    let rb = route(&e.id, "glm-5-air", None);
    create_route(&pool, &ra).await.unwrap();
    create_route(&pool, &rb).await.unwrap();
    let set = create_set(&pool, "Work", None).await.unwrap();
    add_set_item(&pool, set.id, SetItemType::ModelRoute, ra.id).await.unwrap();

    let desired = set_filtered_desired(&pool, &set.id.to_string()).await.unwrap();
    assert_eq!(desired.routes.len(), 1);
    assert_eq!(desired.routes[0].remote_model_id, "glm-5");
}
```

- [ ] **Step 2: Implement commands + screen**

`set_filtered_desired`: `list_set_items` → filter `list_routes`/`list_mcp_servers`/`list_skills` by item ids. `apply_set_preview_cmd`/`apply_set_cmd` reuse the Phase 8 flow with the filtered desired (refactor `build_native_plan` to accept an optional prebuilt `DesiredState`).

`SetsScreen.tsx`: set cards (name, description, item counts, expand to item list with remove buttons), "New Set" modal (name + description), "Add items" multi-pick from routes/MCP/skills, "Apply to harness…" → harness select → preview dialog (reuse `SyncDialog`) → apply.

- [ ] **Step 3: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop crates/database
git commit -m "feat(phase11): configuration sets with apply"
```