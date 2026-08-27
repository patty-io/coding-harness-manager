# Coding Harness Manager — Implementation Plan Index

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement these plans task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

This directory decomposes `docs/plans/Coding-Harness-Manager-Project-Plan.md` into one implementation plan per phase. Each plan is self-contained: exact file paths, interfaces, TDD steps, and commits. Execute strictly in order — every plan consumes interfaces produced by earlier plans.

## Roadmap (execute in order)

| # | Plan file | Builds | Depends on |
|---|-----------|--------|-----------|
| 0 | `phase-00-research-fixtures.md` | `docs/harnesses/*.md` research + `fixtures/` corpus | — |
| 1 | `phase-01-core-database.md` | Rust workspace, SQLite schema, domain entities, repositories, secrets, models.dev client, provider client | 0 (fixture paths) |
| 2 | `phase-02-harness-detection.md` | Harness registry, executable/config/version detection, inventory | 0, 1 |
| 3 | `phase-03-readonly-adapters.md` | `harness-sdk` trait, read-only adapters for Tier 1 | 0, 1, 2 |
| 4 | `phase-04-first-run-import.md` | Tauri app shell, dashboard, scan + import wizard | 1, 2, 3 |
| 5 | `phase-05-provider-management.md` | Provider/endpoint CRUD, credentials, health, discovery | 1, 4 |
| 6 | `phase-06-my-models.md` | My Models list/form, models.dev matching UI, dedup | 1, 4, 5 |
| 7 | `phase-07-reconciliation-engine.md` | `crates/reconciliation` desired→plan engine | 1, 3 |
| 8 | `phase-08-writable-adapters.md` | filesystem crate (atomic/backup/link), write adapters, sync flow | 3, 7 |
| 9 | `phase-09-mcp-management.md` | MCP registry, bindings, native translation, diagnostics | 4, 8 |
| 10 | `phase-10-skills-management.md` | `~/.agents/skills` canonical store, import, bindings, conflicts | 4, 8 |
| 11 | `phase-11-profiles-launcher.md` | Launch profiles, env injection, launcher, `harnessctl` CLI, configuration sets | 1, 4, 8 |
| 12 | `phase-12-drift-history.md` | File watchers, snapshots, transactions, rollback UI, backup/restore, import/export | 8 |
| 13 | `phase-13-doctor.md` | Diagnostics screens + export | 4, 8, 9, 10 |
| 14 | `phase-14-packaging.md` | License, release matrix, installers, docs, V1 acceptance | all |

## Global Constraints (every plan inherits these)

From project plan §4, §71, and §59 — treat as non-negotiable:

1. **Domain model is never flattened:** `Provider ≠ Endpoint ≠ Model Route ≠ Model Identity`. Route identity is `endpoint_id + remote_model_id`.
2. **Central sync model:** `Desired State → Actual State → Plan → Preview → Apply → Verify → Rollback`. No write without preview.
3. **Adapter contract:** all harness integration goes through the version-aware `HarnessAdapter` trait in `crates/harness-sdk`. Harness-specific logic never leaks into app code.
4. **Secrets:** SQLite stores only `credential_refs` (type + reference). Secrets live in OS-native stores (Keychain / Windows Credential Manager / libsecret) or env vars; encrypted vault only as fallback.
5. **Managed ≠ owned:** CHM mutates only the smallest config subtree it manages. Never overwrite unrelated/unmanaged fields. Append vs Replace Managed vs Replace All semantics per plan §20.
6. **All writes are previewable, backed up, atomic, and reversible.** Filesystem layer is the only place that touches disks.
7. **Canonical skills:** `~/.agents/skills` is the canonical source; adapters decide consumption strategy (native read / symlink / junction / copy / unsupported).
8. **Version-aware safety:** unknown harness versions → read-only mode, no destructive writes.
9. **Tech stack:** Tauri 2, Rust (edition 2024), React + TypeScript (Vite, TanStack Query + Table, React Hook Form, Zod), SQLite via sqlx, migrations from day 1.
10. **Repo layout** per project plan §59: `apps/desktop/`, `crates/{core,database,secrets,reconciliation,providers,models-dev,filesystem,harness-sdk}/`, `adapters/{claude-code,codex,opencode,pi,reasonix}/`, `fixtures/`, `docs/`.
11. **Privacy:** no prompts, no telemetry by default, logs redact secrets (API keys, bearer tokens, secret env vars).
12. **CI:** fmt + clippy + tests + TS lint + frontend build on every PR. Native release matrix on tags.

