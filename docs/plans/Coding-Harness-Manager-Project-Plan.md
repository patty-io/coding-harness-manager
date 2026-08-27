# Coding Harness Manager

> Cross-platform desktop application for managing providers, models, MCP servers, skills, launch profiles, and native configuration across AI coding harnesses such as Claude Code, Codex, OpenCode, Pi, Reasonix, and others.

## 1. Project Overview

**Coding Harness Manager** is an open-source desktop application that provides one source of truth for configuration across local AI coding harnesses.

Instead of manually editing:

- `~/.claude/...`
- `~/.codex/...`
- `~/.config/opencode/...`
- `~/.pi/agent/...`
- Reasonix configuration files
- shell environment files
- MCP configuration files
- skill folders

the user manages these resources centrally and Coding Harness Manager translates the desired state into each harness's native format.

The central design principle is:

```text
Desired State
    +
Actual Harness State
    ↓
Reconciliation Plan
    ↓
Preview Diff
    ↓
Apply
    ↓
Verified State
```

The application is not primarily a provider switcher. It is a **desired-state configuration manager for AI coding harnesses**.

---

## 2. Goals

### Primary Goals

1. Detect installed AI coding harnesses automatically.
2. Maintain a central registry of providers and provider endpoints.
3. Discover models exposed by providers.
4. Maintain a canonical "My Models" library.
5. Enrich model metadata using `models.dev`.
6. Push selected models into supported coding harnesses.
7. Read existing model configuration from each harness.
8. Manage global MCP servers centrally.
9. Manage global skills centrally.
10. Support canonical skills stored under `~/.agents/skills`.
11. Symlink or otherwise project skills into harness-specific locations when required.
12. Maintain launch profiles for harness/provider/model combinations.
13. Preview all configuration changes before applying them.
14. Detect external changes and configuration drift.
15. Support safe rollback.
16. Run on macOS, Windows, and Linux.
17. Provide import/export and backup capabilities.
18. Store secrets securely using OS-native credential storage.

### Non-Goals for Initial Release

- Managing arbitrary project-specific harness settings.
- Acting as an LLM API gateway.
- Proxying inference traffic.
- Hosting models.
- Managing cloud infrastructure.
- Replacing native harness configuration entirely.
- Managing every available coding harness in V1.
- Synchronizing configuration across different computers in V1.

---

# 3. Initial Supported Harnesses

V1 should focus on a small number of high-quality adapters.

### Tier 1

- Claude Code
- Codex
- OpenCode
- Pi
- Reasonix

### Detection-Only Initially

Additional tools can be detected and shown as "Detected — support coming".

Potential future targets:

- Gemini CLI
- Qwen Code
- Kimi CLI / Kimi Code
- Cursor
- Cline
- Roo Code
- Aider
- Amp
- Goose
- Continue
- Windsurf-related agents
- other terminal coding agents

Harness detection can take inspiration from or reuse compatible logic from `vercel-labs/skills`.

---

# 4. Technology Stack

## Desktop Framework

### Recommended

**Tauri 2**

Reasons:

- Native desktop applications.
- macOS support.
- Windows support.
- Linux support.
- Much smaller distribution than Electron.
- Rust backend is well suited for local filesystem operations.
- Good process spawning support.
- Good cross-platform path handling.
- Native credential integration is practical.
- Strong security model.

## Backend

**Rust**

Responsibilities:

- Harness detection.
- Filesystem access.
- Atomic file updates.
- Process spawning.
- Environment injection.
- Secret-store access.
- SQLite access.
- File watching.
- Symlink / Windows junction management.
- Config parsing and serialization.
- Backups.
- Reconciliation engine.
- Provider API calls.
- Model discovery.
- Harness version detection.

## Frontend

**React + TypeScript**

Recommended supporting libraries:

- Vite
- TanStack Query
- TanStack Table
- React Hook Form
- Zod or equivalent validation
- lightweight state store only where needed

## Database

**SQLite**

Potential Rust layer:

- SQLx
- rusqlite

Use migrations from the beginning.

---

# 5. Core Architecture

```text
┌────────────────────────────────────────────┐
│              React Desktop UI              │
└──────────────────────┬─────────────────────┘
                       │
              Tauri command boundary
                       │
┌──────────────────────▼─────────────────────┐
│               Application Core             │
│                                            │
│ Providers                                  │
│ Models                                     │
│ MCP Registry                               │
│ Skills Registry                            │
│ Profiles                                   │
│ Harness Inventory                          │
│ Reconciliation Engine                      │
│ History / Backups                          │
└──────────────┬─────────────────────────────┘
               │
       ┌───────┼──────────────────────────┐
       │       │                          │
       ▼       ▼                          ▼
    SQLite   Secret Store             Harness Adapters
                                          │
                        ┌─────────────────┼─────────────────┐
                        ▼                 ▼                 ▼
                     Claude             Codex              Pi
                        ▼                 ▼                 ▼
                    OpenCode          Reasonix            ...
```

