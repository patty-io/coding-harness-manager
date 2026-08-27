# Phase 4 — First-Run Import + App Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Tauri 2 desktop application with the full navigation shell, a working scan/inventory screen, and the first-run import wizard that turns existing harness state into canonical DB state — without ever modifying a native file.

**Architecture:** `apps/desktop` contains the React frontend and `apps/desktop/src-tauri` the Rust backend. The backend exposes typed Tauri commands over an `AppState` (SQLite pool + SecretStore + HTTP client). The frontend talks to it exclusively through a typed `api.ts` wrapper around `invoke`, with TanStack Query for server state. The import wizard is a stepper that calls backend commands which write ONLY to SQLite with provenance recorded; native files are read-only here.

**Tech Stack:** Tauri 2 (Rust backend, existing crates), Vite + React + TypeScript strict, TanStack Query v5, React Router v7, Tailwind CSS v4, Zod. Commands: `npm run tauri dev`, `npm run build`, `npm run lint`.

## Global Constraints

- No native harness file is ever written during Phase 4 — the wizard imports INTO the database only.
- Import never overwrites existing canonical state: name conflicts are reported as duplicates, never auto-merged silently.
- Every imported object records provenance (project plan §39): `{"source": "<harness>", "imported_at": "<ts>"}`.
- All Tauri commands return `Result<T, String>` (stringified error) — the React layer maps them to user-facing messages.
- App identifier: `com.codingharnessmanager.app`. Window: 1200×800, min 900×600.
- Phase exit: `npm run tauri dev` runs, wizard completes end-to-end on a machine with real harness configs, all tests green (Rust + frontend build in CI).

---

### Task 4.1: Tauri App Bootstrap

**Files:**
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/vite.config.ts`
- Create: `apps/desktop/tsconfig.json`
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/src/App.tsx`
- Create: `apps/desktop/src/index.css`
- Create: `apps/desktop/src-tauri/Cargo.toml`
- Create: `apps/desktop/src-tauri/build.rs`
- Create: `apps/desktop/src-tauri/tauri.conf.json`
- Create: `apps/desktop/src-tauri/src/main.rs`
- Create: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src-tauri/src/commands/mod.rs`
- Create: `apps/desktop/src-tauri/capabilities/default.json`
- Modify: `.github/workflows/ci.yml` (frontend job)

**Interfaces:**
- Produces: the app shell + `AppState` that every UI phase (4–13) extends with new commands.

- [ ] **Step 1: Scaffold the frontend**

`apps/desktop/package.json`:

```json
{
  "name": "coding-harness-manager-desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "lint": "tsc -b --noEmit",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tanstack/react-query": "^5.0.0",
    "@tanstack/react-table": "^8.0.0",
    "@tauri-apps/api": "^2.0.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-hook-form": "^7.0.0",
    "react-router-dom": "^7.0.0",
    "zod": "^3.23.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@types/react": "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.0",
    "tailwindcss": "^4.0.0",
    "@tailwindcss/vite": "^4.0.0",
    "typescript": "^5.6.0",
    "vite": "^6.0.0"
  }
}
```

`vite.config.ts`:

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "esnext", outDir: "dist" },
});
```

`tsconfig.json` (strict, `"jsx": "react-jsx"`, `"moduleResolution": "bundler"`, `"noEmit": true`, includes `src`).

`index.html` → standard Vite entry with `<div id="root"></div>` and `<script type="module" src="/src/main.tsx">`.

`src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import "./index.css";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: 1, staleTime: 5_000 } },
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </React.StrictMode>,
);
```

`src/index.css`: `@import "tailwindcss";` plus a body reset.

- [ ] **Step 2: Scaffold the Tauri backend**

`src-tauri/Cargo.toml`:

```toml
[package]
name = "coding-harness-manager"
version = "0.1.0"
edition = "2024"

[lib]
name = "coding_harness_manager_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
adapters = { path = "../../../adapters" }
chm-core = { path = "../../../crates/core" }
chm-database = { path = "../../../crates/database" }
chm-harness-sdk = { path = "../../../crates/harness-sdk" }
chm-secrets = { path = "../../../crates/secrets" }
chrono = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sqlx = { workspace = true }
tauri = { version = "2", features = [] }
tokio = { workspace = true }
uuid = { workspace = true }
```

Note: `[workspace]` must NOT appear here — this crate lives OUTSIDE the cargo workspace root (Tauri requirement), so reference workspace deps via `workspace = true` only if the root workspace resolves; otherwise pin versions explicitly. Simplest: pin versions explicitly (`serde = "1"`, `uuid = { version = "1", features = ["v4", "serde"] }`, etc.).

`src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Coding Harness Manager",
  "version": "0.1.0",
  "identifier": "com.codingharnessmanager.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Coding Harness Manager",
        "width": 1200,
        "height": 800,
        "minWidth": 900,
        "minHeight": 600
      }
    ],
    "security": { "csp": null }
  },
  "bundle": { "active": true, "targets": "all", "icon": [] }
}
```

