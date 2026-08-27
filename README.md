<p align="center">
  <strong>CODING HARNESS MANAGER</strong>
</p>

<p align="center">
  <a href="./LICENSE"><img src="https://img.shields.io/badge/License-Patty--Public--1.0-1769e0.svg?style=flat-square&labelColor=161616" alt="Patty Public License 1.0"/></a>
  <img src="https://img.shields.io/badge/CI-pending-6b7280.svg?style=flat-square&labelColor=161616" alt="CI"/>
  <a href="https://patty.io"><img src="https://img.shields.io/badge/PATTY.IO-patty.io-1769e0.svg?style=flat-square&labelColor=161616" alt="patty.io"/></a>
</p>

<h3 align="center">Configure once. Preview the diff. Sync everywhere.</h3>

<p align="center">
  One source of truth for providers, models, MCP servers, skills, and launch profiles<br/>
  across Claude Code, Codex, OpenCode, Pi, Reasonix — and other AI coding harnesses.
</p>

## What this repository does

Coding Harness Manager is a cross-platform desktop application that ends the
manual juggling of `~/.claude/settings.json`, `~/.codex/*.toml`,
`~/.config/opencode/opencode.jsonc`, and every other per-harness config file.
You keep one canonical registry; the app translates your desired state into
each harness's native format.

This repository contains everything:

- **Rust workspace** — domain model, SQLite persistence, OS keychain secrets,
  reconciliation engine, filesystem safety layer (atomic writes + backups)
- **Harness adapters** — version-aware read/write adapters with golden tests
  against real config snapshots (`fixtures/`)
- **Tauri 2 desktop app** — React + TypeScript UI: scan, import wizard,
  provider/model management, sync previews with diffs, history & rollback
- **Research corpus** — evidence-based documentation of each harness's native
  config format under `docs/harnesses/`
- **Release engineering** — CI, multi-platform build matrix, packaging docs

Detection includes Gemini CLI, Qwen Code, Kimi CLI, Cursor, Cline, Roo Code,
Aider, Amp, Goose, and Continue as "Detected — support coming".

## Design philosophy

1. **Desired state, not file editing.** The app is a reconciliation system:
   Desired State → Plan → Preview → Apply → Verify → Rollback. No destructive
   change surprises you.
2. **Provider ≠ Endpoint ≠ Model Route ≠ Model Identity.** Never flattened.
   Route identity is `(endpoint_id, remote_model_id)`.
3. **Secrets never touch the registry.** SQLite stores only credential
   references; keys live in macOS Keychain, Windows Credential Manager,
   libsecret, or an env var you name.
4. **Managed is not owned.** The app mutates only the smallest config subtree
   it manages and preserves every unmanaged byte around it.
5. **Unknown means read-only.** Untested harness versions get read-only mode,
   not destructive writes.
6. **Native config stays authoritative.** CHM makes it safer and easier —
   it doesn't hide it.

## Architecture

```text
┌──────────────────────────── Desktop (Tauri 2 + React) ────────────────────┐
│  Dashboard · Providers · My Models · Harnesses · MCP · Skills · History    │
└───────────────────────────────────┬────────────────────────────────────────┘
                                    │
                    ┌───────────────▼────────────────┐
                    │     Reconciliation Engine       │
                    │  desired + actual → plan        │
                    │  add · update · remove          │
                    │  conflict · unsupported         │
                    └───────┬──────────────┬──────────┘
                            │              │
             ┌──────────────▼───┐   ┌──────▼───────────────┐
             │  SQLite registry │   │  Harness adapters    │
             │  routes · mcp    │   │  claude-code codex   │
             │  skills · txns   │   │  opencode pi reasonix│
             └──────────────────┘   └──────┬───────────────┘
                                           │  atomic write + backup
                                    ┌──────▼──────┐
                                    │  ~/.claude  ~/.codex
                                    │  ~/.config/opencode …
                                    └─────────────┘
```

## Getting started

### Prerequisites

| Tool | Basis |
|---|---|
| Rust | stable, edition 2024 |
| Node.js | 22.x |
| npm | 11+ |

Platform specifics for Linux/Windows are in
[docs/installation.md](./docs/installation.md).

### Build & test

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