---

# 6. Core Domain Model

A critical architectural rule:

```text
Provider != Endpoint != Model Route != Model Identity
```

Do not collapse these concepts.

## Provider

Examples:

- Anthropic
- OpenAI
- Z.AI
- MiniMax
- Moonshot / Kimi
- OpenRouter
- Together
- Fireworks
- local vLLM
- local SGLang
- custom provider

A provider is the organization/service identity.

## Provider Endpoint

A single provider can expose multiple APIs.

Example:

```text
Z.AI
├── OpenAI-compatible endpoint
└── Anthropic-compatible endpoint
```

Fields may include:

- base URL
- protocol
- authentication method
- credential reference
- custom headers
- query parameters
- `/models` discovery path
- enabled status

## Model Identity

Represents the underlying model concept.

Examples:

- `claude-opus-*`
- `gpt-*`
- `glm-*`
- `minimax-*`

Can be linked to `models.dev`.

## Model Route

Represents a model as served through a specific provider endpoint.

Example:

```text
Provider: OpenRouter
Endpoint: OpenAI-compatible
Remote model ID: anthropic/claude-opus-...
Canonical model: Claude Opus ...
```

Model Route contains provider-specific metadata:

- remote model ID
- context limit
- max input
- max output
- protocol
- modalities
- tool support
- reasoning support
- provider-specific aliases
- provider-specific overrides

This is what should normally appear in **My Models**.

---

# 7. Provider Management

## Provider Creation

User can add a provider manually.

Fields:

- provider name
- display name
- icon/logo optional
- notes
- website optional
- enabled

## Endpoint Creation

Each provider supports one or more endpoints.

Fields:

- endpoint display name
- base URL
- protocol
- credential source
- environment variable name
- custom headers
- model discovery endpoint
- health-check configuration

### Protocol Types

Initial set:

- OpenAI Chat Completions compatible
- OpenAI Responses compatible
- Anthropic Messages compatible
- OpenRouter-style OpenAI compatible
- custom / unknown

Potential future:

- Google Gemini
- native provider-specific transports

---

# 8. Secret Management

Do **not** use encrypted SQLite fields as the primary secret-storage mechanism.

SQLite stores only a reference.

Example:

```text
credential_type = keychain
credential_ref  = coding-harness-manager/providers/<uuid>
```

or:

```text
credential_type = env
credential_ref  = ZAI_API_KEY
```

## Native Secret Stores

### macOS

- Keychain

### Windows

- Windows Credential Manager / native credential APIs

### Linux

- Secret Service / libsecret compatible storage

## Fallback

Provide an encrypted local vault only when no native secret service exists.

## Credential Sources

User-selectable options:

1. Store securely on this computer.
2. Reference environment variable.
3. No authentication.
4. Future: command-based secret provider.

Example future command provider:

```text
op read op://Vault/ZAI/api-key
```

## Export Behavior

Default export:

- secrets excluded
- credential references retained where useful

Optional future secure export:

- secrets included
- archive encrypted with user passphrase

---

# 9. Provider Validation

When a provider endpoint is added:

1. Resolve credential.
2. Validate connectivity.
3. Validate authentication.
4. Attempt model discovery when supported.
5. Store health result.
6. Display failures clearly.

Health statuses:

- healthy
- authentication failed
- endpoint unreachable
- discovery unsupported
- rate limited
- malformed response
- unknown error

---

# 10. Model Discovery

For compatible providers, query:

```text
GET /v1/models
```

or provider-specific equivalent.

Store the provider catalog separately from My Models.

## Provider Catalog Model Fields

- endpoint ID
- remote model ID
- provider response payload
- first seen
- last seen
- availability status
- metadata
- canonical match confidence
- canonical model identity if matched

## Refresh Behavior

Never destructively delete My Models just because a model disappears from `/models`.

Instead:

```text
available
new
missing
deprecated
unknown
```

Suggested fields:

- `first_seen_at`
- `last_seen_at`
- `missing_since`
- `status`

Refresh should calculate a diff.

Example:

```text
+ model-new
= model-existing
! model-old no longer advertised
```

---

# 11. Models.dev Integration

Use `models.dev` as an upstream metadata source.

It should not be the local source of truth.

The local application owns the resolved configuration.

## Metadata Resolution Priority

Recommended:

```text
1. Explicit user override
2. Provider-specific models.dev metadata
3. Canonical models.dev metadata
4. Provider discovery metadata
5. Unknown/default
```

Store field-level provenance.

Example:

```text
context_window = 262144
source = models.dev/zai
```

or:

```text
context_window = 1048576
source = user_override
```

## Conflicting Metadata

If multiple valid values exist, show them.

Example:

```text
Context Window

○ 262,144 — Z.AI provider metadata
○ 1,048,576 — canonical model metadata
● Custom — 524,288
```

