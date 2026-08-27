# Phase 5 — Provider Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Full provider lifecycle management: create/edit/delete providers and endpoints, store or reference credentials securely, validate connectivity, and discover models into the provider catalog.

**Architecture:** New Tauri commands in `apps/desktop/src-tauri/src/commands/providers.rs` (and `credentials.rs`) wrapping the Phase 1 repos and the Phase 1 provider client. UI screens under `src/screens/` consume them through `api.ts` hooks. Credentials are stored via `SecretStore` (Keychain on macOS) with only references in SQLite; the UI offers three sources: store on this computer / reference env var / no auth (project plan §8).

**Tech Stack:** As Phase 4 + React Hook Form + Zod for the provider/endpoint forms.

## Global Constraints

- Secrets flow: UI → command → `SecretStore::set` → returns `credential_ref_id`; the raw value NEVER touches SQLite and NEVER crosses the command boundary back to the UI after save.
- Env-var credentials are validated against the process env at save time (warning if unset, still saved).
- Provider deletion cascades to endpoints, catalog models, and routes (DB FK `ON DELETE CASCADE`); confirm dialog required in UI.
- Model discovery writes catalog rows via `upsert_catalog_model` with `CatalogStatus::New` for never-seen ids (project plan §10 refresh semantics).
- Health checks are async and non-blocking; results are returned to the UI and NOT persisted in V1 (persisted status lands with Phase 13 doctor).
- Phase exit: user can add Z.AI-style provider (Anthropic-compatible endpoint), save key to Keychain, see Healthy, discover 10+ models into catalog.

---

### Task 5.1: Provider CRUD Commands + List Screen

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/providers.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs` (register commands)
- Create: `apps/desktop/src/lib/api.ts` (extend)
- Create: `apps/desktop/src/hooks/useProviders.ts`
- Create: `apps/desktop/src/screens/ProvidersScreen.tsx` (replaces placeholder)

**Interfaces:**
- Consumes: `create_provider`, `list_providers`, `update_provider`, `delete_provider` (Phase 1 Task 1.4).
- Produces:
  - `#[tauri::command] pub async fn create_provider_cmd(state, name: String, display_name: String) -> Result<Provider, String>`
  - `#[tauri::command] pub async fn list_providers_cmd(state) -> Result<Vec<Provider>, String>`
  - `#[tauri::command] pub async fn update_provider_cmd(state, id: String, display_name: String, enabled: bool, notes: Option<String>) -> Result<Provider, String>`
  - `#[tauri::command] pub async fn delete_provider_cmd(state, id: String) -> Result<(), String>`
  - JS: `createProvider`, `listProviders`, `updateProvider`, `deleteProvider` in `api.ts`.

- [ ] **Step 1: Implement `commands/providers.rs`**

```rust
//! Provider CRUD commands.

use chm_core::domain::provider::Provider;
use chm_database::repos::providers;
use sqlx::Sqlite;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn create_provider_cmd(
    state: State<'_, AppState>,
    name: String,
    display_name: String,
) -> Result<Provider, String> {
    providers::create_provider(&state.pool, &name, &display_name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_providers_cmd(state: State<'_, AppState>) -> Result<Vec<Provider>, String> {
    providers::list_providers(&state.pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_provider_cmd(
    state: State<'_, AppState>,
    id: String,
    display_name: String,
    enabled: bool,
    notes: Option<String>,
) -> Result<Provider, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    providers::update_provider(&state.pool, id, &display_name, enabled, notes).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_provider_cmd(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    providers::delete_provider(&state.pool, id).await.map_err(|e| e.to_string())
}

pub async fn _unused(_: &sqlx::Pool<Sqlite>) {}
```

Register all four in `generate_handler!` and `commands/mod.rs`.

- [ ] **Step 2: Extend `api.ts`**

```ts
export interface Provider {
  id: string;
  name: string;
  display_name: string;
  enabled: boolean;
  notes: string | null;
  created_at: string;
  updated_at: string;
}

export async function createProvider(name: string, displayName: string): Promise<Provider> {
  return invoke<Provider>("create_provider_cmd", { name, displayName });
}
export async function listProviders(): Promise<Provider[]> {
  return invoke<Provider[]>("list_providers_cmd");
}
export async function updateProvider(
  id: string,
  displayName: string,
  enabled: boolean,
  notes: string | null,
): Promise<Provider> {
  return invoke<Provider>("update_provider_cmd", { id, displayName, enabled, notes });
}
export async function deleteProvider(id: string): Promise<void> {
  return invoke<void>("delete_provider_cmd", { id });
}
```