`src-tauri/build.rs`: `fn main() { tauri_build::build() }`

`src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Main window capabilities",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

`src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    coding_harness_manager_lib::run();
}
```

`src-tauri/src/lib.rs`:

```rust
//! Tauri backend: commands + app state.

pub mod commands;

use chm_database::connect;
use chm_secrets::{KeychainStore, SecretStore};
use sqlx::{Pool, Sqlite};
use tauri::Manager;

pub struct AppState {
    pub pool: Pool<Sqlite>,
    pub secrets: Box<dyn SecretStore>,
    pub http: reqwest::Client,
}

fn db_path() -> String {
    let dir = std::env::var_os("CHM_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("~/.coding-harness-manager").join("chm.sqlite"));
    dir.display().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let pool = tauri::async_runtime::block_on(connect(&db_path()))
                .expect("database connect");
            #[cfg(target_os = "macos")]
            let secrets: Box<dyn SecretStore> = Box::new(KeychainStore::new("coding-harness-manager"));
            #[cfg(not(target_os = "macos"))]
            let secrets: Box<dyn SecretStore> = Box::new(chm_secrets::EnvStore);
            app.manage(AppState {
                pool,
                secrets,
                http: reqwest::Client::new(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan::scan_harnesses,
            commands::scan::list_installations,
            commands::import::read_harness_state,
            commands::import::import_harness_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn pool(state: &tauri::State<'_, AppState>) -> &Pool<Sqlite> {
    &state.pool
}
```

`src-tauri/src/commands/mod.rs`:

```rust
pub mod import;
pub mod scan;
```

`src/App.tsx` (placeholder shell for now — full nav comes in Task 4.2):

```tsx
export default function App() {
  return (
    <div className="p-8">
      <h1 className="text-2xl font-bold">Coding Harness Manager</h1>
      <p className="mt-2 text-gray-600">Bootstrap OK — navigate to /scan to see the inventory.</p>
    </div>
  );
}
```

- [ ] **Step 3: Install deps and verify dev toolchain**

```bash
cd apps/desktop && npm install && npm run build
cd src-tauri && cargo check
```

Expected: frontend builds clean; Tauri crate compiles. (`cargo check` needs the workspace crates built once — run `cargo build --workspace` from repo root first if adapters/chm-* paths are fresh.)

- [ ] **Step 4: Add frontend CI job**

Append to `.github/workflows/ci.yml`:

```yaml
  frontend:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: apps/desktop
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: apps/desktop/package-lock.json
      - run: npm ci
      - run: npm run lint
      - run: npm run build
```

- [ ] **Step 5: Commit**

```bash
git add apps/desktop .github/workflows/ci.yml
git commit -m "feat(phase4): bootstrap tauri desktop app"
```

---

### Task 4.2: Scan Commands + Inventory Screen

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/scan.rs`
- Create: `apps/desktop/src/lib/api.ts`
- Create: `apps/desktop/src/hooks/useHarnesses.ts`
- Create: `apps/desktop/src/screens/InventoryScreen.tsx`
- Create: `apps/desktop/src/components/Sidebar.tsx`
- Modify: `apps/desktop/src/App.tsx` (router + sidebar layout)

**Interfaces:**
- Consumes: `scan()` (Phase 2 Task 2.4), `upsert_installation`/`list_installations` (Phase 1 Task 1.6), adapters registry (Phase 3 Task 3.7).
- Produces: the command surface later phases extend:
  - `#[tauri::command] pub async fn scan_harnesses(state: State<'_, AppState>) -> Result<Vec<HarnessInstallation>, String>`
  - `#[tauri::command] pub async fn list_installations(state: State<'_, AppState>) -> Result<Vec<HarnessInstallation>, String>`
  - JS: `export async function scanHarnesses(): Promise<HarnessInstallation[]>` and `listInstallations()` in `api.ts`.

- [ ] **Step 1: Write the failing backend test**

`apps/desktop/src-tauri/tests/scan_import.rs` (unit test of the command bodies without Tauri runtime — the scan/import functions take `&Pool` directly so they are testable):

```rust
use chm_database::connect_test;
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;

#[tokio::test]
async fn scan_writes_inventory_to_db() {
    let pool = connect_test().await.unwrap();
    // fake machine: opencode installed under temp dir
    let dir = tempfile::TempDir::new().unwrap();
    let bindir = dir.path().join("bin");
    let homedir = dir.path().join("home");
    std::fs::create_dir_all(&bindir).unwrap();
    std::fs::create_dir_all(homedir.join(".config/opencode")).unwrap();
    std::fs::write(bindir.join("opencode"), "#!/bin/sh\nprintf 'opencode 0.30.0\n'\n").unwrap();
    std::fs::write(homedir.join(".config/opencode/opencode.json"), "{}").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(bindir.join("opencode"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let inventory = scan(Platform::MacOs, Some(&homedir), Some(&bindir.display().to_string()));
    assert_eq!(inventory.installations.len(), 1);
    // persist like the command does
    for inst in &inventory.installations {
        chm_database::repos::harness::upsert_installation(&pool, inst).await.unwrap();
    }
    let all = chm_database::repos::harness::list_installations(&pool).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].harness_type.as_str(), "opencode");
}
```

- [ ] **Step 2: Implement `commands/scan.rs`**

```rust
//! Scan + inventory commands.

use chm_core::domain::harness::HarnessInstallation;
use chm_database::repos::harness::{list_installations, upsert_installation};
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;
use sqlx::Pool;
use sqlx::Sqlite;
use tauri::State;

use crate::AppState;

pub async fn scan_and_persist(pool: &Pool<Sqlite>) -> Result<Vec<HarnessInstallation>, String> {
    #[cfg(target_os = "macos")]
    let platform = Platform::MacOs;
    #[cfg(target_os = "windows")]
    let platform = Platform::Windows;
    #[cfg(all(unix, not(target_os = "macos")))]
    let platform = Platform::Linux;

    let inventory = scan(platform, None, None);
    for inst in &inventory.installations {
        upsert_installation(pool, inst).await.map_err(|e| e.to_string())?;
    }
    Ok(inventory.installations)
}

#[tauri::command]
pub async fn scan_harnesses(state: State<'_, AppState>) -> Result<Vec<HarnessInstallation>, String> {
    scan_and_persist(&state.pool).await
}

#[tauri::command]
pub async fn list_installations(state: State<'_, AppState>) -> Result<Vec<HarnessInstallation>, String> {
    list_installations(&state.pool).await.map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Write the typed API client `src/lib/api.ts`**

```ts
// Typed wrapper around Tauri invoke. Every backend command gets one function.

import { invoke } from "@tauri-apps/api/core";

export type HarnessType = "claude-code" | "codex" | "opencode" | "pi" | "reasonix";

export interface HarnessInstallation {
  id: string;
  harness_type: HarnessType;
  executable_path: string | null;
  version: string | null;
  config_path: string | null;
  detected_at: string;
  last_scanned_at: string | null;
  status: "detected" | "installed" | "config-missing" | "error";
}

export async function scanHarnesses(): Promise<HarnessInstallation[]> {
  return invoke<HarnessInstallation[]>("scan_harnesses");
}

export async function listInstallations(): Promise<HarnessInstallation[]> {
  return invoke<HarnessInstallation[]>("list_installations");
}
```

- [ ] **Step 4: Write the hook `src/hooks/useHarnesses.ts`**

```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listInstallations, scanHarnesses, type HarnessInstallation } from "../lib/api";

export function useInstallations() {
  return useQuery({ queryKey: ["installations"], queryFn: listInstallations });
}

export function useScanHarnesses() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: scanHarnesses,
    onSuccess: (data) => qc.setQueryData(["installations"], data),
  });
}
```

- [ ] **Step 5: Write `Sidebar.tsx` and the router**

`src/components/Sidebar.tsx` — nav list per project plan §44 (Dashboard, Providers, Models, Harnesses, MCP Servers, Skills, Profiles, Sets, Changes, History, Doctor, Settings) using `NavLink` from react-router-dom; active link styled with Tailwind.

`src/App.tsx`:

```tsx
import { Route, Routes } from "react-router-dom";
import Sidebar from "./components/Sidebar";
import InventoryScreen from "./screens/InventoryScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { PlaceholderScreen } from "./screens/PlaceholderScreen";

const PLACEHOLDERS = [
  "providers", "models", "mcp", "skills", "profiles", "sets",
  "changes", "history", "doctor", "settings",
];

export default function App() {
  return (
    <div className="flex h-screen">
      <Sidebar />
      <main className="flex-1 overflow-auto bg-gray-50 p-6">
        <Routes>
          <Route path="/" element={<DashboardScreen />} />
          <Route path="/scan" element={<InventoryScreen />} />
          {PLACEHOLDERS.map((p) => (
            <Route key={p} path={`/${p}`} element={<PlaceholderScreen title={p} />} />
          ))}
        </Routes>
      </main>
    </div>
  );
}
```

- [ ] **Step 6: Write `InventoryScreen.tsx`**

```tsx
import { useInstallations, useScanHarnesses } from "../hooks/useHarnesses";

export default function InventoryScreen() {
  const { data: installations, isLoading } = useInstallations();
  const scan = useScanHarnesses();

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Harnesses</h1>
        <button
          onClick={() => scan.mutate()}
          disabled={scan.isPending}
          className="rounded bg-blue-600 px-4 py-2 text-white disabled:opacity-50"
        >
          {scan.isPending ? "Scanning..." : "Scan Harnesses"}
        </button>
      </div>
      {scan.isError && <p className="mt-2 text-red-600">Scan failed: {scan.error.message}</p>}
      {isLoading && <p className="mt-4">Loading…</p>}
      <table className="mt-4 w-full bg-white">
        <thead>
          <tr className="border-b text-left">
            <th className="p-2">Harness</th>
            <th className="p-2">Status</th>
            <th className="p-2">Version</th>
            <th className="p-2">Config</th>
          </tr>
        </thead>
        <tbody>
          {(installations ?? []).map((i) => (
            <tr key={i.id} className="border-b">
              <td className="p-2 font-medium">{i.harness_type}</td>
              <td className="p-2">{i.status}</td>
              <td className="p-2">{i.version ?? "—"}</td>
              <td className="p-2 font-mono text-xs">{i.config_path ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

`DashboardScreen` and `PlaceholderScreen` are trivial (heading + "coming in phase N" note).

- [ ] **Step 7: Verify**

```bash
cd apps/desktop && npm run lint && npm run build && npm run tauri dev
```

Expected: app launches, sidebar renders, `/scan` shows the machine's real harnesses after clicking "Scan Harnesses".

- [ ] **Step 8: Commit**

```bash
git add apps/desktop
git commit -m "feat(phase4): scan commands and inventory screen"
```

---

### Task 4.3: Read Harness State Command + Import Backend

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/import.rs`
- Create: `apps/desktop/src-tauri/tests/import_tests.rs`

**Interfaces:**
- Consumes: `ParsedState` (Phase 3), repos (Phase 1).
- Produces:
  - `#[tauri::command] pub async fn read_harness_state(state, installation_id: String) -> Result<ParsedStateView, String>` — JS-visible serializable view of `ParsedState`.
  - `#[tauri::command] pub async fn import_harness_state(state, installation_id: String, options: ImportOptions) -> Result<ImportReport, String>`
  - `pub struct ImportOptions { pub import_models: bool, pub import_mcp: bool, pub import_skills: bool }` (serde camelCase)
  - `pub struct ImportReport { pub providers_created: usize, pub models_imported: usize, pub mcp_imported: usize, pub skills_imported: usize, pub duplicates: Vec<String> }`
  - `pub async fn run_import(pool: &Pool<Sqlite>, installation_id: &str, options: &ImportOptions) -> Result<ImportReport, String>` — testable without Tauri.

- [ ] **Step 1: Write the failing test `tests/import_tests.rs`**

Uses a fixture-based fake install (no real harness needed):

```rust
use chm_core::domain::harness::{HarnessInstallation, InstallationStatus, HarnessType};
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{HarnessMcp, HarnessModel, ParsedState};
use chm_database::connect_test;
use chm_database::repos::harness::upsert_installation;
use chm_database::repos::providers::{list_providers, create_provider};
use chm_database::repos::mcp::list_mcp_servers;
use chm_database::repos::skills::list_skills;
use coding_harness_manager_lib::commands::import::{run_import, ImportOptions};
use chrono::Utc;
use uuid::Uuid;

fn fake_install(pool: &sqlx::Pool<sqlx::Sqlite>) -> HarnessInstallation {
    let inst = HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: Some("/fake/opencode".into()),
        version: Some("0.30.0".into()),
        config_path: Some("/fake/.config/opencode".into()),
        detected_at: Utc::now(),
        last_scanned_at: Some(Utc::now()),
        status: InstallationStatus::Installed,
    };
    // register the adapter read result for this installation id:
    // (see Step 2 — registry is a module-level HashMap<install_id, ParsedState>)
    register_fake_state(inst.id, ParsedState {
        models: vec![HarnessModel {
            native_id: "glm-5".into(),
            route: ModelRoute {
                id: Uuid::new_v4(),
                endpoint_id: Uuid::new_v4(),
                model_identity_id: None,
                remote_model_id: "glm-5".into(),
                display_name: "GLM-5".into(),
                context_window: Some(1_048_576),
                max_input: None, max_output: None,
                capabilities: serde_json::json!({}),
                overrides: serde_json::json!({"env_key": "ZAI_API_KEY", "base_url": "https://api.z.ai/api/anthropic", "protocol": "anthropic-messages"}),
                enabled: true,
                created_at: Utc::now(), updated_at: Utc::now(),
            },
        }],
        providers: vec![serde_json::json!({
            "native_provider_id": "zai",
            "base_url": "https://api.z.ai/api/anthropic",
            "env_key": "ZAI_API_KEY",
            "protocol": "anthropic-messages",
        })],
        mcp: vec![HarnessMcp {
            native_name: "github".into(),
            server: McpServer {
                id: Uuid::new_v4(),
                name: "github".into(),
                transport: McpTransport::Stdio,
                command: Some("npx".into()),
                args: vec!["-y".into(), "@modelcontextprotocol/server-github".into()],
                url: None,
                env: Default::default(),
                scope_type: ScopeType::Global,
                scope_path: None,
                provenance: serde_json::json!({}),
                enabled: true,
            },
        }],
        skills: vec![],
        profiles: vec![],
        warnings: vec![],
    });
    let _ = pool; // registration is global for the test process
    inst
}

#[tokio::test]
async fn import_creates_provider_model_and_mcp() {
    let pool = connect_test().await.unwrap();
    let inst = fake_install(&pool);
    upsert_installation(&pool, &inst).await.unwrap();

    let report = run_import(&pool, &inst.id.to_string(), &ImportOptions {
        import_models: true, import_mcp: true, import_skills: true,
    }).await.unwrap();

    assert_eq!(report.providers_created, 1);
    assert_eq!(report.models_imported, 1);
    assert_eq!(report.mcp_imported, 1);
    assert!(report.duplicates.is_empty());

    assert_eq!(list_providers(&pool).await.unwrap().len(), 1);
    assert_eq!(list_mcp_servers(&pool).await.unwrap().len(), 1);
    assert_eq!(list_skills(&pool).await.unwrap().len(), 0);
}

#[tokio::test]
async fn import_reports_duplicates_without_overwriting() {
    let pool = connect_test().await.unwrap();
    let inst = fake_install(&pool);
    upsert_installation(&pool, &inst).await.unwrap();

    // pre-existing provider with the same name
    create_provider(&pool, "zai", "Z.AI").await.unwrap();
    // pre-existing mcp with the same name
    let existing = McpServer {
        id: Uuid::new_v4(),
        name: "github".into(),
        transport: McpTransport::Stdio,
        command: Some("node".into()),
        args: vec![],
        url: None,
        env: Default::default(),
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({}),
        enabled: true,
    };
    chm_database::repos::mcp::create_mcp_server(&pool, &existing).await.unwrap();

    let report = run_import(&pool, &inst.id.to_string(), &ImportOptions {
        import_models: true, import_mcp: true, import_skills: true,
    }).await.unwrap();

    assert_eq!(report.providers_created, 0, "provider name conflict must not overwrite");
    assert_eq!(report.mcp_imported, 0, "mcp name conflict must not overwrite");
    assert_eq!(report.duplicates.len(), 2);
    let servers = list_mcp_servers(&pool).await.unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].command.as_deref(), Some("node"), "existing server untouched");
}
```

- [ ] **Step 2: Implement the import registry + `commands/import.rs`**

The `read_state` command needs the adapter's real read path; tests need deterministic injection. Add a test-only registry in the import module:

```rust
//! Import commands: read native state, write canonical state (never native files).

use adapters::all_adapters;
use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::mcp::{McpServer, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::skills::Skill;
use chm_database::repos::harness::list_installations;
use chm_database::repos::mcp::create_mcp_server;
use chm_database::repos::models::{create_route, upsert_catalog_model};
use chm_database::repos::providers::{create_credential_ref, create_endpoint, create_provider, list_endpoints, list_providers};
use chm_database::repos::skills::create_skill;
use chm_harness_sdk::adapter::types::ParsedState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
static FAKE_STATES: Mutex<Option<std::collections::HashMap<Uuid, ParsedState>>> = Mutex::new(None);

#[cfg(test)]
pub fn register_fake_state(id: Uuid, state: ParsedState) {
    let mut guard = FAKE_STATES.lock().unwrap();
    guard.get_or_insert_with(std::collections::HashMap::new).insert(id, state);
}

#[cfg(test)]
fn resolve_state(install_id: Uuid) -> Option<ParsedState> {
    FAKE_STATES.lock().unwrap().as_ref().and_then(|m| m.get(&install_id).cloned())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedStateView {
    pub models: Vec<serde_json::Value>,
    pub mcp: Vec<serde_json::Value>,
    pub skills: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub import_models: bool,
    pub import_mcp: bool,
    pub import_skills: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub providers_created: usize,
    pub models_imported: usize,
    pub mcp_imported: usize,
    pub skills_imported: usize,
    pub duplicates: Vec<String>,
}

fn adapter_for(harness_type: &str) -> Option<Box<dyn chm_harness_sdk::adapter::types::HarnessAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == harness_type)
}

fn read_parsed_state(pool: &Pool<Sqlite>, installation_id: &str) -> Result<(HarnessInstallation, ParsedState), String> {
    let inst = list_installations(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;
    #[cfg(test)]
    if let Some(fake) = resolve_state(inst.id) {
        return Ok((inst, fake));
    }
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter for harness")?;
    let state = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    Ok((inst, state))
}

#[tauri::command]
pub async fn read_harness_state(state: State<'_, AppState>, installation_id: String) -> Result<ParsedStateView, String> {
    let (_inst, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    Ok(ParsedStateView {
        models: parsed.models.iter().map(|m| serde_json::json!({
            "native_id": m.native_id,
            "remote_model_id": m.route.remote_model_id,
            "display_name": m.route.display_name,
            "context_window": m.route.context_window,
        })).collect(),
        mcp: parsed.mcp.iter().map(|m| serde_json::json!({
            "native_name": m.native_name,
            "transport": m.server.transport.as_str(),
            "command": m.server.command,
        })).collect(),
        skills: parsed.skills.iter().map(|s| serde_json::json!({ "name": s.name, "symlinked": s.symlinked })).collect(),
        warnings: parsed.warnings.clone(),
    })
}

pub async fn run_import(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    options: &ImportOptions,
) -> Result<ImportReport, String> {
    let (inst, parsed) = read_parsed_state(pool, installation_id).await?;
    let mut report = ImportReport::default();
    let provenance = serde_json::json!({ "source": inst.harness_type.as_str(), "imported_at": Utc::now().to_rfc3339() });

    for pv in &parsed.providers {
        let name = pv.get("native_provider_id").and_then(|v| v.as_str()).unwrap_or("imported");
        if list_providers(pool).await.map_err(|e| e.to_string())?.iter().any(|p| p.name == name) {
            report.duplicates.push(format!("provider:{name}"));
            continue;
        }
        let provider = create_provider(pool, name, name).map_err(|e| e.to_string())?;
        report.providers_created += 1;
        // endpoint from base_url + protocol (protocol guess: anthropic-messages | openai-* | custom)
        let base_url = pv.get("base_url").and_then(|v| v.as_str()).map(String::from);
        let protocol = pv.get("protocol").and_then(|v| v.as_str()).unwrap_or("custom");
        let env_key = pv.get("env_key").and_then(|v| v.as_str());
        let credential_ref: Option<CredentialRef> = match env_key {
            Some(key) => Some(create_credential_ref(pool, CredentialKind::Env, key).await.map_err(|e| e.to_string())?),
            None => None,
        };
        if let Some(base_url) = base_url {
            let endpoint = chm_core::domain::provider::ProviderEndpoint {
                id: Uuid::new_v4(),
                provider_id: provider.id,
                name: format!("{name}-imported"),
                base_url,
                protocol: match protocol {
                    "anthropic-messages" => chm_core::domain::provider::Protocol::AnthropicMessages,
                    "openai-chat" => chm_core::domain::provider::Protocol::OpenAiChatCompletions,
                    "openai-responses" => chm_core::domain::provider::Protocol::OpenAiResponses,
                    "openrouter-openai" => chm_core::domain::provider::Protocol::OpenRouterOpenAi,
                    _ => chm_core::domain::provider::Protocol::Custom,
                },
                discovery_path: Some("/v1/models".into()),
                auth_type: chm_core::domain::provider::AuthType::BearerToken,
                credential_ref,
                headers: Default::default(),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            create_endpoint(pool, &endpoint).await.map_err(|e| e.to_string())?;
        }
    }

    if options.import_models {
        for m in &parsed.models {
            let route = ModelRoute {
                id: Uuid::new_v4(),
                endpoint_id: imported_endpoint_id(pool, &inst).await?,
                model_identity_id: None,
                remote_model_id: m.route.remote_model_id.clone(),
                display_name: m.route.display_name.clone(),
                context_window: m.route.context_window,
                max_input: m.route.max_input,
                max_output: m.route.max_output,
                capabilities: m.route.capabilities.clone(),
                overrides: m.route.overrides.clone(),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            match create_route(pool, &route).await {
                Ok(_) => {
                    report.models_imported += 1;
                    // also record in the provider catalog (status New) for Phase 6 discovery flow
                    let now = Utc::now();
                    upsert_catalog_model(pool, &chm_core::domain::models::ProviderCatalogModel {
                        id: Uuid::new_v4(),
                        endpoint_id: route.endpoint_id,
                        remote_model_id: route.remote_model_id.clone(),
                        raw_metadata: serde_json::json!({"source": "harness-import"}),
                        canonical_model_id: None,
                        match_confidence: None,
                        first_seen_at: now,
                        last_seen_at: now,
                        missing_since: None,
                        status: chm_core::domain::models::CatalogStatus::New,
                    }).await.map_err(|e| e.to_string())?;
                }
                Err(_) => report.duplicates.push(format!("model:{}", m.route.remote_model_id)),
            }
        }
    }

    if options.import_mcp {
        for m in &parsed.mcp {
            let existing = chm_database::repos::mcp::list_mcp_servers(pool).await.map_err(|e| e.to_string())?;
            if existing.iter().any(|s| s.name == m.server.name) {
                report.duplicates.push(format!("mcp:{}", m.server.name));
                continue;
            }
            let server = McpServer {
                id: Uuid::new_v4(),
                name: m.server.name.clone(),
                transport: m.server.transport,
                command: m.server.command.clone(),
                args: m.server.args.clone(),
                url: m.server.url.clone(),
                env: m.server.env.clone(),
                scope_type: ScopeType::Global,
                scope_path: None,
                provenance: provenance.clone(),
                enabled: true,
            };
            create_mcp_server(pool, &server).await.map_err(|e| e.to_string())?;
            report.mcp_imported += 1;
        }
    }

    if options.import_skills {
        for s in &parsed.skills {
            if s.symlinked {
                // symlinked skills are already canonical — binding is created in Phase 10
                report.skills_imported += 1;
                continue;
            }
            let existing = chm_database::repos::skills::list_skills(pool).await.map_err(|e| e.to_string())?;
            if existing.iter().any(|sk| sk.canonical_path == s.path) {
                report.duplicates.push(format!("skill:{}", s.name));
                continue;
            }
            let skill = Skill {
                id: Uuid::new_v4(),
                name: s.name.clone(),
                canonical_path: s.path.clone(),
                source_type: chm_core::domain::skills::SkillSourceType::HarnessImport,
                source_url: None,
                content_hash: None,
                provenance: provenance.clone(),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            create_skill(pool, &skill).await.map_err(|e| e.to_string())?;
            report.skills_imported += 1;
        }
    }

    Ok(report)
}

/// Endpoint used for imported routes: first endpoint of the first imported
/// provider, else a placeholder "imported" endpoint (enabled=false, base_url="").
async fn imported_endpoint_id(pool: &Pool<Sqlite>, inst: &HarnessInstallation) -> Result<Uuid, String> {
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    for p in providers {
        let endpoints = list_endpoints(pool, p.id).await.map_err(|e| e.to_string())?;
        if let Some(e) = endpoints.first() {
            return Ok(e.id);
        }
    }
    let placeholder = chm_core::domain::provider::ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: Uuid::new_v4(),
        name: format!("{}-imported", inst.harness_type.as_str()),
        base_url: String::new(),
        protocol: chm_core::domain::provider::Protocol::Custom,
        discovery_path: None,
        auth_type: chm_core::domain::provider::AuthType::None,
        credential_ref: None,
        headers: Default::default(),
        enabled: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    // placeholder has no provider row — create one
    let provider = create_provider(pool, "imported", "Imported (needs setup)").map_err(|e| e.to_string())?;
    let endpoint = chm_core::domain::provider::ProviderEndpoint { provider_id: provider.id, ..placeholder };
    create_endpoint(pool, &endpoint).await.map_err(|e| e.to_string())?;
    Ok(endpoint.id)
}

#[tauri::command]
pub async fn import_harness_state(
    state: State<'_, AppState>,
    installation_id: String,
    options: ImportOptions,
) -> Result<ImportReport, String> {
    run_import(&state.pool, &installation_id, &options).await
}
```

- [ ] **Step 3: Run tests**

```bash
cd apps/desktop/src-tauri && cargo test
```

Expected: both import tests pass (provider/model/mcp created; duplicates reported without overwriting). Adjust `fake_install` registration wiring if the compiler complains about the `pool` param being unused — it is intentionally unused.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri
git commit -m "feat(phase4): import backend with duplicate detection"
```

---

### Task 4.4: Import Wizard UI

**Files:**
- Create: `apps/desktop/src/screens/ImportWizard.tsx`
- Create: `apps/desktop/src/hooks/useImport.ts`
- Modify: `apps/desktop/src/App.tsx` (route `/import`)

**Interfaces:**
- Consumes: `readHarnessState`, `importHarnessState` from `api.ts`; `useInstallations`/`useScanHarnesses`.
- Produces: the wizard users see on first run (project plan §37): Welcome → Scan → Select harnesses → Review parsed state → Import → Duplicates summary → Done.

- [ ] **Step 1: Extend `api.ts`**

```ts
export interface ParsedStateView {
  models: { native_id: string; remote_model_id: string; display_name: string; context_window: number | null }[];
  mcp: { native_name: string; transport: string; command: string | null }[];
  skills: { name: string; symlinked: boolean }[];
  warnings: string[];
}

export interface ImportOptions {
  importModels: boolean;
  importMcp: boolean;
  importSkills: boolean;
}

export interface ImportReport {
  providersCreated: number;
  modelsImported: number;
  mcpImported: number;
  skillsImported: number;
  duplicates: string[];
}

export async function readHarnessState(installationId: string): Promise<ParsedStateView> {
  return invoke<ParsedStateView>("read_harness_state", { installationId });
}

export async function importHarnessState(
  installationId: string,
  options: ImportOptions,
): Promise<ImportReport> {
  return invoke<ImportReport>("import_harness_state", { installationId, options });
}
```

- [ ] **Step 2: Write `useImport.ts`**

```ts
import { useMutation, useQuery } from "@tanstack/react-query";
import { importHarnessState, readHarnessState, type ImportOptions } from "../lib/api";

export function useReadHarnessState(installationId: string | null) {
  return useQuery({
    queryKey: ["harness-state", installationId],
    queryFn: () => readHarnessState(installationId!),
    enabled: !!installationId,
  });
}

export function useImportHarnessState(installationId: string) {
  return useMutation({
    mutationFn: (options: ImportOptions) => importHarnessState(installationId, options),
  });
}
```

- [ ] **Step 3: Write `ImportWizard.tsx`**

A 5-step stepper (state machine, one component):

```tsx
import { useState } from "react";
import { useInstallations, useScanHarnesses } from "../hooks/useHarnesses";
import { useImportHarnessState, useReadHarnessState } from "../hooks/useImport";

type Step = "welcome" | "scan" | "select" | "review" | "done";

const STEPS: Step[] = ["welcome", "scan", "select", "review", "done"];

export default function ImportWizard() {
  const [step, setStep] = useState<Step>("welcome");
  const [selected, setSelected] = useState<string[]>([]);
  const { data: installations } = useInstallations();
  const scan = useScanHarnesses();
  const active = selected[0] ?? null;
  const review = useReadHarnessState(active);
  const importMutation = useImportHarnessState(active ?? "");

  // Step 1 — Welcome: explains "we never modify your harness files; we import into our registry"
  // Step 2 — Scan: "Scan Computer" button → scan.mutate(); show count from installations
  // Step 3 — Select: checkbox list of installations (tier-1 only, status installed/config-missing);
  //          Next disabled when none selected
  // Step 4 — Review: shows review.data (models/mcp/skills counts + warnings);
  //          three checkboxes importModels/importMcp/importSkills (all default true);
  //          Import button → importMutation.mutate({importModels:true, importMcp:true, importSkills:true});
  // Step 5 — Done: report summary + duplicates list; "Open Dashboard" link → navigate("/")

  return <div className="mx-auto max-w-2xl">{/* stepper body per above */}</div>;
}
```

Also export a small `Stepper` header rendering `STEPS` with the current index highlighted.

- [ ] **Step 4: Wire the route**

Add `<Route path="/import" element={<ImportWizard />} />` in `App.tsx`; add an "Import Wizard" link in the sidebar.

- [ ] **Step 5: Verify end-to-end**

```bash
cd apps/desktop && npm run lint && npm run build && npm run tauri dev
```

Expected: navigate to `/import`, scan the machine, select a real harness, review parsed counts (matches Phase 3 adapter output), import, see the report. Re-importing shows duplicates.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop
git commit -m "feat(phase4): first-run import wizard"
```

---

### Task 4.5: Dashboard + Phase Exit

**Files:**
- Create: `apps/desktop/src/screens/DashboardScreen.tsx` (real implementation replacing placeholder)
- Modify: `apps/desktop/src/lib/api.ts` (`dashboardStats` command)

**Interfaces:**
- Consumes: repos (counts), `list_installations`.
- Produces: `#[tauri::command] pub async fn dashboard_stats(state) -> Result<DashboardStats, String>` where `DashboardStats { harnesses: usize, providers: usize, models: usize, mcp: usize, skills: usize, drifted: usize }` (drifted = 0 until Phase 12).

- [ ] **Step 1: Implement `dashboard_stats` command**

Add to `commands/scan.rs` (or a new `commands/dashboard.rs`):

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub harnesses: usize,
    pub providers: usize,
    pub models: usize,
    pub mcp: usize,
    pub skills: usize,
    pub drifted: usize,
}

#[tauri::command]
pub async fn dashboard_stats(state: State<'_, AppState>) -> Result<DashboardStats, String> {
    let pool = &state.pool;
    Ok(DashboardStats {
        harnesses: list_installations(pool).await.map_err(|e| e.to_string())?.len(),
        providers: chm_database::repos::providers::list_providers(pool).await.map_err(|e| e.to_string())?.len(),
        models: chm_database::repos::models::list_routes(pool).await.map_err(|e| e.to_string())?.len(),
        mcp: chm_database::repos::mcp::list_mcp_servers(pool).await.map_err(|e| e.to_string())?.len(),
        skills: chm_database::repos::skills::list_skills(pool).await.map_err(|e| e.to_string())?.len(),
        drifted: 0,
    })
}
```

Register in `generate_handler!`. Add `dashboardStats()` to `api.ts` and a `useDashboardStats` hook (staleTime 15s).

- [ ] **Step 2: Implement `DashboardScreen.tsx`**

Stat cards (count per §45) + quick actions (Scan Harnesses → `/scan`, Add Provider → `/providers`, Sync → disabled until Phase 8, Run Doctor → `/doctor`).

- [ ] **Step 3: Full gate**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
```

Expected: green.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop
git commit -m "feat(phase4): dashboard with stats"
```

Phase complete when all steps green.