Do not silently decide where provider differences are meaningful.

---

# 12. Model Matching

Provider model IDs may not exactly match `models.dev`.

Support confidence-scored matching.

Possible matching strategies:

- exact ID
- known alias
- normalized ID
- provider mapping
- family/name heuristic
- fuzzy matching

Example confidence:

```text
100% Exact
 95% Known alias
 85% Normalized match
 60% Candidate
  0% Unknown
```

Only auto-link above a conservative threshold.

Ambiguous matches require user selection.

---

# 13. My Models

**My Models** is the user's curated list of model routes.

Models can be added by:

1. Import from provider discovery.
2. Manual creation.
3. Import from an existing harness.
4. Import from configuration backup.
5. Future: import from shared catalog.

## Manual Model Form

Fields:

- provider
- endpoint
- remote model ID
- display name
- canonical model identity
- context window
- max input
- max output
- input modalities
- output modalities
- reasoning capability
- tool support
- structured output support
- enabled
- notes

Use models.dev to prefill where possible.

---

# 14. Harness Detection

Use a registry-driven detector.

Conceptual interface:

```text
HarnessDefinition
- id
- name
- executable names
- config paths
- skill paths
- MCP paths
- platform support
```

Detection should check:

- executable availability in PATH
- known install paths
- configuration directories
- package manager metadata where appropriate
- version command

Results:

```text
Claude Code
Installed
Version: x.y.z
Path: /...
Configuration: found

Codex
Installed
Version: ...
Configuration: found

Pi
Not installed
```

---

# 15. Harness Adapter Contract

Every supported harness should implement a stable adapter interface.

Conceptually:

```text
detect()
version()

readModels()
readProviders()
readMcpServers()
readSkills()
readProfiles()

capabilities()

plan(desiredState, actualState)

apply(plan)

validate()

launch(profile)

backup()
restore()
```

Adapters must be version-aware.

Do not allow harness-specific logic to leak throughout the application.

---

# 16. Harness Capabilities

Different harnesses support different concepts.

A capabilities object can expose things such as:

```text
supports_custom_models
supports_custom_providers
supports_model_catalog
supports_profiles
supports_mcp_global
supports_mcp_project
supports_global_skills
supports_project_skills
supports_runtime_env
supports_model_aliases
supports_symlinked_skills
```

The UI should adapt based on capabilities.

---

# 17. Harness Detail Screen

Example:

```text
Claude Code

Overview
Models
MCP Servers
Skills
Profiles
Configuration
History
```

## Overview

- installation status
- executable path
- version
- config path
- last scanned
- sync status
- errors/warnings

## Models

Show native current models and managed models.

Allow:

- add from My Models
- remove
- update
- push selected models
- sync desired models
- append mode
- replace-managed mode

## MCP

Show global MCP servers.

Allow:

- import into canonical registry
- add canonical MCP
- remove binding
- enable/disable
- inspect command/env

## Skills

Show:

- native
- canonical
- symlinked
- copied
- shared
- conflicting

---

# 18. Managed vs Unmanaged Configuration

This is critical.

Coding Harness Manager should not assume it owns entire configuration files.

Track:

- managed by Coding Harness Manager
- imported but unmanaged
- native/unrecognized
- externally modified

The application should mutate the smallest possible configuration subtree.

Example:

If it manages:

```text
model_providers.my-provider
```

it should not overwrite unrelated settings such as:

```text
sandbox_mode
approval_policy
notifications
feature flags
```

---

# 19. Desired State Reconciliation

The application's core engine should operate on:

```text
Desired State
Actual State
Reconciliation Plan
```

Example:

```text
Codex

Models
+ minimax/M2.5
~ zai/glm-5       context 262k -> 1m
- old-provider/foo

MCP
+ playwright
= github
- obsolete-server

Skills
= superpowers
+ frontend-design
```

Actions:

- add
- update
- remove
- unchanged
- conflict
- unsupported

---

# 20. Append vs Replace

Every bulk operation should explicitly define behavior.

## Append

Add missing selected items.

Do not remove existing items.

Update matching managed items when fields differ.

## Replace Managed

Selected desired items become the source of truth for resources managed by Coding Harness Manager.

Do not destroy unrelated native/unmanaged configuration.

## Replace All

Potentially dangerous.

If offered at all:

- advanced mode
- strong warning
- mandatory preview
- automatic backup

---

# 21. Intelligent Deduplication

Models must not be deduplicated solely by display name.

Recommended route identity:

```text
endpoint_id + remote_model_id
```

Canonical identity can additionally link equivalent models across providers.

When pushing:

```text
existing route found
    ↓
compare fields
    ↓
update changed values
```

Do not create a duplicate if only:

- context window changed
- display name changed
- capability metadata changed

---

# 22. MCP Registry

V1 focuses on **global/user-level MCP servers**.

