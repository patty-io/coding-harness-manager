//! Doctor diagnostics + redaction (Phase 13).

use chm_database::repos::harness::list_installations;
use chm_database::repos::providers::{list_endpoints, list_providers};
use chm_providers::{discover_models, health_check, resolve_credential};
use regex::Regex;
use serde::Serialize;
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
            }
            None => checks.push(CheckResult {
                check: "config readable".into(),
                passed: false,
                detail: "no config detected".into(),
            }),
        }
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
            let auth_ok = !matches!(status, chm_providers::HealthStatus::AuthFailed);
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
                        passed: matches!(err, chm_providers::ProviderError::Unreachable),
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
    let total: usize = harness.iter().map(|g| g.checks.len()).sum::<usize>()
        + provider.iter().map(|g| g.checks.len()).sum::<usize>()
        + mcp.len();
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
        .collect();
    let checks_passed = total - failures.len();
    Ok(DoctorReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        harness_checks: harness,
        provider_checks: provider,
        mcp_checks: mcp,
        skill_checks: vec![],
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
    run_doctor_core(&state.pool, state.secrets.as_ref(), &state.http).await
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
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let path = std::path::Path::new(dest_dir).join(format!("chm-diagnostics-{stamp}.json"));
    std::fs::write(&path, redacted.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn export_diagnostics_cmd(
    state: State<'_, AppState>,
    dest_dir: String,
) -> Result<String, String> {
    export_diagnostics_core(&state.pool, state.secrets.as_ref(), &state.http, &dest_dir).await
}
