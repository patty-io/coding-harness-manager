# OpenCode Native Credential Sync Design

**Date:** 2026-08-31  
**Status:** Approved direction; implementation pending

## Problem

CHM can import an authenticated custom provider from Pi, store a literal API
key in the OS credential store, and sync the provider and its models into
OpenCode. The resulting OpenCode provider is still unusable because the sync
layer currently exports only environment-variable credential references.
Keychain-backed credentials stop at the CHM boundary.

For example, Yolo-Auto reaches OpenCode with the correct provider ID, base URL,
runtime package, and `qwen3.8-27b` model, but without either an `options.apiKey`
value or a `yolo-auto` record in OpenCode's native credential store.

## Goals

- Sync a Keychain-backed custom provider and its models to OpenCode entirely
  through CHM.
- Use OpenCode's native provider and credential formats.
- Keep API-key values out of SQLite, adapter state, previews, plan hashes,
  diffs, snapshots, history, logs, diagnostics, and exports.
- Preserve unrelated OpenCode configuration and credentials.
- Make credential deployment transactional with the existing config sync.
- Present every format-aware harness as supported in the public README without
  an artificial primary/additional adapter distinction.

## Non-goals

- Replacing OpenCode's authentication system.
- Storing OAuth credentials imported from one product into another.
- Executing command-backed credentials found in harness configuration.
- Adding a general-purpose secret export format.
- Claiming that every harness supports every configuration surface.

## OpenCode Contract

Current OpenCode 1.18 uses two native files for this flow:

- `~/.config/opencode/opencode.jsonc` contains a custom provider keyed by its
  provider ID. The provider contains the runtime package, base URL, and models.
- `~/.local/share/opencode/auth.json` contains the provider credential keyed by
  the same provider ID, with the shape `{ "type": "api", "key": "..." }`.

The provider ID is the join key. One credential serves every model nested
under that provider. CHM must therefore write `yolo-auto` consistently in both
places; it must not create model-level copies of the key.

OpenCode itself writes `auth.json` with mode `0600`. CHM will follow that
native contract instead of embedding literal keys in `opencode.jsonc`.

## Architecture

### Safe planning

Preview and plan construction remain secret-free. Desired model routes keep
only endpoint IDs and safe provider metadata. A preview may report that a
provider credential will be configured, but it must never resolve or serialize
the value.

### Credential deployment task

When applying a model sync to OpenCode, the sync orchestrator derives a
deduplicated credential task for each selected provider endpoint:

```text
provider ID + endpoint credential reference + OpenCode auth path
```

The task contains no resolved value until apply time. Environment-backed
credentials retain the existing `{env:NAME}` configuration behavior. For a
Keychain-backed API credential, apply resolves the value through CHM's
`SecretStore` and prepares a native OpenCode auth entry.

Missing or unreadable credentials are blockers. CHM must fail before changing
OpenCode rather than install an unusable provider silently.

### Transaction order

1. Build and validate the secret-free native configuration plan.
2. Resolve all required credential references as an apply-time preflight.
3. Read and validate OpenCode's existing `auth.json`; retain its prior bytes in
   memory only.
4. Apply the normal OpenCode configuration plan using existing backups and
   atomic writes.
5. Merge the selected provider credentials into the in-memory auth document.
6. Atomically replace `auth.json` with mode `0600`.
7. Re-read both files and validate that the provider ID, models, and credential
   entry exist.
8. Finish the transaction only after both surfaces validate.

If any step after the config write fails, restore the normal config backup and
restore the previous auth document from memory. No secret-bearing backup or
snapshot is written to CHM's database or `.chm-backups` directory.

### Native credential writer

The credential writer will:

- accept a parsed JSON object, provider ID, and resolved API key;
- normalize a trailing slash from the provider ID in the same manner as
  OpenCode;
- preserve every unrelated provider entry byte-for-byte where possible and
  semantically when JSON serialization is required;
- replace only the selected provider's API credential;
- reject malformed/non-object auth documents rather than treating them as
  empty;
- use the filesystem safety layer for atomic replacement;
- enforce owner-read/write permissions (`0600`) on Unix;
- avoid debug formatting of secret-bearing structures.

The auth file is a harness-native credential store, not a CHM persistence
store. `SECURITY.md` will explicitly distinguish OS-native storage inside CHM
from credentials deliberately deployed to a harness's own protected store.

### Concurrency

CHM will serialize its own OpenCode credential writes and use atomic replace to
prevent partial files. OpenCode does not currently expose an offline,
non-interactive credential-set CLI, so CHM cannot coordinate a shared lock with
an independently running OpenCode process. Immediately before replacement,
CHM will verify that the file has not changed since it was read; a mismatch
aborts safely and asks the user to retry instead of overwriting concurrent
changes.

## Error handling

- **Missing CHM credential:** block apply with the provider and endpoint name,
  never a reference value or secret.
- **Malformed `auth.json`:** block apply and leave both files unchanged.
- **Concurrent auth change:** block replacement and roll back the config write.
- **Permission failure:** roll back the config write and retain the original
  auth file.
- **Post-write validation failure:** restore both surfaces and mark the
  transaction failed.
- **Environment reference not set:** retain the existing environment-reference
  behavior and surface the missing variable during preflight.

## Testing

Tests will be written before implementation and must prove:

1. A Keychain-backed Yolo-Auto endpoint creates an OpenCode credential task.
2. The native writer adds `yolo-auto` with `{ type: "api", key: ... }`.
3. Existing OpenCode credentials are preserved.
4. Updating Yolo-Auto replaces only that provider's key.
5. The auth file is written atomically with `0600` permissions.
6. Malformed auth JSON and concurrent modification fail without data loss.
7. A failed config or credential write restores both original documents.
8. Previews, snapshots, logs, and reports never contain the key.
9. One provider credential serves multiple custom models without duplicate
   entries.
10. The Pi -> CHM -> OpenCode regression produces a configured `yolo-auto`
    provider, `qwen3.8-27b` model, and matching credential.

The full Rust workspace, frontend tests, lint, production build, and public
secret scan remain release gates.

## Documentation changes

The English and Korean READMEs will show one supported-harness list:

Claude Code, Codex, OpenCode, Pi, Reasonix, Gemini CLI, Qwen Code, Kimi CLI,
Cursor, Cline, Roo Code, Aider, Amp, Goose, and Continue.

The existing capability explanation remains: support is per native surface,
and CHM does not invent unsupported writes. Documentation will also state that
credential sync targets a harness's protected native credential store when the
harness requires one.

## Acceptance criteria

- Starting from the currently imported Yolo-Auto endpoint, a CHM sync to
  OpenCode requires no `/connect`, manual environment variable, or manual file
  edit.
- `opencode auth list` reports `yolo-auto` after sync.
- `yolo-auto/qwen3.8-27b` can authenticate using the deployed provider key.
- No API-key value appears in CHM's database, preview, history, logs, backups,
  or Git working tree.
- Existing OpenCode credentials remain intact.
- Both README languages describe all format-aware adapters as supported.