Schema should still support scope from the beginning.

Fields:

- name
- display name
- transport
- command
- args
- URL
- environment references
- enabled
- scope type
- scope path
- notes
- provenance

Scope:

```text
global
project
```

V1 UI can expose only global.

---

# 23. MCP Bindings

Canonical MCP definitions are independent from harness bindings.

Example:

```text
Canonical MCP: GitHub
├── Claude Code
├── Codex
└── OpenCode
```

A harness binding can contain native overrides if needed.

---

# 24. MCP Validation

Provide tests for:

- command exists
- executable launches
- environment available
- HTTP endpoint reachable
- initialization works
- duplicate MCP names
- duplicate tool names where detectable

---

# 25. Skills Registry

Recommended canonical global skill location:

```text
~/.agents/skills
```

This aligns with the emerging shared skills ecosystem.

However, each harness adapter determines how skills become available.

Possible strategies:

- native shared read
- symlink
- Windows junction
- directory link
- copy fallback
- unsupported

---

# 26. Skill Database Model

Store metadata in SQLite, but keep actual skill files on disk.

Fields:

- skill ID
- name
- canonical path
- description
- source
- source URL optional
- version/hash
- created
- modified
- provenance
- enabled

Do not store the entire skill filesystem as database blobs unless there is a strong reason.

---

# 27. Skill Import

Sources:

- existing `~/.agents/skills`
- harness skill folders
- local folder
- Git repository
- future package registry
- future remote catalog

Detect duplicate skills using:

- canonical name
- path
- content hash
- source URL

---

# 28. Skill Conflict Detection

Detect:

- duplicate names
- different content under same name
- shadowed global/project skill
- broken symlink
- missing canonical source
- incompatible target path
- unsupported symlink behavior

---

# 29. Launch Profiles

Profiles provide convenience without modifying shell startup files.

Example:

```text
Profile: Z.AI Claude

Harness: Claude Code
Provider: Z.AI

Default model: glm-5

Role mappings:
Opus   -> glm-5
Sonnet -> glm-5-air
Haiku  -> glm-4.7-flash
```

Launch behavior:

```text
resolve profile
resolve secret
construct process environment
spawn harness
```

Avoid editing `.zshrc`, `.bashrc`, PowerShell profiles, etc., whenever possible.

---

# 30. Companion CLI

Not mandatory for the first GUI release, but strongly recommended.

Potential binary:

```text
harnessctl
```

Examples:

```bash
harnessctl list
harnessctl scan
harnessctl status
harnessctl sync
harnessctl diff

harnessctl run claude --profile zai
harnessctl run codex --profile openai
```

The CLI and desktop application should use the same Rust core library.

---

# 31. Dry Run / Diff

Every mutating action should support preview.

Example:

```text
3 files will change
1 symlink will be created
2 model records will be updated
1 MCP server will be added
0 unmanaged settings will be modified
```

User can inspect native changes before Apply.

---

# 32. Atomic Writes

Never write directly to configuration files in a way that can leave them partially written.

Process:

1. Read.
2. Parse.
3. Validate.
4. Create backup.
5. Write temporary file.
6. fsync where appropriate.
7. Atomic rename/replace.
8. Re-read.
9. Validate final state.

---

# 33. Rollback

Every configuration transaction should be reversible.

Store:

- transaction ID
- timestamp
- affected harness
- before snapshot
- desired state
- applied changes
- after snapshot
- success/failure
- app version
- adapter version

UI:

```text
History

Today 08:32
Synced 5 models to Claude Code
[View Diff] [Rollback]
```

---

# 34. Drift Detection

Watch managed config files.

When a file changes externally:

1. Read.
2. Normalize.
3. Compare with last known state.
4. Identify managed-field changes.
5. Mark harness as drifted.

Statuses:

- In Sync
- Pending
- Externally Modified
- Conflict
- Error

Do not automatically overwrite external edits by default.

---

# 35. Harness Doctor

Create a diagnostics screen.

Checks:

## Harness

- executable exists
- version supported
- config readable
- config parse valid
- config writable
- backup directory writable

## Providers

- endpoint reachable
- authentication works
- `/models` works
- sample inference optional
- streaming optional
- tool calling optional
- image support optional

## MCP

- command valid
- server starts
- environment available
- initialization succeeds

## Skills

- canonical path exists
- links resolve
- duplicates
- broken links
- permissions

---

# 36. Compatibility Engine

Before pushing a route to a harness, validate protocol/capability compatibility.

Examples:

```text
Claude Code adapter requires Anthropic-compatible endpoint
```

or:

```text
Codex configuration supports this provider through OpenAI Responses
```

The engine should distinguish:

- supported
- supported with limitations
- experimental
- unsupported
- unknown

---

# 37. First-Run Import Wizard

Initial experience should be:

```text
Welcome
  ↓
Scan computer
  ↓
Found 5 harnesses
  ↓
Import current providers/models
  ↓
Import MCP servers
  ↓
Import skills
  ↓
Resolve duplicates/conflicts
  ↓
Create canonical state
```

Never overwrite anything during first-run discovery.

---

# 38. Configuration Sets / Bundles

Allow reusable collections.

Examples:

```text
Frontier
Local
Cheap
Work
Personal
Research
```

A configuration set can include:

- model routes
- MCP servers
- skills
- profiles

Apply a set to selected harnesses.

---

# 39. Provenance

Every imported object should record its source.

Examples:

```text
Imported from Pi
Imported from Codex
Discovered from Z.AI /v1/models
Matched through models.dev
Created manually
Imported from ~/.agents/skills
```

Provenance should be visible in UI.

---

# 40. Change History / Audit Log

Track meaningful local actions.

Examples:

- provider created
- endpoint changed
- model imported
- model metadata changed
- harness synced
- skill linked
- MCP added
- external drift detected
- rollback performed

This is local audit history, not telemetry.

---

# 41. Import / Export

## Export

Export configuration as a portable archive or structured JSON/YAML.

Include:

- providers
- endpoints without raw credentials
- models
- metadata overrides
- MCP definitions
- skills metadata
- profiles
- harness bindings
- configuration sets
- application preferences

By default exclude secrets.

## Import

Support:

- merge
- replace managed state
- preview conflicts

Always show diff before import.

---

# 42. Database Backup

Provide:

- Backup Now
- Restore Backup
- automatic rotating backups
- export portable configuration

Suggested automatic backup events:

- before migration
- before bulk sync
- before destructive replace
- before restore/import

---

# 43. Suggested SQLite Schema

This is an initial logical schema, not final SQL.

## `providers`

- id
- name
- display_name
- enabled
- notes
- created_at
- updated_at

## `provider_endpoints`

- id
- provider_id
- name
- base_url
- protocol
- discovery_path
- auth_type
- credential_ref_id
- headers_json
- enabled
- created_at
- updated_at

## `credential_refs`

- id
- type
- reference
- created_at
- updated_at

## `model_identities`

- id
- canonical_id
- display_name
- family
- models_dev_id
- metadata_json
- created_at
- updated_at

## `provider_catalog_models`

- id
- endpoint_id
- remote_model_id
- raw_metadata_json
- canonical_model_id
- match_confidence
- first_seen_at
- last_seen_at
- missing_since
- status

## `model_routes`

- id
- endpoint_id
- model_identity_id
- remote_model_id
- display_name
- context_window
- max_input
- max_output
- capabilities_json
- overrides_json
- enabled
- created_at
- updated_at

Unique candidate:

```text
(endpoint_id, remote_model_id)
```

## `harness_installations`

- id
- harness_type
- executable_path
- version
- config_path
- detected_at
- last_scanned_at
- status

## `harness_model_bindings`

- id
- harness_installation_id
- model_route_id
- native_id
- native_config_json
- managed
- created_at
- updated_at

## `mcp_servers`

- id
- name
- transport
- command
- args_json
- url
- env_json
- scope_type
- scope_path
- provenance_json
- enabled

## `harness_mcp_bindings`

- id
- harness_installation_id
- mcp_server_id
- native_name
- native_config_json
- managed

## `skills`

- id
- name
- canonical_path
- source_type
- source_url
- content_hash
- provenance_json
- enabled
- created_at
- updated_at

## `harness_skill_bindings`

- id
- harness_installation_id
- skill_id
- target_path
- binding_type
- managed
- status

## `launch_profiles`

- id
- name
- harness_type
- model_route_id
- provider_endpoint_id
- env_json
- role_mappings_json
- native_overrides_json
- created_at
- updated_at

## `configuration_sets`

- id
- name
- description
- created_at
- updated_at

## `configuration_set_items`

- id
- configuration_set_id
- item_type
- item_id

## `sync_transactions`

- id
- transaction_type
- started_at
- completed_at
- status
- summary
- plan_json
- error_json

## `config_snapshots`

- id
- transaction_id
- harness_installation_id
- path
- before_content
- after_content
- before_hash
- after_hash

---

# 44. Suggested Navigation

```text
Dashboard

Providers
Models
Harnesses
MCP Servers
Skills
Profiles
Sets

Changes
History

Doctor

Settings
```

---

# 45. Dashboard

Useful information:

- detected harness count
- configured provider count
- My Models count
- MCP count
- skill count
- harnesses with drift
- provider health issues
- pending changes
- recently changed configuration

Quick actions:

- Scan Harnesses
- Add Provider
- Refresh Models
- Sync
- Run Doctor

---

# 46. Providers Screen

List:

```text
Z.AI
2 endpoints
14 discovered models
12 My Models
Healthy

MiniMax
1 endpoint
5 discovered models
Authentication failed
```

Provider detail:

- overview
- endpoints
- credentials
- discovered models
- My Models
- health
- refresh

---

# 47. Models Screen

Tabs:

- My Models
- Discovered
- Missing/Deprecated

Filters:

- provider
- protocol
- capability
- context window
- multimodal
- reasoning
- availability

Bulk actions:

- Add to My Models
- Push to Harnesses
- Add to Set
- Disable

---

# 48. Push Models Workflow

Example:

```text
Selected Models: 12

Target Harnesses:
[x] Claude Code
[x] Codex
[x] OpenCode
[x] Pi
[ ] Reasonix

Mode:
(o) Append/update
( ) Replace managed models

[Preview Changes]
```

Then show harness-specific diff.

---

# 49. Push Everything Workflow

Convenience feature:

```text
Sync My Models to Harnesses
```

Select:

- all My Models
- selected providers
- selected models

Select targets:

- all supported detected harnesses
- specific harnesses

Mode:

- append/update
- replace managed

Always preview.

---

# 50. Duplicate Handling

Model duplicate rules must understand:

```text
same endpoint
same remote model ID
```

When found:

- compare normalized settings
- update changed values
- preserve unchanged bindings

Do not create a duplicate if only:

- context window changed
- display name changed
- capability metadata changed

---

# 51. File Watching

Watch:

- supported harness config files
- canonical skill directory
- relevant harness skill directories
- potentially MCP config files

Debounce updates.

Do not immediately reconcile on every change.

Instead update status and offer action.

---

# 52. Cross-Platform Filesystem Strategy

## macOS/Linux

Use symlinks where appropriate.

## Windows

Support:

- directory junctions
- symbolic links where permissions allow
- copy fallback

The adapter/filesystem layer should abstract:

```text
linkDirectory(source, target)
```

rather than making UI/domain code aware of OS details.

---

# 53. Windows Build Strategy

Windows is manageable with Tauri.

Use GitHub Actions to build release artifacts natively.

Matrix:

```text
macOS
Windows
Linux
```

Do not rely on cross-compiling everything from macOS.

Example release pipeline:

```text
push tag
   ↓
GitHub Actions matrix
   ├── macOS runner
   ├── Windows runner
   └── Linux runner
   ↓
build + test
   ↓
sign where configured
   ↓
GitHub Release
```

Likely output formats:

### macOS

- `.dmg`
- `.app`

Future:

- signing
- notarization

### Windows

- `.msi`
- NSIS `.exe` if desired

Future:

- code signing certificate

### Linux

Potential:

- `.AppImage`
- `.deb`
- `.rpm`

Do not block the first open-source release on paid code signing.

Clearly document unsigned-build warnings until signing is available.

---

# 54. GitHub Actions

Suggested workflows:

```text
ci.yml
release.yml
nightly.yml          optional later
dependency-audit.yml
```

## CI

Run on PR:

- Rust format
- Rust clippy
- Rust tests
- TypeScript lint
- TypeScript tests
- frontend build
- adapter fixture tests
- migration tests

## Release

Triggered by tag.

Build natively on each OS.

Attach artifacts to GitHub Releases.

---

# 55. Adapter Fixture Testing

Harness adapters are the riskiest part of the system.

Maintain fixtures for multiple known versions.

Example:

```text
fixtures/
  claude/
    version-x/
    version-y/
  codex/
    version-x/
  opencode/
  pi/
  reasonix/
```

Tests:

```text
native config
   ↓
parse
   ↓
normalized state
   ↓
modify desired state
   ↓
serialize
   ↓
expected native config
```

Use golden/snapshot tests heavily.

---

# 56. Schema / Config Version Detection

Native harness formats change.

Each adapter should detect:

- harness version
- config schema version if possible
- supported serializer

If unsupported:

```text
Harness version newer than tested adapter.
Read-only mode recommended.
```

Avoid destructive writes to unknown formats.

---

# 57. Read-Only Safety Mode

For unsupported or newly detected harness versions:

- allow inspection
- allow import into central state
- disable writes until adapter compatibility is known

Advanced users may override with warning.

---

# 58. Plugin / Adapter Architecture

Longer term, external contributors should be able to add harness support.

Possible structure:

```text
crates/
  core
  database
  secrets
  reconciliation
  provider
  models-dev
  filesystem
  harness-sdk

adapters/
  claude-code
  codex
  opencode
  pi
  reasonix
```

Initially compile adapters into the application.

Do not rush into dynamic plugin loading in V1.

A stable internal adapter SDK is enough initially.

---

# 59. Recommended Repository Structure

