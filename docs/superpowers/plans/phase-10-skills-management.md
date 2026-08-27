# Phase 10 — Skills Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Canonical skills management: scan/import skills into `~/.agents/skills`, hash-based dedup, per-harness binding strategies (symlink/junction/copy/native), and conflict detection (project plan §25, §26, §27, §28, §52).

**Architecture:** Backend commands in `apps/desktop/src-tauri/src/commands/skills.rs`. Skills metadata lives in SQLite; files live on disk under `~/.agents/skills/<name>/`. Binding to harnesses goes through `link_directory` (filesystem crate) into each harness's skill dir, with `HarnessSkillBinding` rows recording strategy + target. Conflicts (same name different content, broken symlinks, shadowed paths) are computed by scanning both sides and hashing content.

**Tech Stack:** Rust + `sha2` for content hashing (directory hash = hash of sorted (relative path, file hash) pairs). Frontend as Phase 4.

## Global Constraints

- Canonical location: `~/.agents/skills` (project plan §25, Decision 7). The app never relocates existing skill files — import copies/symlinks into the canonical dir only when the user opts in; otherwise the skill's canonical_path points at its existing location.
- Skill files are NEVER stored in SQLite (project plan §26) — only metadata + content_hash.
- Dedup by: canonical name, path, content hash, source URL (project plan §27).
- Bindings via `link_directory` ONLY; adapter capability `supports_symlinked_skills` gates binding type (Unsupported → report + no link).
- Import sources (project plan §27): existing `~/.agents/skills`, harness skill folders, local folder, git repository. V1 implements folder + harness + agents dir; git import is V1 (shallow clone) but package/remote catalogs are future.
- Phase exit: user imports skills from a harness and an existing `~/.agents/skills`, sees conflicts resolved, binds a skill to a harness via symlink, and re-scan shows clean status.

---

### Task 10.1: Scan + Hash + Import Commands

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/skills.rs`
- Create: `apps/desktop/src-tauri/src/skill_lib.rs` (pure logic, testable)
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`, `lib.rs`
- Create: `apps/desktop/src-tauri/tests/skills_commands.rs`
- Modify: `apps/desktop/src/lib/api.ts`
- Create: `apps/desktop/src/hooks/useSkills.ts`
- Create: `apps/desktop/src/screens/SkillsScreen.tsx` (replaces placeholder)

**Interfaces:**
- Consumes: `Skill` domain type, `create_skill`/`list_skills` (Phase 1 Task 1.5).
- Produces:
  - `pub fn hash_directory(dir: &Path) -> Result<String, String>` — SHA-256 over `sorted(relative_path + ":" + file_sha256)` lines; symlinks hashed as their target string.
  - `pub async fn scan_skills_dir(state, dir: String) -> Result<Vec<ScannedSkill>, String>` where `ScannedSkill { pub name: String, pub path: String, pub content_hash: String }` (recursive: each subdir containing a `SKILL.md` is a skill; other files ignored).
  - `#[tauri::command] pub async fn import_skills_cmd(state, paths: Vec<String>) -> Result<ImportSkillReport, String>` where `ImportSkillReport { pub imported: usize, pub duplicates: Vec<String>, pub conflicts: Vec<String> }` — for each path: hash, dedup checks, insert `Skill` row with `canonical_path` = path (files stay put), provenance `{"source": "<parent dir name>", "imported_at": ...}`.
  - `#[tauri::command] pub async fn adopt_canonical_dir(state) -> Result<usize, String>` — one-time: scan `~/.agents/skills`, import everything (idempotent via hash dedup).

- [ ] **Step 1: Write the failing tests `tests/skills_commands.rs`**

