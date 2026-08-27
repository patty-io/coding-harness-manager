# Codex — Research Notes

> Status: COMPLETE
> Last updated: 2026-08-27
> Harness version observed: 0.150.0 (`codex-cli 0.150.0`)
> Config schema version observed: modern per-provider config files (`#:schema` comment on line 1 of each)

## 1. Detection
- Executable names: `codex` (also `codex-cli` in `--version` output)
- Install paths observed: `~/.nvm/versions/node/v24.7.0/bin/codex` (npm global). Also official installer, Homebrew.
- Version command: `codex --version` → `codex-cli 0.150.0` (semver token `0.150.0` parseable)
- PATH detection notes: same nvm caveat as Claude Code; also probe `~/.codex/` config dir.

## 2. Configuration
- Config directory: `~/.codex/`
- **Main config**: `~/.codex/config.toml` — top-level model selection (`model`, `model_reasoning_effort`), `[mcp_servers.*]`, `[hooks]`, `[features]`, `[agents]`, `[tui]`, `[plugins]`, `[projects."<path>"]` trust levels, `[notice]`, `[apps]`. Large file (32KB on this machine) — CHM must merge, never rewrite.
- **Per-provider config files (MODERN FORMAT, 0.150 observed)**: `~/.codex/<id>.config.toml` — e.g. `glm.config.toml`, `minimax.config.toml`, `gpt.config.toml`, `kimi.config.toml`, `cgpt.config.toml`, `yc.config.toml`, `patty.config.toml`. Each contains:
  - top-level: `model = "<provider>/<model>"`, `model_provider = "<id>"`, `model_reasoning_effort`, `model_context_window`, `model_auto_compact_token_limit`, `model_supports_reasoning_summaries`
  - `[model_providers.<id>]`: `name`, `base_url`, `wire_api` (`"responses"` | `"chat"`), `env_key`, optional `stream_idle_timeout_ms`
  - `[mcp_servers.<name>]`: `command`, `args`, `startup_timeout_sec`
  - `[projects."<path>"]`: `trust_level`
  - First line: `#:schema https://developers.openai.com/codex/config-schema.json`
- **Legacy format (older versions)**: `model_providers` / `models.<id>` / `[providers.<id>]` tables directly in `config.toml` — documented in the official config reference; adapters must handle both (per-file detection: file exists + `#:schema` comment or `[model_providers]` table).
- Auth: `~/.codex/auth.json` — `auth_mode` (`"chatgpt"` | `"api"`), `OPENAI_API_KEY` (null or key), `tokens` (id/access/refresh/account_id). CHM never reads token values; fixture captures shape only.
- Formats: TOML everywhere.
- **Candidate managed subtree**: per-provider `<id>.config.toml` files entirely (CHM may own these); main `config.toml` → only the `[mcp_servers]` entries CHM manages (append/merge, never replace — file also holds projects/trust, hooks, plugins).

## 3. Models & Providers
- Provider definition (modern): `[model_providers.<id>]` in the per-provider config file: `name`, `base_url`, `wire_api` (`responses` for OpenAI Responses protocol, `chat` for Chat Completions), `env_key` (env var name holding the key).
- Model selection: top-level `model = "<id>/<model>"` (provider-prefixed) or bare `<model>` for built-in providers; `model_reasoning_effort`, `model_context_window`, `model_auto_compact_token_limit`, `model_supports_reasoning_summaries` per config.
- **No `[models.<id>]` table in the modern format** — models are selected via `model` + provider config; a single per-provider file carries one active model. Older versions supported `[models.<id>]` with per-model temperature/reasoning overrides (see §7).
- No role-mapping concept (no opus/sonnet/haiku); single `model` + `model_reasoning_effort`.
- Per-model context/capability overrides: `model_context_window` top-level.

## 4. MCP Servers
- Location: `[mcp_servers.<name>]` in `~/.codex/config.toml` AND in per-provider config files. No `~/.codex/mcp.json` exists in 0.150.
- Entry shape (verified): `command`, `args` (array), `startup_timeout_sec`; no `env` table observed yet — env likely injected via process env at launch (CHM launcher must inject).
- `codex mcp` CLI exists for management (not verified locally).

## 5. Skills
- Global skill path: `~/.codex/skills/` — verified, ~117 skills on this machine.
- **Symlinks ARE followed**: at least `impeccable` → `../../.agents/skills/impeccable` and `video-production` → absolute project path.
- Format: `<name>/SKILL.md` folders (same convention as `~/.agents/skills`).

## 6. Launch Behavior
- Invoked as `codex`. Per-provider config selection: `codex --config <file>` flag / `CODEX_CONFIG` env var (verify exact names in official docs §9).
- Env vars respected: `env_key`-named vars from `[model_providers]`, e.g. `AGENTS_PATTY_API_KEY`, `MINIMAX_API_KEY`.
- Does NOT read shell startup files.
- `--version` → `codex-cli 0.150.0` (stdout).

## 7. Version Differences
- Legacy layout (documented, pre-0.9x): `[providers.<id>]` (name, base_url, env_key, wire_api) + `[models.<id>]` (name, provider, model, temperature, reasoning_effort, context_window) inside `config.toml`.
- Modern layout (0.150 observed): per-provider `<id>.config.toml` files with `[model_providers.<id>]` + top-level `model`/`model_provider` keys; main config.toml keeps only selection + MCP + non-provider settings.
- Version detection: `codex --version` (single source); `version.json` records latest known.

## 8. Fixtures Collected
| Fixture | Covers |
|---------|--------|
| `fixtures/codex/0.150.0/config-toml-main.toml` | full main config.toml (32KB: model selection, mcp_servers, hooks, projects/trust, features, agents, plugins) |
| `fixtures/codex/0.150.0/config-toml-glm.toml` | per-provider config: model_providers + model selection + mcp + projects |
| `fixtures/codex/0.150.0/config-toml-minimax.toml` | second per-provider config (different base_url/env_key) |
| `fixtures/codex/0.150.0/auth-json-shape.json` | auth.json structure with all token values REDACTED |
| `fixtures/codex/0.150.0/skills-listing.txt` | `ls -la ~/.codex/skills/` output (names + symlink targets) |

Redaction notes: auth.json token values → REDACTED (shape preserved: `auth_mode`, `OPENAI_API_KEY`, `tokens.{id,access,refresh,account_id}`, `last_refresh`). No secrets found in any .config.toml (they reference env_key names only). Skill content not copied (117 skills; covered by ~/.agents/skills fixtures in Phase 10).

## 9. Sources
- Local inspection: `codex --version`, `ls -la ~/.codex/ ~/.codex/skills/`, full read of `config.toml`, `glm.config.toml`, `minimax.config.toml`, `auth.json` (shape only), `version.json`
- Official docs: https://developers.openai.com/codex/config/ (config reference, per-provider files, wire_api values, legacy providers/models layout)