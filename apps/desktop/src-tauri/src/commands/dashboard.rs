//! Dashboard stats command.

use chm_database::repos::harness::list_installations;
use chm_database::repos::mcp::list_mcp_servers;
use chm_database::repos::models::list_routes;
use chm_database::repos::providers::list_providers;
use chm_database::repos::skills::list_skills;
use serde::Serialize;
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
    Ok(DashboardStats {
        harnesses: list_installations(pool)
            .await
            .map_err(|e| e.to_string())?
            .len(),
        providers: list_providers(pool).await.map_err(|e| e.to_string())?.len(),
        models: list_routes(pool).await.map_err(|e| e.to_string())?.len(),
        mcp: list_mcp_servers(pool)
            .await
            .map_err(|e| e.to_string())?
            .len(),
        skills: list_skills(pool).await.map_err(|e| e.to_string())?.len(),
        drifted: 0, // Phase 12
    })
}
