# Harness Research

Evidence-based notes on each supported harness's native configuration.
Status legend: DRAFT / COMPLETE / PARTIAL (not all verifiable locally)

| Harness | Doc | Status | Version observed |
|---------|-----|--------|------------------|
| Claude Code | docs/harnesses/claude-code.md | COMPLETE | 2.1.246 |
| Codex | docs/harnesses/codex.md | COMPLETE | 0.150.0 |
| OpenCode | docs/harnesses/opencode.md | COMPLETE | 1.18.23 |
| Pi | docs/harnesses/pi.md | COMPLETE | 0.84.3 |
| Reasonix | docs/harnesses/reasonix.md | COMPLETE | 1.31.4 |
| Detection logic | docs/harnesses/detection.md | COMPLETE | vercel-labs/skills HEAD |
| Additional adapters | adapters/detection | COMPLETE | official harness docs/source |

Rule: a doc is COMPLETE only if every section of the template has
evidence (a path, command output, config excerpt, or explicit
"not verifiable locally" with a source link).

## Detection verification (Phase 2 smoke test, 2026-08-27)

`cargo run -p chm-harness-sdk --example scan` on this machine:
- claude-code | installed | 2.1.246 | ~/.claude/settings.json
- codex | installed | 0.150.0 | ~/.codex/config.toml
- opencode | installed | 1.18.23 | ~/.config/opencode/opencode.jsonc
- pi | installed | 0.84.3 | ~/.pi/agent/models.json
- reasonix | installed | 1.31.4 | ~/.reasonix/config.toml
- additional adapters: Gemini CLI, Qwen Code, Kimi CLI, Cursor, Cline, Roo
  Code, Aider, Amp, Goose, and Continue are read and validated through their
  documented config files; write support is capability-specific.