```rust
use coding_harness_manager_lib::skill_lib::{hash_directory, scan_skill_dirs};
use tempfile::TempDir;

fn write_skill(dir: &std::path::Path, name: &str) {
    let skill_dir = dir.join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), format!("# {name}\n\nbody\n")).unwrap();
}

#[test]
fn hash_directory_is_stable_and_content_sensitive() {
    let d1 = TempDir::new().unwrap();
    let d2 = TempDir::new().unwrap();
    write_skill(d1.path(), "brainstorming");
    write_skill(d2.path(), "brainstorming");
    let h1 = hash_directory(d1.path()).unwrap();
    let h2 = hash_directory(d2.path()).unwrap();
    assert_eq!(h1, h2, "identical content → identical hash");
    std::fs::write(d2.path().join("brainstorming/SKILL.md"), "# brainstorming\n\nCHANGED\n").unwrap();
    let h3 = hash_directory(d2.path()).unwrap();
    assert_ne!(h1, h3, "content change → different hash");
}

#[test]
fn scan_detects_skills_by_sk_md() {
    let dir = TempDir::new().unwrap();
    write_skill(dir.path(), "brainstorming");
    write_skill(dir.path(), "frontend-design");
    std::fs::write(dir.path().join("README.md"), "not a skill").unwrap();
    let skills = scan_skill_dirs(dir.path()).unwrap();
    assert_eq!(skills.len(), 2);
    assert!(skills.iter().any(|s| s.name == "brainstorming"));
}

#[tokio::test]
async fn import_dedups_by_hash() {
    let pool = connect_test().await.unwrap();
    let dir = TempDir::new().unwrap();
    write_skill(dir.path(), "brainstorming");
    let paths = vec![dir.path().join("brainstorming").display().to_string()];
    let report = import_skills_core(&pool, &paths).await.unwrap();
    assert_eq!(report.imported, 1);
    let report2 = import_skills_core(&pool, &paths).await.unwrap();
    assert_eq!(report2.imported, 0, "same hash → duplicate");
    assert_eq!(report2.duplicates.len(), 1);
}
```

- [ ] **Step 2: Implement `skill_lib.rs`**

```rust
//! Pure skill logic: hashing, scanning, dedup.

use sha2::{Digest, Sha256};
use std::path::Path;

pub fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

pub fn hash_directory(dir: &Path) -> Result<String, String> {
    if !dir.is_dir() {
        return Err(format!("not a directory: {}", dir.display()));
    }
    let mut lines = Vec::new();
    collect_hashes(dir, dir, &mut lines)?;
    lines.sort();
    let mut hasher = Sha256::new();
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_hashes(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path.strip_prefix(root).map_err(|e| e.to_string())?
            .display().to_string();
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            let target = std::fs::read_link(&path).map_err(|e| e.to_string())?
                .display().to_string();
            out.push(format!("{rel}:symlink:{target}"));
        } else if ft.is_dir() {
            collect_hashes(root, &path, out)?;
        } else {
            let h = hash_file(&path)?;
            out.push(format!("{rel}:{h}"));
        }
    }
    Ok(())
}

pub struct ScannedSkill {
    pub name: String,
    pub path: String,
    pub content_hash: String,
}

/// A directory is a skill iff it contains SKILL.md.
pub fn scan_skill_dirs(dir: &Path) -> Result<Vec<ScannedSkill>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("SKILL.md").exists() {
            out.push(ScannedSkill {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.display().to_string(),
                content_hash: hash_directory(&path)?,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
```

- [ ] **Step 3: Implement `commands/skills.rs`**

`import_skills_core(pool, paths) -> Result<ImportSkillReport, String>` (testable; command wrapper adds State): for each path → `hash_directory` → check `list_skills` for same `content_hash` (duplicate) or same `canonical_path` (duplicate) or same name with DIFFERENT hash (conflict — record, don't import) → else `create_skill`. `import_skills_cmd` is the State wrapper; `adopt_canonical_dir` scans `~/.agents/skills` (via `home_dir`) and calls the core with all found paths.

- [ ] **Step 4: Frontend**

`useSkills.ts`: `useSkills()`, `useImportSkills(paths)`, `useAdoptCanonical()`. `SkillsScreen.tsx`: table (name, canonical path, content hash (first 8 chars), source, enabled) + "Import from folder…" (file picker via `@tauri-apps/plugin-dialog` — add the plugin) + "Scan ~/.agents/skills" button + conflict rows highlighted.

- [ ] **Step 5: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase10): skill scan, hashing, import"
```

---

### Task 10.2: Harness Bindings (symlink/junction/copy)

**Files:**
- Modify: `crates/database/src/repos/skills.rs` (binding methods)
- Modify: `apps/desktop/src-tauri/src/commands/skills.rs` (`bind_skill_cmd`)
- Create: `apps/desktop/src-tauri/tests/skill_bindings.rs`

**Interfaces:**
- Consumes: `link_directory` (Phase 8.1), `HarnessSkillBinding` domain type.
- Produces:
  - `pub async fn create_skill_binding(pool, b: &HarnessSkillBinding) -> Result<(), DbError>` and `pub async fn list_skill_bindings(pool, installation_id) -> Result<Vec<HarnessSkillBinding>, DbError>` in `repos/skills.rs`.
  - `#[tauri::command] pub async fn bind_skill_cmd(state, installation_id: String, skill_id: String) -> Result<BindOutcome, String>` where `BindOutcome { pub binding_type: String, pub target_path: String }` — computes target `~/.<harness skill dir>/<skill name>` from the harness definition's `skill_paths`, calls `link_directory`, records binding row (`binding_type` from `LinkOutcome`), returns the outcome.
  - `#[tauri::command] pub async fn unbind_skill_cmd(state, binding_id: String) -> Result<(), String>` — removes the link (symlink/junction only — copy bindings warn "manual removal") and deletes the binding row.

