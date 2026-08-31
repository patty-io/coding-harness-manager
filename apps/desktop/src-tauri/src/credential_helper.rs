//! Minimal, non-interactive credential helper used by native harnesses.
//!
//! Harnesses such as Codex and Claude Code execute this helper on demand. It
//! resolves a credential reference from CHM's database and OS secret store,
//! writes only the secret to stdout, and never includes references or secret
//! values in diagnostics.

use chm_core::domain::credentials::CredentialKind;
use chm_database::repos::providers::get_credential_ref;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug)]
enum HelperRequest {
    CredentialRef(Uuid),
    Binding(Uuid),
}

fn parse_request<I>(args: I) -> Result<HelperRequest, String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let mut args = args.iter().map(String::as_str);
    match args.next() {
        Some("read") => {}
        Some(_) => return Err("unsupported credential-helper command".into()),
        None => return Err("credential-helper command is required".into()),
    }
    let kind = args
        .next()
        .ok_or_else(|| "credential-helper target is required".to_string())?;
    let value = args
        .next()
        .ok_or_else(|| "credential-helper target value is required".to_string())?;
    if args.next().is_some() {
        return Err("too many credential-helper arguments".into());
    }
    let id = Uuid::parse_str(value).map_err(|_| "credential-helper target is invalid".to_string())?;
    match kind {
        "--credential-ref" => Ok(HelperRequest::CredentialRef(id)),
        "--binding" => Ok(HelperRequest::Binding(id)),
        _ => Err("unsupported credential-helper target".into()),
    }
}

fn database_path() -> Result<PathBuf, String> {
    let dir = crate::app_data_dir();
    std::fs::create_dir_all(&dir).map_err(|_| "credential database is unavailable".to_string())?;
    Ok(dir.join("chm.sqlite"))
}

async fn resolve_request(request: HelperRequest) -> Result<String, String> {
    let path = database_path()?;
    let pool = chm_database::connect(
        path.to_str()
            .ok_or_else(|| "credential database path is invalid".to_string())?,
    )
    .await
    .map_err(|_| "credential database is unavailable".to_string())?;

    let credential_id = match request {
        HelperRequest::CredentialRef(id) => {
            // A helper command is embedded in a harness config, so only
            // return refs currently attached to a provider endpoint. This
            // prevents an arbitrary UUID from becoming a database oracle.
            let referenced = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM provider_endpoints WHERE credential_ref_id = ?)",
            )
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .map_err(|_| "credential database is unavailable".to_string())?;
            if referenced != 1 {
                return Err("credential is unavailable".into());
            }
            id
        }
        HelperRequest::Binding(binding_id) => {
            let native_config = sqlx::query_scalar::<_, String>(
                "SELECT native_config_json FROM harness_model_bindings WHERE id = ? AND managed = 1",
            )
            .bind(binding_id.to_string())
            .fetch_optional(&pool)
            .await
            .map_err(|_| "credential database is unavailable".to_string())?
            .ok_or_else(|| "credential is unavailable".to_string())?;
            let config: serde_json::Value = serde_json::from_str(&native_config)
                .map_err(|_| "credential is unavailable".to_string())?;
            let value = config
                .get("credential_ref_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "credential is unavailable".to_string())?;
            Uuid::parse_str(value).map_err(|_| "credential is unavailable".to_string())?
        }
    };

    let credential = get_credential_ref(&pool, credential_id)
        .await
        .map_err(|_| "credential is unavailable".to_string())?;
    let value = match credential.kind {
        CredentialKind::Env => std::env::var(&credential.reference).ok(),
        _ => chm_secrets::default_store()
            .get(&credential.reference)
            .map_err(|_| "credential is unavailable".to_string())?,
    }
    .filter(|value| !value.trim().is_empty())
    .ok_or_else(|| "credential is unavailable".to_string())?;
    Ok(value)
}

/// Run the helper and return a process exit code. Diagnostics are deliberately
/// generic because this process is commonly invoked by a shell hook.
pub fn run<I>(args: I) -> i32
where
    I: IntoIterator<Item = String>,
{
    let request = match parse_request(args) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("credential helper unavailable");
            return 1;
        }
    };
    match runtime.block_on(resolve_request(request)) {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

