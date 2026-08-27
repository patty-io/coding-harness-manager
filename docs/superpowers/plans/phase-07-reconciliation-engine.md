# Phase 7 — Reconciliation Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the pure desired-state engine: `DesiredState + ActualState → ReconciliationPlan` (project plan §19, §20, §31). No writes — the engine only produces plans; Phase 8 executes them.

**Architecture:** `crates/reconciliation` is a pure library: inputs are normalized domain collections (routes, MCP servers, skills from the DB + parsed native state), output is a `ReconciliationPlan` of classified `PlanAction`s (Add/Update/Remove/Unchanged/Conflict/Unsupported/NoOp). The engine is harness-agnostic — the harness adapter (Phase 8) translates the plan into native edits, and the compatibility engine (Phase 8 Task 8.0) filters actions per `HarnessCapabilities`.

**Tech Stack:** Rust edition 2024, no I/O deps (only `chm-core` + serde). Extensively unit-tested: this is the most important pure-logic crate in the system.

## Global Constraints

- The engine NEVER writes. `reconcile()` is pure; `ReconciliationPlan` is the only output. (Test enforces this by design — the crate has no filesystem/DB deps.)
- Identity rules: models dedup by `(endpoint_id, remote_model_id)`; MCP by name; skills by canonical path (project plan §21, §22, §26).
- Classification precedence: `Conflict` > `Unsupported` > `Remove` > `Update` > `Add` > `NoOp` (a conflicting route is never silently updated).
- Update semantics (project plan §21): if ONLY context_window / display_name / capability metadata changed → `Update` (fields differ), never Remove+Add; if remote_model_id or endpoint changed → Remove+Add (identity change).
- Append vs Replace-Managed semantics (project plan §20): `Mode::Append` never plans Removes; `Mode::ReplaceManaged` plans Removes only for items whose `managed` flag is true.
- Dry-run preview (project plan §31) renders as a stable string summary: `N files will change; N model records updated; N MCP servers added; 0 unmanaged settings modified.`
- Phase exit: all unit tests green; `reconcile()` covered for every action type × both modes; clippy clean.

---

### Task 7.1: Engine Types

**Files:**
- Create: `crates/reconciliation/src/lib.rs`
- Create: `crates/reconciliation/src/plan.rs`
- Create: `crates/reconciliation/tests/plan_types.rs`

**Interfaces:**
- Produces (the core types all of Phase 7–8 use):
  - `pub enum Mode { Append, ReplaceManaged }`
  - `pub struct DesiredState { pub routes: Vec<ModelRoute>, pub mcp_servers: Vec<McpServer>, pub skills: Vec<Skill> }`
  - `pub struct ActualState { pub routes: Vec<HarnessModel>, pub mcp: Vec<HarnessMcp>, pub skills: Vec<HarnessSkill>, pub managed_flags: std::collections::HashMap<String, bool> }` — `managed_flags` keys are identity strings (`"route:<endpoint_id>:<remote_id>"`, `"mcp:<name>"`, `"skill:<path>"`) telling the engine which actual items CHM owns (project plan §18).
  - `pub enum PlanAction { Add(AddAction), Update(UpdateAction), Remove(RemoveAction), Unchanged(UnchangedAction), Conflict(ConflictAction), Unsupported(UnsupportedAction), NoOp(String) }`
  - `pub struct AddAction { pub kind: String, pub identity: String, pub payload: serde_json::Value }`
  - `pub struct UpdateAction { pub kind: String, pub identity: String, pub changed_fields: Vec<String>, pub desired: serde_json::Value, pub current: serde_json::Value }`
  - `pub struct RemoveAction { pub kind: String, pub identity: String }`
  - `pub struct UnchangedAction { pub kind: String, pub identity: String }`
  - `pub struct ConflictAction { pub kind: String, pub identity: String, pub reason: String, pub desired: serde_json::Value, pub current: serde_json::Value }`
  - `pub struct UnsupportedAction { pub kind: String, pub identity: String, pub reason: String }`
  - `pub struct ReconciliationPlan { pub actions: Vec<PlanAction> }`
  - `impl ReconciliationPlan { pub fn summary(&self) -> String }` — the dry-run line from Global Constraints.
  - `pub enum ReconcileError { InvalidDesired(String), InvalidActual(String) }` (thiserror)

