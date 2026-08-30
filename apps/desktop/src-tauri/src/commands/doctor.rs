//! Doctor diagnostics + redaction (Phase 13).

use chm_core::domain::history::{TransactionStatus, TransactionType};
use chm_database::repos::harness::list_installations;
use chm_database::repos::history::{begin_transaction, finish_transaction};
use chm_database::repos::providers::{list_endpoints, list_providers};
use chm_database::repos::skills::list_skills;
use chm_providers::{discover_models, health_check, resolve_credential};
use chrono::Utc;
use regex::Regex;
use serde::Serialize;
use serde_json::json;
use sqlx::{Pool, Sqlite};
use tauri::State;

pub use crate::commands::mcp::CheckResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessCheckGroup {
    pub harness_type: String,
    pub version: Option<String>,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCheckGroup {
    pub provider_name: String,
    pub endpoint_name: String,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub generated_at: String,
    pub app_version: String,
    pub harness_checks: Vec<HarnessCheckGroup>,
    pub provider_checks: Vec<ProviderCheckGroup>,
    pub mcp_checks: Vec<crate::commands::mcp::CheckResult>,
    pub skill_checks: Vec<CheckResult>,
    pub system_checks: Vec<CheckResult>,
    pub summary: String,
}

pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    let patterns = [
        r"sk-(ant-)?[A-Za-z0-9_\-]{8,}",
        r"ghp_[A-Za-z0-9]{20,}",
        r"github_pat_[A-Za-z0-9_]{20,}",
        r"Bearer [A-Za-z0-9._\-]{8,}",
        r#"x-api-key["'\s:=]+[A-Za-z0-9._\-]{8,}"#,
        r#"(?i)([?&](?:api[_-]?key|access[_-]?token|token|secret|password)=)[^&\s"']+"#,
        r"(?i)(https?://[^/\s:@]+:)[^@/\s]+(@)",
    ];
    for pat in patterns {
        if let Ok(re) = Regex::new(pat) {
            out = re.replace_all(&out, "<REDACTED>").into_owned();
        }
    }
    out
}

async fn harness_checks(pool: &Pool<Sqlite>) -> Result<Vec<HarnessCheckGroup>, String> {
    let installs = list_installations(pool).await.map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    for inst in &installs {
        let mut checks = Vec::new();
        // executable exists
        match &inst.executable_path {
            Some(exe) => {
                let exists = std::path::Path::new(exe).is_file();
                checks.push(CheckResult {
                    check: "executable exists".into(),
                    passed: exists,
                    detail: exe.clone(),
                });
            }
            None => checks.push(CheckResult {
                check: "executable exists".into(),
                passed: false,
                detail: "not detected on PATH".into(),
            }),
        }
        // config readable / parse valid
        match &inst.config_path {
            Some(path) => {
                checks.push(CheckResult {
                    check: "config path set".into(),
                    passed: true,
                    detail: path.clone(),
                });
                match std::fs::read_to_string(path) {
                    Ok(_) => checks.push(CheckResult {
                        check: "config readable".into(),
                        passed: true,
                        detail: String::new(),
                    }),
                    Err(e) => checks.push(CheckResult {
                        check: "config readable".into(),
                        passed: false,
                        detail: e.to_string(),
                    }),
                }
                let parse_result = crate::commands::sync::adapter_for(inst.harness_type.as_str())
                    .ok_or_else(|| format!("no adapter for {}", inst.harness_type.as_str()))
                    .and_then(|adapter| adapter.read_state(inst).map_err(|e| e.to_string()));
                checks.push(CheckResult {
                    check: "config parses".into(),
                    passed: parse_result.is_ok(),
                    detail: parse_result
                        .err()
                        .unwrap_or_else(|| "adapter parsed the installed state".into()),
                });
                let config_path = std::path::Path::new(path);
                let writable = std::fs::metadata(config_path)
                    .map(|meta| !meta.permissions().readonly())
                    .unwrap_or(false);
                checks.push(CheckResult {
                    check: "config writable".into(),
                    passed: writable,
                    detail: if writable {
                        "file permissions allow updates".into()
                    } else {
                        "file is missing or read-only".into()
                    },
                });
                let backup_dir = config_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."));
                let backup_ready = std::fs::metadata(backup_dir)
                    .map(|meta| meta.is_dir() && !meta.permissions().readonly())
                    .unwrap_or(false);
                checks.push(CheckResult {
                    check: "backup location ready".into(),
                    passed: backup_ready,
                    detail: backup_dir.display().to_string(),
                });
            }
            None => checks.push(CheckResult {
                check: "config readable".into(),
                passed: false,
                detail: "no config detected".into(),
            }),
        }
        let version_ok = inst
            .version
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty());
        checks.push(CheckResult {
            check: "version detected".into(),
            passed: version_ok,
            detail: inst.version.clone().unwrap_or_else(|| "unknown".into()),
        });
        groups.push(HarnessCheckGroup {
            harness_type: inst.harness_type.as_str().to_string(),
            version: inst.version.clone(),
            checks,
        });
    }
    Ok(groups)
}

