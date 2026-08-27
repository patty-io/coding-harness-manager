//! Dashboard stats command.

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub harnesses: usize,
    pub providers: usize,
    pub models: usize,
    pub mcp: usize,
    pub skills: usize,
    pub drifted: usize,
}

#[tauri::command]
pub async fn dashboard_stats(state: State<'_, AppState>) -> Result<DashboardStats, String> {
    let pool = &state.pool;
    let counts = sqlx::query(
        "SELECT
           (SELECT COUNT(*) FROM harness_installations),
           (SELECT COUNT(*) FROM providers),
           (SELECT COUNT(*) FROM model_routes),
           (SELECT COUNT(*) FROM mcp_servers),
           (SELECT COUNT(*) FROM skills)",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(DashboardStats {
        harnesses: counts.get::<i64, _>(0) as usize,
        providers: counts.get::<i64, _>(1) as usize,
        models: counts.get::<i64, _>(2) as usize,
        mcp: counts.get::<i64, _>(3) as usize,
        skills: counts.get::<i64, _>(4) as usize,
        drifted: 0, // Phase 12
    })
}
