//! Shared adapter helpers: detection, skills scanning, MCP parsing.
//! Consolidates logic that every harness adapter previously duplicated.

use std::path::Path;

use chm_core::domain::harness::{HarnessInstallation, InstallationStatus};
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chrono::Utc;
use uuid::Uuid;

use crate::adapter::types::{ApplyResult, HarnessSkill, NativePlan};
use crate::definition::{HarnessDefinition, Platform};

/// One harness through the standard detection pipeline (executable → config →
/// version). Shared by `scan()` and every adapter's `detect()`.
pub fn detect_one(
    def: &HarnessDefinition,
    home: &Path,
    platform: Platform,
    path_env: Option<&str>,
) -> Option<HarnessInstallation> {
    if def.detection_only {
        let exe = def
            .executable_names
            .iter()
            .find_map(|n| crate::detect::paths::find_executable(n, path_env))?;
        return Some(HarnessInstallation {
            id: Uuid::new_v4(),
            harness_type: chm_core::domain::harness::HarnessType::Custom(def.id.to_string()),
            executable_path: Some(exe),
            version: None,
            config_path: None,
            detected_at: Utc::now(),
            last_scanned_at: None,
            status: InstallationStatus::Detected,
        });
    }
    let exe = def
        .executable_names
        .iter()
        .find_map(|n| crate::detect::paths::find_executable(n, path_env));
    let config = crate::detect::paths::resolve_config_path(def, home, platform);
    match (exe, config) {
        (None, None) => None,
        (Some(executable_path), config_path) => {
            let version = crate::detect::version::detect_version(
                &executable_path,
                crate::detect::version::version_args_for(def),
            );
            Some(HarnessInstallation {
                id: Uuid::new_v4(),
                harness_type: chm_core::domain::harness::HarnessType::parse_str(def.id),
                executable_path: Some(executable_path),
                version,
                config_path: config_path.map(|c| c.display().to_string()),
                detected_at: Utc::now(),
                last_scanned_at: Some(Utc::now()),
                status: InstallationStatus::Installed,
            })
        }
        (None, Some(config_path)) => Some(HarnessInstallation {
            id: Uuid::new_v4(),
            harness_type: chm_core::domain::harness::HarnessType::parse_str(def.id),
            executable_path: None,
            version: None,
            config_path: Some(config_path.display().to_string()),
            detected_at: Utc::now(),
            last_scanned_at: Some(Utc::now()),
            status: InstallationStatus::ConfigMissing,
        }),
    }
}

/// Scans a skills directory (subdirs with SKILL.md convention). Empty dir or
/// missing dir yields an empty list.
pub fn scan_skills_dir(dir: &Path) -> Vec<HarnessSkill> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            out.push(HarnessSkill {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path().display().to_string(),
                content_hash: None,
                symlinked: entry
                    .path()
                    .symlink_metadata()
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false),
            });
        }
    }
    out
}