async fn provider_checks(
    pool: &Pool<Sqlite>,
    secrets: &dyn chm_secrets::SecretStore,
    http: &reqwest::Client,
) -> Result<Vec<ProviderCheckGroup>, String> {
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    for p in &providers {
        for e in list_endpoints(pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            let cred =
                resolve_credential(e.credential_ref.as_ref().unwrap_or(&fake_ref()), secrets);
            let status = health_check(&e, cred.as_deref(), http).await;
            let reachable = !matches!(status, chm_providers::HealthStatus::Unreachable);
            let auth_ok = !matches!(
                status,
                chm_providers::HealthStatus::AuthFailed
                    | chm_providers::HealthStatus::Unreachable
                    | chm_providers::HealthStatus::MalformedResponse
                    | chm_providers::HealthStatus::Unknown
            );
            let mut checks = vec![
                CheckResult {
                    check: "endpoint reachable".into(),
                    passed: reachable,
                    detail: format!("{:?} ({})", status, e.base_url),
                },
                CheckResult {
                    check: "authentication works".into(),
                    passed: auth_ok,
                    detail: if auth_ok {
                        "accepted".into()
                    } else {
                        "credentials rejected".into()
                    },
                },
            ];
            if !matches!(e.protocol, chm_core::domain::provider::Protocol::Custom)
                && e.discovery_path.is_some()
            {
                match discover_models(&e, cred.as_deref(), http).await {
                    Ok(models) => checks.push(CheckResult {
                        check: "discovery works".into(),
                        passed: true,
                        detail: format!("{} models advertised", models.len()),
                    }),
                    Err(err) => checks.push(CheckResult {
                        check: "discovery works".into(),
                        passed: false,
                        detail: format!("discovery error: {err}"),
                    }),
                }
            }
            groups.push(ProviderCheckGroup {
                provider_name: p.display_name.clone(),
                endpoint_name: e.name.clone(),
                checks,
            });
        }
    }
    Ok(groups)
}

fn fake_ref() -> chm_core::domain::credentials::CredentialRef {
    use chrono::Utc;
    chm_core::domain::credentials::CredentialRef {
        id: uuid::Uuid::nil(),
        kind: chm_core::domain::credentials::CredentialKind::Env,
        reference: String::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub async fn run_doctor_core(
    pool: &Pool<Sqlite>,
    secrets: &dyn chm_secrets::SecretStore,
    http: &reqwest::Client,
) -> Result<DoctorReport, String> {
    let harness = harness_checks(pool).await?;
    let provider = provider_checks(pool, secrets, http).await?;
    let mcp = crate::commands::mcp::mcp_list_servers_for_doctor(pool).await?;
    let mut system_checks = Vec::new();
    match sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(pool)
        .await
    {
        Ok(1) => system_checks.push(CheckResult {
            check: "database health".into(),
            passed: true,
            detail: "SQLite responded to a health query".into(),
        }),
        Ok(value) => system_checks.push(CheckResult {
            check: "database health".into(),
            passed: false,
            detail: format!("unexpected health result {value}; restart the app"),
        }),
        Err(error) => system_checks.push(CheckResult {
            check: "database health".into(),
            passed: false,
            detail: format!("{error}; restart the app or restore a database backup"),
        }),
    }
    match secrets.get("__chm_doctor_probe__") {
        Ok(_) => system_checks.push(CheckResult {
            check: "secret store available".into(),
            passed: true,
            detail: "secret store responded without reading a credential value".into(),
        }),
        Err(error) => system_checks.push(CheckResult {
            check: "secret store available".into(),
            passed: false,
            detail: format!("{error}; check the OS keychain or configured environment variables"),
        }),
    }
    let symlink_probe = std::env::temp_dir().join(format!(
        "chm-symlink-probe-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let symlink_target = symlink_probe.with_extension("target");
    let symlink_ok = std::fs::write(&symlink_target, b"probe")
        .and_then(|_| {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&symlink_target, &symlink_probe)
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(&symlink_target, &symlink_probe)
            }
        })
        .is_ok();
    let _ = std::fs::remove_file(&symlink_probe);
    let _ = std::fs::remove_file(&symlink_target);
    system_checks.push(CheckResult {
        check: "symlink capability".into(),
        passed: symlink_ok,
        detail: if symlink_ok {
            "temporary symlink probe succeeded".into()
        } else {
            "cannot create symlinks; use copy bindings or grant filesystem permission".into()
        },
    });
    let skill_checks: Vec<CheckResult> = list_skills(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|skill| {
            let path = std::path::Path::new(&skill.canonical_path);
            let exists = path.is_dir();
            let readable = exists && std::fs::read_dir(path).is_ok();
            CheckResult {
                check: format!("skill available: {}", skill.name),
                passed: readable,
                detail: if readable {
                    format!("{} is present and readable", skill.canonical_path)
                } else {
                    format!("missing or unreadable: {}", skill.canonical_path)
                },
            }
        })
        .collect();
    let total: usize = harness.iter().map(|g| g.checks.len()).sum::<usize>()
        + provider.iter().map(|g| g.checks.len()).sum::<usize>()
        + mcp.len()
        + skill_checks.len()
        + system_checks.len();
    let failures: Vec<String> = harness
        .iter()
        .flat_map(|g| {
            g.checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| format!("{}: {}", g.harness_type, c.check))
                .collect::<Vec<_>>()
        })
        .chain(provider.iter().flat_map(|g| {
            g.checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| format!("{}/{}", g.provider_name, c.check))
        }))
        .chain(
            skill_checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| c.check.clone()),
        )
        .chain(mcp.iter().filter(|c| !c.passed).map(|c| c.check.clone()))
        .chain(
            system_checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| c.check.clone()),
        )
        .collect();
    let checks_passed = total - failures.len();
    Ok(DoctorReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        harness_checks: harness,
        provider_checks: provider,
        mcp_checks: mcp,
        skill_checks,
        system_checks,
        summary: if failures.is_empty() {
            format!("{checks_passed}/{} checks passed", total)
        } else {
            format!("issues: {}", failures.join(", "))
        },
    })
}

