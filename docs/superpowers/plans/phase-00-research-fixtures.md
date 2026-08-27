# Phase 0 — Research and Fixtures Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Document the native configuration of all five Tier-1 harnesses (Claude Code, Codex, OpenCode, Pi, Reasonix) and collect a representative fixture corpus so every later adapter phase can be built and tested offline.

**Architecture:** Each harness gets one research document under `docs/harnesses/<harness>.md` following a fixed template, plus versioned fixture files under `fixtures/<harness>/<version>/`. Research is evidence-based: every claim about a config path, field, or behavior is verified against the real installed harness on this machine and recorded with the harness version it was observed on.

**Tech Stack:** None (no code). Shell inspection commands, markdown docs, fixture files. Requires the actual harnesses installed on the dev machine for verification; where a harness is not installed, the doc records "not verifiable locally" and research moves to official docs + community sources, flagged as such.

## Global Constraints

- Redact ALL secrets in fixtures and docs: replace real API keys with `sk-EXAMPLE-...`, real tokens with `REDACTED`. Never commit a real credential.
- Every doc must record the harness version and config schema version observed (or "unknown").
- Deliverable paths are fixed: `docs/harnesses/<id>.md` and `fixtures/<id>/<version>/...` where `<id>` ∈ {claude-code, codex, opencode, pi, reasonix}.
- Fixtures must be **static snapshots** (files copied as-is, contents only redacted), never generated or synthesized — adapters are tested against real shapes.
- Phase exit: all 5 harness docs exist with every template section answered from evidence, `fixtures/README.md` documents the corpus, and the vercel-labs/skills detection-logic findings are recorded in `docs/harnesses/detection.md`.

---

### Task 0.1: Research Infrastructure + Doc Template

**Files:**
- Create: `docs/harnesses/README.md`
- Create: `docs/harnesses/_template.md`
- Create: `fixtures/README.md`

**Interfaces:**
- Produces: the exact template every harness research task (0.2–0.6) must fill in, and fixture naming rules every fixture task uses.

- [ ] **Step 1: Create the research index**

`docs/harnesses/README.md`:

```markdown
# Harness Research

Evidence-based notes on each supported harness's native configuration.
Status legend: DRAFT / COMPLETE / PARTIAL (not all verifiable locally)

| Harness | Doc | Status | Version observed |
|---------|-----|--------|------------------|
| Claude Code | docs/harnesses/claude-code.md | | |
| Codex | docs/harnesses/codex.md | | |
| OpenCode | docs/harnesses/opencode.md | | |
| Pi | docs/harnesses/pi.md | | |
| Reasonix | docs/harnesses/reasonix.md | | |
| Detection logic | docs/harnesses/detection.md | | |

Rule: a doc is COMPLETE only if every section of the template has
evidence (a path, command output, config excerpt, or explicit
"not verifiable locally" with a source link).
```

- [ ] **Step 2: Create the doc template**

`docs/harnesses/_template.md` — copy this verbatim to create each harness doc:

```markdown
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
```

- [ ] **Step 3: Create fixture corpus rules**

`fixtures/README.md`:

```markdown
# Adapter Fixtures

Static, real config snapshots used by adapter golden tests.
Rules:
- Path: fixtures/<harness-id>/<version>/<description>.<ext>
  e.g. fixtures/codex/0.24.0/config-toml-full.toml
- Copies of real configs; contents redacted only for secrets
  (API keys → sk-EXAMPLE..., tokens → REDACTED). Keep structure identical.
- Never edit values to "make tests pass" — if a fixture breaks a test,
  the adapter is wrong, not the fixture.
- Each fixture file may start with a comment line: `# fixture: <what it covers>`
- Version dir `unknown/` is allowed when version could not be observed.
```

- [ ] **Step 4: Commit**

```bash
git add docs/harnesses/ fixtures/README.md
git commit -m "docs(phase0): add harness research template and fixture rules"
```

---

### Task 0.2: Claude Code Research + Fixtures

**Files:**
- Create: `docs/harnesses/claude-code.md` (from `_template.md`)
- Create: `fixtures/claude-code/<version>/...` (files observed below)

**Interfaces:**
- Consumes: template from Task 0.1.
- Produces: `docs/harnesses/claude-code.md` + fixtures that Phase 3 Task 3.5 and Phase 8 Task 8.5 will parse.

- [ ] **Step 1: Gather local evidence**

Run these and record output in the doc (redact secrets):

```bash
claude --version 2>/dev/null || echo "NOT INSTALLED"
which claude 2>/dev/null
ls -la ~/.claude/ 2>/dev/null
ls -la ~/.claude.json 2>/dev/null
# user-level settings + model overrides:
cat ~/.claude/settings.json 2>/dev/null
# global MCP servers (claude mcp list output):
claude mcp list 2>/dev/null
# skills dir:
ls -la ~/.claude/skills/ 2>/dev/null
```

- [ ] **Step 2: Fill every template section**

Answer each section of `_template.md` using the evidence above. Pay special attention to:
- **§3 Models & Providers:** env vars `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_MODEL`, `ANTHROPIC_SMALL_FAST_MODEL`, `ANTHROPIC_DEFAULT_OPUS_MODEL`, `ANTHROPIC_DEFAULT_SONNET_MODEL`, `ANTHROPIC_DEFAULT_HAIKU_MODEL`, `ANTHROPIC_CUSTOM_HEADERS` — verify which are real today and record whether model overrides live in `~/.claude/settings.json` (env block) or `~/.claude.json`.
- **§4 MCP:** record the exact shape of the `mcpServers` entry in `~/.claude.json` (name → {type: stdio/http, command, args, env}).
- **§5 Skills:** confirm `~/.claude/skills` exists in current versions and whether symlinked skill dirs are followed; record SKILL.md frontmatter conventions.
- **§7 Version differences:** check the changelog for changes to settings.json schema in the last 3 minor versions.

If `claude` is not installed: mark the doc PARTIAL, answer what is verifiable from official docs (https://docs.anthropic.com/claude-code), and link sources in §9. The fixtures task below then only runs if config files exist.

- [ ] **Step 3: Collect fixtures**

Copy real config files into `fixtures/claude-code/<version>/`, e.g.:

```bash
mkdir -p fixtures/claude-code/<observed-version>
cp ~/.claude/settings.json fixtures/claude-code/<observed-version>/settings-full.json
cp ~/.claude.json fixtures/claude-code/<observed-version>/claude-json-mcp.json
cp -r ~/.claude/skills fixtures/claude-code/<observed-version>/skills/ 2>/dev/null
```

Then redact: replace every real API key with `sk-EXAMPLE...`, every token with `REDACTED`, and strip anything identifiable. Add the `# fixture:` comment line where the format allows it.

- [ ] **Step 4: Verify no secrets leaked**

```bash
rg -i "sk-[a-zA-Z0-9]{20,}|api[_-]?key|REDACTED" fixtures/claude-code/ | grep -v "sk-EXAMPLE\|REDACTED" || echo "CLEAN"
```

Expected: `CLEAN` (no real key pattern remains).

- [ ] **Step 5: Update index + commit**

Set the `claude-code` row in `docs/harnesses/README.md` to its real status/version, then:

```bash
git add docs/harnesses/claude-code.md fixtures/claude-code/
git commit -m "docs(phase0): claude-code research and fixtures"
```

---

### Task 0.3: Codex Research + Fixtures

**Files:**
- Create: `docs/harnesses/codex.md`
- Create: `fixtures/codex/<version>/...`

**Interfaces:**
- Consumes: template from Task 0.1.
- Produces: Codex doc + fixtures for Phase 3 Task 3.4 and Phase 8 Task 8.4.

- [ ] **Step 1: Gather local evidence**

```bash
codex --version 2>/dev/null || echo "NOT INSTALLED"
which codex 2>/dev/null
ls -la ~/.codex/ 2>/dev/null
cat ~/.codex/config.toml 2>/dev/null
# auth file — DO NOT print values; record only that it exists:
test -f ~/.codex/auth.json && echo "auth.json present (secrets redacted)"
# MCP:
cat ~/.codex/mcp.json 2>/dev/null || echo "no mcp.json"
# skills:
ls -la ~/.codex/skills/ 2>/dev/null || echo "no skills dir"
```