- [ ] **Step 1: Write the failing test `tests/plan_types.rs`**

```rust
use chm_reconciliation::plan::{AddAction, PlanAction, ReconciliationPlan};

#[test]
fn summary_renders_stable_dry_run_line() {
    let plan = ReconciliationPlan {
        actions: vec![
            PlanAction::Add(AddAction { kind: "model".into(), identity: "e1:glm-5".into(), payload: serde_json::json!({}) }),
            PlanAction::Add(AddAction { kind: "mcp".into(), identity: "playwright".into(), payload: serde_json::json!({}) }),
            PlanAction::Add(AddAction { kind: "skill".into(), identity: "brainstorming".into(), payload: serde_json::json!({}) }),
        ],
    };
    let s = plan.summary();
    assert!(s.contains("1 model"), "{s}");
    assert!(s.contains("1 MCP server"), "{s}");
    assert!(s.contains("1 skill"), "{s}");
    assert!(s.contains("will change"), "{s}");
}
```

- [ ] **Step 2: Implement `plan.rs` and `lib.rs`**

```rust
// lib.rs
//! Desired-state reconciliation engine. Pure: no I/O.

pub mod plan;

pub use plan::*;
```

```rust
// plan.rs
//! Plan types + summary rendering.

use chm_core::domain::mcp::McpServer;
use chm_core::domain::models::ModelRoute;
use chm_core::domain::skills::Skill;
use chm_harness_sdk::adapter::types::{HarnessMcp, HarnessModel, HarnessSkill};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Append,
    ReplaceManaged,
}

#[derive(Debug, Clone, Default)]
pub struct DesiredState {
    pub routes: Vec<ModelRoute>,
    pub mcp_servers: Vec<McpServer>,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, Default)]
pub struct ActualState {
    pub routes: Vec<HarnessModel>,
    pub mcp: Vec<HarnessMcp>,
    pub skills: Vec<HarnessSkill>,
    pub managed_flags: HashMap<String, bool>,
}

#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error("invalid desired state: {0}")]
    InvalidDesired(String),
    #[error("invalid actual state: {0}")]
    InvalidActual(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAction {
    pub kind: String,
    pub identity: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAction {
    pub kind: String,
    pub identity: String,
    pub changed_fields: Vec<String>,
    pub desired: serde_json::Value,
    pub current: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveAction {
    pub kind: String,
    pub identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnchangedAction {
    pub kind: String,
    pub identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictAction {
    pub kind: String,
    pub identity: String,
    pub reason: String,
    pub desired: serde_json::Value,
    pub current: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedAction {
    pub kind: String,
    pub identity: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanAction {
    Add(AddAction),
    Update(UpdateAction),
    Remove(RemoveAction),
    Unchanged(UnchangedAction),
    Conflict(ConflictAction),
    Unsupported(UnsupportedAction),
    NoOp(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconciliationPlan {
    pub actions: Vec<PlanAction>,
}

impl ReconciliationPlan {
    pub fn count(&self, kind: &str) -> usize {
        self.actions
            .iter()
            .filter(|a| matches!(a, PlanAction::Add(x) if x.kind == kind)
                || matches!(a, PlanAction::Update(x) if x.kind == kind)
                || matches!(a, PlanAction::Remove(x) if x.kind == kind))
            .count()
    }

    /// Dry-run line (project plan §31).
    pub fn summary(&self) -> String {
        let models = self.count("model");
        let mcp = self.count("mcp");
        let skills = self.count("skill");
        format!(
            "{n} items will change; {models} model records updated; {mcp} MCP server(s) added; {skills} skill(s) linked; 0 unmanaged settings modified.",
            n = models + mcp + skills
        )
    }
}
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p chm-reconciliation
git add crates/reconciliation
git commit -m "feat(phase7): reconciliation plan types"
```

