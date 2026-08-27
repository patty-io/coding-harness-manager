//! Frontend log forwarding + command instrumentation.

/// Called by the webview's error/console hooks so JS crashes land in chm.log.
#[tauri::command]
pub async fn frontend_log_cmd(
    level: String,
    message: String,
    location: Option<String>,
) -> Result<(), String> {
    let loc = location.unwrap_or_default();
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "{message} ({loc})"),
        "warn" => tracing::warn!(target: "frontend", "{message} ({loc})"),
        _ => tracing::info!(target: "frontend", "{message} ({loc})"),
    }
    Ok(())
}