- [ ] **Step 2: Fill every template section**

Answer each section of `_template.md`. Pay special attention to:
- **§3 Models & Providers:** the `model_providers.<id>` table in `config.toml` (base_url, env_key, wire_api — OpenAI Chat vs Responses), the `models.<id>` table (name, provider, model, temperature, reasoning_effort, etc.), and how `model` / `model_reasoning_effort` selection works.
- **§4 MCP:** whether MCP lives in `~/.codex/mcp.json` or a `[mcp_servers]` table in config.toml, exact entry shape (command, args, env).
- **§5 Skills:** whether `~/.codex/skills` is a real current feature and how skills are referenced (e.g. `AGENTS.md`, `~/.codex/AGENTS.md`).
- **§7 Version differences:** note when `model_providers` replaced the older `[providers]`/`[model_providers]` layouts (config schema versioning), since old versions of Codex use `providers.<id>`.

If not installed: mark PARTIAL, use official docs (https://developers.openai.com/codex/), link sources.

- [ ] **Step 3: Collect fixtures**

```bash
mkdir -p fixtures/codex/<observed-version>
cp ~/.codex/config.toml fixtures/codex/<observed-version>/config-toml-full.toml
cp ~/.codex/mcp.json fixtures/codex/<observed-version>/mcp-json.json 2>/dev/null
```

Redact all secrets in the copies (see Task 0.2 Step 4 pattern for verification).

- [ ] **Step 4: Verify no secrets leaked**

```bash
rg -i "sk-[a-zA-Z0-9]{20,}|api[_-]?key" fixtures/codex/ | grep -v "sk-EXAMPLE" || echo "CLEAN"
```

Expected: `CLEAN`.

- [ ] **Step 5: Update index + commit**

```bash
git add docs/harnesses/codex.md fixtures/codex/
git commit -m "docs(phase0): codex research and fixtures"
```

---

### Task 0.4: OpenCode Research + Fixtures

**Files:**
- Create: `docs/harnesses/opencode.md`
- Create: `fixtures/opencode/<version>/...`

**Interfaces:**
- Consumes: template from Task 0.1.
- Produces: OpenCode doc + fixtures for Phase 3 Task 3.2 and Phase 8 Task 8.2 (the FIRST writable adapter — this doc must be the most precise).

- [ ] **Step 1: Gather local evidence**

```bash
opencode --version 2>/dev/null || echo "NOT INSTALLED"
which opencode 2>/dev/null
ls -la ~/.config/opencode/ 2>/dev/null
cat ~/.config/opencode/opencode.json 2>/dev/null
cat ~/.config/opencode/opencode.jsonc 2>/dev/null
# MCP config:
cat ~/.config/opencode/opencode-mcp.json 2>/dev/null || echo "no opencode-mcp.json"
# skills:
ls -la ~/.config/opencode/skills/ 2>/dev/null || echo "no skills dir"
# auth:
ls ~/.config/opencode/auth.json 2>/dev/null && echo "auth.json present (redact)"
```

- [ ] **Step 2: Fill every template section**

Pay special attention to:
- **§3 Models & Providers:** the `provider.<id>` object shape (npm/@ai-sdk style: `models` map of `id → {name, limit, ...}`), `model` top-level selection, env var mapping (e.g. `ZAI_API_KEY` → `apiKey` via `env`), and the `"experimental"`/`"options"` fields for baseURL and custom headers.
- **§4 MCP:** the `mcp` top-level object in opencode.json AND/OR the separate `opencode-mcp.json` file; exact `{ type: "local"|"remote", command, args, env, url, enabled }` shape.
- **§5 Skills:** whether skills are loaded from `~/.config/opencode/skills` natively or via `~/.agents/skills`; confirm symlink following.
- **§7 Version differences:** note any schema changes in the last 3 releases (changelog).

If not installed: mark PARTIAL, use official docs (https://opencode.ai/docs/config/), link sources.

- [ ] **Step 3: Collect fixtures**

```bash
mkdir -p fixtures/opencode/<observed-version>
cp ~/.config/opencode/opencode.json fixtures/opencode/<observed-version>/opencode-full.json 2>/dev/null
cp ~/.config/opencode/opencode-mcp.json fixtures/opencode/<observed-version>/opencode-mcp.json 2>/dev/null
```

Also create a **minimal synthetic variant is NOT allowed** — fixtures must be real snapshots. If the user's config is empty, add a second fixture later from a teammate's machine; keep the empty one (empty configs are a real shape adapters must parse).

Redact secrets; verify CLEAN.

- [ ] **Step 4: Update index + commit**

```bash
git add docs/harnesses/opencode.md fixtures/opencode/
git commit -m "docs(phase0): opencode research and fixtures"
```

---

### Task 0.5: Pi Research + Fixtures

**Files:**
- Create: `docs/harnesses/pi.md`
- Create: `fixtures/pi/<version>/...`

**Interfaces:**
- Consumes: template from Task 0.1.
- Produces: Pi doc + fixtures for Phase 3 Task 3.3 and Phase 8 Task 8.3.

- [ ] **Step 1: Gather local evidence**

```bash
pi --version 2>/dev/null || echo "NOT INSTALLED"
which pi 2>/dev/null
ls -la ~/.pi/ 2>/dev/null
ls -la ~/.pi/agent/ 2>/dev/null
cat ~/.pi/agent/config.toml 2>/dev/null
ls -la ~/.pi/agent/skills/ 2>/dev/null || echo "no skills dir"
```

- [ ] **Step 2: Fill every template section**

Pay special attention to:
- **§3 Models & Providers:** how Pi configures provider endpoints (base URL, API key env var) and model selection; whether role mapping exists (Pi supports role→model config — verify exact field names).
- **§4 MCP:** Pi's MCP server config location and shape.
- **§5 Skills:** `~/.pi/agent/skills` — whether it follows symlinks, skill format conventions.
- **§7 Version differences:** anything notable in recent changelogs.

If not installed: mark PARTIAL, research official docs + community, link sources.

- [ ] **Step 3: Collect fixtures**

```bash
mkdir -p fixtures/pi/<observed-version>
cp ~/.pi/agent/config.toml fixtures/pi/<observed-version>/config-toml-full.toml 2>/dev/null
cp -r ~/.pi/agent/skills fixtures/pi/<observed-version>/skills/ 2>/dev/null
```

Redact secrets; verify CLEAN.

- [ ] **Step 4: Update index + commit**

```bash
git add docs/harnesses/pi.md fixtures/pi/
git commit -m "docs(phase0): pi research and fixtures"
```

---

### Task 0.6: Reasonix Research + Fixtures

**Files:**
- Create: `docs/harnesses/reasonix.md`
- Create: `fixtures/reasonix/<version>/...`

**Interfaces:**
- Consumes: template from Task 0.1.
- Produces: Reasonix doc + fixtures for Phase 3 Task 3.6 and Phase 8 Task 8.6.

- [ ] **Step 1: Gather local evidence**

```bash
reasonix --version 2>/dev/null || echo "NOT INSTALLED"
which reasonix 2>/dev/null
ls -la ~/.config/reasonix/ 2>/dev/null || ls -la ~/.reasonix/ 2>/dev/null || echo "no config dir found"
# enumerate what exists — exact paths go in the doc
```

Record whatever config layout exists. Reasonix's exact layout is the least standardized of the Tier-1 set — the doc must capture the real observed paths even if they don't match the guesses above.

- [ ] **Step 2: Fill every template section**

For any section where local evidence is missing, research official docs/community sources and link them in §9. Mark the doc PARTIAL if Reasonix is not installed.

- [ ] **Step 3: Collect fixtures**

Copy whatever real config files exist into `fixtures/reasonix/<observed-version>/`. If no config exists locally, create the `unknown/` version dir and note in `fixtures/README.md` that Reasonix fixtures are pending.

- [ ] **Step 4: Update index + commit**

```bash
git add docs/harnesses/reasonix.md fixtures/reasonix/
git commit -m "docs(phase0): reasonix research and fixtures"
```

---

### Task 0.7: Detection-Logic Research (vercel-labs/skills)

**Files:**
- Create: `docs/harnesses/detection.md`

**Interfaces:**
- Produces: the detection rules inventory that Phase 2 Task 2.1 turns into `HarnessDefinition` entries.

- [ ] **Step 1: Study vercel-labs/skills detection logic**

Fetch the repo and inspect its harness detection code:

```bash
git clone --depth 1 https://github.com/vercel-labs/skills /var/folders/0h/rss8mh356zdbrrcv0phmm40w0000gn/T/opencode/vercel-skills 2>/dev/null
ls /var/folders/0h/rss8mh356zdbrrcv0phmm40w0000gn/T/opencode/vercel-skills/
```

Find where it detects installed coding agents (search for executable names, config paths, platform branches). Record:

- Which harnesses it detects and HOW (executable in PATH? config dir exists? package manager metadata?)
- Its list of executable names per harness (reuse directly for our `HarnessDefinition.executable_names`)
- Its config path assumptions per platform
- Which of its detection checks are reusable vs proprietary

- [ ] **Step 2: Cross-reference with our Tier-1 + detection-only lists**

Write `docs/harnesses/detection.md`:

```markdown
# Detection Logic — Research Notes

> Status: DRAFT
> Last updated: <date>

## vercel-labs/skills detection (reusable)
<for each harness it detects: executable names, config paths, method>

## Our Tier-1 definitions (candidates for HarnessDefinition)
<harness id, executable names, config paths, skill paths, mcp paths, platforms>

## Detection-only list (V1 shows "Detected — support coming")
Gemini CLI, Qwen Code, Kimi CLI, Cursor, Cline, Roo Code, Aider, Amp, Goose, Continue

## Version-detection commands per harness
<exact command + expected output shape for each Tier-1 harness>

## Platform notes
- macOS/Linux specifics (PATH search, ~/.config vs ~/Library/Application Support)
- Windows specifics (where.exe, %APPDATA%, junctions)
```

- [ ] **Step 3: Verify version commands**

For each Tier-1 harness installed locally, run its version command and paste the exact output into the doc (this becomes the parse test in Phase 2 Task 2.3).

- [ ] **Step 4: Commit**

```bash
git add docs/harnesses/detection.md
git commit -m "docs(phase0): detection logic research for harness registry"
```

---

### Task 0.8: Phase Exit Verification

**Files:**
- Modify: `docs/harnesses/README.md` (statuses)

**Interfaces:**
- Consumes: all Tasks 0.1–0.7.

- [ ] **Step 1: Verify all deliverables exist**

```bash
for f in docs/harnesses/{claude-code,codex,opencode,pi,reasonix,detection}.md; do
  test -f "$f" && echo "OK $f" || echo "MISSING $f"
done
test -f fixtures/README.md && echo "OK fixtures/README.md"
```

Expected: all six `OK` lines.

- [ ] **Step 2: Verify every template section is answered**

For each harness doc, grep for the 9 section headers; any missing header means the doc is incomplete:

```bash
for f in docs/harnesses/claude-code.md docs/harnesses/codex.md docs/harnesses/opencode.md docs/harnesses/pi.md docs/harnesses/reasonix.md; do
  echo "== $f"; rg -c "^## [1-9]\." "$f"
done
```

Expected: `9` per file (or an explicit documented exception in the doc).

- [ ] **Step 3: Final secret sweep**

```bash
rg -i "sk-[a-zA-Z0-9]{20,}|ghp_|Bearer [a-zA-Z0-9]{20,}" docs/harnesses/ fixtures/ | grep -v "sk-EXAMPLE\|REDACTED" || echo "CLEAN"
```

Expected: `CLEAN`.

- [ ] **Step 4: Commit any remaining index updates**

```bash
git add docs/harnesses/README.md
git commit -m "docs(phase0): finalize research index"  # only if changes exist
```

Phase complete when all steps green.