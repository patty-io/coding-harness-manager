# Provider Route Portability Design

**Date:** 2026-08-31  
**Status:** Approved; implementation pending

## Product invariant

Coding Harness Manager exists to make a configured model route portable. A
route is not merely a model ID. It is the complete usable unit:

```text
provider identity + protocol + endpoint + credential + model metadata
```

When CHM deploys a route to a harness, the harness must be able to select the
model and authenticate a request without a second manual setup step. A sync
that writes a provider or model but omits its credential is a failed sync, not
a partial success.

If a target harness cannot represent the route's protocol, provider topology,
credential mechanism, or model metadata, CHM must block the operation before
writing and explain the native limitation. It must never silently substitute a
`custom` provider, drop a credential, or claim success for an unusable model.

## Problem

CHM's normalized library correctly associates a model route with a provider
endpoint and a credential reference. The sync boundary loses that association:

- `HarnessCapabilities` has booleans for custom models and providers, but no
  credential-deployment or protocol capabilities.
- desired-state construction exports only environment-variable credential
  references; Keychain, Credential Manager, and libsecret references disappear.
- reconciliation plans models independently rather than planning a complete
  provider route.
- adapter writers improvise provider IDs and credential behavior. Several use
  `custom` as a fallback.
- validation checks whether configuration parses, not whether the selected
  model resolves through the expected provider with a usable credential.

The observed Yolo-Auto failure is one instance of this architectural gap. Pi
contained a valid `Yolo-Auto` provider, key, endpoint, and `qwen3.8-27b`. CHM
imported them, then wrote the provider and model to OpenCode without deploying
the Keychain-backed key. OpenCode displayed the model but could not use it.

## Goals

- Treat provider, credential, endpoint, protocol, and selected models as one
  transactional deployment unit.
- Make a CHM-only deployment usable without editing shell profiles, running a
  harness login command, or manually copying a key.
- Use each harness's documented native format and credential mechanism.
- Resolve secret values only during apply, never during preview or hashing.
- Preserve unrelated native configuration and credentials.
- Back up, atomically write, validate, and roll back every affected surface.
- Report incompatibility before mutation with a precise, user-facing reason.
- Remove the artificial primary/additional adapter distinction. All registered
  adapters are supported for the native surfaces their adapter implements,
  with truthful per-route capability reporting.

## Non-goals

- Pretending every harness accepts every API protocol or arbitrary endpoint.
- Converting OAuth sessions between products.
- Executing credential commands imported from an untrusted harness config.
- Modifying `.zshrc`, `.bashrc`, or another global shell profile.
- Persisting plaintext credentials in SQLite, logs, previews, history, or
  ordinary CHM backups.
- Maintaining a generic plaintext secret export format.

## Native capability model

The current booleans are insufficient. Each adapter will declare a structured
route-deployment capability:

```rust
struct RouteDeploymentCapabilities {
    provider_topology: ProviderTopology,
    protocols: Vec<Protocol>,
    credential_targets: Vec<CredentialTarget>,
    model_identity: ModelIdentityRules,
    metadata: ModelMetadataCapabilities,
}

enum ProviderTopology {
    Multiple,
    SingleGlobalOverride,
    FixedProvider,
    None,
}

enum CredentialTarget {
    NativeSecretStore,
    CommandHelper,
    HarnessEnvFile,
    ProtectedConfig,
    ManagedRemoteApi,
}
```

The declaration describes what CHM can actually write and validate, not merely
what the upstream product can do through an interactive UI.

“Supported harness” therefore means CHM has a maintained, format-aware adapter
for at least one native surface. It does not mean every operation is compatible
with every route. The UI exposes model-route sync only when the adapter can
deploy the complete selected bundle; it does not show a disabled or knowingly
partial sync action and call that support.

Before reconciliation, a compatibility check evaluates the complete route:

- provider topology and uniqueness;
- wire protocol (OpenAI Chat, OpenAI Responses, Anthropic Messages, Gemini,
  or another explicitly supported protocol);
- endpoint shape and required headers;
- credential kind and available native delivery target;
- model identifier, context window, and required capabilities.

An incompatible route becomes a blocker with a concrete reason, such as
"Codex requires an OpenAI Responses-compatible endpoint" or "Cursor exposes
only one global OpenAI override and cannot install this second provider."