```text
coding-harness-manager/
├── apps/
│   └── desktop/
│       ├── src/
│       └── src-tauri/
│
├── crates/
│   ├── core/
│   ├── database/
│   ├── secrets/
│   ├── reconciliation/
│   ├── providers/
│   ├── models-dev/
│   ├── filesystem/
│   └── harness-sdk/
│
├── adapters/
│   ├── claude-code/
│   ├── codex/
│   ├── opencode/
│   ├── pi/
│   └── reasonix/
│
├── fixtures/
│
├── docs/
│   ├── architecture/
│   ├── adapters/
│   └── development/
│
├── .github/
│   └── workflows/
│
├── LICENSE
├── README.md
├── CONTRIBUTING.md
├── SECURITY.md
└── CODE_OF_CONDUCT.md
```

Exact Tauri workspace structure can be adjusted during implementation.

---

# 60. Open Source License

Recommended initial choice:

**Apache-2.0** or **MIT**

MIT offers minimal friction.

Apache-2.0 offers explicit patent protections.

A final license decision should be made before meaningful outside contributions begin.

---

# 61. Privacy

Default policy should be straightforward:

- no model prompts are read
- no conversation data is collected
- no source code is uploaded
- no API keys leave the machine except to the provider they belong to
- no analytics by default

If telemetry is ever introduced:

- strictly opt-in
- transparent
- never include secrets/config content

---

# 62. Logging

Logs must redact:

- API keys
- authorization headers
- bearer tokens
- secret environment variables

Provide:

- normal logs
- debug logs
- export diagnostic bundle

Diagnostic bundle must perform secret redaction.

---

# 63. Error Handling

Errors should be actionable.

Bad:

```text
Error 500
```

Good:

```text
Codex config could not be updated.

The existing TOML contains a field this adapter version
does not recognize.

No files were modified.

[View Details]
```

---

# 64. Ten High-Value Additional Features

## 1. Dry-Run Diff + Atomic Rollback

Every mutation is previewable and reversible.

## 2. Drift Detection

Detect changes made outside Coding Harness Manager.

## 3. Launch Profiles

Launch harnesses using provider/model/environment combinations without modifying shell startup files.

## 4. Harness Compatibility Engine

Prevent invalid model/provider/harness combinations.

## 5. Provider/Harness Doctor

One-click diagnostics.

## 6. Versioned Harness Adapter System

Keep configuration logic isolated and version aware.

## 7. First-Run Import Wizard

Turn existing local state into canonical managed state safely.

## 8. Configuration Sets

Apply groups of models/MCPs/skills together.

## 9. Provenance + Change History

Know where every configuration came from and how it changed.

## 10. Conflict / Collision Analyzer

Detect model, MCP, skill, and path collisions before sync.

---

# 65. Future Features

Potential post-V1 features:

- configuration sync across computers
- Git-backed configuration storage
- team/shared configurations
- remote provider catalogs
- community adapter registry
- community provider templates
- project-scoped MCP management
- project-scoped skill management
- model benchmark metadata
- model pricing comparison
- provider latency benchmarking
- provider failover profiles
- agent rules/instruction management
- IDE integrations
- auto-update application
- auto-update adapter metadata
- encrypted cloud backup
- secret-manager integrations
- SSH/remote machine harness management
- WSL-aware support
- Nix/Homebrew/Chocolatey/Scoop packaging

---

# 66. Phased Implementation

## Phase 0 — Research and Fixtures

Before UI implementation:

- document native configs for Tier 1 harnesses
- collect representative fixtures
- document global model behavior
- document MCP behavior
- document global skill paths
- document model/provider overrides
- document launch behavior
- identify version differences

Deliverable:

```text
docs/harnesses/<harness>.md
```

---

## Phase 1 — Core + Database

Build:

- Rust workspace
- SQLite migrations
- domain entities
- repository/data layer
- secret reference abstraction
- models.dev client
- provider client abstraction

No complicated UI yet.

---

## Phase 2 — Harness Detection

Build:

- harness registry
- executable detection
- config path detection
- version detection
- platform abstractions

Output normalized inventory.

---

## Phase 3 — Read-Only Adapters

Implement Tier 1 adapters in read-only mode.

Each adapter can:

- detect
- identify version
- read configuration
- normalize models
- normalize MCP
- normalize skills
- normalize profiles

Do not write yet.

This allows safe validation against real systems.

---

## Phase 4 — First-Run Import

Build UI to:

- scan harnesses
- view imported state
- merge duplicates
- create canonical providers
- create canonical models
- import MCP
- import skills

---

## Phase 5 — Provider Management

Build:

- provider CRUD
- endpoint CRUD
- native secret storage
- environment-variable references
- health checks
- `/models` discovery
- provider model catalog

---

## Phase 6 — My Models

Build:

- model import
- manual model creation
- models.dev matching
- context/output selection
- model provenance
- duplicate handling

---

## Phase 7 — Reconciliation Engine

Implement:

```text
desired + actual -> plan
```

Support:

- add
- update
- remove
- conflict
- unsupported
- no-op

No writes without preview.

---

## Phase 8 — Writable Adapters

For each adapter:

- plan native changes
- make backup
- atomic write
- validate
- rollback