npm install --prefix apps/desktop
npm run lint --prefix apps/desktop
npm run build --prefix apps/desktop
```

### Run the desktop app

```bash
cd apps/desktop
npm run tauri dev
```

The app ships a SQLite registry at `~/.coding-harness-manager/chm.sqlite` on
first run. Scan harnesses → run the import wizard → manage providers and
models → preview a sync before applying anything.

## Common commands

| Command | Purpose |
|---|---|
| `cargo test --workspace` | Backend + adapter test suite |
| `cargo fmt --all --check` | Formatting gate (CI) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint gate (CI) |
| `npm run lint --prefix apps/desktop` | TypeScript checks |
| `npm run tauri dev --prefix apps/desktop` | Dev app window |
| `harnessctl scan` / `list` / `status` | Companion CLI (same core library) |

## Repository layout

```text
coding-harness-manager/
├── apps/desktop/                Tauri 2 desktop app (React + TS frontend)
│   ├── src/                     screens, hooks, typed API layer
│   └── src-tauri/               commands, services, drift, skill lib, tests
├── crates/
│   ├── core/                    pure domain types — no I/O
│   ├── database/                sqlx migrations + repositories
│   ├── secrets/                 Keychain / Credential Mgr / libsecret / env
│   ├── models-dev/              models.dev client + matched index
│   ├── providers/               health checks + /v1/models discovery
│   ├── reconciliation/          desired→plan engine (pure, no I/O)
│   ├── filesystem/              atomic writes, backups, links
│   └── harness-sdk/             adapter contract, detection registry, helpers
├── adapters/                    claude-code · codex · opencode · pi · reasonix
├── fixtures/                    real (redacted) config snapshots for goldens
├── docs/
│   ├── plans/                   original project plan
│   ├── superpowers/plans/       per-phase implementation plans
│   ├── harnesses/               research notes + detection logic
│   ├── installation.md          first-run guide
│   └── development.md           contributor setup
├── .github/workflows/           ci.yml · release.yml (3-OS matrix)
└── LICENSE                      Patty Public License 1.0
```

## Working order

1. Read the project plan first:
   [docs/plans/Coding-Harness-Manager-Project-Plan.md](./docs/plans/Coding-Harness-Manager-Project-Plan.md).
2. Adding or changing a parser? The fixture goldens lock behavior — collect a
   real, redacted snapshot into `fixtures/`, then make the adapter match.
   Never edit a fixture to satisfy code.
3. Every mutation must be previewable before apply. If it can't be rolled
   back, it doesn't ship.
4. Keep secrets out of SQLite, fixtures, logs, and exports.
5. Pass the full gate before pushing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run lint --prefix apps/desktop && npm run build --prefix apps/desktop
```

## Troubleshooting

- **Keychain prompt loops** — the Keychain item belongs to the signing
  identity of the dev build; delete the "coding-harness-manager" entry from
  Keychain Access and re-save the key.
- **Scan misses a harness** — PATH may differ between terminal and GUI.
  Check `docs/harnesses/detection.md`; config-dir fallbacks cover most cases.
- **Sync shows unexpected Update** — capability metadata drifted; re-run
  Enrich to refresh from models.dev, or set an explicit user override.
- **Rollback left a new file** — files created by a transaction have no
  backup by definition; rollback deletes them instead. Report any case where
  it didn't.
- **DB errors after manual editing** — don't edit
  `~/.coding-harness-manager/chm.sqlite` by hand; restore via History.

## Reference documents

- [docs/plans/Coding-Harness-Manager-Project-Plan.md](./docs/plans/Coding-Harness-Manager-Project-Plan.md) — goals, architecture decisions, phases
- [docs/harnesses/](./docs/harnesses/) — per-harness research notes
- [docs/installation.md](./docs/installation.md) — platform installers & first run
- [docs/development.md](./docs/development.md) — contributor setup
- [CONTRIBUTING.md](./CONTRIBUTING.md) — contribution rules & adapter guide

## License

Licensed under the **[Patty Public License 1.0](./LICENSE)** — Apache 2.0 with
one addition: organizations with annual revenue of $100M USD or more require a
commercial license from Patty Co., Ltd. ([licensing@patty.io](mailto:licensing@patty.io)).
Everyone else — individuals, startups, academia, non-profits — uses it freely,
forever.

<p align="center">
  <strong>Patty Coding Harness Manager</strong><br/>
  <sub>source-available · fair-code · built in the open</sub>
</p>