## Cross-Plan Canonical Types

Types below are defined once (Phase 1) and reused by name in all later plans. Signatures are restated in each plan's `Interfaces:` blocks.

```rust
// crates/core — domain (see phase-01 for full definitions)
pub struct Provider; pub struct ProviderEndpoint; pub struct CredentialRef;
pub struct ModelIdentity; pub struct ProviderCatalogModel; pub struct ModelRoute;
pub struct HarnessInstallation; pub struct HarnessModelBinding;
pub struct McpServer; pub struct HarnessMcpBinding; pub struct Skill; pub struct HarnessSkillBinding;
pub struct LaunchProfile; pub struct ConfigurationSet; pub struct ConfigurationSetItem;
pub struct SyncTransaction; pub struct ConfigSnapshot;
pub enum Protocol { OpenAiChatCompletions, OpenAiResponses, AnthropicMessages, OpenRouterOpenAi, Custom }
pub enum AuthType { None, ApiKeyHeader, BearerToken, CustomHeader }
pub enum CatalogStatus { Available, New, Missing, Deprecated, Unknown }
pub enum ScopeType { Global, Project }
pub enum BindingType { Symlink, Junction, Copy, Native, Unsupported }

// crates/harness-sdk — adapter contract (see phase-03)
pub trait HarnessAdapter { /* detect, version, read_*, capabilities, plan, apply, validate, launch */ }
pub struct HarnessDefinition; pub struct HarnessCapabilities;

// crates/reconciliation — engine (see phase-07)
pub struct DesiredState; pub struct ActualState; pub struct ReconciliationPlan;
pub enum PlanAction { Add, Update, Remove, Unchanged, Conflict, Unsupported }
pub fn reconcile(desired: &DesiredState, actual: &ActualState) -> ReconciliationPlan;

// crates/filesystem (see phase-08)
pub fn atomic_write(path: &Path, content: &str) -> Result<(), FsError>;
pub fn backup_file(path: &Path) -> Result<PathBuf, FsError>;
pub fn link_directory(source: &Path, target: &Path) -> Result<LinkOutcome, FsError>;

// crates/secrets (see phase-01)
pub trait SecretStore { fn set/get/delete }
pub struct KeychainStore; pub struct EnvStore; pub struct EncryptedVaultStore;

// crates/providers (see phase-01)
pub enum HealthStatus { Healthy, AuthFailed, Unreachable, DiscoveryUnsupported, RateLimited, MalformedResponse, Unknown }
pub async fn health_check(endpoint, credential) -> HealthStatus;
pub async fn discover_models(endpoint, credential) -> Result<Vec<ProviderModel>, ProviderError>;

// crates/models-dev (see phase-01)
pub async fn fetch_catalog(http: &reqwest::Client) -> Result<ModelsDevCatalog, MdError>;
pub fn match_model(remote_id: &str, catalog: &ModelsDevCatalog) -> MatchResult;
```

## Open Questions Carried Forward

These stay open until their owning phase (per project plan §72). Each is marked `[DECISION GATE]` where it blocks work:

- License MIT vs Apache-2.0 → Phase 14 (default: MIT).
- CLI name / bundle `harnessctl` in V1 → Phase 11 (default: yes, same core lib).
- Unknown harness version behavior → Phase 3 (default: read-only mode).
- Export format JSON vs YAML vs zip → Phase 4 (default: JSON v1).
- History retention (snapshots vs patches) → Phase 12 (default: full snapshots, 90-day rotation).
- Min supported OS versions → Phase 14.
- OpenRouter-style provider metadata handling → Phase 6.
- Auto-refresh of provider catalogs → Phase 5 (default: manual refresh in V1).
- Configuration Sets UI → Phase 11 Task 11.5.
- Portable import/export + Backup/Restore UI → Phase 12 Task 12.5.

## Phase Exit Criteria

A phase is complete only when: all checkbox steps are done, all its tests pass, `cargo fmt --check && cargo clippy -- -D warnings && cargo test` are green (plus `npm run lint && npm run build` for UI phases), and the phase's deliverable exists. Do not start the next phase until the previous one's exit criteria are met.