## Portable route bundle

Desired state will contain normalized, secret-free provider bundles rather
than only a flat list of model routes:

```rust
struct ProviderRouteBundle {
    provider_id: String,
    display_name: String,
    endpoint_id: Uuid,
    base_url: String,
    protocol: Protocol,
    credential: CredentialRequirement,
    models: Vec<ModelRoute>,
}

struct CredentialRequirement {
    credential_ref: CredentialRef,
    auth_type: AuthType,
}
```

`CredentialRef` contains only the existing opaque OS-store or environment
reference. The resolved value is never serializable and never enters a
reconciliation action, `NativePlan`, plan hash, preview, diagnostic, or log.

Provider identity is derived from the CHM provider attached to the endpoint.
Adapters may normalize that identity according to documented native rules, but
must not replace it with `custom` merely because metadata is missing.

## Secret-free plan and protected apply

Native plans will distinguish ordinary changes from protected changes:

```rust
struct NativePlan {
    changes: Vec<NativeChange>,
    protected_changes: Vec<ProtectedChangePlan>,
    links: Vec<NativeLink>,
    warnings: Vec<String>,
}
```

`ProtectedChangePlan` records only safe metadata: target path or native store,
provider ID, operation, and credential reference. It never contains before or
after bytes.

At apply time, the orchestrator:

1. validates every selected bundle against the adapter capability declaration;
2. builds the secret-free native plan and preview;
3. resolves every required credential as a preflight through `SecretStore`;
4. aborts before mutation if any credential is missing or unreadable;
5. captures protected native state in memory;
6. applies ordinary and protected changes atomically where the native format
   permits, or as a coordinated transaction otherwise;
7. re-reads the harness and performs route-level validation;
8. commits history only after provider, credential, and model all validate;
9. restores every changed surface if any write or validation step fails.

Secret-bearing before/after documents are not written to SQLite or ordinary
backup files. Durable undo for a protected document stores structural,
redacted history plus any required secret material in the OS credential store,
referenced by an opaque snapshot ID. Removing a history entry also removes its
secret snapshot. Secret snapshots inherit the history record's retention and
are deleted on successful rollback, history deletion, provider deletion, or
retention expiry. Failed transactions remove newly created secret snapshots.
Transaction-local rollback uses in-memory bytes only.

## Native credential strategies

CHM selects the safest documented mechanism supported by each adapter. The
strategy is part of the adapter contract, not guessed in the sync command.

### Native secret stores

Use a harness-owned protected store when it has a stable documented contract.
OpenCode, for example, stores provider credentials in
`~/.local/share/opencode/auth.json`, keyed by the same provider ID used in
`opencode.jsonc`. CHM writes the API credential entry atomically with mode
`0600`, preserves unrelated entries, and validates the join across both files.

### Command helpers

Where a harness documents command-backed credential resolution, CHM writes a
provider-scoped command reference to a small CHM credential helper. The helper
reads the opaque reference from the OS secret store at runtime and prints only
the requested credential to the child process. This avoids shell-profile edits
and plaintext config. Codex's provider `auth.command`, Claude Code's
`apiKeyHelper`, and compatible Pi credential commands use this strategy when
their protocol contract matches the route.

The helper accepts only a fixed credential-reference argument, never an
arbitrary command from imported configuration. It emits no diagnostics on
stdout and redacts references from errors. The executable and generated helper
configuration must be owned by the current user and non-writable by other
users. Helper access is limited to credential references already bound to the
calling harness deployment; it is not a general key-dump interface.

### Harness-specific environment files

Use an isolated, protected environment file only when the harness officially
auto-loads it. Examples include Qwen Code's `~/.qwen/.env`, Continue's
`~/.continue/.env`, Reasonix's home `.env`, and Aider's supported `.env` path.
CHM assigns a collision-resistant provider variable name, writes the native
model/provider config to reference it, and writes the secret file with `0600`
permissions. It never edits a user's shell profile or general project `.env`.

### Protected native configuration

Some harnesses require credentials in their own main config. Kimi Code, for
example, resolves provider credentials from `config.toml` rather than the
ambient shell. CHM may write that documented field, but the entire file becomes
a protected change: previews and history are redacted, permissions are
restricted, and rollback follows the protected-state rules above.

### Managed remote APIs

