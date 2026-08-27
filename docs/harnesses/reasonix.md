# Reasonix — Research Notes

> Status: COMPLETE
> Last updated: 2026-08-27
> Harness version observed: 1.31.4 (`reasonix v1.31.4`)
> Config schema version observed: `config_version = 7` (explicit marker field in config.toml)

## 1. Detection
- Executable names: `reasonix`
- Install paths observed: `/opt/homebrew/bin/reasonix` (Homebrew)
- Version command: `reasonix --version` → `reasonix v1.31.4` (semver after `v` prefix — parse the token following `v`)
- Config dir: `~/.reasonix/` — NOT `~/.config/reasonix` (non-XDG, verified)

## 2. Configuration
- Main config: `~/.reasonix/config.toml` — TOML with `config_version = 7` marker as the FIRST key (explicit schema marker for diagnostics — ideal for CHM's version-aware adapter).
- Local override: `~/.reasonix/reasonix.toml` (resolution order per header comment: flag > `./reasonix.toml` > `~/.reasonix/config.toml` > built-in defaults).
- Secrets: `~/.reasonix/.env` — dotenv file; providers reference keys by NAME via `api_key_env` (never inline). Header comment states: "Secrets are named via api_key_env and stored in Reasonix's global .env; never put keys here."
- Other dirs: `logs/`, `mcp-state/`, `projects/`, `state/`, `stats/`, `themes/`, `archive/` (backups), `remote/`.
- **Candidate managed subtree**: `[[providers]]` array entries in config.toml (CHM owns whole provider entries); `[permissions]` in reasonix.toml untouched.

## 3. Models & Providers
- Providers: array of tables `[[providers]]` with:
  - `name`, `kind` (`"openai"` | `"anthropic"` — protocol!), `base_url`
  - `models` — array of model ids (e.g. `["k3", "k3-256k", "kimi-for-coding"]`)
  - `default` — default model id
  - `api_key_env` — env var name for the key (secrets live in `~/.reasonix/.env`)
  - `context_window`, `max_output_tokens`
  - `price = { cache_hit = ..., cache_miss = ..., output = ... }`
- Selection: top-level `default_model` (provider-prefixed, e.g. `anthropic-api-kimi-com/k3-256k`).
- No role mapping. Per-model context overrides via provider `context_window`/`max_output_tokens`.

## 4. MCP Servers
- Managed under `~/.reasonix/mcp-state/` (migration marker file `mcp-global-migration-v1` exists). Global MCP state lives there, not in config.toml — CHM V1 treats Reasonix MCP as read-only + `mcp-state` inspection (exact schema TBD during Phase 3 adapter work; record after inspecting mcp-state/).

## 5. Skills
- Global skill path: `~/.reasonix/skills/` — 1 entry on this machine: `ego-browser` → symlink to `~/.local/share/ego/ego-skills` (absolute symlink). Symlinks appear supported.

## 6. Launch Behavior
- Invoked as `reasonix` (CLI) with a desktop app variant (config has `[desktop]` section — desktop app reads the same config.toml).
- Env vars respected: `api_key_env`-named keys from `~/.reasonix/.env` (also `REASONIX_THEME`, `REASONIX_LANG`, `REASONIX_PROXY_PASSWORD` per config comments).
- Does NOT read shell startup files; `.env` is loaded by Reasonix itself.

## 7. Version Differences
- `config_version` field (currently 7) is the schema marker CHM should gate on: unknown higher value → read-only mode.
- 1.3x: provider model lists are flat `models = [...]` arrays; older versions used per-model tables (verify in changelogs during adapter work if needed).

## 8. Fixtures Collected
| Fixture | Covers |
|---------|--------|
| `fixtures/reasonix/1.31.4/config-toml-full.toml` | full config.toml: 6 providers (openai+anthropic kinds), models arrays, prices, ui/desktop/telemetry/agent sections, config_version=7 |
| `fixtures/reasonix/1.31.4/reasonix-toml-local.toml` | local override (permissions) |
| `fixtures/reasonix/1.31.4/env-shape.txt` | `~/.reasonix/.env` key names with values REDACTED |
| `fixtures/reasonix/1.31.4/skills-listing.txt` | `ls -la ~/.reasonix/skills/` (absolute symlink) |

Redaction notes: `.env` values → REDACTED (key names preserved); config.toml contains NO inline secrets (api_key_env references only). mcp-state/ NOT copied (schema TBD — Phase 3 task documents it).

## 9. Sources
- Local inspection: `reasonix --version`, `ls -la ~/.reasonix/`, full read of `config.toml`, `reasonix.toml`, `.env` (names only), `skills/`
- Official docs: https://reasonix.dev/docs (verify during adapter work; config_version semantics)