# <Harness> — Research Notes

> Status: DRAFT
> Last updated: <YYYY-MM-DD>
> Harness version observed: <x.y.z or "not installed">
> Config schema version observed: <as reported by harness, or "unknown">

## 1. Detection
- Executable names (e.g. `claude`, `claude-code`)
- Known install paths per platform (macOS/Windows/Linux)
- Version command and exact output format (show a real run)
- PATH detection notes

## 2. Configuration
- Config file paths: user-level and project-level, per platform
- File formats (JSON/JSONC/TOML/YAML)
- Candidate "managed subtree" (which section CHM would own) vs unmanaged sections
- Any existing managed/unmanaged markers (comments, prefixes)

## 3. Models & Providers
- How providers are defined natively (config block, auth file, env vars)
- How models are defined / selected
- Env var overrides: API key, base URL, model selection
- Role/model mapping support (e.g. Opus/Sonnet/Haiku → custom models)
- Per-model context/capability overrides

## 4. MCP Servers
- Global MCP config location + exact format (show a real excerpt)
- Environment variable references in MCP config
- Transport support (stdio / http / sse)

## 5. Skills
- Global skill path(s), and whether the harness follows symlinks
- Skill folder format expectations (SKILL.md? frontmatter? conventions)

## 6. Launch Behavior
- How the harness is invoked
- Env vars respected at launch
- Whether it reads shell startup files (must NOT be required by CHM launcher)

## 7. Version Differences
- Notable differences between recent versions (from changelogs/docs)
- Where version detection lives (config field, `--version` output)

## 8. Fixtures Collected
- Table: fixture path → what it covers (models / mcp / skills / full config)
- Redaction notes for each fixture

## 9. Sources
- Local inspection evidence (commands run, paths observed)
- Official docs / repo links for anything not verifiable locally