- [ ] **Step 3: Write `useProviders.ts`**

```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createProvider, deleteProvider, listProviders, updateProvider } from "../lib/api";

export function useProviders() {
  return useQuery({ queryKey: ["providers"], queryFn: listProviders });
}

export function useCreateProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, displayName }: { name: string; displayName: string }) => createProvider(name, displayName),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

export function useUpdateProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, displayName, enabled, notes }: { id: string; displayName: string; enabled: boolean; notes: string | null }) =>
      updateProvider(id, displayName, enabled, notes),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

export function useDeleteProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: deleteProvider,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}
```

- [ ] **Step 4: Write `ProvidersScreen.tsx`**

List per project plan §46 (name, endpoint count, discovered count, My Models count, health badge) + "Add Provider" button opening an inline form (name + display name) → `useCreateProvider`. Delete button with `window.confirm("Delete provider and ALL its endpoints, models, and bindings? This cannot be undone.")`. Row click → `/providers/:id` (Task 5.5). Endpoint/model counts come from a new `providerSummary` command in Task 5.5; until then show `—`.

- [ ] **Step 5: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo check
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase5): provider CRUD commands and list screen"
```

---

### Task 5.2: Endpoint CRUD

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/providers.rs`
- Create: `apps/desktop/src/hooks/useEndpoints.ts`
- Create: `apps/desktop/src/components/EndpointForm.tsx`
- Create: `apps/desktop/src/screens/ProviderDetailScreen.tsx` (endpoints section only; extended in 5.4/5.5)

**Interfaces:**
- Consumes: `create_endpoint`, `list_endpoints` (Phase 1 Task 1.4).
- Produces:
  - `#[tauri::command] pub async fn list_endpoints_cmd(state, provider_id: String) -> Result<Vec<ProviderEndpoint>, String>`
  - `#[tauri::command] pub async fn create_endpoint_cmd(state, input: EndpointInput) -> Result<ProviderEndpoint, String>`
  - `pub struct EndpointInput { pub provider_id: String, pub name: String, pub base_url: String, pub protocol: String, pub discovery_path: Option<String>, pub auth_type: String, pub credential_ref_id: Option<String>, pub headers: serde_json::Map<String, serde_json::Value>, pub enabled: bool }` (serde camelCase)
  - JS mirror types + `listEndpoints`, `createEndpoint`.

- [ ] **Step 1: Implement the commands**

`create_endpoint_cmd` builds a `ProviderEndpoint` from `EndpointInput`, resolving `credential_ref_id` by fetching the row via `fetch_credential` (add `pub async fn get_credential_ref(pool, id) -> Result<CredentialRef, DbError>` to `repos/providers.rs` if not already public — it must be made public for this task), then calls `create_endpoint`.

- [ ] **Step 2: Write `EndpointForm.tsx`**

React Hook Form + Zod:

```tsx
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";

const schema = z.object({
  name: z.string().min(1, "Name is required"),
  baseUrl: z.string().url("Must be a valid URL"),
  protocol: z.enum(["openai-chat", "openai-responses", "anthropic-messages", "openrouter-openai", "custom"]),
  discoveryPath: z.string().optional(),
  authType: z.enum(["none", "api-key-header", "bearer-token", "custom-header"]),
  credentialSource: z.enum(["keychain", "env", "none"]),
  envVarName: z.string().optional(),
  enabled: z.boolean().default(true),
});

export type EndpointFormValues = z.infer<typeof schema>;
```

The form: protocol select (labels per §7: "OpenAI Chat Completions compatible", "OpenAI Responses compatible", "Anthropic Messages compatible", "OpenRouter-style OpenAI compatible", "Custom / unknown"), credential source radio group, and — when `credentialSource === "env"` — an env var name field. Submit → `createEndpoint({...input, credentialRefId: null})` (credential creation is Task 5.3).

- [ ] **Step 3: Provider detail endpoint list + create flow**

