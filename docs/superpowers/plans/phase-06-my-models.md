# Phase 6 — My Models Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the My Models library: the curated list of model routes with discovery import, manual creation, models.dev enrichment with user-visible conflict resolution, deduplication, and provenance display (project plan §11, §12, §13, §47, §50).

**Architecture:** New Tauri commands in `apps/desktop/src-tauri/src/commands/models.rs` operating on `model_routes`, `model_identities`, `provider_catalog_models` via Phase 1 repos + the Phase 1 models-dev matcher. Metadata resolution follows the documented priority (user override > provider metadata > canonical metadata > discovery metadata > unknown) with field-level provenance stored in `ModelRoute.overrides` and `ModelIdentity.metadata`. The UI shows conflicts instead of deciding silently.

**Tech Stack:** As Phase 4 + TanStack Table for the models list with filters.

## Global Constraints

- Route identity is `(endpoint_id, remote_model_id)` — dedup NEVER by display name (project plan §21, §50).
- Adding a catalog model to My Models creates a `ModelRoute` and links `model_identity_id` when a match exists; unmatched routes keep `model_identity_id = None` and show "unknown" provenance.
- Metadata resolution must record `source` per field: `"user_override"`, `"models.dev/<provider>"`, `"models.dev"`, `"provider_discovery"`, `"unknown"` (project plan §11).
- Auto-link only at confidence ≥ 85. Confidence 60 (candidate) and ambiguous matches require user selection (project plan §12).
- Never delete catalog rows or routes during matching — only update linkage/confidence.
- Phase exit: user can import discovered models into My Models, see models.dev enrichment, resolve a conflict, and the list shows provenance + dedup behavior verified by tests.

---

### Task 6.1: Route CRUD Commands + My Models List Screen

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/models.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Create: `apps/desktop/src/hooks/useModels.ts`
- Create: `apps/desktop/src/screens/ModelsScreen.tsx` (replaces placeholder; tabs My Models / Discovered / Missing-Deprecated)

**Interfaces:**
- Consumes: `create_route`, `update_route`, `delete_route`, `list_routes`, `list_catalog_models` (Phase 1 Task 1.4).
- Produces:
  - `#[tauri::command] pub async fn list_routes_cmd(state) -> Result<Vec<ModelRouteView>, String>`
  - `pub struct ModelRouteView { pub id: String, pub endpoint_id: String, pub provider_name: String, pub remote_model_id: String, pub display_name: String, pub context_window: Option<i64>, pub max_input: Option<i64>, pub max_output: Option<i64>, pub capabilities: serde_json::Value, pub overrides: serde_json::Value, pub enabled: bool, pub identity_name: Option<String>, pub provenance: serde_json::Value }` (camelCase)
  - `#[tauri::command] pub async fn update_route_cmd(state, id: String, input: RouteUpdateInput) -> Result<(), String>`
  - `pub struct RouteUpdateInput { pub display_name: Option<String>, pub context_window: Option<i64>, pub max_input: Option<i64>, pub max_output: Option<i64>, pub enabled: Option<bool>, pub capabilities: Option<serde_json::Value>, pub overrides: Option<serde_json::Value> }`
  - `#[tauri::command] pub async fn delete_route_cmd(state, id: String) -> Result<(), String>`

- [ ] **Step 1: Write the failing backend test**

`apps/desktop/src-tauri/tests/models_commands.rs` (tests the command bodies via `&Pool`):

```rust
use chm_core::domain::models::ModelRoute;
use chm_database::connect_test;
use chm_database::repos::models::{create_route, delete_route, list_routes, update_route};
use chm_database::repos::providers::{create_endpoint, create_provider};
use coding_harness_manager_lib::commands::models::{list_route_views};
use chrono::Utc;
use uuid::Uuid;

#[tokio::test]
async fn route_view_includes_provider_name() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = chm_core::domain::provider::ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: p.id,
        name: "anthropic".into(),
        base_url: "https://api.z.ai/api/anthropic".into(),
        protocol: chm_core::domain::provider::Protocol::AnthropicMessages,
        discovery_path: Some("/v1/models".into()),
        auth_type: chm_core::domain::provider::AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let route = ModelRoute {
        id: Uuid::new_v4(),
        endpoint_id: e.id,
        model_identity_id: None,
        remote_model_id: "glm-5".into(),
        display_name: "GLM-5".into(),
        context_window: Some(1_048_576),
        max_input: None,
        max_output: None,
        capabilities: serde_json::json!({}),
        overrides: serde_json::json!({"context_window": {"value": 1048576, "source": "user_override"}}),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    create_route(&pool, &route).await.unwrap();

    let views = list_route_views(&pool).await.unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].provider_name, "Z.AI");
    assert_eq!(views[0].remote_model_id, "glm-5");
    assert_eq!(views[0].context_window, Some(1_048_576));
}
```