use crate::AppState;

#[tauri::command]
pub async fn run_doctor_cmd(state: State<'_, AppState>) -> Result<DoctorReport, String> {
    let audit = begin_transaction(
        &state.pool,
        TransactionType::Manual,
        json!({"action":"doctor"}),
    )
    .await
    .map_err(|e| e.to_string())?;
    match run_doctor_core(&state.pool, state.secrets.as_ref(), &state.http).await {
        Ok(report) => {
            finish_transaction(
                &state.pool,
                audit.id,
                TransactionStatus::Succeeded,
                Some(format!("doctor diagnostics: {}", report.summary)),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(report)
        }
        Err(error) => {
            let _ = finish_transaction(
                &state.pool,
                audit.id,
                TransactionStatus::Failed,
                None,
                Some(error.clone()),
            )
            .await;
            Err(error)
        }
    }
}

pub async fn export_diagnostics_core(
    pool: &Pool<Sqlite>,
    secrets: &dyn chm_secrets::SecretStore,
    http: &reqwest::Client,
    dest_dir: &str,
) -> Result<String, String> {
    let report = run_doctor_core(pool, secrets, http).await?;
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    let redacted = redact(&json);
    let destination = if dest_dir.trim().is_empty() {
        crate::app_data_dir()
    } else {
        crate::expand_user_path(dest_dir.trim())
    };
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let path = destination.join(format!("chm-diagnostics-{stamp}.json"));
    std::fs::write(&path, redacted.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn export_diagnostics_cmd(
    state: State<'_, AppState>,
    dest_dir: String,
) -> Result<String, String> {
    let audit = begin_transaction(
        &state.pool,
        TransactionType::Manual,
        json!({"action":"export_diagnostics", "destination": dest_dir}),
    )
    .await
    .map_err(|e| e.to_string())?;
    match export_diagnostics_core(&state.pool, state.secrets.as_ref(), &state.http, &dest_dir).await
    {
        Ok(path) => {
            finish_transaction(
                &state.pool,
                audit.id,
                TransactionStatus::Succeeded,
                Some(format!("diagnostics exported to {path}")),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(path)
        }
        Err(error) => {
            let _ = finish_transaction(
                &state.pool,
                audit.id,
                TransactionStatus::Failed,
                None,
                Some(error.clone()),
            )
            .await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_common_credentials_without_touching_labels() {
        let input = r#"{"token":"sk-test-secret-12345678", "authorization":"Bearer abcdefghijklmnop", "name":"demo"}"#;
        let output = redact(input);
        assert!(!output.contains("sk-test-secret-12345678"));
        assert!(!output.contains("Bearer abcdefghijklmnop"));
        assert!(output.contains("<REDACTED>"));
        assert!(output.contains("demo"));
    }

    #[test]
    fn redacts_credentials_embedded_in_urls() {
        let input = "https://user:password@example.test/v1?api_key=secret-value&mode=fast";
        let output = redact(input);
        assert!(!output.contains("password@example"));
        assert!(!output.contains("api_key=secret-value"));
        assert!(output.contains("mode=fast"));
    }
}
