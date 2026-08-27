# Detection Logic — Research Notes

> Status: COMPLETE
> Last updated: 2026-08-27
> Source: vercel-labs/skills (cloned 2026-08-27, HEAD) + local verification of all Tier-1 harnesses

## vercel-labs/skills detection (reusable)

`src/agents.ts` defines a registry of `AgentConfig` entries. **The universal detection method is CONFIG-DIR EXISTENCE, not executable-in-PATH**:

```ts
detectInstalled: async () => existsSync(join(home, '<config-dir>'))
```

Key mechanics:
1. Home-relative config dirs with **env overrides**: `CLAUDE_CONFIG_DIR` (→ `~/.claude`), `CODEX_HOME` (→ `~/.codex`), `VIBE_HOME`, `HERMES_HOME`, `AUTOHAND_HOME`, `GROK_HOME`.
2. XDG-aware for some agents: `xdgConfig ?? join(home, '.config')` — opencode, goose, amp use `~/.config/<name>`.
3. Secondary paths: codex also checks `/etc/codex`; openclaw checks legacy dirs.
4. App-bundle checks for desktop apps: `existsSync('/Applications/ZCode.app')`, `/Applications/MiniMax Code.app` (macOS-only patterns).
5. `packageJsonHasDependency()` — npm package detection used for IDE-embedded agents (cline via package.json deps).
6. Universal skills dir convention: `.agents/skills` (project) and `~/.agents/skills` (global) — vercel calls these "universal agents" (amp, replit, antigravity, gemini-cli).

**Reusable directly** (per Tier-1 + detection-only harness):
| Agent | config dir (existence check) | env override |
|-------|------------------------------|--------------|
| claude-code | `~/.claude` | CLAUDE_CONFIG_DIR |
| codex | `~/.codex` or `/etc/codex` | CODEX_HOME |
| opencode | `~/.config/opencode` (XDG) | — |
| pi | `~/.pi/agent` (note: checks agent subdir!) | — |
| reasonix | `~/.reasonix` | — |
| gemini-cli | `~/.gemini` | — |
| qwen-code | `~/.qwen` | — |
| kimi-code-cli | `~/.kimi-code` or `~/.kimi` | — |
| cursor | `~/.cursor` | — |
| cline | `~/.cline` (+ package.json deps) | — |
| roo | `~/.roo` | — |
| aider | (not present in this registry snapshot) | — |
| amp | `~/.config/amp` | — |
| goose | `~/.config/goose` | — |
| continue | `~/.continue` (or cwd `.continue`) | — |

## Our Tier-1 definitions (candidates for HarnessDefinition)

Verified against real machines (2026-08-27) — ALL FIVE installed:

| Harness id | executable(s) | config dir(s) | skill dir(s) | mcp path(s) | version cmd → output |
|-----------|---------------|---------------|--------------|-------------|----------------------|
| claude-code | `claude`, `claude-code` | `~/.claude/settings.json`, `~/.claude.json` | `~/.claude/skills` | `~/.claude.json` mcpServers | `claude --version` → `2.1.246 (Claude Code)` |
| codex | `codex` | `~/.codex/config.toml`, `~/.codex/<id>.config.toml` | `~/.codex/skills` | `[mcp_servers]` in config.toml | `codex --version` → `codex-cli 0.150.0` |
| opencode | `opencode` | `~/.config/opencode/opencode.jsonc` | `~/.config/opencode/skills` | `mcp` in opencode.jsonc | `opencode --version` → `1.18.23` |
| pi | `pi` | `~/.pi/agent/models.json`, `mcp.json`, `settings.json` | `~/.pi/agent/skills` (+ settings.json `skills` array) | `~/.pi/agent/mcp.json` | `pi --version` → `0.84.3` |
| reasonix | `reasonix` | `~/.reasonix/config.toml` (+ `reasonix.toml`) | `~/.reasonix/skills` | `~/.reasonix/mcp-state/` | `reasonix --version` → `reasonix v1.31.4` |

**Strategy recommendation**: CHM combines BOTH signals — executable-in-PATH (via `find_executable`) AND config-dir existence (vercel style). Config-dir-only → `ConfigMissing` status (detected but no executable — matters for the import wizard); executable + config → `Installed`. Env overrides `CLAUDE_CONFIG_DIR`/`CODEX_HOME` honored for the two harnesses that define them.

## Detection-only list (V1 shows "Detected — support coming")

Gemini CLI, Qwen Code, Kimi CLI, Cursor, Cline, Roo Code, Aider, Amp, Goose, Continue — config dirs from the table above; detection-only means NO version/config parsing in V1.

## Version-detection commands per harness (verified)

| Harness | Command | Output | Parse rule |
|---------|---------|--------|------------|
| claude-code | `claude --version` | `2.1.246 (Claude Code)` | first semver token |
| codex | `codex --version` | `codex-cli 0.150.0` | first semver token |
| opencode | `opencode --version` | `1.18.23` | first semver token |
| pi | `pi --version` | `0.84.3` | first semver token |
| reasonix | `reasonix --version` | `reasonix v1.31.4` | token following `v` |

All five print to stdout; the generic "first semver-like token" parser (split on whitespace, token matching `\d+\.\d+`) handles all five.

## Platform notes
- macOS/Linux: PATH search via split_paths; config via `$HOME` (+ XDG `~/.config` for opencode/amp/goose); app bundles checked via `/Applications/*.app` for desktop agents.
- Windows: `where.exe`-equivalent PATH walk with PATHEXT (`.EXE/.CMD/.BAT`); config via `%USERPROFILE%` + `%APPDATA%` fallback (Phase 14).
- Link abstraction: symlink (unix) vs junction (Windows) vs copy fallback — CHM filesystem layer owns this, vercel uses symlink-or-copy (`--copy` flag) which matches our `LinkOutcome::Copy` fallback.
- Broken-symlink handling: vercel `use.ts`/`sync.ts` resolve links; ours detects dangling links in Phase 10 conflict scan.