- [ ] **Step 2: Implement `commands/models.rs`**

```rust
//! My Models commands: route CRUD + views.

use chm_core::domain::models::ModelRoute;
use chm_database::repos::models::{delete_route, list_routes, update_route};
use chm_database::repos::providers::{list_endpoints, list_providers};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRouteView {
    pub id: String,
    pub endpoint_id: String,
    pub provider_name: String,
    pub remote_model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub max_input: Option<i64>,
    pub max_output: Option<i64>,
    pub capabilities: serde_json::Value,
    pub overrides: serde_json::Value,
    pub enabled: bool,
    pub identity_name: Option<String>,
    pub provenance: serde_json::Value,
}

pub async fn list_route_views(pool: &Pool<Sqlite>) -> Result<Vec<ModelRouteView>, String> {
    let routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut endpoint_map = std::collections::HashMap::new();
    for p in &providers {
        for e in list_endpoints(pool, p.id).await.map_err(|e| e.to_string())? {
            endpoint_map.insert(e.id, (p.display_name.clone(), e.name.clone()));
        }
    }
    let views = routes
        .into_iter()
        .map(|r| {
            let (provider_name, _endpoint_name) = endpoint_map.get(&r.endpoint_id).cloned().unwrap_or_default();
            ModelRouteView {
                id: r.id.to_string(),
                endpoint_id: r.endpoint_id.to_string(),
                provider_name,
                remote_model_id: r.remote_model_id.clone(),
                display_name: r.display_name.clone(),
                context_window: r.context_window,
                max_input: r.max_input,
                max_output: r.max_output,
                capabilities: r.capabilities.clone(),
                overrides: r.overrides.clone(),
                enabled: r.enabled,
                identity_name: None, // Phase 6 Task 6.4 fills identity names
                provenance: r.overrides.get("provenance").cloned().unwrap_or(serde_json::json!({"source": "unknown"})),
            }
        })
        .collect();
    Ok(views)
}

#[tauri::command]
pub async fn list_routes_cmd(state: State<'_, AppState>) -> Result<Vec<ModelRouteView>, String> {
    list_route_views(&state.pool).await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteUpdateInput {
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_input: Option<i64>,
    pub max_output: Option<i64>,
    pub enabled: Option<bool>,
    pub capabilities: Option<serde_json::Value>,
    pub overrides: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn update_route_cmd(
    state: State<'_, AppState>,
    id: String,
    input: RouteUpdateInput,
) -> Result<(), String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let mut routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let route = routes.iter_mut().find(|r| r.id == id).ok_or("route not found")?;
    if let Some(v) = input.display_name { route.display_name = v; }
    if let Some(v) = input.context_window { route.context_window = Some(v); }
    if let Some(v) = input.max_input { route.max_input = Some(v); }
    if let Some(v) = input.max_output { route.max_output = Some(v); }
    if let Some(v) = input.enabled { route.enabled = v; }
    if let Some(v) = input.capabilities { route.capabilities = v; }
    if let Some(v) = input.overrides { route.overrides = v; }
    update_route(&state.pool, route).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_route_cmd(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    delete_route(&state.pool, id).await.map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Frontend**

`useModels.ts`: `useRoutes()` (query), `useUpdateRoute()`, `useDeleteRoute()`.

`ModelsScreen.tsx` — tabs per §47:
- **My Models**: TanStack Table with columns provider, display name, remote model id, context window (formatted), enabled, provenance source badge; row actions: Edit (inline → `useUpdateRoute` with `overrides` merged `{"provenance": ...}` unchanged), Disable/Enable, Delete (confirm).
- **Discovered** (from `useCatalogAll` — new command listing catalog models with endpoint/provider names; Task 6.2).
- **Missing/Deprecated** (filter catalog `status = missing|deprecated`).

Filters (client-side, above the table): provider select, protocol select, capability checkboxes (multimodal, reasoning), availability.

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase6): route CRUD and my models list"
```

