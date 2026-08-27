# Phase 14 — Cross-Platform Packaging + V1 Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lock the license, finish Windows/Linux platform gaps (secret stores, path handling), build the native release matrix on GitHub Actions, write installation docs, and verify the full V1 acceptance checklist (project plan §53, §54, §60, §67).

**Architecture:** Release pipeline: tag → GitHub Actions matrix (macOS/Windows/Linux runners) → build + test → sign where configured → GitHub Release artifacts. The desktop app already runs on macOS; this phase closes the Windows/Linux branches (compile-gated stubs from Phase 1 Task 1.7 become real implementations), wires Tauri bundle config, and produces the docs.

**Tech Stack:** Tauri bundler (dmg/app, msi/nsis, appimage/deb/rpm), GitHub Actions, Windows Credential Manager via `keyring` crate (or `windows` crate APIs), Linux libsecret via `keyring`/`secret-service` crate. Frontend unchanged.

## Global Constraints

- Do NOT block the first open-source release on paid code signing (project plan §53). Sign on macOS when a certificate is configured (`APPLE_*` secrets present), otherwise build unsigned and document the warning. Windows/Linux ship unsigned in V1.
- Release artifacts per platform: macOS `.dmg` + `.app`; Windows `.msi` + NSIS `.exe`; Linux `.AppImage` + `.deb`.
- Native builds only — no cross-compiling from macOS (project plan §53).
- Min supported versions (decision gates from §72, defaulted): macOS 12+, Windows 10 1903+, Ubuntu 22.04+ / Fedora 38+ (AppImage covers most distros).
- License: **Patty Public License 1.0** (Apache 2.0 + $100M revenue limitation) — decided at gate, recorded in LICENSE.
- Phase exit: `cargo test` green on all three OSes in CI, installers attach to a draft release, V1 acceptance checklist (Task 14.6) fully verified on macOS with a real harness.

---

### Task 14.1: License + Repo Polishing

**Files:**
- Modify: `LICENSE` (verify MIT text + year)
- Create: `CONTRIBUTING.md`
- Create: `SECURITY.md`
- Create: `CODE_OF_CONDUCT.md`
- Modify: `README.md` (install instructions section placeholder for Task 14.5)

**Interfaces:**
- Consumes: nothing. Produces: the open-source surface (project plan §59, §60, §70).

- [ ] **Step 1: Lock the license decision** — confirm MIT with the user (single-question decision gate; default MIT per plan §60). Record in `docs/plans/Coding-Harness-Manager-Project-Plan.md` §72 as resolved.

- [ ] **Step 2: Write the three community docs** — `CONTRIBUTING.md`: dev setup (Rust + Node toolchains, `npm run tauri dev`), adapter contribution guide referencing `docs/harnesses/` + the adapter trait, test requirements (fixture golden tests mandatory). `SECURITY.md`: report via GitHub private vulnerability reporting; no telemetry; secrets stay local. `CODE_OF_CONDUCT.md`: standard Contributor Covenant v2.1.

- [ ] **Step 3: Commit**

```bash
git add LICENSE CONTRIBUTING.md SECURITY.md CODE_OF_CONDUCT.md README.md docs/plans/
git commit -m "docs(phase14): license decision and community docs"
```

---

### Task 14.2: Windows Secret Store (Windows Credential Manager)

**Files:**
- Modify: `crates/secrets/src/lib.rs`
- Modify: `crates/secrets/Cargo.toml` (windows deps)
- Create: `crates/secrets/tests/windows_store.rs` (gated)

**Interfaces:**
- Consumes: `SecretStore` trait (Phase 1).
- Produces: real `WindowsCredentialManagerStore` — `set/get/delete` via the Windows Credential Manager using the `keyring` crate with the `windows-native` feature (compile-gated `#[cfg(target_os = "windows")]`).

- [ ] **Step 1: Add the dependency**

`crates/secrets/Cargo.toml`:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "3", features = ["windows-native"] }
```

- [ ] **Step 2: Implement the store**

```rust
#[cfg(target_os = "windows")]
impl SecretStore for WindowsCredentialManagerStore {
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new("coding-harness-manager", key)
            .map_err(|e| SecretError::Crypto(e.to_string()))?;
        entry.set_password(value).map_err(|e| SecretError::Keychain(e.to_string()))
    }
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let entry = keyring::Entry::new("coding-harness-manager", key)
            .map_err(|e| SecretError::Crypto(e.to_string()))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Keychain(e.to_string())),
        }
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new("coding-harness-manager", key)
            .map_err(|e| SecretError::Crypto(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Keychain(e.to_string())),
        }
    }
}
```

- [ ] **Step 3: Linux libsecret**

Same pattern with `keyring` `sync-secret-service` feature on `#[cfg(all(unix, not(target_os = "macos")))]` (target-specific dependency `keyring = { version = "3", features = ["sync-secret-service"] }` for Linux targets). `default_store()` already dispatches correctly.

