# Phase 2 — Harness Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the registry-driven detector that finds installed harnesses on the user's machine and produces a normalized `HarnessInventory`, reusing the detection rules documented in Phase 0 Task 0.7 (`docs/harnesses/detection.md`).

**Architecture:** A `HarnessDefinition` registry (one entry per harness: executable names, config paths, skill paths, MCP paths, platform support) drives generic detection: PATH walk + known install paths + config-dir existence + version-command parsing. Detection is pure and testable: all filesystem inputs are injected (fake PATH, temp dirs) so tests run anywhere, including CI.

**Tech Stack:** Rust edition 2024, std `process::Command`, `dirs` crate for platform home/config dirs, `tempfile` (dev), `which`-style PATH walk implemented by hand (no extra dep).

## Global Constraints

- Registry entries come ONLY from `docs/harnesses/detection.md` (Phase 0) — no invented paths. If the research doc marks something unknown, the definition marks it `None` and the detector skips that check.
- Detection must be read-only: it never creates, moves, or modifies anything.
- Platform abstraction: macOS/Linux use `$PATH` + `~/.config`; Windows uses `%PATH%` + `%APPDATA%` (compile-gated, with a test-visible abstraction so logic is unit-tested on macOS).
- `HarnessDefinition` includes the detection-only harnesses (Gemini CLI, Qwen Code, Kimi CLI, Cursor, Cline, Roo Code, Aider, Amp, Goose, Continue) with `detection_only: true` — they show "Detected — support coming".
- Phase exit: `scan()` returns a full inventory for the local machine; all tests green; clippy clean.

---

### Task 2.1: Harness Definition Registry

**Files:**
- Create: `crates/harness-sdk/src/lib.rs`
- Create: `crates/harness-sdk/src/definition.rs`
- Create: `crates/harness-sdk/tests/registry.rs`

**Interfaces:**
- Produces (used by every later detection/scan task and by Phase 3 adapters):
  - `pub struct HarnessDefinition { pub id: &'static str, pub name: &'static str, pub executable_names: &'static [&'static str], pub config_paths: &'static [&'static str], pub skill_paths: &'static [&'static str], pub mcp_paths: &'static [&'static str], pub platforms: &'static [Platform], pub detection_only: bool }`
  - `pub enum Platform { MacOs, Windows, Linux }`
  - `pub fn tier1_definitions() -> Vec<HarnessDefinition>` — the five Tier-1 harnesses (detection_only: false).
  - `pub fn detection_only_definitions() -> Vec<HarnessDefinition>` — the ten detection-only harnesses (detection_only: true).
  - `pub fn all_definitions() -> Vec<HarnessDefinition>`

- [ ] **Step 1: Write the failing test `tests/registry.rs`**

```rust
use chm_harness_sdk::definition::{all_definitions, tier1_definitions, detection_only_definitions};

#[test]
fn tier1_has_exactly_five_harnesses() {
    let defs = tier1_definitions();
    assert_eq!(defs.len(), 5);
    let ids: Vec<&str> = defs.iter().map(|d| d.id).collect();
    assert!(ids.contains(&"claude-code"));
    assert!(ids.contains(&"codex"));
    assert!(ids.contains(&"opencode"));
    assert!(ids.contains(&"pi"));
    assert!(ids.contains(&"reasonix"));
    assert!(defs.iter().all(|d| !d.detection_only));
}

#[test]
fn every_definition_has_at_least_one_executable_name() {
    for d in all_definitions() {
        assert!(!d.executable_names.is_empty(), "{} has no executables", d.id);
    }
}

#[test]
fn tier1_ids_match_domain_harness_types() {
    let defs = tier1_definitions();
    for d in &defs {
        assert_eq!(chm_core::domain::harness::HarnessType::parse_str(d.id).as_str(), d.id);
    }
}
```

- [ ] **Step 2: Implement `crates/harness-sdk/src/lib.rs` and `definition.rs`**

```rust
// lib.rs
//! Harness adapter contract and registry definitions.

pub mod definition;
```