---

### Task 6.2: Import from Discovery

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/models.rs`
- Modify: `apps/desktop/src/screens/ModelsScreen.tsx` (Discovered tab actions)

**Interfaces:**
- Consumes: `list_catalog_models` (Phase 1), `create_route`, `create_identity` (Phase 1).
- Produces:
  - `#[tauri::command] pub async fn list_catalog_all(state) -> Result<Vec<CatalogView>, String>` where `CatalogView { pub id: String, pub endpoint_id: String, pub provider_name: String, pub endpoint_name: String, pub remote_model_id: String, pub status: String, pub match_confidence: Option<u8>, pub identity_name: Option<String> }`
  - `#[tauri::command] pub async fn add_catalog_to_my_models(state, catalog_id: String) -> Result<String, String>` — creates a route from the catalog row (display name = remote id; capabilities from raw_metadata; overrides `{"provenance": {"source": "provider_discovery", "catalog_id": "<id>"}}`); returns route id. Duplicate `(endpoint_id, remote_model_id)` → `Err("already in My Models")`.
  - `#[tauri::command] pub async fn add_catalog_batch(state, catalog_ids: Vec<String>) -> Result<usize, String>` — returns count added (skips duplicates, no error).

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn add_catalog_creates_route_and_rejects_duplicate() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = endpoint(&p.id);
    create_endpoint(&pool, &e).await.unwrap();
    let now = Utc::now();
    let cat = chm_core::domain::models::ProviderCatalogModel {
        id: Uuid::new_v4(),
        endpoint_id: e.id,
        remote_model_id: "glm-5".into(),
        raw_metadata: serde_json::json!({"id": "glm-5"}),
        canonical_model_id: None,
        match_confidence: None,
        first_seen_at: now,
        last_seen_at: now,
        missing_since: None,
        status: chm_core::domain::models::CatalogStatus::Available,
    };
    chm_database::repos::models::upsert_catalog_model(&pool, &cat).await.unwrap();

    let route_id = coding_harness_manager_lib::commands::models::add_catalog_to_my_models(&pool, &cat.id.to_string()).await.unwrap();
    assert!(Uuid::parse_str(&route_id).is_ok());

    let err = coding_harness_manager_lib::commands::models::add_catalog_to_my_models(&pool, &cat.id.to_string()).await.unwrap_err();
    assert!(err.contains("already in My Models"));
}
```

- [ ] **Step 2: Implement**

`list_catalog_all` joins catalog rows with endpoints/providers (same endpoint_map pattern as `list_route_views`). `add_catalog_to_my_models` is a `pub async fn` taking `&Pool` + catalog id (command wrapper adds State). It looks up the catalog row, checks `list_routes` for `(endpoint_id, remote_model_id)` conflict, then creates the route with provenance overrides and returns its id. Batch variant loops and counts successes.

- [ ] **Step 3: Discovered tab UI**

Discovered table with checkbox selection + "Add to My Models" button (per-selection or all in batch) → success toast + invalidate `["routes"]` and `["catalog"]`.

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase6): import from provider discovery"
```

---

### Task 6.3: Manual Model Form

**Files:**
- Create: `apps/desktop/src/components/ModelForm.tsx`
- Modify: `apps/desktop/src-tauri/src/commands/models.rs` (`create_route_cmd`)
- Modify: `apps/desktop/src/screens/ModelsScreen.tsx` (Add Model button)

**Interfaces:**
- Produces: `#[tauri::command] pub async fn create_route_cmd(state, input: RouteCreateInput) -> Result<String, String>` with `RouteCreateInput { pub endpoint_id: String, pub remote_model_id: String, pub display_name: Option<String>, pub context_window: Option<i64>, pub max_input: Option<i64>, pub max_output: Option<i64>, pub capabilities: Option<serde_json::Value> }` — always sets `overrides: {"provenance": {"source": "manual"}}`; validates non-empty remote_model_id and endpoint existence.