`ProviderDetailScreen.tsx` shows endpoints as cards (base_url, protocol badge, enabled toggle stub) with "Add Endpoint" → `EndpointForm` in a modal. Wire `useEndpoints(providerId)`.

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo check && cargo test -p chm-database
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase5): endpoint CRUD and form"
```

---

### Task 5.3: Credential Storage UI

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/credentials.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Modify: `apps/desktop/src/components/EndpointForm.tsx` (credential save branch)
- Create: `apps/desktop/src/hooks/useCredentials.ts`

**Interfaces:**
- Consumes: `SecretStore` (Phase 1 Task 1.7), `create_credential_ref`, `get_credential_ref`.
- Produces:
  - `#[tauri::command] pub async fn save_api_key(state, key_name: String, value: String) -> Result<String, String>` — stores value in `state.secrets` under key `providers/<key_name>` and returns the credential ref id (persisted ref with kind `keychain`).
  - `#[tauri::command] pub async fn env_var_set(state, var_name: String) -> Result<bool, String>` — true if env var is set in the process env (used for the warning).
  - JS: `saveApiKey(keyName, value)`, `envVarSet(varName)`.

- [ ] **Step 1: Implement `commands/credentials.rs`**

```rust
//! Credential commands: OS-native secret storage via references.

use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_database::repos::providers::create_credential_ref;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn save_api_key(
    state: State<'_, AppState>,
    key_name: String,
    value: String,
) -> Result<String, String> {
    let store_key = format!("providers/{key_name}");
    state.secrets.set(&store_key, &value).map_err(|e| e.to_string())?;
    let reference = format!("coding-harness-manager/{store_key}");
    let cred = create_credential_ref(&state.pool, CredentialKind::Keychain, &reference)
        .await
        .map_err(|e| e.to_string())?;
    Ok(cred.id.to_string())
}

#[tauri::command]
pub async fn env_var_set(state: State<'_, AppState>, var_name: String) -> Result<bool, String> {
    Ok(state.secrets.get(&var_name).map_err(|e| e.to_string())?.is_some())
}
```

Note: `state.secrets` for EnvStore reads the process env directly (Phase 1 implementation), so `env_var_set` works on all platforms; `save_api_key` requires a store that supports `set` (Keychain on macOS).

- [ ] **Step 2: Extend `EndpointForm`**

When `credentialSource === "keychain"`: render an API-key password input; on submit call `saveApiKey(endpointName, value)` FIRST, then `createEndpoint` with the returned ref id. Show "saved to macOS Keychain" confirmation.

When `credentialSource === "env"`: on submit call `envVarSet(name)`; if false, show warning "Environment variable not currently set — the endpoint will fail validation until it is exported." Still save the endpoint.

- [ ] **Step 3: Write `useCredentials.ts`**

`useSaveApiKey` (mutation → returns refId), `useEnvVarSet` (mutation → boolean). Both invalidate `["providers"]` on success.

- [ ] **Step 4: Verify manually + commit**

Manual: add a Z.AI provider, Anthropic endpoint, save key to Keychain; confirm `security find-generic-password -s coding-harness-manager -a coding-harness-manager/providers/...` shows the entry (or via Keychain Access UI).

```bash
cd apps/desktop/src-tauri && cargo check
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase5): credential storage via OS secret store"
```

---

### Task 5.4: Health Check + Model Discovery Commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/providers.rs`
- Create: `apps/desktop/src/hooks/useProviderActions.ts`
- Modify: `apps/desktop/src/screens/ProviderDetailScreen.tsx` (health + discovery buttons)

**Interfaces:**
- Consumes: `health_check`, `discover_models`, `resolve_credential` (Phase 1 Task 1.9), `upsert_catalog_model`, `list_catalog_models` (Phase 1 Task 1.4).
- Produces:
  - `#[tauri::command] pub async fn check_endpoint_health(state, endpoint_id: String) -> Result<String, String>` — returns `HealthStatus` as its `Debug`-style string (`Healthy`, `AuthFailed`, …).
  - `#[tauri::command] pub async fn discover_endpoint_models(state, endpoint_id: String) -> Result<DiscoverReport, String>`
  - `pub struct DiscoverReport { pub total: usize, pub added: usize, pub updated: usize }` (camelCase)
  - `#[tauri::command] pub async fn list_catalog_models_cmd(state, endpoint_id: String) -> Result<Vec<ProviderCatalogModel>, String>`

- [ ] **Step 1: Implement the commands**

