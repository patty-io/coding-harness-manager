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