- [ ] **Step 1: Implement `create_route_cmd`** (test: rejects unknown endpoint; creates route with manual provenance).

- [ ] **Step 2: Write `ModelForm.tsx`** (React Hook Form + Zod):

Fields per project plan §13: provider (select of providers), endpoint (select filtered by provider), remote model ID (required), display name, context window, max input, max output, capabilities toggles (multimodal, reasoning, tool support, structured output → stored in `capabilities`), notes. The provider/endpoint cascading selects consume `useProviders` + `useEndpoints(providerId)`.

- [ ] **Step 3: Wire "Add Model" on the Models screen** (modal with `ModelForm`; submit → `createRouteCmd` → invalidate routes).

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase6): manual model creation form"
```

---

### Task 6.4: models.dev Enrichment + Conflict Resolution

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/models.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Create: `apps/desktop/src/components/ConflictResolver.tsx`
- Modify: `apps/desktop/src/screens/ModelsScreen.tsx`

**Interfaces:**
- Consumes: `match_model` (Phase 1 Task 1.8), `create_identity`, catalog update.
- Produces:
  - `#[tauri::command] pub async fn enrich_route(state, route_id: String) -> Result<EnrichOutcome, String>`
  - `pub enum EnrichOutcome { Matched { confidence: u8, identity_id: String, identity_name: String }, Ambiguous { candidates: Vec<EnrichCandidate>, current: serde_json::Value }, Unknown }` (serialize as tagged JSON)
  - `pub struct EnrichCandidate { pub models_dev_id: String, pub display_name: String, pub context_window: Option<i64>, pub max_output: Option<i64>, pub confidence: u8 }`
  - `#[tauri::command] pub async fn resolve_enrichment(state, route_id: String, identity_id: String) -> Result<(), String>` — links `model_identity_id`, writes field provenance into `overrides` (e.g. `{"context_window": {"value": 1048576, "source": "models.dev"}}`), updates catalog `canonical_model_id` + `match_confidence`.
  - `#[tauri::command] pub async fn set_user_override(state, route_id: String, field: String, value: Option<serde_json::Value>) -> Result<(), String>` — sets `overrides[field] = {"value": value, "source": "user_override"}`.

- [ ] **Step 1: Write the failing test**

```rust
use coding_harness_manager_lib::commands::models::{enrich_route, resolve_enrichment, set_user_override};

#[tokio::test]
async fn enrich_matches_exact_id_and_applies_provenance() {
    let pool = connect_test().await.unwrap();
    // provider + endpoint + route "gpt-4o" (exists in the phase-1 fixture catalog)
    let p = create_provider(&pool, "openai", "OpenAI").await.unwrap();
    let e = endpoint(&p.id);
    create_endpoint(&pool, &e).await.unwrap();
    let route = route(&e.id, "gpt-4o");
    create_route(&pool, &route).await.unwrap();

    let outcome = enrich_route(&pool, &route.id.to_string()).await.unwrap();
    match outcome {
        EnrichOutcome::Matched { confidence, identity_id, .. } => {
            assert!(confidence >= 85);
            resolve_enrichment(&pool, &route.id.to_string(), &identity_id).await.unwrap();
        }
        _ => panic!("expected a match for gpt-4o"),
    }
    let views = list_route_views(&pool).await.unwrap();
    assert!(views[0].overrides.get("context_window").is_some(), "field provenance written");
    assert_eq!(views[0].overrides["context_window"]["source"], "models.dev");
}

#[tokio::test]
async fn user_override_beats_models_dev() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "openai", "OpenAI").await.unwrap();
    let e = endpoint(&p.id);
    create_endpoint(&pool, &e).await.unwrap();
    let route = route(&e.id, "gpt-4o");
    create_route(&pool, &route).await.unwrap();

    // user override on context_window — models.dev enrichment must NOT overwrite it
    set_user_override(&pool, &route.id.to_string(), "context_window", Some(serde_json::json!(524288))).await.unwrap();
    if let EnrichOutcome::Matched { identity_id, .. } = enrich_route(&pool, &route.id.to_string()).await.unwrap() {
        resolve_enrichment(&pool, &route.id.to_string(), &identity_id).await.unwrap();
    }
    let views = list_route_views(&pool).await.unwrap();
    assert_eq!(views[0].overrides["context_window"]["value"], 524288);
    assert_eq!(views[0].overrides["context_window"]["source"], "user_override");
}
```