```rust
use chm_providers::{discover_models, health_check, resolve_credential};

#[tauri::command]
async fn find_endpoint(pool: &sqlx::Pool<Sqlite>, id: uuid::Uuid) -> Result<chm_core::domain::provider::ProviderEndpoint, String> {
    let providers = providers::list_providers(pool).await.map_err(|e| e.to_string())?;
    for p in &providers {
        for e in providers::list_endpoints(pool, p.id).await.map_err(|e| e.to_string())? {
            if e.id == id {
                return Ok(e);
            }
        }
    }
    Err("endpoint not found".into())
}

#[tauri::command]
pub async fn check_endpoint_health(state: State<'_, AppState>, endpoint_id: String) -> Result<String, String> {
    let id = uuid::Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    let endpoint = find_endpoint(&state.pool, id).await?;
    let cred = endpoint.credential_ref.as_ref()
        .and_then(|c| resolve_credential(c, state.secrets.as_ref()));
    let status = health_check(&endpoint, cred.as_deref(), &state.http).await;
    Ok(format!("{status:?}"))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverReport {
    pub total: usize,
    pub added: usize,
    pub updated: usize,
}

#[tauri::command]
pub async fn discover_endpoint_models(state: State<'_, AppState>, endpoint_id: String) -> Result<DiscoverReport, String> {
    let id = uuid::Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    let endpoint = find_endpoint(&state.pool, id).await?;
    let cred = endpoint.credential_ref.as_ref().and_then(|c| resolve_credential(c, state.secrets.as_ref()));
    let models = discover_models(&endpoint, cred.as_deref(), &state.http).await.map_err(|e| e.to_string())?;
    let existing = providers::list_catalog_models(&state.pool, id).await.map_err(|e| e.to_string())?;
    let mut added = 0;
    let mut updated = 0;
    let now = chrono::Utc::now();
    for m in &models {
        let is_new = !existing.iter().any(|c| c.remote_model_id == m.id);
        upsert_catalog_model(&state.pool, &chm_core::domain::models::ProviderCatalogModel {
            id: uuid::Uuid::new_v4(),
            endpoint_id: id,
            remote_model_id: m.id.clone(),
            raw_metadata: m.raw.clone(),
            canonical_model_id: None,
            match_confidence: None,
            first_seen_at: if is_new { now } else { now },
            last_seen_at: now,
            missing_since: None,
            status: if is_new { chm_core::domain::models::CatalogStatus::New } else { chm_core::domain::models::CatalogStatus::Available },
        }).await.map_err(|e| e.to_string())?;
        if is_new { added += 1 } else { updated += 1 }
    }
    Ok(DiscoverReport { total: models.len(), added, updated })
}
```

Extract the endpoint lookup into a private helper `async fn find_endpoint(pool: &Pool<Sqlite>, id: Uuid) -> Result<ProviderEndpoint, String>` to avoid duplication.

- [ ] **Step 2: Frontend**

`useProviderActions.ts`: `useCheckHealth(endpointId)` → mutation returning status string; `useDiscover(endpointId)` → mutation returning `DiscoverReport`; `useCatalog(endpointId)` → query `list_catalog_models_cmd`.

`ProviderDetailScreen`: per endpoint — "Check Health" button (badge shows result: Healthy green, AuthFailed red, RateLimited amber, Unreachable gray) and "Discover Models" button (shows `added`/`updated` toast + invalidates catalog query).

- [ ] **Step 3: Verify + commit**

Manual: with the Z.AI provider from Task 5.3, expect Healthy + a non-zero discovered model count.

```bash
cd apps/desktop/src-tauri && cargo check
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase5): health checks and model discovery"
```

---

### Task 5.5: Provider Detail Screen Completion + Phase Exit

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/providers.rs`
- Modify: `apps/desktop/src/screens/ProviderDetailScreen.tsx`
- Modify: `apps/desktop/src/screens/ProvidersScreen.tsx` (link rows)

**Interfaces:**
- Produces: `#[tauri::command] pub async fn provider_summary(state, provider_id: String) -> Result<ProviderSummary, String>` where `ProviderSummary { pub endpoints: usize, pub discovered_models: usize, pub my_models: usize, pub health: String }` (health = "unknown" or last check result string; persisted health lands in Phase 13).

- [ ] **Step 1: Implement `provider_summary`**

Counts: endpoints via `list_endpoints`; discovered via `list_catalog_models` summed across endpoints; my_models via `list_routes` filtered by endpoint ids of this provider; health = "unknown".

- [ ] **Step 2: Complete the detail screen**

Tabs per project plan §46: Overview (provider fields + summary numbers), Endpoints (list + add + health + discovery from Task 5.4), Credentials (ref kind/name per endpoint), Discovered Models (table from catalog query: remote_model_id, status badge, match confidence, first/last seen), My Models (routes for this provider's endpoints — link to `/models` in Phase 6).

- [ ] **Step 3: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase5): provider detail screen"
```

Phase complete when all steps green.