```rust
// definition.rs
//! Registry of harness definitions. Data comes from docs/harnesses/detection.md.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

#[derive(Debug, Clone)]
pub struct HarnessDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub executable_names: &'static [&'static str],
    pub config_paths: &'static [&'static str],
    pub skill_paths: &'static [&'static str],
    pub mcp_paths: &'static [&'static str],
    pub platforms: &'static [Platform],
    pub detection_only: bool,
}

const fn def(
    id: &'static str,
    name: &'static str,
    executables: &'static [&'static str],
    config: &'static [&'static str],
    skills: &'static [&'static str],
    mcp: &'static [&'static str],
    detection_only: bool,
) -> HarnessDefinition {
    HarnessDefinition {
        id,
        name,
        executable_names: executables,
        config_paths: config,
        skill_paths: skills,
        mcp_paths: mcp,
        platforms: &[Platform::MacOs, Platform::Windows, Platform::Linux],
        detection_only,
    }
}

pub fn tier1_definitions() -> Vec<HarnessDefinition> {
    vec![
        def(
            "claude-code",
            "Claude Code",
            &["claude", "claude-code"],
            // config path candidates relative to home: filled from docs/harnesses/claude-code.md
            &[".claude/settings.json", ".claude.json"],
            &[".claude/skills"],
            &[".claude.json"],
            false,
        ),
        def(
            "codex",
            "Codex",
            &["codex"],
            &[".codex/config.toml", ".codex/mcp.json"],
            &[".codex/skills"],
            &[".codex/mcp.json"],
            false,
        ),
        def(
            "opencode",
            "OpenCode",
            &["opencode"],
            &[".config/opencode/opencode.json", ".config/opencode/opencode.jsonc", ".config/opencode/opencode-mcp.json"],
            &[".config/opencode/skills"],
            &[".config/opencode/opencode-mcp.json"],
            false,
        ),
        def(
            "pi",
            "Pi",
            &["pi"],
            &[".pi/agent/config.toml"],
            &[".pi/agent/skills"],
            &[],
            false,
        ),
        def(
            "reasonix",
            "Reasonix",
            &["reasonix"],
            &[".config/reasonix"],
            &[],
            &[],
            false,
        ),
    ]
}

pub fn detection_only_definitions() -> Vec<HarnessDefinition> {
    vec![
        def("gemini-cli", "Gemini CLI", &["gemini"], &[".gemini"], &[], &[], true),
        def("qwen-code", "Qwen Code", &["qwen-code", "qwen"], &[".config/qwen-code"], &[], &[], true),
        def("kimi-cli", "Kimi CLI", &["kimi"], &[".config/kimi"], &[], &[], true),
        def("cursor", "Cursor", &["cursor"], &[".cursor"], &[], &[], true),
        def("cline", "Cline", &["cline"], &[], &[], &[], true),
        def("roo-code", "Roo Code", &["roo"], &[], &[], &[], true),
        def("aider", "Aider", &["aider"], &[".aider.conf.yml"], &[], &[], true),
        def("amp", "Amp", &["amp"], &[".amp"], &[], &[], true),
        def("goose", "Goose", &["goose"], &[".config/goose"], &[], &[], true),
        def("continue", "Continue", &["continue"], &[".continue"], &[], &[], true),
    ]
}

pub fn all_definitions() -> Vec<HarnessDefinition> {
    let mut all = tier1_definitions();
    all.extend(detection_only_definitions());
    all
}
```

Note: exact paths must be reconciled against `docs/harnesses/detection.md` when implementing — the research doc is authoritative; adjust the arrays above to match it if they differ.

- [ ] **Step 3: Run tests**

```bash
cargo test -p chm-harness-sdk
```

Expected: all three pass.

- [ ] **Step 4: Commit**

```bash
git add crates/harness-sdk/
git commit -m "feat(phase2): harness definition registry"
```

---

### Task 2.2: Executable and Config-Path Detection

**Files:**
- Create: `crates/harness-sdk/src/detect/mod.rs`
- Create: `crates/harness-sdk/src/detect/paths.rs`
- Create: `crates/harness-sdk/tests/paths.rs`

**Interfaces:**
- Consumes: `HarnessDefinition` (Task 2.1).
- Produces:
  - `pub fn find_executable(name: &str, path_env: Option<&str>) -> Option<String>` — walks `PATH` (or the injected value), returns first match that is executable (Unix: mode bit; Windows: PATHEXT handling, compile-gated).
  - `pub fn resolve_config_path(def: &HarnessDefinition, home: &Path, platform: Platform) -> Option<PathBuf>` — first candidate path from `def.config_paths` that exists.
  - `pub fn home_dir(platform: Platform, injected_home: Option<&Path>) -> PathBuf` — test-injectable home resolution (macOS/Linux: `$HOME` or injected; Windows: `%USERPROFILE%`, compile-gated).

- [ ] **Step 1: Write the failing test `tests/paths.rs`**

