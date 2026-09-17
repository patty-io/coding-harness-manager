# Changelog

All notable changes to Coding Harness Manager (CHM) are documented here.
CHM follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) and
this file uses the conventions from [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.2.0] - 2026-09-17

### Highlights

- **Honest endpoint health checks.** `Check Health` now reports
  `CredentialMissing` instead of silently probing an endpoint without
  authentication when a credential is required but unresolved. Provider
  discovery and apply use the same gate, so a missing credential fails fast
  with a clear status instead of a misleading "Healthy".
- **Per-model thinking / reasoning configuration.** My Models now stores
  which thinking levels each model supports, and the edit dialog surfaces
  them as a checkbox group. CHM syncs this to the three harnesses that have
  a native per-model thinking field:
  - **pi** — `reasoning: true` plus a `thinkingLevelMap` with one entry per
    canonical level (`off`, `minimal`, `low`, `medium`, `high`, `xhigh`,
    `max`). Listed levels map to their own provider value; everything else
    is `null` so pi hides it. Validated against pi's runtime
    `getSupportedThinkingLevels`.
  - **codex** — top-level `model_reasoning_effort` per Codex profile file,
    collapsed to the highest openai-compatible level the user picked
    (`minimal`/`low`/`medium`/`high`); CHM-only levels (`xhigh`/`max`)
    fall back to `high` and only when no standard level is also selected.
  - **opencode** — per-model `variants: { <level>: { "reasoningEffort":
    "<level>" } }`, matching the 1.18.23 fixture shape. `reasoning = false`
    clears the entire `variants` block.
- **Endpoint edit and delete.** The Providers screen now exposes an
  Edit dialog (URL, protocol, auth type, discovery path, credential source,
  enabled toggle) and a Delete action for every endpoint. Backed by new
  `update_endpoint_cmd` and `delete_endpoint_cmd` Tauri commands.
- **Discover plan surfaced up front.** The provider detail screen now
  shows which endpoint CHM will probe and which it will skip, with the
  reason, *before* you press Discover. Backed by a new
  `discovery_plan_cmd`.
- **Fix: preview-stale false positive.** A long-standing bug made every
  selection-scoped preview fail with "preview is stale" because adapters
  fabricated per-read identity (fresh UUIDs + wall-clock timestamps) on
  each `read_state` call, and those volatile values were part of the
  preview/apply consistency hash. `plan_hash` now normalizes the volatile
  fields on the `current` side of the plan; real library and harness
  changes still invalidate the preview.

### New commands

- `update_endpoint_cmd`, `delete_endpoint_cmd` — endpoint lifecycle.
- `discovery_plan_cmd` — which endpoint(s) a provider-level discovery
  will probe, with reasons for skipped ones.
- `RouteUpdateInput` accepts granular `reasoning: bool` and
  `thinkingLevels: string[]`; both are merged into the existing
  `capabilities` blob without disturbing other capability keys.

### Fixed

- "preview is stale" on every selection-scoped sync after editing a model.
- pi: "Provider cline-pass: no \"api\" specified" — CHM now writes the
  `api` field on every provider entry (and on Update, so existing entries
  get repaired in place).
- pi: "baseUrl is required when defining custom models" — CHM now writes
  `baseUrl` on every provider entry (same repair-on-update behavior).
- Provider detail screen silently swallowed endpoint-load errors and
  showed "No endpoints yet" instead; it now displays the actual error.
- pi adapter was missing the `reasoning` + `thinkingLevelMap` fields on
  the model entry; opencode adapter was not writing per-level variants.
- A race during initial endpoint configuration could leave a route with
  an "env" reference whose name was the literal key (instead of an env
  var name) — the editor now validates the value against the resolved
  env var on save.
- `pi` was never on the `PATH` from interactive zsh. Added cargo's env
  to `~/.zshenv` and `~/.zshrc`; `cargo` (1.97) is now available in all
  new shells, and `chm`/`chm-dev`/`chm-build` aliases work from any
  directory.

### Changed

- `ModelMetadataCapabilities` gained a `thinking: bool` field so future
  UI can indicate which harnesses carry thinking config. pi, codex, and
  opencode declare `true`; claude-code, reasonix, and the detection
  adapters declare `false`. The flag is wired through the SDK and
  currently has no UI surface (follow-up).
- `normalize_volatile_actual_identity` in `sync.rs` clones the plan
  before hashing — negligible at realistic plan sizes, but flagged.
- `useDiscoveryPlan` query now caches for 15 seconds (matches
  `useProviderSummaries`) and re-fetches only when invalidations fire.
- Endpoint credentials stored as env vars are validated against the
  current process environment at save time; missing references surface a
  warning before the apply.

### Test additions

- `crates/providers/tests/health_discovery.rs` — 3 tests updated and 2
  new (`health_check_sends_bearer_credential`, the credential-missing
  guard).
- `apps/desktop/src-tauri/src/commands/sync.rs` — `selection_scoped_preview_hash_is_accepted_by_apply`
  regression test pinned by an `api`/`baseUrl`/full thinkingLevelMap
  fixture, plus `codex_thinking_sync_writes_model_reasoning_effort` and
  `opencode_thinking_sync_writes_variants` end-to-end tests.
- `adapters/pi/tests/writer_thinking.rs` — 4 unit tests for
  `set_model_thinking` (every level pinned, clear behavior, no-op when
  nothing is declared).
- `adapters/codex/tests/writer_thinking.rs` — 5 unit tests for
  `reasoning_effort_for_levels` and `set_reasoning_effort` (openai
  ordering, xhigh/max collapse, no-op clears the key).
- `adapters/opencode/tests/writer_thinking.rs` — 4 unit tests for
  `set_model_thinking` and a guard against raw `reasoning` /
  `thinking_levels` leaking as flat model fields.

### Internal / housekeeping

- `cargo fmt --all` clean across the workspace.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Workspace versions bumped to 0.2.0 (`Cargo.toml`,
  `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/package.json`,
  `Cargo.lock`).

### Notes

- CHM is still pre-1.0. Adapter capabilities are surfaced per harness
  and per configuration surface; support is not treated as one
  all-or-nothing checkbox.

[Unreleased]: https://github.com/patty-io/coding-harness-manager/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/patty-io/coding-harness-manager/compare/v0.1.3...v0.2.0
