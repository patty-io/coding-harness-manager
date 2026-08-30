//! Dashboard stats command.

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::AppState;

/// Describes how much of a detected harness the app can currently inspect.
/// Detection-only definitions intentionally have no adapter yet; they are not
/// broken installations and should not be presented as config read failures.
pub(crate) fn harness_support(harness_type: &str) -> &'static str {
    let detection_only = chm_harness_sdk::definition::all_definitions()
        .into_iter()
        .find(|definition| definition.id == harness_type)
        .is_some_and(|definition| definition.detection_only);
    if detection_only {
        "detection-only"
    } else if crate::commands::sync::adapter_for(harness_type).is_some() {
        "supported"
    } else {
        "unsupported"
    }
}

fn harness_display_name(harness_type: &str) -> String {
    chm_harness_sdk::definition::all_definitions()
        .into_iter()
        .find(|definition| definition.id == harness_type)
        .map(|definition| definition.name.to_string())
        .unwrap_or_else(|| harness_type.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub harnesses: usize,
    pub providers: usize,
    pub models: usize,
    pub mcp: usize,
    pub skills: usize,
    pub drifted: usize,
    pub harness_details: Vec<DashboardHarnessSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardHarnessSummary {
    pub installation_id: String,
    pub models: Option<usize>,
    pub mcp: Option<usize>,
    pub skills: Option<usize>,
    pub drifted: bool,
    pub state_error: Option<String>,
    pub support: String,
}

#[cfg(test)]
mod tests {
    use super::harness_support;

    #[test]
    fn classifies_kimi_as_detection_only() {
        assert_eq!(harness_support("kimi-cli"), "detection-only");
    }

    #[test]
    fn classifies_supported_and_unknown_harnesses() {
        assert_eq!(harness_support("pi"), "supported");
        assert_eq!(harness_support("made-up-harness"), "unsupported");
    }
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

    // Real drift count: compare each installation's config on disk with the
    // last state we wrote (same logic as the per-harness drift command).
    let installations = chm_database::repos::harness::list_installations(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut drifted = 0usize;
    let mut harness_details = Vec::with_capacity(installations.len());
    for inst in &installations {
        let (_, is_drifted, _, _) = crate::commands::drift::installation_drifted(pool, inst)
            .await
            .unwrap_or((false, false, None, None));
        if is_drifted {
            drifted += 1;
        }
        let support = harness_support(inst.harness_type.as_str()).to_string();
        if support == "detection-only" {
            let name = harness_display_name(inst.harness_type.as_str());
            harness_details.push(DashboardHarnessSummary {
                installation_id: inst.id.to_string(),
                models: None,
                mcp: None,
                skills: None,
                drifted: is_drifted,
                state_error: Some(format!(
                    "{name} is detected, but configuration reading is not supported yet"
                )),
                support,
            });
            continue;
        }
        let state = crate::commands::sync::adapter_for(inst.harness_type.as_str())
            .ok_or_else(|| format!("no adapter for {}", inst.harness_type.as_str()))
            .and_then(|adapter| adapter.read_state(inst).map_err(|e| e.to_string()));
        match state {
            Ok(parsed) => harness_details.push(DashboardHarnessSummary {
                installation_id: inst.id.to_string(),
                models: Some(parsed.models.len()),
                mcp: Some(parsed.mcp.len()),
                skills: Some(parsed.skills.len()),
                drifted: is_drifted,
                state_error: None,
                support: support.clone(),
            }),
            Err(error) => harness_details.push(DashboardHarnessSummary {
                installation_id: inst.id.to_string(),
                models: None,
                mcp: None,
                skills: None,
                drifted: is_drifted,
                state_error: Some(error),
                support: support.clone(),
            }),
        }
    }

    Ok(DashboardStats {
        harnesses: counts.get::<i64, _>(0) as usize,
        providers: counts.get::<i64, _>(1) as usize,
        models: counts.get::<i64, _>(2) as usize,
        mcp: counts.get::<i64, _>(3) as usize,
        skills: counts.get::<i64, _>(4) as usize,
        drifted,
        harness_details,
    })
}
