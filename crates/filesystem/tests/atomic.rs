use chm_filesystem::{LinkOutcome, atomic_write, backup_file, link_directory, restore_backup};
use tempfile::TempDir;

#[test]
fn atomic_write_replaces_content() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("config.json");
    atomic_write(&f, "{\"a\":1}").unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "{\"a\":1}");
    atomic_write(&f, "{\"a\":2}").unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "{\"a\":2}");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("chm-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp files must be cleaned up");
}

#[test]
fn atomic_write_creates_parent_dirs() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("nested/deeper/config.toml");
    atomic_write(&f, "[x]").unwrap();
    assert!(f.exists());
}

#[test]
fn backup_then_restore_roundtrip() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("config.toml");
    atomic_write(&f, "before").unwrap();
    let backup = backup_file(&f).unwrap();
    assert!(backup.exists());
    atomic_write(&f, "after").unwrap();
    restore_backup(&backup, &f).unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "before");
}

#[test]
fn link_directory_creates_symlink_or_reports_outcome() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("skills/brainstorming");
    std::fs::create_dir_all(&source).unwrap();
    let target = dir.path().join("linked-skills");
    let outcome = link_directory(&source, &target).unwrap();
    match outcome {
        LinkOutcome::Symlink => {
            assert!(
                std::fs::symlink_metadata(&target)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            let again = link_directory(&source, &target).unwrap();
            assert!(matches!(again, LinkOutcome::AlreadyLinked));
        }
        LinkOutcome::Copy => {
            assert!(target.exists());
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn restore_backup_missing_backup_errors() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("config.toml");
    let res = restore_backup(&dir.path().join("nope.bak"), &f);
    assert!(res.is_err());
}