- [ ] **Step 1: Write the failing tests `tests/skill_bindings.rs`**

```rust
#[tokio::test]
async fn bind_creates_symlink_and_binding_row() {
    let pool = connect_test().await.unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let skills = home.join(".agents/skills/brainstorming");
    std::fs::create_dir_all(&skills).unwrap();
    std::fs::write(skills.join("SKILL.md"), "# brainstorming").unwrap();
    // seed skill + installation (opencode, config dir under tmp home)
    let skill = chm_core::domain::skills::Skill {
        id: uuid::Uuid::new_v4(),
        name: "brainstorming".into(),
        canonical_path: skills.display().to_string(),
        source_type: chm_core::domain::skills::SkillSourceType::Folder,
        source_url: None,
        content_hash: Some("abc".into()),
        provenance: serde_json::json!({}),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    chm_database::repos::skills::create_skill(&pool, &skill).await.unwrap();
    let install = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: None,
        version: Some("0.30.0".into()),
        config_path: Some(home.join(".config/opencode").display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    chm_database::repos::harness::upsert_installation(&pool, &install).await.unwrap();

    let outcome = bind_skill_core(&pool, &install.id.to_string(), &skill.id.to_string(), &home).await.unwrap();
    let target = home.join(".config/opencode/skills/brainstorming");
    assert!(target.symlink_metadata().is_ok(), "symlink created");
    let bindings = chm_database::repos::skills::list_skill_bindings(&pool, install.id).await.unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].binding_type, chm_core::domain::harness::BindingType::Symlink);
    let _ = outcome;
}

#[tokio::test]
async fn bind_unsupported_harness_reports_unsupported() {
    // harness capability supports_symlinked_skills=false → Unsupported outcome,
    // no binding row, no filesystem change
}
```

- [ ] **Step 2: Implement**

`bind_skill_core(pool, installation_id, skill_id, home) -> Result<BindOutcome, String>` (testable): load install + skill; find the harness definition's first `skill_paths` entry; check adapter capabilities (`supports_symlinked_skills`) — if false, `BindOutcome { binding_type: "unsupported", ... }` + warning, no row; else `link_directory(skill.canonical_path, home.join(skill_path).join(skill.name))`, map `LinkOutcome` → `BindingType` (Symlink/Junction/Copy/AlreadyLinked→existing row ok), insert binding row. `unbind_skill_cmd` removes via `std::fs::remove_file` if symlink/junction.

- [ ] **Step 3: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
git add apps/desktop crates/database
git commit -m "feat(phase10): skill bindings with link abstraction"
```

---

### Task 10.3: Conflict Detection

**Files:**
- Modify: `apps/desktop/src-tauri/src/skill_lib.rs` (`detect_conflicts`)
- Modify: `apps/desktop/src-tauri/src/commands/skills.rs` (`skill_conflicts_cmd`)
- Create: `apps/desktop/src-tauri/tests/conflicts.rs`

**Interfaces:**
- Produces:
  - `pub fn detect_conflicts(registry: &[Skill], harness_skills: &[HarnessSkill]) -> Vec<Conflict>` where `Conflict { pub kind: String, pub name: String, pub detail: String }`
  - `pub enum ConflictKind { DuplicateName, ContentMismatch, Shadowed, BrokenSymlink, MissingSource, IncompatibleTarget, UnsupportedSymlink }` (serialized as string)
  - `#[tauri::command] pub async fn skill_conflicts_cmd(state, installation_id: String) -> Result<Vec<Conflict>, String>` — registry (DB) vs adapter `read_state().skills`.

