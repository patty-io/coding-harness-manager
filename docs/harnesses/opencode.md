# OpenCode — Research Notes

> Status: COMPLETE
> Last updated: 2026-08-27
> Harness version observed: 1.18.23
> Config schema version observed: `$schema: https://opencode.ai/config.json` (stable shape across 1.1x–1.2x)

## 1. Detection
- Executable names: `opencode`
- Install paths observed: `~/.opencode/bin/opencode` (installer). Also npm/bun global, Homebrew.
- Version command: `opencode --version` → `1.18.23` (plain semver on stdout)
- Config dir: `~/.config/opencode/` (XDG standard).

## 2. Configuration
- Main config: `~/.config/opencode/opencode.jsonc` — **JSONC** (comments tolerated; this machine's file is plain JSON). `$schema` first key.
- Other files in the dir: `auth.json` (absent here), `tui.json`, `AGENTS.md`, `plugins/`, `package.json` (bun dependency lock for plugin npm deps), `node_modules/`.
- No `opencode-mcp.json` observed on this machine — MCP lives inside the main config (`mcp` object).
- **Candidate managed subtree**: `provider.<id>` objects (whole subtree per provider) and `mcp.<name>` entries (whole subtree per server). Everything else (`command`, `permission`, `plugin`, `compaction`, `snapshot`, `theme`) untouched.

## 3. Models & Providers
- Provider definition: `provider.<id>` object with:
  - `npm` — AI SDK package name (e.g. `@ai-sdk/anthropic`)
  - `options` — `baseURL`, `apiKey` with **`{env:VAR}` templating** (e.g. `"{env:OMNIROUTE_API_KEY}"`)
  - `models.<id>` — map of model-id → model spec:
    - `name` (display), `limit: {context, output}`, `modalities: {input: [...], output: [...]}`, `variants.<name>: {reasoningEffort}`
- Model ids can be namespaced: `cc/claude-opus-4-8`, `mm/MiniMax-M3`, `gl/glm-5.2`, `km/kimi-for-coding`, `cx/gpt-5.5`.
- Top-level `model` key selects the default (not present on this machine — selection is per-session).
- Per-model context/capability overrides: `limit.context`, `limit.output`, `modalities` — matches CHM's `ModelRoute` fields directly.
- Env template syntax `{env:VAR}` is the native way to reference secrets — CHM import should map this to `CredentialKind::Env` references.

## 4. MCP Servers
- Location: top-level `mcp.<name>` object in opencode.jsonc (verified; separate opencode-mcp.json is legacy/optional).
- Entry shape (verified live):
  - `type`: `"local"` | `"remote"`
  - local: `command` is an **ARRAY** (e.g. `["uvx", "minimax-coding-plan-mcp", "-y"]`), plus `environment` object (NOTE: key is `environment`, not `env`) and `enabled`
  - remote: `url`, `headers` object, `enabled`
- `enabled: false` supported per server (disable without removal) — CHM sync should use this for enable/disable.

## 5. Skills
- Global skill path: `~/.config/opencode/skills/` — verified, 5 skills on this machine.
- **Symlinks ARE followed**: all 5 entries symlink to `../../../.agents/skills/<name>` (canonical `~/.agents/skills` in use).
- Format: `<name>/SKILL.md` folders (same convention).

## 6. Launch Behavior
- Invoked as `opencode`. Env vars respected: those referenced by `{env:VAR}` in provider options; process env inherits normally.
- `opencode` respects `OPENCODE_CONFIG` env var to point at an alternate config path (verify in docs §9).
- Does NOT read shell startup files.

## 7. Version Differences
- 1.1x–1.2x: `provider.<id>.npm` replaced the older per-provider `npm` resolution; `variants` (reasoningEffort presets) added in 1.1x; `mcp` entries gained `enabled` flag. `opencode-mcp.json` separation deprecated in favor of in-config `mcp` object.
- Version detection: `opencode --version` (plain semver).

## 8. Fixtures Collected
| Fixture | Covers |
|---------|--------|
| `fixtures/opencode/1.18.23/opencode-full.jsonc` | full real config: providers (9 models across 1 provider w/ variants), mcp (9 servers: local+remote, command arrays, environment, headers), command, plugin, permission |
| `fixtures/opencode/1.18.23/skills-listing.txt` | `ls -la ~/.config/opencode/skills/` (5 symlinks → canonical) |

Redaction notes: `CONTEXT7_API_KEY` (ctx7sk-…), `MINIMAX_API_KEY` (sk-cp-…), and 3× `headers.Authorization` (Z.AI tokens) → REDACTED; structure preserved. NOTE: this is the reference fixture — the adapter golden tests will assert `command` is an array and env is under `environment`.

## 9. Sources
- Local inspection: `opencode --version`, `ls -la ~/.config/opencode/`, full read of `opencode.jsonc`, `ls -la ~/.config/opencode/skills/`
- Official docs: https://opencode.ai/docs/config/ (provider/options/apiKey templating, mcp schema)