Enable one harness at a time.

Suggested order:

1. OpenCode
2. Pi
3. Codex
4. Claude Code
5. Reasonix

Actual order can change based on research complexity.

---

## Phase 9 — MCP Management

Build:

- canonical MCP registry
- MCP bindings
- global scope
- native translation
- diagnostics
- deduplication

---

## Phase 10 — Skills Management

Build:

- canonical `~/.agents/skills`
- import
- hashing
- binding strategies
- symlink/junction/copy abstraction
- conflict detection

---

## Phase 11 — Profiles + Launcher

Build:

- launch profiles
- environment injection
- role mappings
- process spawning
- launch history
- command copy

---

## Phase 12 — Drift + History

Build:

- file watchers
- managed/unmanaged tracking
- snapshots
- transaction history
- rollback UI

---

## Phase 13 — Doctor

Build:

- harness diagnostics
- provider diagnostics
- MCP diagnostics
- skill diagnostics
- diagnostic export

---

## Phase 14 — Cross-Platform Packaging

Test:

- macOS
- Windows
- Linux

Build GitHub Actions release matrix.

Create installation documentation.

---

# 67. V1 Acceptance Criteria

A V1 release is successful if a new user can:

1. Install the desktop application.
2. Scan their machine.
3. See installed supported harnesses.
4. Import existing supported configuration without changing files.
5. Add a provider.
6. Store or reference an API key securely.
7. Validate the provider.
8. Discover models.
9. Import selected models into My Models.
10. Enrich metadata using models.dev.
11. Select models.
12. Select harnesses.
13. Preview native configuration diffs.
14. Apply changes safely.
15. Configure global MCP servers.
16. Manage global skills.
17. Use canonical shared skill storage.
18. Create a launch profile.
19. Detect external changes.
20. Roll back a previous sync.

---

# 68. V1 UX Principle

The application should always answer three questions clearly:

```text
What do I have?

What do I want?

What will change?
```

No destructive change should surprise the user.

---

# 69. Repository Name

**Coding Harness Manager**

Suggested repository slug:

```text
coding-harness-manager
```

Suggested GitHub description:

> Cross-platform manager for Claude Code, Codex, OpenCode, Pi and other AI coding harnesses. Sync models, providers, MCP servers, skills and profiles from one place.

Suggested topics:

```text
ai-coding-agents
coding-agents
coding-harness
claude-code
codex
opencode
pi
reasonix
mcp
model-context-protocol
agent-skills
models-dev
llm
tauri
desktop-app
rust
```

---

# 70. Proposed README Positioning

```text
# Coding Harness Manager

Manage models, providers, MCP servers, skills, and profiles across
Claude Code, Codex, OpenCode, Pi, Reasonix, and other AI coding harnesses
from one desktop application.

Configure once. Preview the diff. Sync everywhere.
```

---

# 71. Core Architectural Decisions to Lock Before Implementation

These decisions should be treated as foundational:

## Decision 1

Use:

```text
Provider
  ↓
Endpoint
  ↓
Model Route
  ↓
Model Identity
```

Do not flatten provider/model relationships.

## Decision 2

Use:

```text
Desired State
Actual State
Plan
Apply
Verify
Rollback
```

as the central synchronization model.

## Decision 3

All harness integration occurs through a version-aware adapter contract.

## Decision 4

Secrets are references to OS-native secret stores or environment variables.

## Decision 5

Coding Harness Manager owns only explicitly managed configuration.

## Decision 6

All writes are previewable, backed up, atomic, and reversible.

## Decision 7

`~/.agents/skills` is the preferred canonical global skill source, while adapters control how each harness consumes those skills.

---

# 72. Questions to Resolve During Detailed Design

These should be answered before implementation reaches the relevant subsystem:

- MIT vs Apache-2.0?
- Exact binary/CLI name?
- Should the app bundle `harnessctl` in V1?
- How should unknown harness versions behave?
- Should model discovery support provider-specific adapters immediately?
- Should OpenRouter-style provider metadata receive special handling?
- What is the minimum supported Windows version?
- What is the minimum supported macOS version?
- Which Linux distributions/formats are officially supported?
- Should config history store full file snapshots or patches plus snapshots?
- How long should history/backups be retained?
- Should canonical skills be opt-in or enabled by default?
- Should project-scoped resources appear as read-only in V1?
- Should the app automatically refresh provider model catalogs?
- How should provider aliases be shared/imported?
- Should configuration exports use JSON, YAML, or a zipped portable format?

---

# 73. Guiding Principle

Coding Harness Manager should make native harness configuration **safer and easier**, not hide it.

The native configuration remains authoritative for the harness itself.

Coding Harness Manager provides:

```text
Discovery
Normalization
Desired State
Reconciliation
Translation
Validation
Safety
Convenience
```

That separation should keep the project maintainable as the number of supported coding harnesses grows.