```rust
use chm_harness_sdk::definition::{Platform, HarnessDefinition};
use chm_harness_sdk::detect::paths::{find_executable, resolve_config_path};
use std::path::PathBuf;
use tempfile::TempDir;

fn def_with_config(candidates: &'static [&'static str]) -> HarnessDefinition {
    HarnessDefinition {
        id: "test",
        name: "Test",
        executable_names: &["test-tool"],
        config_paths: candidates,
        skill_paths: &[],
        mcp_paths: &[],
        platforms: &[Platform::MacOs, Platform::Linux, Platform::Windows],
        detection_only: false,
    }
}

#[test]
fn find_executable_walks_path_in_order() {
    let dir = TempDir::new().unwrap();
    let first = dir.path().join("a");
    let second = dir.path().join("b");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("tool"), "#!/bin/sh\n").unwrap();
    std::fs::write(second.join("tool"), "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(first.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(second.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path_env = format!("{}:{}", first.display(), second.display());
    assert_eq!(find_executable("tool", Some(&path_env)), Some(first.join("tool").display().to_string()));
}

#[test]
fn find_executable_returns_none_when_missing() {
    let dir = TempDir::new().unwrap();
    let path_env = dir.path().display().to_string();
    assert_eq!(find_executable("does-not-exist", Some(&path_env)), None);
}

#[test]
fn resolve_config_path_picks_first_existing_candidate() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".codex")).unwrap();
    std::fs::write(dir.path().join(".codex/config.toml"), "model = \"gpt-5\"\n").unwrap();
    let def = def_with_config(&[".codex/config.toml", ".codex/mcp.json"]);
    let resolved = resolve_config_path(&def, dir.path(), Platform::MacOs);
    assert_eq!(resolved, Some(dir.path().join(".codex/config.toml")));
}

#[test]
fn resolve_config_path_returns_none_when_nothing_exists() {
    let dir = TempDir::new().unwrap();
    let def = def_with_config(&[".codex/config.toml"]);
    assert_eq!(resolve_config_path(&def, dir.path(), Platform::MacOs), None);
}
```

- [ ] **Step 2: Implement `detect/mod.rs` and `detect/paths.rs`**

```rust
// detect/mod.rs
pub mod paths;
```

```rust
// detect/paths.rs
//! Generic executable + config-path detection. All inputs injectable for tests.

use std::path::{Path, PathBuf};

use crate::definition::{HarnessDefinition, Platform};

pub fn home_dir(platform: Platform, injected_home: Option<&Path>) -> PathBuf {
    if let Some(h) = injected_home {
        return h.to_path_buf();
    }
    match platform {
        Platform::MacOs | Platform::Linux => {
            std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
        }
        Platform::Windows => std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default(),
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file() && path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn find_executable(name: &str, path_env: Option<&str>) -> Option<String> {
    let path_value = path_env.or_else(|| std::env::var_os("PATH").map(|v| v.to_string_lossy().into_owned()))?;
    for dir in std::env::split_paths(&path_value) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate.display().to_string());
        }
    }
    None
}

pub fn resolve_config_path(def: &HarnessDefinition, home: &Path, platform: Platform) -> Option<PathBuf> {
    for candidate in def.config_paths {
        let full = home.join(candidate);
        if full.exists() {
            return Some(full);
        }
    }
    let _ = platform; // platform-specific dirs (e.g. Windows %APPDATA%) land in phase 14
    None
}
```

- [ ] **Step 3: Run tests**

In `crates/harness-sdk/Cargo.toml` add `[dev-dependencies] tempfile.workspace = true`. Then:

```bash
cargo test -p chm-harness-sdk
```

Expected: all four tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/harness-sdk/
git commit -m "feat(phase2): executable and config path detection"
```

---

### Task 2.3: Version Detection

**Files:**
- Create: `crates/harness-sdk/src/detect/version.rs`
- Create: `crates/harness-sdk/tests/version.rs`

**Interfaces:**
- Consumes: `HarnessDefinition`; `find_executable` (Task 2.2).
- Produces:
  - `pub fn detect_version(executable_path: &str, version_args: &[&str]) -> Option<String>` — runs the executable with args (default `["--version"]`), parses the first semver-like token from stdout (regex-free: split on whitespace, keep token matching `\d+\.\d+` prefix), returns `None` on spawn failure or no match.
  - `pub fn version_args_for(def: &HarnessDefinition) -> &'static [&'static str]` — per-harness version args recorded in `docs/harnesses/detection.md` (default `["--version"]`).

- [ ] **Step 1: Write the failing test `tests/version.rs`**

```rust
use chm_harness_sdk::detect::version::detect_version;

#[test]
fn parses_semver_from_standard_output() {
    // `cargo --version` prints "cargo 1.85.0 (d73c2c44b 2025-02-04)" — use cargo as a
    // guaranteed-available binary on any rust toolchain machine.
    let v = detect_version("cargo", &["--version"]).expect("cargo must be on PATH");
    assert!(v.split('.').count() >= 2, "expected semver, got {v}");
    assert!(v.chars().next().unwrap().is_ascii_digit());
}