If a product exposes a supported API for provider credentials rather than a
stable local format, the adapter uses that API and validates the returned
provider state. Secrets are submitted only to the product's documented
endpoint. A UI-only or undocumented internal store is not an acceptable write
target.

## Harness compatibility baseline

Official documentation and upstream source establish the following baseline.
This is a route compatibility matrix, not a marketing tier:

| Harness | Native route mechanism | CHM deployment rule |
|---|---|---|
| OpenCode | Custom providers/models plus provider-keyed `auth.json` | Full bundle through config + native auth store |
| Pi | Multiple providers/models; protocol, endpoint, and credential fields | Full bundle; prefer command helper, protected config only when required |
| Kimi CLI | Multiple providers/models in `config.toml`; several protocols | Full bundle through protected config |
| Qwen Code | `modelProviders`, `providerProtocol`, provider `envKey`; auto-loaded `.qwen/.env` | Full bundle through settings + harness env file |
| Reasonix | `[[providers]]` plus Reasonix home `.env` | Full bundle through config + harness env file |
| Codex | Custom providers with explicit wire protocol and command/env auth | Full bundle only for protocols Codex supports |
| Claude Code | Anthropic-compatible gateway and model environment settings; `apiKeyHelper` | Full bundle only for Claude Code gateway-compatible routes |
| Continue | Model entries with provider, model, API base, and secrets/env resolution | Full bundle through config + Continue secret/env mechanism |
| Aider | LiteLLM model route plus supported `.env`/config credentials | Full bundle for protocols supported by Aider/LiteLLM |
| Goose | Provider/model configuration with native provider credential handling | Full bundle where the installed Goose provider backend supports the protocol |
| Cline | OpenAI-compatible and named provider profiles with API key, base URL, model | Block route deployment until CHM has a documented extension/API integration; never write VS Code secret storage directly |
| Roo Code | OpenAI-compatible and named provider profiles | Block route deployment until CHM has a documented extension/API integration; never write VS Code secret storage directly |
| Gemini CLI | Google model/auth configuration; no general arbitrary OpenAI provider contract | Block incompatible custom routes; support Gemini-compatible routes |
| Cursor | Curated BYOK providers and a single global OpenAI base override | Deploy only compatible single-provider routes; block topology conflicts |
| Amp | Curated provider keys and managed model routing; no arbitrary local endpoint contract | Deploy only through supported managed API; block arbitrary endpoints |