- [ ] **Step 1: Write the failing tests `tests/conflicts.rs`**

```rust
#[test]
fn detects_content_mismatch_same_name() {
    // registry skill "x" hash A; harness skill "x" (same name) hash B → ContentMismatch
    let registry = vec![Skill {
        id: Uuid::new_v4(),
        name: "brainstorming".into(),
        canonical_path: "/reg/skills/brainstorming".into(),
        source_type: SkillSourceType::Folder,
        source_url: None,
        content_hash: Some("hash-A".into()),
        provenance: serde_json::json!({}),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }];
    let harness_skills = vec![HarnessSkill {
        name: "brainstorming".into(),
        path: "/harness/skills/brainstorming".into(),
        content_hash: Some("hash-B".into()),
        symlinked: false,
    }];
    let conflicts = detect_conflicts(&registry, &harness_skills);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, "content-mismatch");
}

#[test]
fn detects_broken_symlink_in_harness_skills() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dangling = tmp.path().join("dangling");
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("does-not-exist"), &dangling).unwrap();
    let harness_skills = vec![HarnessSkill {
        name: "ghost".into(),
        path: dangling.display().to_string(),
        content_hash: None,
        symlinked: true,
    }];
    let conflicts = detect_conflicts(&[], &harness_skills);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, "broken-symlink");
}

#[test]
fn no_conflicts_when_identical() {
    let registry = vec![Skill {
        id: Uuid::new_v4(),
        name: "brainstorming".into(),
        canonical_path: "/shared/skills/brainstorming".into(),
        source_type: SkillSourceType::Folder,
        source_url: None,
        content_hash: Some("same".into()),
        provenance: serde_json::json!({}),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }];
    let harness_skills = vec![HarnessSkill {
        name: "brainstorming".into(),
        path: "/harness/skills/brainstorming".into(),
        content_hash: Some("same".into()),
        symlinked: false,
    }];
    assert!(detect_conflicts(&registry, &harness_skills).is_empty());
}
```

- [ ] **Step 2: Implement**

`detect_conflicts`: match registry vs harness skills by name — same name + different hash → `ContentMismatch` (with both hashes in detail); registry skill whose canonical_path is a symlink with missing target → `BrokenSymlink`; registry path under a harness's skill dir AND not the harness's own binding → `Shadowed`; binding target existing as real dir (not symlink to source) → `IncompatibleTarget`; adapter capability false + any registry skill → `UnsupportedSymlink` (per harness). Dedup by `(kind, name)`.

- [ ] **Step 3: Wire into `SkillsScreen`** — "Conflicts" panel per harness: list with kind badges + detail; actions: "Re-link" (unbind + bind), "Ignore" (dismiss for session).

- [ ] **Step 4: Verify + commit**

```bash
cd apps/desktop/src-tauri && cargo test
cd ../ && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase10): skill conflict detection and UI"
```

---

### Task 10.4: Git Import + Phase Exit

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/skills.rs` (`import_skill_from_git_cmd`)
- Create: `apps/desktop/src-tauri/tests/git_import.rs`

**Interfaces:**
- Produces: `#[tauri::command] pub async fn import_skill_from_git_cmd(state, repo_url: String, dest_dir: String) -> Result<ImportSkillReport, String>` — `git clone --depth 1 <url> <dest_dir>/<repo-name>` via tokio `Command`, then `import_skills_core` on the cloned dir; records `source_type: Git`, `source_url: repo_url`.

- [ ] **Step 1: Write the test** (uses a local git repo created in the temp dir as the "remote" — no network):

```rust
#[tokio::test]
async fn git_import_clones_and_imports() {
    // init a local repo with a skill dir, commit, then import from its path
    // assert skill row exists with source_type Git + source_url set
}
```

- [ ] **Step 2: Implement + UI button ("Import from Git repository…" — URL input modal)**

- [ ] **Step 3: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/desktop && npm run lint && npm run build
git add apps/desktop
git commit -m "feat(phase10): git skill import"
```

Phase complete when all steps green.