- [ ] **Step 2: Implement**

`enrich_route(pool, route_id)`:
1. Load route (via `list_routes` find). If `overrides[field]["source"] == "user_override"` for a field, that field keeps user value (per §11 priority).
2. Load the phase-1 `ModelsDevCatalog` — must be fetched once and cached in `AppState` (`pub catalog: Option<ModelsDevCatalog>` field; `ensure_catalog(state)` loads from a bundled asset). Bundle `crates/models-dev/fixtures/catalog.json` into the Tauri binary via `include_str!` in a `chm-models-dev` helper: `pub fn bundled_catalog() -> ModelsDevCatalog` (parses `include_str!("../fixtures/catalog.json")`).
3. `match_model(remote_model_id, &catalog)`.
   - confidence ≥ 85 → auto-`Matched` (create identity if none for `models_dev_id`, via `create_identity` with canonical_id = models_dev id; then write provenance for context_window/max_output where catalog values exist and route has no user override).
   - confidence 60 → `Ambiguous` with candidates = all catalog models scoring ≥ 60 (call `match_model` per family), current = route's current overrides.
   - 0 → `Unknown`.
4. Catalog row (if exists) gets `canonical_model_id` + `match_confidence` via `upsert_catalog_model`.

`resolve_enrichment(pool, route_id, identity_id)`: sets `model_identity_id`, writes `{"context_window": {"value": <from identity metadata>, "source": "models.dev"}}` style provenance (source `"models.dev"`; `"models.dev/<provider>"` when the matched model's provider differs from the route's endpoint provider — per §11 priority 2 vs 3), updates catalog linkage.

`set_user_override(pool, route_id, field, value)`: merges into `overrides` with source `user_override`.

- [ ] **Step 3: `ConflictResolver.tsx`**

Modal shown for `Ambiguous` outcomes (and for catalog rows with `match_confidence = 60`): renders the conflict UI from project plan §11 — radio list of candidates showing `context_window` from each source ("Z.AI provider metadata" vs "canonical model metadata") plus "Custom" input; confirm → `resolveEnrichment` or `setUserOverride`.

- [ ] **Step 4: Enrichment UX on Models screen**

Each route row: "Enrich" button → `enrich_route`; outcome badges: Matched (green, confidence %), Ambiguous (amber → opens resolver), Unknown (gray). Auto-run enrichment on newly imported routes from discovery (Task 6.2) when confidence ≥ 85 silently.

- [ ] **Step 5: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop crates/models-dev
git commit -m "feat(phase6): models.dev enrichment and conflict resolution"
```

---

### Task 6.5: Dedup Verification + Phase Exit

**Files:**
- Create: `apps/desktop/src-tauri/tests/dedup_tests.rs`
- Modify: `apps/desktop/src/screens/ModelsScreen.tsx` (identity badge + provenance tooltip)

**Interfaces:**
- Consumes: everything in this phase.

- [ ] **Step 1: Write the dedup test**

```rust
#[tokio::test]
async fn same_endpoint_same_remote_id_never_duplicates() {
    // create route (e.id, "glm-5") twice via create_route → second must Err
    // then update context_window + display_name on the first and confirm
    // still exactly one row
}

#[tokio::test]
async fn equivalent_routes_across_endpoints_stay_distinct() {
    // (e1.id, "glm-5") and (e2.id, "glm-5") are TWO routes — both created
    // (cross-endpoint equivalence is the identity link's job, not dedup)
}
```

- [ ] **Step 2: Provenance display**

Row tooltip shows provenance chain: `provider_discovery` → `models.dev` → `user_override` per field, rendered as a small stacked list ("Context: user override 524,288 · Source: user_override").

- [ ] **Step 3: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase6): dedup guarantees and provenance display"
```

Phase complete when all steps green.