/// Derives the home directory from a config path like `~/.<dot-dir>/...`
/// (walks ancestors looking for the dot-dir component).
pub fn install_home_from_config(config_path: &str, dot_dir: &str) -> std::path::PathBuf {
    let path = Path::new(config_path);
    for ancestor in path.ancestors() {
        if ancestor.file_name().is_some_and(|f| f == dot_dir) {
            return ancestor.parent().map(Path::to_path_buf).unwrap_or_default();
        }
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// Return the command and arguments for CHM's fixed credential helper.
///
/// The desktop binary can service helper requests itself (the command is
/// invoked with `--credential-helper`), while adapter tests and package-level
/// installs can provide a standalone `chm-credential-helper` on PATH. An
/// explicit override is useful for packaged launchers and never accepts a
/// command from a harness configuration file.
pub fn credential_helper_invocation(credential_ref_id: Uuid) -> (String, Vec<String>) {
    let suffix = credential_ref_id.to_string();
    if let Some(command) = std::env::var_os("CHM_CREDENTIAL_HELPER_COMMAND") {
        return (
            command.to_string_lossy().into_owned(),
            vec!["read".into(), "--credential-ref".into(), suffix],
        );
    }
    if let Ok(executable) = std::env::current_exe()
        && executable
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("coding-harness-manager"))
    {
        return (
            executable.display().to_string(),
            vec![
                "--credential-helper".into(),
                "read".into(),
                "--credential-ref".into(),
                suffix,
            ],
        );
    }
    (
        "chm-credential-helper".into(),
        vec!["read".into(), "--credential-ref".into(), suffix],
    )
}

/// Shell-safe form used by Claude Code's documented `apiKeyHelper` setting.
/// Claude invokes this string through the user's shell; every component is
/// quoted so a path or future helper argument cannot become shell syntax.
pub fn credential_helper_shell_command(credential_ref_id: Uuid) -> String {
    let (command, args) = credential_helper_invocation(credential_ref_id);
    std::iter::once(command)
        .chain(args)
        .map(|part| format!("'{}'", part.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a file if present; missing files yield `None`, other errors propagate.
pub fn read_optional(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Shared JSON MCP server mapping used by the JSON-config harnesses.
/// Preserves headers and directTools losslessly inside `env` so native
/// translation (Phase 8/9) can round-trip them.
pub fn parse_mcp_json(
    name: &str,
    spec: &serde_json::Value,
    provenance: serde_json::Value,
) -> McpServer {
    // MCP clients use a few spellings for the same transport.  Normalize the
    // explicit type/transportType first, then fall back to the documented
    // URL-vs-command shapes.  In particular, Cline uses `streamableHttp`
    // while Roo/Continue use `streamable-http`.
    let transport_type = spec
        .get("type")
        .or_else(|| spec.get("transportType"))
        .or_else(|| spec.get("transport"))
        .and_then(|value| value.as_str())
        .map(|value| {
            value
                .chars()
                .filter(|character| *character != '-' && *character != '_')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        });
    let transport = match transport_type.as_deref() {
        Some("remote") | Some("http") | Some("streamablehttp") => McpTransport::Http,
        Some("sse") => McpTransport::Sse,
        _ if spec.get("httpUrl").is_some() => McpTransport::Http,
        _ if spec.get("url").is_some() && spec.get("command").is_none() => McpTransport::Sse,
        _ => McpTransport::Stdio,
    };
    let command_array: Vec<String> = spec
        .get("command")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let (command, args) = match command_array.split_first() {
        Some((first, rest)) => (Some(first.clone()), rest.to_vec()),
        None => (
            spec.get("command")
                .and_then(|v| v.as_str())
                .map(String::from),
            spec.get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        ),
    };
    let mut env: serde_json::Map<String, serde_json::Value> = spec
        .get("environment")
        .or_else(|| spec.get("env"))
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if let Some(headers) = spec.get("headers").and_then(|v| v.as_object()) {
        env.insert("headers".into(), serde_json::Value::Object(headers.clone()));
    }
    if let Some(dt) = spec.get("directTools") {
        env.insert("_direct_tools".into(), dt.clone());
    }
    McpServer {
        id: Uuid::new_v4(),
        name: name.to_string(),
        transport,
        command,
        args,
        url: spec
            .get("url")
            .or_else(|| spec.get("httpUrl"))
            .and_then(|v| v.as_str())
            .map(String::from),
        env,
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance,
        enabled: spec
            .get("enabled")
            .and_then(|value| value.as_bool())
            .or_else(|| {
                spec.get("disabled")
                    .and_then(|value| value.as_bool())
                    .map(|disabled| !disabled)
            })
            .unwrap_or(true),
    }
}

/// Applies a native plan: backups + atomic writes for every change.
/// The sync flow ALSO backs up before apply; this helper is for adapter
/// direct-apply paths (bind_mcp_sync).
pub fn apply_native_plan(plan: &NativePlan) -> Result<ApplyResult, String> {
    let mut result = ApplyResult {
        files_written: vec![],
        links_created: vec![],
    };
    for change in &plan.changes {
        let after = change.after.clone().ok_or("change without after content")?;
        let _ = chm_filesystem::backup_file(std::path::Path::new(&change.file_path));
        chm_filesystem::atomic_write(std::path::Path::new(&change.file_path), &after)
            .map_err(|e| e.to_string())?;
        result.files_written.push(change.file_path.clone());
    }
    Ok(result)
}