---

### Task 7.2: Model Reconciliation

**Files:**
- Create: `crates/reconciliation/src/models.rs`
- Create: `crates/reconciliation/tests/models_reconcile.rs`

**Interfaces:**
- Consumes: `plan` types (Task 7.1).
- Produces: `pub fn reconcile_models(desired: &[ModelRoute], actual: &[HarnessModel], mode: Mode, managed: &HashMap<String, bool>) -> Vec<PlanAction>` — pure function; identity key for actual items is `native_id` (the harness's own id); desired identity is `remote_model_id`; managed key is `"route:<endpoint_id>:<remote_model_id>"`.

- [ ] **Step 1: Write the failing tests `tests/models_reconcile.rs`**

```rust
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::HarnessModel;
use chm_reconciliation::models::reconcile_models;
use chm_reconciliation::plan::{Mode, PlanAction};
use chrono::Utc;
use uuid::Uuid;

fn route(endpoint_id: Uuid, remote_id: &str, context: Option<i64>) -> ModelRoute {
    ModelRoute {
        id: Uuid::new_v4(),
        endpoint_id,
        model_identity_id: None,
        remote_model_id: remote_id.into(),
        display_name: remote_id.into(),
        context_window: context,
        max_input: None,
        max_output: None,
        capabilities: serde_json::json!({}),
        overrides: serde_json::json!({}),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn actual_model(native_id: &str) -> HarnessModel {
    HarnessModel {
        native_id: native_id.into(),
        route: route(Uuid::new_v4(), native_id, None),
    }
}

fn managed(ids: &[&str]) -> std::collections::HashMap<String, bool> {
    ids.iter().map(|s| (s.to_string(), true)).collect()
}

#[test]
fn append_plans_add_for_missing_models() {
    let endpoint = Uuid::new_v4();
    let desired = vec![route(endpoint, "glm-5", Some(1_048_576))];
    let actual = vec![];
    let plan = reconcile_models(&desired, &actual, Mode::Append, &managed(&[]));
    assert_eq!(plan.len(), 1);
    assert!(matches!(&plan[0], PlanAction::Add(a) if a.identity == "glm-5"));
}

#[test]
fn append_never_plans_removes() {
    let endpoint = Uuid::new_v4();
    let desired: Vec<ModelRoute> = vec![];
    let actual = vec![actual_model("old-model")];
    let plan = reconcile_models(&desired, &actual, Mode::Append, &managed(&["route:x:old-model"]));
    assert!(plan.is_empty(), "append mode must not remove");
}

#[test]
fn replace_managed_removes_only_managed_items() {
    let desired: Vec<ModelRoute> = vec![];
    let actual = vec![actual_model("managed-one"), actual_model("native-two")];
    let plan = reconcile_models(
        &desired,
        &actual,
        Mode::ReplaceManaged,
        &managed(&["route:x:managed-one"]),
    );
    let removes: Vec<_> = plan.iter().filter_map(|a| match a {
        PlanAction::Remove(r) => Some(r.identity.clone()),
        _ => None,
    }).collect();
    assert_eq!(removes, vec!["managed-one".to_string()], "only managed items removed");
}

#[test]
fn update_when_only_metadata_changed() {
    let endpoint = Uuid::new_v4();
    let desired = vec![route(endpoint, "glm-5", Some(2_000_000))];
    let actual = vec![actual_model("glm-5")];
    let plan = reconcile_models(&desired, &actual, Mode::Append, &managed(&["route:x:glm-5"]));
    assert_eq!(plan.len(), 1);
    match &plan[0] {
        PlanAction::Update(u) => {
            assert!(u.changed_fields.contains(&"context_window".to_string()));
            assert_eq!(u.identity, "glm-5");
        }
        other => panic!("expected Update, got {other:?}"),
    }
}

#[test]
fn unchanged_when_equal() {
    let endpoint = Uuid::new_v4();
    let desired = vec![route(endpoint, "glm-5", Some(1_048_576))];
    let mut actual = actual_model("glm-5");
    actual.route.context_window = Some(1_048_576);
    let plan = reconcile_models(&desired, &actual, Mode::Append, &managed(&["route:x:glm-5"]));
    assert!(matches!(&plan[0], PlanAction::Unchanged(_)));
}
```

- [ ] **Step 2: Implement `models.rs`**

```rust
//! Model reconciliation: desired routes vs actual harness models.

use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::HarnessModel;
use std::collections::HashMap;

use crate::plan::{
    AddAction, ConflictAction, PlanAction, RemoveAction, UnchangedAction, UpdateAction, Mode,
};

const UPDATABLE_FIELDS: &[&str] = &["context_window", "max_input", "max_output", "display_name", "capabilities"];

pub fn reconcile_models(
    desired: &[ModelRoute],
    actual: &[HarnessModel],
    mode: Mode,
    managed: &HashMap<String, bool>,
) -> Vec<PlanAction> {
    let mut actions = Vec::new();
    let actual_by_native: HashMap<&str, &HarnessModel> =
        actual.iter().map(|m| (m.native_id.as_str(), m)).collect();

    // desired -> actual matching
    for d in desired {
        let native_id = d.remote_model_id.as_str();
        match actual_by_native.get(native_id) {
            None => {
                // check conflict: exists under a different native id? (alias collision)
                let alias_hit = actual.iter().find(|a| {
                    a.route.remote_model_id == d.remote_model_id && a.native_id != native_id
                });
                if let Some(hit) = alias_hit {
                    actions.push(PlanAction::Conflict(ConflictAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                        reason: format!("exists in harness as native id '{}'", hit.native_id),
                        desired: serde_json::json!(d),
                        current: serde_json::json!(hit.route),
                    }));
                } else {
                    actions.push(PlanAction::Add(AddAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                        payload: serde_json::json!({
                            "remote_model_id": d.remote_model_id,
                            "display_name": d.display_name,
                            "context_window": d.context_window,
                            "max_input": d.max_input,
                            "max_output": d.max_output,
                            "capabilities": d.capabilities,
                            "overrides": d.overrides,
                        }),
                    }));
                }
            }
            Some(a) => {
                let changed: Vec<String> = UPDATABLE_FIELDS
                    .iter()
                    .filter(|f| field_differs(*f, &a.route, d))
                    .map(|f| f.to_string())
                    .collect();
                if changed.is_empty() {
                    actions.push(PlanAction::Unchanged(UnchangedAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                    }));
                } else {
                    actions.push(PlanAction::Update(UpdateAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                        changed_fields: changed,
                        desired: serde_json::json!(d),
                        current: serde_json::json!(a.route),
                    }));
                }
            }
        }
    }

    // removals only in ReplaceManaged mode, only for managed items
    if matches!(mode, Mode::ReplaceManaged) {
        for a in actual {
            let key = format!("route:{}:{}", a.route.endpoint_id, a.native_id);
            let is_managed = managed.get(&key).copied().unwrap_or(false);
            if !is_managed {
                continue;
            }
            let still_desired = desired.iter().any(|d| d.remote_model_id == a.native_id);
            if !still_desired {
                actions.push(PlanAction::Remove(RemoveAction {
                    kind: "model".into(),
                    identity: a.native_id.clone(),
                }));
            }
        }
    }

    actions
}

fn field_differs(field: &str, actual: &ModelRoute, desired: &ModelRoute) -> bool {
    match field {
        "context_window" => actual.context_window != desired.context_window,
        "max_input" => actual.max_input != desired.max_input,
        "max_output" => actual.max_output != desired.max_output,
        "display_name" => actual.display_name != desired.display_name,
        "capabilities" => actual.capabilities != desired.capabilities,
        _ => false,
    }
}
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p chm-reconciliation
git add crates/reconciliation
git commit -m "feat(phase7): model reconciliation"
```

---

### Task 7.3: MCP and Skill Reconciliation

**Files:**
- Create: `crates/reconciliation/src/mcp.rs`
- Create: `crates/reconciliation/src/skills.rs`
- Create: `crates/reconciliation/tests/mcp_skills_reconcile.rs`

**Interfaces:**
- Produces:
  - `pub fn reconcile_mcp(desired: &[McpServer], actual: &[HarnessMcp], mode: Mode, managed: &HashMap<String, bool>) -> Vec<PlanAction>` — identity: server `name`; managed key `"mcp:<name>"`; comparing fields: command, args, url, env, transport. Conflict when same name but different command AND different transport.
  - `pub fn reconcile_skills(desired: &[Skill], actual: &[HarnessSkill], mode: Mode, managed: &HashMap<String, bool>) -> Vec<PlanAction>` — identity: canonical path (`Skill.canonical_path` vs `HarnessSkill.path`); managed key `"skill:<path>"`; `Unsupported` when the harness reports a skill path that cannot be linked (adapter capability `supports_symlinked_skills == false` and skill isn't native — passed via a `supported_binding: bool` set on the actual item by the caller).

- [ ] **Step 1: Write the failing tests**

MCP: append adds missing github server; append keeps existing; replace removes only managed; conflict when same name different command+transport; unchanged when equal. Skills: append adds symlink plan for canonical skill; native-existing skill → Unchanged; unsupported binding → `Unsupported` action.

- [ ] **Step 2: Implement both modules**

Follow the model-reconciliation structure exactly (match by identity → Add/Update/Unchanged/Conflict; removals only ReplaceManaged + managed flag). MCP update compares the field set `[command, args, url, env, transport]`; skills have no update path in V1 (content changes are Phase 10 hashing) — same path → Unchanged, else Remove+Add is NOT used (keep `Conflict` with reason `"skill content changed outside CHM"` — Phase 10 drift owns this).

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p chm-reconciliation
git add crates/reconciliation
git commit -m "feat(phase7): mcp and skill reconciliation"
```

---

### Task 7.4: Top-Level Reconcile + Compatibility Filter

**Files:**
- Create: `crates/reconciliation/src/engine.rs`
- Create: `crates/reconciliation/tests/engine.rs`

**Interfaces:**
- Produces:
  - `pub fn reconcile(desired: &DesiredState, actual: &ActualState, mode: Mode) -> Result<ReconciliationPlan, ReconcileError>` — dispatches to the three sub-reconcilers and concatenates actions.
  - `pub fn filter_unsupported(plan: ReconciliationPlan, caps: &HarnessCapabilities) -> ReconciliationPlan` — converts actions for kinds the harness cannot support into `Unsupported` (models when `!caps.supports_custom_models`, mcp when `!caps.supports_mcp_global`, skills when `!caps.supports_global_skills`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn reconcile_dispatches_all_three_kinds() {
    let endpoint = Uuid::new_v4();
    let desired = DesiredState {
        routes: vec![route(endpoint, "glm-5", Some(1_048_576))],
        mcp_servers: vec![mcp_server("github")],
        skills: vec![skill("brainstorming")],
    };
    let actual = ActualState::default();
    let plan = reconcile(&desired, &actual, Mode::Append).unwrap();
    let kinds: Vec<&str> = plan.actions.iter().map(|a| match a {
        PlanAction::Add(x) => x.kind.as_str(),
        _ => "other",
    }).collect();
    assert!(kinds.contains(&"model"));
    assert!(kinds.contains(&"mcp"));
    assert!(kinds.contains(&"skill"));
}

#[test]
fn reconcile_rejects_empty_remote_model_id() {
    let desired = DesiredState {
        routes: vec![route(Uuid::new_v4(), "", None)],
        ..Default::default()
    };
    let err = reconcile(&desired, &ActualState::default(), Mode::Append).unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn filter_unsupported_converts_actions() {
    let endpoint = Uuid::new_v4();
    let desired = DesiredState {
        routes: vec![route(endpoint, "glm-5", Some(1_048_576))],
        mcp_servers: vec![mcp_server("github")],
        skills: vec![],
    };
    let plan = reconcile(&desired, &ActualState::default(), Mode::Append).unwrap();
    // harness supports MCP but NOT custom models
    let caps = HarnessCapabilities::none().with_mcp_global(true);
    let filtered = filter_unsupported(plan, &caps);
    let model_unsupported = filtered.actions.iter().any(|a| matches!(a, PlanAction::Unsupported(u) if u.identity == "glm-5"));
    let mcp_added = filtered.actions.iter().any(|a| matches!(a, PlanAction::Add(x) if x.kind == "mcp"));
    assert!(model_unsupported, "model action must become unsupported");
    assert!(mcp_added, "mcp action must survive");
}
```

- [ ] **Step 2: Implement `engine.rs`**

```rust
//! Top-level reconciliation: desired + actual -> plan.

use chm_harness_sdk::adapter::types::HarnessCapabilities;

use crate::mcp::reconcile_mcp;
use crate::models::reconcile_models;
use crate::plan::{DesiredState, ActualState, Mode, PlanAction, ReconcileError, ReconciliationPlan, UnsupportedAction};
use crate::skills::reconcile_skills;

pub fn reconcile(
    desired: &DesiredState,
    actual: &ActualState,
    mode: Mode,
) -> Result<ReconciliationPlan, ReconcileError> {
    if desired.routes.iter().any(|r| r.remote_model_id.trim().is_empty()) {
        return Err(ReconcileError::InvalidDesired("route with empty remote_model_id".into()));
    }
    if actual.routes.iter().any(|r| r.native_id.trim().is_empty()) {
        return Err(ReconcileError::InvalidActual("actual model with empty native_id".into()));
    }
    let mut actions = Vec::new();
    actions.extend(reconcile_models(&desired.routes, &actual.routes, mode, &actual.managed_flags));
    actions.extend(reconcile_mcp(&desired.mcp_servers, &actual.mcp, mode, &actual.managed_flags));
    actions.extend(reconcile_skills(&desired.skills, &actual.skills, mode, &actual.managed_flags));
    Ok(ReconciliationPlan { actions })
}

pub fn filter_unsupported(plan: ReconciliationPlan, caps: &HarnessCapabilities) -> ReconciliationPlan {
    let actions = plan
        .actions
        .into_iter()
        .map(|action| {
            let supports = match &action {
                PlanAction::Add(a) | PlanAction::Update(a) | PlanAction::Remove(a) => match a.kind.as_str() {
                    "model" => caps.supports_custom_models,
                    "mcp" => caps.supports_mcp_global,
                    "skill" => caps.supports_global_skills,
                    _ => true,
                },
                _ => true,
            };
            if supports {
                action
            } else {
                let (identity, reason) = match &action {
                    PlanAction::Add(a) => (a.identity.clone(), "harness does not support this resource".into()),
                    PlanAction::Update(a) => (a.identity.clone(), "harness does not support updates to this resource".into()),
                    PlanAction::Remove(a) => (a.identity.clone(), "harness does not support this resource".into()),
                    _ => (String::new(), String::new()),
                };
                PlanAction::Unsupported(UnsupportedAction { kind: "resource".into(), identity, reason })
            }
        })
        .collect();
    ReconciliationPlan { actions }
}
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p chm-reconciliation
git add crates/reconciliation
git commit -m "feat(phase7): top-level engine and compatibility filter"
```

---

### Task 7.5: Phase Exit — Property Coverage

**Files:**
- Create: `crates/reconciliation/tests/property_coverage.rs`

**Interfaces:**
- Consumes: engine.

- [ ] **Step 1: Write the coverage test**

A table-driven test asserting every `PlanAction` variant is reachable through the public API (Add/Update/Remove/Unchanged/Conflict/Unsupported/NoOp) plus `summary()` covering both modes; also assert `ReconcileError` fires on empty remote id.

- [ ] **Step 2: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/reconciliation
git commit -m "feat(phase7): reconciliation property coverage"
```

Phase complete when all steps green.