- [ ] **Step 4: Verify (cross-check compile on macOS)**

The Windows/Linux branches are compile-gated; verify nothing breaks on macOS:

```bash
cargo test -p chm-secrets
```

Then verify the branches compile via CI (push a PR; the `rust` CI job is ubuntu → exercises the Linux branch).

- [ ] **Step 5: Commit**

```bash
git add crates/secrets
git commit -m "feat(phase14): windows and linux native secret stores"
```

---

### Task 14.3: Windows/Linux Platform Gaps

**Files:**
- Modify: `crates/harness-sdk/src/detect/paths.rs` (Windows `%APPDATA%` config resolution)
- Modify: `crates/harness-sdk/src/detect/paths.rs` (`find_executable` PATHEXT)
- Modify: `crates/harness-sdk/src/detect/version.rs` (Windows `cmd /c version` behavior)

**Interfaces:**
- Consumes: Phase 2 detection code.
- Produces: correct detection on Windows/Linux: config path candidates under `%APPDATA%`/`~/.config` per platform; `find_executable` appends `.exe/.cmd/.bat` from `PATHEXT` on Windows; `resolve_config_path` prefers `%APPDATA%` for Windows when the harness definition's paths are `~/.config`-style.

- [ ] **Step 1: Platform-aware config resolution**

```rust
pub fn resolve_config_path(def: &HarnessDefinition, home: &Path, platform: Platform) -> Option<PathBuf> {
    for candidate in def.config_paths {
        let full = home.join(candidate);
        if full.exists() {
            return Some(full);
        }
    }
    // Windows: also try %APPDATA%\<candidate basename> — e.g. .config/opencode → %APPDATA%\opencode
    if matches!(platform, Platform::Windows) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let base = std::path::PathBuf::from(appdata);
            for candidate in def.config_paths {
                let last = std::path::Path::new(candidate)
                    .file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                let full = base.join(&last);
                if full.exists() {
                    return Some(full);
                }
            }
        }
    }
    None
}
```

Add tests: `resolve_config_path_falls_back_to_appdata_on_windows` (unit-test by directly calling with `Platform::Windows` + setting `APPDATA` env var, restoring after).

- [ ] **Step 2: PATHEXT handling**

```rust
#[cfg(windows)]
pub fn find_executable(name: &str, path_env: Option<&str>) -> Option<String> {
    let path_value = path_env.or_else(|| std::env::var_os("PATH").map(|v| v.to_string_lossy().into_owned()))?;
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".into());
    let exts: Vec<String> = pathext.split(';').map(|s| s.to_lowercase()).collect();
    for dir in std::env::split_paths(&path_value) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}
```

- [ ] **Step 3: Verify via CI + commit**

```bash
cargo test -p chm-harness-sdk
git add crates/harness-sdk
git commit -m "feat(phase14): windows path handling"
```

---

### Task 14.4: Release Workflow + Bundler Config

**Files:**
- Create: `.github/workflows/release.yml`
- Modify: `apps/desktop/src-tauri/tauri.conf.json` (bundle config)
- Create: `apps/desktop/src-tauri/icons/` (app icons — placeholder generation via `npm run tauri icon`)

**Interfaces:**
- Consumes: the app as built through Phase 13.
- Produces: `release.yml` — on tag push: matrix `{os: [macos-latest, windows-latest, ubuntu-22.04]}`; steps: checkout, rust toolchain, node, `npm ci`, `cargo test` (workspace, incl. adapters), `cargo build` via `npm run tauri build`; macOS: sign+notarize when `APPLE_CERTIFICATE` + `APPLE_API_KEY` secrets exist (via `tauri-action` with `APPLE_*` inputs), otherwise skip; upload artifacts to the release via `tauri-action` (or `softprops/action-gh-release` for raw artifacts).

- [ ] **Step 1: Write `release.yml`**

```yaml
name: Release
on:
  push:
    tags: ["v*"]

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-latest
            args: --target aarch64-apple-darwin
          - os: macos-latest
            args: --target x86_64-apple-darwin
          - os: windows-latest
            args: ""
          - os: ubuntu-22.04
            args: ""
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: apps/desktop/package-lock.json
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.os == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: apps/desktop/src-tauri
      - name: Install Linux deps
        if: matrix.os == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libsecret-1-dev
      - run: cargo test --workspace
        working-directory: apps/desktop/src-tauri
        env:
          CHM_SKIP_KEYCHAIN: "1"
      - name: Build + bundle
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_API_KEY: ${{ secrets.APPLE_API_KEY }}
          APPLE_API_ISSUER: ${{ secrets.APPLE_API_ISSUER }}
          APPLE_API_KEY_ID: ${{ secrets.APPLE_API_KEY_ID }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Coding Harness Manager ${{ github.ref_name }}"
          releaseDraft: true
          prerelease: false
          args: ${{ matrix.args }}
```

