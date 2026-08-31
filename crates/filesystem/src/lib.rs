//! Filesystem safety layer: atomic writes, backups, directory links.
//! THE ONLY module allowed to mutate native files.

use sha2::{Digest, Sha256};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("concurrent change detected for {0}")]
    ConcurrentChange(String),
}

fn content_hash(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Apply-time guard for files that may contain credentials. The original
/// bytes stay in memory only; the guard never serializes or logs them.
#[derive(Debug, Clone)]
pub struct ProtectedWriteGuard {
    path: PathBuf,
    before: Option<Vec<u8>>,
    /// The hash that must still be present before the next guarded mutation.
    /// It starts at the preflight snapshot and advances after each successful
    /// replace, allowing a rollback to restore the original bytes without
    /// treating CHM's own write as an external concurrent change.
    expected_hash: Arc<Mutex<Option<[u8; 32]>>>,
    #[cfg(unix)]
    before_mode: Option<u32>,
}

impl ProtectedWriteGuard {
    pub fn capture(path: &Path) -> Result<Self, FsError> {
        let before = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(FsError::Io(error)),
        };
        #[cfg(unix)]
        let before_mode = std::fs::metadata(path).ok().map(|metadata| {
            std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()) & 0o777
        });
        Ok(Self {
            expected_hash: Arc::new(Mutex::new(before.as_deref().map(content_hash))),
            path: path.to_path_buf(),
            before,
            #[cfg(unix)]
            before_mode,
        })
    }

    fn assert_unchanged(&self) -> Result<(), FsError> {
        let current = match std::fs::read(&self.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(FsError::Io(error)),
        };
        let current_hash = current.as_deref().map(content_hash);
        let expected_hash = self
            .expected_hash
            .lock()
            .map_err(|_| FsError::ConcurrentChange(self.path.display().to_string()))?;
        if current_hash != *expected_hash {
            return Err(FsError::ConcurrentChange(self.path.display().to_string()));
        }
        Ok(())
    }

    /// Verify that the guarded target still matches the bytes captured during
    /// preflight. This is intentionally read-only and lets a coordinator
    /// check protected files before unrelated native files are written.
    pub fn verify_unchanged(&self) -> Result<(), FsError> {
        self.assert_unchanged()
    }

    /// Return the guarded path without exposing the protected file contents.
    /// Coordinators use this for safe, human-readable apply reports.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn replace(&self, bytes: &[u8], mode: u32) -> Result<(), FsError> {
        self.assert_unchanged()?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| FsError::InvalidPath(self.path.display().to_string()))?;
        std::fs::create_dir_all(parent)?;
        let tmp = self
            .path
            .with_extension(format!("chm-protected-tmp-{}", std::process::id()));
        {
            let mut file = std::fs::File::create(&tmp)?;
            #[cfg(unix)]
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        self.assert_unchanged()?;
        match std::fs::rename(&tmp, &self.path) {
            Ok(()) => {
                let mut expected_hash = self
                    .expected_hash
                    .lock()
                    .map_err(|_| FsError::ConcurrentChange(self.path.display().to_string()))?;
                *expected_hash = Some(content_hash(bytes));
                Ok(())
            }
            Err(_error) if cfg!(windows) && std::fs::metadata(&self.path).is_ok() => {
                std::fs::remove_file(&self.path)?;
                std::fs::rename(&tmp, &self.path)?;
                let mut expected_hash = self
                    .expected_hash
                    .lock()
                    .map_err(|_| FsError::ConcurrentChange(self.path.display().to_string()))?;
                *expected_hash = Some(content_hash(bytes));
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::remove_file(&tmp);
                Err(FsError::Io(error))
            }
        }
    }

    pub fn restore(&self) -> Result<(), FsError> {
        self.assert_unchanged()?;
        match &self.before {
            Some(bytes) => self.replace(bytes, {
                #[cfg(unix)]
                {
                    self.before_mode.unwrap_or(0o600)
                }
                #[cfg(not(unix))]
                {
                    0
                }
            }),
            None => match std::fs::remove_file(&self.path) {
                Ok(()) => {
                    let mut expected_hash = self
                        .expected_hash
                        .lock()
                        .map_err(|_| FsError::ConcurrentChange(self.path.display().to_string()))?;
                    *expected_hash = None;
                    Ok(())
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(FsError::Io(error)),
            },
        }
    }
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
    match std::fs::read_to_string(backup) {
        Ok(content) => atomic_write(target, &content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // no backup for this file = it did not exist before apply — remove it
            match std::fs::remove_file(target) {
                Ok(()) => Ok(()),
                Err(e2) if e2.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e2) => Err(FsError::Io(e2)),
            }
        }
        Err(e) => Err(FsError::Io(e)),
    }
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
