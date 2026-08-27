//! Filesystem safety layer: atomic writes, backups, directory links.
//! THE ONLY module allowed to mutate native files.

use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub fn atomic_write(path: &Path, content: &str) -> Result<(), FsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("chm-tmp-{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_e) if cfg!(windows) && std::fs::metadata(path).is_ok() => {
            // Windows rename-over-existing: replace via remove+rename (caller backed up first)
            std::fs::remove_file(path)?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(FsError::Io(e))
        }
    }
}

pub fn backup_file(path: &Path) -> Result<PathBuf, FsError> {
    let parent = path
        .parent()
        .ok_or_else(|| FsError::InvalidPath(path.display().to_string()))?;
    let backups_dir = parent.join(".chm-backups");
    std::fs::create_dir_all(&backups_dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%f");
    let file_name = path
        .file_name()
        .ok_or_else(|| FsError::InvalidPath(path.display().to_string()))?;
    let backup = backups_dir.join(format!("{stamp}-{}", file_name.to_string_lossy()));
    if path.exists() {
        std::fs::copy(path, &backup)?;
    }
    Ok(backup)
}

pub fn restore_backup(backup: &Path, target: &Path) -> Result<(), FsError> {
    let content = std::fs::read_to_string(backup)?;
    atomic_write(target, &content)
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkOutcome {
    Symlink,
    Junction,
    Copy,
    AlreadyLinked,
    Unsupported(String),
}

#[cfg(unix)]
pub fn link_directory(source: &Path, target: &Path) -> Result<LinkOutcome, FsError> {
    if std::fs::symlink_metadata(target).is_ok() {
        if let Ok(link_target) = std::fs::read_link(target) {
            if link_target == source {
                return Ok(LinkOutcome::AlreadyLinked);
            }
            std::fs::remove_file(target)?; // broken/foreign symlink — replace
        } else {
            return Ok(LinkOutcome::Copy); // real dir exists — treat as shared, do not clobber
        }
    }
    match std::os::unix::fs::symlink(source, target) {
        Ok(()) => Ok(LinkOutcome::Symlink),
        Err(e)
            if e.raw_os_error() == Some(libc::EPERM) || e.raw_os_error() == Some(libc::EACCES) =>
        {
            copy_tree(source, target)?;
            Ok(LinkOutcome::Copy)
        }
        Err(e) => Err(FsError::Io(e)),
    }
}

#[cfg(windows)]
pub fn link_directory(source: &Path, target: &Path) -> Result<LinkOutcome, FsError> {
    let out = std::process::Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &target.display().to_string(),
            &source.display().to_string(),
        ])
        .output()?;
    if out.status.success() {
        return Ok(LinkOutcome::Junction);
    }
    copy_tree(source, target)?;
    Ok(LinkOutcome::Copy)
}

#[cfg(unix)]
fn copy_tree(source: &Path, target: &Path) -> Result<(), FsError> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn copy_tree(source: &Path, target: &Path) -> Result<(), FsError> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