Note: `CHM_SKIP_KEYCHAIN=1` gates the Phase 1 keychain test (`#[ignore]`-worthy in CI): update the keychain test to skip when the env var is set (read it in the test and `return` early).

- [ ] **Step 2: Bundle config in `tauri.conf.json`**

```json
"bundle": {
  "active": true,
  "targets": ["dmg", "app", "msi", "nsis", "appimage", "deb"],
  "category": "DeveloperTool",
  "shortDescription": "Manage AI coding harness configurations",
  "longDescription": "Manage models, providers, MCP servers, skills, and profiles across Claude Code, Codex, OpenCode, Pi, and Reasonix.",
  "macOS": { "minimumSystemVersion": "12.0" },
  "windows": { "wix": { "language": "en-US" } },
  "linux": { "deb": { "depends": ["libwebkit2gtk-4.1-0", "libsecret-1-0"] }, "appimage": { "bundleMediaFramework": false } }
}
```

- [ ] **Step 3: Icons**

```bash
cd apps/desktop && npm run tauri icon <1024x1024-source.png>
```

(Source icon asset is a design task — any square PNG; the command generates the full icon set.)

- [ ] **Step 4: Dry-run the Linux build locally (macOS can't)** — push the workflow and verify the ubuntu job passes end-to-end via the Actions tab on a branch (tag not required; add a `workflow_dispatch` trigger for testing).

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml apps/desktop/src-tauri
git commit -m "feat(phase14): release workflow and bundler config"
```

---

### Task 14.5: Installation Documentation

**Files:**
- Modify: `README.md`
- Create: `docs/installation.md`
- Create: `docs/development.md`

**Interfaces:**
- Produces: the docs a new user needs (project plan §67 criterion 1 + §53 unsigned-build warnings).

- [ ] **Step 1: `docs/installation.md`** — per-OS install steps (download from Releases, open dmg / run msi / appimage chmod +x), first-run walkthrough (scan → import → provider → models → sync), the unsigned-build warning box ("macOS builds are unsigned until a signing certificate is configured — right-click → Open, or `xattr -d com.apple.quarantine`"), uninstall notes.

- [ ] **Step 2: `docs/development.md`** — prerequisites (Rust stable, Node 22, platform deps incl. `libwebkit2gtk-4.1-dev` on Linux, VS Build Tools + WebView2 on Windows), dev commands (`npm run tauri dev`), test commands (`cargo test --workspace`, `npm run lint`), how to add a fixture, how to add an adapter (link to CONTRIBUTING).

- [ ] **Step 3: Update `README.md`** — badges (CI, license), install section pointing at the docs, screenshot placeholder, the §70 positioning block.

- [ ] **Step 4: Commit**

```bash
git add README.md docs/
git commit -m "docs(phase14): installation and development docs"
```

---

### Task 14.6: V1 Acceptance Verification + Phase Exit

**Files:**
- Create: `docs/v1-acceptance.md` (checklist log)

**Interfaces:**
- Consumes: the running app + `harnessctl`.

- [ ] **Step 1: Walk the §67 checklist on macOS with real harnesses**

Execute all 20 criteria in order (install app artifact → scan → see supported harnesses → import without file changes → add provider → store key → validate → discover → import to My Models → enrich via models.dev → select models → select harnesses → preview diffs → apply safely → configure global MCP → manage global skills → use canonical storage → create launch profile → detect external change → roll back a sync). Record each as PASS/FAIL with a one-line note in `docs/v1-acceptance.md`. Any FAIL blocks the release — fix or explicitly document as V1.1.

- [ ] **Step 2: CLI parity check**

```bash
harnessctl scan && harnessctl list && harnessctl status
harnessctl run claude --profile zai   # spot check
```

- [ ] **Step 3: Security self-review** — grep the repo for committed secrets (`.env`, real key patterns in fixtures — rerun the Phase 0 sweep across the whole repo):

```bash
rg -i "sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{20,}|api[_-]?key\s*=\s*['\"][A-Za-z0-9]{16,}" . --glob '!target/**' --glob '!node_modules/**' --glob '!crates/models-dev/fixtures/catalog.json' || echo "CLEAN"
```

- [ ] **Step 4: Final gate**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
```

- [ ] **Step 5: Cut the release**

```bash
git tag v0.1.0 && git push origin v0.1.0
```

Monitor the release workflow; attach the checklist + acceptance doc to the draft release notes; publish.

- [ ] **Step 6: Commit the acceptance log**

```bash
git add docs/v1-acceptance.md
git commit -m "docs(phase14): v1 acceptance verification"
```

Phase complete when all steps green — and V1 is shipped.