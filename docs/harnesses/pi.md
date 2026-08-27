# Pi — Research Notes

> Status: COMPLETE
> Last updated: 2026-08-27
> Harness version observed: 0.84.3
> Config schema version observed: unknown (JSON files; stable across 0.8x)

## 1. Detection
- Executable names: `pi`
- Install paths observed: `~/.nvm/versions/node/v24.7.0/bin/pi` (npm global)
- Version command: `pi --version` → `0.84.3` (plain semver)
- Config dir: `~/.pi/agent/`

## 2. Configuration
- **NO `config.toml` exists** — Pi 0.84 uses JSON config files under `~/.pi/agent/`:
  - `models.json` — provider + model definitions (the managed subtree for providers/models)
  - `mcp.json` — MCP servers + `imports`
  - `settings.json` — default provider/model selection, packages, skills, theme
  - `auth.json` — per-provider credentials (api keys / oauth tokens)
  - `trust.json`, `models-store.json` (runtime cache — NOT managed by CHM)
- Formats: JSON everywhere.
- **Candidate managed subtree**: `models.json` → `providers.<name>` objects; `mcp.json` → `mcpServers.<name>`; `settings.json` → only `defaultProvider`/`defaultModel`/`defaultThinkingLevel` when CHM manages selection.

## 3. Models & Providers
- Provider definition in `models.json`:
```json
{
  "providers": {
    "<name>": {
      "baseUrl": "https://.../v1",
      "api": "openai-completions",
      "apiKey": "<secret or reference>",
      "compat": { "supportsDeveloperRole": false, "supportsStore": false, "supportsReasoningEffort": true, "thinkingFormat": "qwen-chat-template" },
      "models": [
        { "id": "qwen3.8-27b", "name": "qwen3.8-27b", "reasoning": true, "input": ["text"], "contextWindow": 128123,
          "cost": { "input": 0.6, "output": 2, "cacheRead": 0.2, "cacheWrite": 0.6 } }
      ]
    }
  }
}
```
- `api` field values: `openai-completions`, `anthropic` (verify full enum in docs; observed `openai-completions`).
- **`apiKey` is stored INLINE in models.json** — a secret-in-config antipattern CHM must NOT replicate: import maps existing keys to env refs / keychain and rewrites models.json without inline values when the user opts in.
- Selection: `settings.json` → `defaultProvider` + `defaultModel` (e.g. `openrouter/stealth/ox-alpha` — provider-prefixed), `defaultThinkingLevel`.
- No role mapping (opus/sonnet/haiku). Per-model: `contextWindow`, `reasoning`, `input`/`output` modalities, `cost`.
- Models may also be discovered from providers at runtime (models-store.json is a cache of that — not a CHM target).

## 4. MCP Servers
- Location: `~/.pi/agent/mcp.json`:
```json
{ "imports": ["claude-code"], "mcpServers": { "<name>": { "type": "http", "url": "...", "headers": {...}, "directTools": true } } }
```
- `imports: ["claude-code"]` — Pi can REUSE Claude Code's MCP config directly (verified live). CHM must record this capability: binding MCP to Pi may be a no-op when the same servers are already bound to Claude Code.
- Entry fields: stdio entries use `command` + `args`; http entries use `type: "http"`, `url`, `headers`; `directTools` flag observed on http servers.
- Per-project `.mcp.json` also supported (`~/.pi/agent/.mcp.json` exists on this machine with the same shape).

## 5. Skills
- Skills come from TWO sources:
  1. `settings.json` → `"skills": ["~/.claude/skills"]` — **Pi references OTHER harness skill dirs by path** (verified: it reads Claude Code's skills dir!)
  2. `~/.pi/agent/skills/` — own dir with 3 real entries (plan-on-linear, semble-search, zoom-out — real dirs, no symlinks on this machine)
- Format: `<name>/SKILL.md` folders.

## 6. Launch Behavior
- Invoked as `pi`. Env vars respected: those referenced by provider `apiKey` values? (keys are inline — no env refs observed). Process env inherited normally.
- Does NOT read shell startup files.
- `pi --version` → `0.84.3`.

## 7. Version Differences
- 0.8x: JSON config layout above. Earlier 0.7x used TOML (`~/.pi/agent/config.toml`) per community reports — adapters must detect: if `config.toml` exists → legacy TOML; if `models.json` exists → modern JSON.
- Version detection: `pi --version`.

## 8. Fixtures Collected
| Fixture | Covers |
|---------|--------|
| `fixtures/pi/0.84.3/models-json.json` | providers (baseUrl, api, apiKey→REDACTED, compat, models w/ cost+contextWindow) |
| `fixtures/pi/0.84.3/mcp-json.json` | mcp.json with `imports: ["claude-code"]` + stdio + http servers |
| `fixtures/pi/0.84.3/settings-json.json` | defaultProvider/defaultModel/thinking, packages, skills paths |
| `fixtures/pi/0.84.3/auth-json-shape.json` | auth.json structure (api keys + oauth → REDACTED) |
| `fixtures/pi/0.84.3/skills-listing.txt` | `ls -la ~/.pi/agent/skills/` (real dirs, no symlinks) |

Redaction notes: all `apiKey` values in models.json → REDACTED; all auth.json keys/tokens → REDACTED (shape preserved: `linear.key/hash`, `minimax.type/key`, `deepseek.type/key`, `openai-codex.type/access/refresh`); `headers.Authorization` in mcp.json + `.mcp.json` → REDACTED. `models-store.json` NOT copied (runtime cache, contains duplicates of the same secrets).

## 9. Sources
- Local inspection: `pi --version`, `ls -la ~/.pi/ ~/.pi/agent/`, full read of `models.json`, `mcp.json`, `settings.json`, `.mcp.json`, `auth.json` (shape), `skills/`
- Official docs: https://pi-docs.org (verify current layout; config.toml legacy references)