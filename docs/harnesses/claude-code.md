# Claude Code — Research Notes

> Status: COMPLETE
> Last updated: 2026-08-27
> Harness version observed: 2.1.246
> Config schema version observed: unknown (settings.json shape stable across recent versions)

## 1. Detection
- Executable names: `claude` (primary), `claude-code` (legacy alias)
- Install paths observed: `~/.nvm/versions/node/v24.7.0/bin/claude` (npm global install). Also installable via `npm install -g @anthropic-ai/claude-code`, native installer, Homebrew.
- Version command: `claude --version` → `2.1.246 (Claude Code)` (semver first token parseable)
- PATH detection notes: npm global bin is on PATH when nvm is sourced; detection must also probe `~/.claude/` config dir since PATH may not include nvm dirs in GUI-launched processes.

## 2. Configuration
- User-level config files (all under `~/.claude/`):
  - `~/.claude/settings.json` — user settings: `env`, `permissions`, `hooks`, `enabledPlugins`, `theme`, `effortLevel`, etc.
  - `~/.claude/settings.local.json` — machine-local overrides (same shape as settings.json)
  - `~/.claude.json` (HOME root) — global state: `mcpServers` (user-scope MCP), plus non-config state (projects, history pointers, tips flags) which must NEVER be touched
  - `~/.claude/.mcp.json` — project-level MCP when the "project" is HOME (see below)
- Project-level: `.mcp.json` at any project root (shared via git). NOTE: this machine has a `~/.mcp.json` too — project-level MCP files also live at arbitrary project roots.
- Formats: JSON everywhere (settings.json tolerates comments in some versions; treat as JSON).
- **Candidate managed subtree**: 
  - models/roles: `settings.json` → `env` block keys `ANTHROPIC_DEFAULT_*_MODEL`, `ANTHROPIC_MODEL`, `ANTHROPIC_SMALL_FAST_MODEL`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`
  - MCP: `~/.claude.json` → `mcpServers` object (merge, never replace whole file — file also holds project history)
- No managed/unmanaged markers exist natively; CHM tracks its own bindings in SQLite.

## 3. Models & Providers
- No native "provider registry" file — providers are configured via env vars at launch:
  - `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` — credentials
  - `ANTHROPIC_BASE_URL` — custom endpoint base URL (e.g. `https://api.z.ai/api/anthropic`)
  - `ANTHROPIC_CUSTOM_HEADERS` — JSON string of extra headers
- Model selection env vars:
  - `ANTHROPIC_MODEL` — default model
  - `ANTHROPIC_SMALL_FAST_MODEL` — small/fast model
  - `ANTHROPIC_DEFAULT_OPUS_MODEL`, `ANTHROPIC_DEFAULT_SONNET_MODEL`, `ANTHROPIC_DEFAULT_HAIKU_MODEL` — role mappings
- Settings.json `env` block is persisted; launch env vars override settings (per official docs, env vars take precedence).
- No per-model context/capability overrides in config — models.dev metadata is the source for those (CHM enrichment).
- Sources: official docs https://code.claude.com/docs/en/settings#environment-variables (verified partially locally — env block present, values not in use on this machine).

## 4. MCP Servers
- Global (user-scope) MCP: `~/.claude.json` → `mcpServers` — verified live shape:
```json
{
  "semble": { "command": "uvx", "args": ["--from", "semble[mcp]", "semble"], "type": "stdio" },
  "mem0-mcp": { "url": "https://mcp.mem0.ai/mcp", "type": "http" },
  "codegraph": { "type": "stdio", "command": "<node>", "args": ["<codegraph>", "serve", "--mcp"] },
  "lightpanda": { "type": "http", "url": "http://127.0.0.1:9333/mcp" },
  "google-docs": { "type": "stdio", "command": "node", "args": [...], "env": { "SERVICE_ACCOUNT_PATH": "...", "GOOGLE_IMPERSONATE_USER": "..." } }
}
```
- Fields per entry: `type` (stdio | http | sse), `command`, `args`, `env` (object), `url` (http/sse), `headers` (object, http — observed in project .mcp.json)
- Project MCP: `.mcp.json` → `mcpServers` (same shape). `claude mcp list` shows both; diagnostics output confirms per-server status (Connected / Needs authentication / Pending approval).
- Env references: literal values in `env` object; `$VAR` substitution is NOT performed inside env values by Claude Code (values are literal) — CHM launcher must inject resolved values into the process env instead.
- Transports verified locally: stdio + http.

## 5. Skills
- Global skill path: `~/.claude/skills/` — verified, 45 skills present on this machine.
- **Symlinks ARE followed**: 44/45 entries are symlinks (most → `../../.agents/skills/<name>` relative, one absolute → `~/.local/share/ego/ego-skills`).
- Skill dir format: `<name>/SKILL.md` with YAML frontmatter (`name`, `description`); sub-agent skills convention from superpowers ecosystem. No manifest file required for detection; folder name + SKILL.md presence is the detection rule (same as `~/.agents/skills`).

## 6. Launch Behavior
- Invoked as `claude` (CLI). Env vars respected at launch: all `ANTHROPIC_*` above.
- Does NOT read shell startup files — env must be injected by the launcher process.
- `claude` also supports `--model <id>` flag and `--settings <path>`.
- Version output goes to stdout: `2.1.246 (Claude Code)`.

## 7. Version Differences
- Recent versions (2.0.x → 2.1.x): no breaking change to settings.json/env or mcpServers shape observed in changelogs; `claude mcp` management CLI stable.
- settings.json gained `enabledPlugins` + marketplace keys in the plugin era (2.0+).
- Version detection: `claude --version` (single source).

## 8. Fixtures Collected
| Fixture | Covers |
|---------|--------|
| `fixtures/claude-code/2.1.246/settings-full.json` | full user settings.json (env, permissions, hooks, plugins) |
| `fixtures/claude-code/2.1.246/claude-json-mcp.json` | `mcpServers` object extracted from ~/.claude.json (env paths kept; no tokens present) |
| `fixtures/claude-code/2.1.246/mcp-json-project.json` | `~/.claude/.mcp.json` — project MCP with `headers.Authorization` → REDACTED; MiniMax API key → REDACTED |
| `fixtures/claude-code/2.1.246/skills-listing.txt` | real `ls -la ~/.claude/skills/` output (names + symlink targets); full skill content NOT copied (45 skills ≈ 2MB, content covered by ~/.agents/skills fixtures in Phase 10) |

Redaction notes: real MiniMax key (`sk-cp-...`) and Z.AI Authorization tokens in mcp-json-project.json replaced with `REDACTED`; structure preserved.

## 9. Sources
- Local inspection: `claude --version`, `which claude`, `ls -la ~/.claude/ ~/.claude/skills/`, `cat ~/.claude/settings.json ~/.claude/settings.local.json ~/.claude/.mcp.json`, `python3` extraction of `mcpServers` from `~/.claude.json`, `claude mcp list`
- Official docs: https://code.claude.com/docs/en/settings, https://code.claude.com/docs/en/mcp (env var precedence + mcp config reference)