//! Generic executable + config-path detection. All inputs injectable for tests.

use std::path::{Path, PathBuf};

use crate::definition::{HarnessDefinition, Platform};

pub fn home_dir(platform: Platform, injected_home: Option<&Path>) -> PathBuf {
    if let Some(h) = injected_home {
        return h.to_path_buf();
    }
    match platform {
        Platform::MacOs | Platform::Linux => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default(),
        Platform::Windows => std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default(),
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn find_executable(name: &str, path_env: Option<&str>) -> Option<String> {
    let owned;
    let path_value = match path_env {
        Some(v) => v,
        None => {
            owned = std::env::var_os("PATH")?.to_string_lossy().into_owned();
            owned.as_str()
        }
    };
    for dir in std::env::split_paths(path_value) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate.display().to_string());
        }
    }
    None
}

pub fn resolve_config_path(
    def: &HarnessDefinition,
    home: &Path,
    platform: Platform,
) -> Option<PathBuf> {
    for candidate in def.config_paths {
        let full = home.join(candidate);
        if full.exists() {
            return Some(full);
        }
    }
    // Windows: also try %APPDATA%\<candidate basename> — e.g. .config/opencode → %APPDATA%\opencode
    if matches!(platform, Platform::Windows)
        && let Some(appdata) = std::env::var_os("APPDATA")
    {
        let base = PathBuf::from(appdata);
        for candidate in def.config_paths {
            let last = Path::new(candidate)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let full = base.join(&last);
            if full.exists() {
                return Some(full);
            }
        }
    }
    None
}