#[test]
fn missing_binary_returns_none() {
    assert_eq!(detect_version("/nonexistent/definitely-not-a-binary", &["--version"]), None);
}

#[test]
fn unknown_output_returns_none() {
    // run a binary that prints no version-ish token
    let dir = tempfile::TempDir::new().unwrap();
    let script = dir.path().join("no-version");
    std::fs::write(&script, "#!/bin/sh\nprintf 'hello world'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(detect_version(&script.display().to_string(), &[]), None);
}
```

- [ ] **Step 2: Implement `detect/version.rs`**

```rust
//! Version detection: run the harness binary, parse a semver-like token.

use std::process::Command;

use crate::definition::HarnessDefinition;

pub fn version_args_for(_def: &HarnessDefinition) -> &'static [&'static str] {
    // Per-harness overrides recorded in docs/harnesses/detection.md.
    // Default is universal: `--version`.
    &["--version"]
}

pub fn detect_version(executable_path: &str, version_args: &[&str]) -> Option<String> {
    let out = Command::new(executable_path).args(version_args).output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let all = format!("{stdout}\n{stderr}");
    all.split_whitespace()
        .map(|tok| tok.trim_matches(|c: char| !c.is_ascii_digit() && c != '.'))
        .find(|tok| {
            let mut parts = tok.split('.');
            let major = parts.next().and_then(|p| p.parse::<u64>().ok());
            let minor = parts.next().and_then(|p| p.parse::<u64>().ok());
            matches!((major, minor), (Some(_), Some(_)))
        })
        .map(|tok| tok.to_string())
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p chm-harness-sdk
```

Expected: all three pass.

- [ ] **Step 4: Commit**

```bash
git add crates/harness-sdk/
git commit -m "feat(phase2): version detection"
```

---

### Task 2.4: Scan Orchestration + Inventory

**Files:**
- Create: `crates/harness-sdk/src/detect/scan.rs`
- Create: `crates/harness-sdk/tests/scan.rs`

**Interfaces:**
- Consumes: `all_definitions()`, `find_executable`, `resolve_config_path`, `detect_version`, `home_dir`; domain `HarnessInstallation`, `HarnessType`, `InstallationStatus`.
- Produces:
  - `pub struct HarnessInventory { pub installations: Vec<HarnessInstallation> }`
  - `pub fn scan(platform: Platform, home: Option<&Path>, path_env: Option<&str>) -> HarnessInventory` — for each definition: find executable; if found → status `Installed`, detect version + config path; if config exists but no executable → `ConfigMissing`; if neither → skipped (not in inventory).

- [ ] **Step 1: Write the failing test `tests/scan.rs`**

```rust
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn scan_finds_tier1_harness_with_executable_and_config() {
    let dir = TempDir::new().unwrap();
    let bindir = dir.path().join("bin");
    let homedir = dir.path().join("home");
    std::fs::create_dir_all(&bindir).unwrap();
    std::fs::create_dir_all(homedir.join(".config/opencode")).unwrap();
    std::fs::write(bindir.join("opencode"), "#!/bin/sh\nprintf 'opencode 0.30.0\n'\n").unwrap();
    std::fs::write(homedir.join(".config/opencode/opencode.json"), "{}").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(bindir.join("opencode"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let inv = scan(Platform::MacOs, Some(Path::new(&homedir.display().to_string())), Some(&bindir.display().to_string()));
    let oc = inv.installations.iter().find(|i| i.harness_type.as_str() == "opencode").expect("opencode detected");
    assert_eq!(oc.status, chm_core::domain::harness::InstallationStatus::Installed);
    assert_eq!(oc.version.as_deref(), Some("0.30.0"));
    assert!(oc.config_path.as_deref().unwrap_or("").contains("opencode.json"));
}

#[test]
fn scan_marks_config_missing_when_only_executable_absent() {
    let dir = TempDir::new().unwrap();
    let homedir = dir.path().join("home");
    std::fs::create_dir_all(homedir.join(".claude")).unwrap();
    std::fs::write(homedir.join(".claude/settings.json"), "{}").unwrap();
    let inv = scan(Platform::MacOs, Some(Path::new(&homedir.display().to_string())), Some("/nonexistent"));
    let cc = inv.installations.iter().find(|i| i.harness_type.as_str() == "claude-code").expect("claude-code present");
    assert_eq!(cc.status, chm_core::domain::harness::InstallationStatus::ConfigMissing);
}
```

- [ ] **Step 2: Implement `detect/scan.rs`**

```rust
//! Orchestrates full-machine detection into a normalized inventory.

use std::path::Path;

use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chrono::Utc;
use uuid::Uuid;

use crate::definition::Platform;
use crate::definition::all_definitions;

use super::paths::{find_executable, home_dir, resolve_config_path};
use super::version::{detect_version, version_args_for};

#[derive(Debug, Default)]
pub struct HarnessInventory {
    pub installations: Vec<HarnessInstallation>,
}

pub fn scan(platform: Platform, home: Option<&Path>, path_env: Option<&str>) -> HarnessInventory {
    let home = home_dir(platform, home);
    let mut inventory = HarnessInventory::default();
    for def in all_definitions() {
        if def.detection_only {
            // detection-only harnesses: report presence via executable, no config/version work
            if let Some(exe) = def.executable_names.iter().find_map(|n| find_executable(n, path_env)) {
                inventory.installations.push(HarnessInstallation {
                    id: Uuid::new_v4(),
                    harness_type: HarnessType::parse_str(def.id),
                    executable_path: Some(exe),
                    version: None,
                    config_path: None,
                    detected_at: Utc::now(),
                    last_scanned_at: None,
                    status: InstallationStatus::Detected,
                });
            }
            continue;
        }
        let exe = def.executable_names.iter().find_map(|n| find_executable(n, path_env));
        let config = resolve_config_path(def, &home, platform);
        let installation = match (exe, config) {
            (Some(executable_path), config_path) => {
                let version = detect_version(&executable_path, version_args_for(def));
                HarnessInstallation {
                    id: Uuid::new_v4(),
                    harness_type: HarnessType::parse_str(def.id),
                    executable_path: Some(executable_path),
                    version,
                    config_path,
                    detected_at: Utc::now(),
                    last_scanned_at: Some(Utc::now()),
                    status: InstallationStatus::Installed,
                }
            }
            (None, Some(config_path)) => HarnessInstallation {
                id: Uuid::new_v4(),
                harness_type: HarnessType::parse_str(def.id),
                executable_path: None,
                version: None,
                config_path: Some(config_path),
                detected_at: Utc::now(),
                last_scanned_at: Some(Utc::now()),
                status: InstallationStatus::ConfigMissing,
            },
            (None, None) => continue,
        };
        inventory.installations.push(installation);
    }
    inventory
}
```

- [ ] **Step 3: Add `chm-core` dependency and run tests**

In `crates/harness-sdk/Cargo.toml` add `chm-core.workspace = true`, `chrono.workspace = true`, `uuid.workspace = true`. Then:

```bash
cargo test -p chm-harness-sdk
```

Expected: both tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/harness-sdk/
git commit -m "feat(phase2): scan orchestration and inventory"
```

---

### Task 2.5: Real-Machine Smoke Test + Phase Exit

**Files:**
- Modify: `docs/harnesses/README.md` (record observed detection results)

**Interfaces:**
- Consumes: Task 2.4.

- [ ] **Step 1: Run the scan against the real machine**

Add a throwaway example binary `crates/harness-sdk/examples/scan.rs`:

```rust
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;

fn main() {
    #[cfg(target_os = "macos")]
    let platform = Platform::MacOs;
    #[cfg(target_os = "windows")]
    let platform = Platform::Windows;
    #[cfg(all(unix, not(target_os = "macos")))]
    let platform = Platform::Linux;

    let inv = scan(platform, None, None);
    for i in &inv.installations {
        println!("{} | {} | exe={:?} | version={:?} | config={:?}",
            i.harness_type.as_str(), i.status_v(), i.executable_path, i.version, i.config_path);
    }
    println!("total: {}", inv.installations.len());
}
```

Add `fn status_v(&self) -> &'static str` helper on `HarnessInstallation` in `crates/core/src/domain/harness.rs` (`match self.status { Detected => "detected", Installed => "installed", ConfigMissing => "config-missing", Error => "error" }`), plus a matching test asserting the roundtrip.

Run:

```bash
cargo run -p chm-harness-sdk --example scan
```

Expected: the machine's real harnesses appear with correct versions (cross-check versions with `claude --version`, `codex --version`, etc.). Discrepancies → fix `definition.rs` paths/version args, do NOT adjust tests to lie.

- [ ] **Step 2: Record results in research index**

Update the `docs/harnesses/README.md` table with the real observed statuses/versions.

- [ ] **Step 3: Full gate**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: green.

- [ ] **Step 4: Commit**

```bash
git add crates/ docs/harnesses/README.md
git commit -m "feat(phase2): real-machine detection smoke test"
```

Phase complete when all steps green.