This matrix is versioned behavior. Adapter validation must gate on installed
versions where upstream formats change. Documentation links used for the
baseline include the official [OpenCode provider guide](https://opencode.ai/docs/providers/),
[Qwen model-provider guide](https://qwenlm.github.io/qwen-code-docs/en/users/configuration/model-providers/),
[Kimi provider guide](https://moonshotai.github.io/kimi-code/en/configuration/providers),
[Claude Code gateway guide](https://docs.anthropic.com/en/docs/claude-code/llm-gateway),
[Continue provider guide](https://docs.continue.dev/customize/model-providers/top-level/openai),
[Aider credential guide](https://aider.chat/docs/config/api-keys.html), and
[Cursor BYOK guide](https://docs.cursor.com/settings/api-keys).

## Validation and success semantics

Parsing the resulting file is necessary but not sufficient. Apply succeeds
only when post-write validation proves:

- the expected native provider exists;
- its endpoint and protocol match the selected CHM endpoint;
- the expected model exists under that provider or native route;
- the credential target exists and can be resolved without exposing its value;
- no unrelated provider/model/credential entry was removed;
- protected files have restrictive permissions where applicable.

Before native mutation, CHM also verifies the endpoint credential through an
authenticated, non-billable provider operation such as model discovery when
the endpoint supports one. After mutation, where a harness exposes a safe
configuration or auth-status command, the adapter uses it to prove that the
native credential path resolves. When neither check exists, the UI explicitly
reports that only structural validation was possible. CHM must not send a paid
inference request merely to prove sync. The result shown to the user is one of:

- **Ready:** the complete route is installed and validated;
- **Blocked:** nothing was written; the target cannot represent the route or a
  required credential is unavailable;
- **Failed and rolled back:** a write or validation failed and prior native
  state was restored.

There is no successful partial state.

## Conflict and ownership rules

- Provider identity is unique within the target harness after native
  normalization.
- A model identity cannot be duplicated under the same native provider.
- An existing unmanaged provider with the same ID but a different endpoint or
  protocol is a conflict, not an update.
- Append mode preserves unrelated and unmanaged routes.
- Replace Managed removes only entries with durable CHM bindings.
- Credential rotation updates only the selected provider's credential target.
- Multiple models on one endpoint share one provider credential deployment.

## Security policy change

`SECURITY.md` currently says API keys are stored only in OS-native secret stores
or referenced by environment name. That remains true for CHM's own persistence,
but it is incomplete for an application whose job is to configure other
products.

The policy will distinguish:

- **CHM persistence:** secret values remain in OS-native stores; SQLite stores
  opaque references only.
- **Explicit native deployment:** with user-approved sync, CHM may materialize
  a credential into a harness's documented protected secret store, isolated
  environment file, protected config, or managed credential API when that is
  required for the route to work.
- **Audit surfaces:** values never enter previews, hashes, logs, diagnostics,
  ordinary backups, exports, or Git-managed files.

## User experience

“Sync from library” previews complete route bundles. Each selected model shows
its provider, protocol, credential destination, and target harness. The key
itself is never shown.

Examples:

```text
Yolo-Auto / qwen3.8-27b
OpenAI Chat -> OpenCode provider yolo-auto
Credential -> OpenCode native auth store
Result -> Ready after apply
```

```text
Yolo-Auto / qwen3.8-27b
OpenAI Chat -> Codex
Blocked -> this Codex version requires the Responses protocol
```

The UI does not offer Apply while any selected bundle is blocked. It gives the
user a direct explanation rather than a generic “run Doctor” instruction.

## Migration

- Existing model routes remain valid; desired-state construction groups them
  by endpoint into provider bundles.
- Existing environment-backed endpoints retain their references and are
  migrated to the adapter's declared native strategy.
- Existing CHM bindings gain provider endpoint and credential-target metadata
  on the next successful sync.
- Existing harness rows attributed to `custom` are re-read. Where the endpoint
  matches a CHM provider, CHM previews a provider-ID repair; it does not rewrite
  automatically without apply.
- Previously successful but incomplete bindings are marked `needsRepair` until
  route-level validation passes.

## Test strategy

Implementation is test-driven. Tests must fail before production changes and
cover:

1. desired state groups multiple models on one endpoint into one provider
   bundle and one credential requirement;
2. Keychain, Credential Manager, libsecret, and environment references survive
   planning without resolving their values;
3. previews, plan hashes, reports, logs, snapshots, and diagnostics contain no
   credential values;
4. missing credentials and protocol mismatches block before any write;
5. every adapter's declared capability matrix matches fixture behavior;
6. native writers preserve unrelated providers, models, credentials, comments,
   and user settings;
7. protected writes use atomic replacement and restrictive permissions;
8. a failure on any surface restores every ordinary and protected change;
9. route-level validation rejects provider/model-without-credential states;
10. no adapter falls back to `custom` when a canonical provider is known;
11. Pi -> CHM -> OpenCode deploys Yolo-Auto, `qwen3.8-27b`, and one matching
    native credential with no manual step;
12. the same fixture deploys through every compatible adapter and produces an
    explicit blocker for each incompatible adapter;
13. credential rotation updates one provider without duplicating its models;
14. README and in-app capability copy use one supported-harness list and
    explain per-route compatibility without primary/additional tiers.

Release gates include the full Rust workspace, adapter fixture suites,
frontend tests, lint, production build, permission checks, secret scans, and a
real temporary-home end-to-end matrix that never touches the user's configs.

## Acceptance criteria

- From a provider and model imported into CHM, a compatible target harness is
  usable after one CHM apply with no manual key, login, shell, or file step.
- Provider identity, endpoint, protocol, credential target, and model are
  validated as one route bundle.
- Incompatible targets are blocked before mutation with the precise native
  reason.
- A failed apply restores all affected files and credential stores.
- No plaintext credential appears in CHM persistence or audit surfaces.
- The current Yolo-Auto/OpenCode regression is fixed by the general mechanism,
  not an OpenCode-only special case.
- All registered harnesses participate in the same capability-driven sync
  workflow; none is presented as “detection-only” or falsely shown as capable
  of a route it cannot use.
