//! Configuration set commands.

use chm_core::domain::sets::SetItemType;
use chm_database::repos::models::list_routes;
use chm_database::repos::providers::{list_endpoints, list_providers};
use chm_database::repos::profiles::{
    add_set_item, delete_set, list_set_items, list_set_items_for_sets, list_sets, remove_set_item,
};
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetItemView {
    pub item_type: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub items: Vec<SetItemView>,
}

#[tauri::command]
pub async fn list_sets_cmd(state: State<'_, AppState>) -> Result<Vec<SetView>, String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let pool = &state.pool;
    let sets = list_sets(pool).await.map_err(|e| e.to_string())?;
    let set_ids: Vec<_> = sets.iter().map(|set| set.id).collect();
    let items_by_set = list_set_items_for_sets(pool, &set_ids)
        .await
        .map_err(|e| e.to_string())?;
    let mut views = Vec::new();
    for s in sets {
        let items = items_by_set.get(&s.id).cloned().unwrap_or_default();
        views.push(SetView {
            id: s.id.to_string(),
            name: s.name,
            description: s.description,
            items: items
                .into_iter()
                .map(|i| SetItemView {
                    item_type: i.item_type.as_str().to_string(),
                    item_id: i.item_id.to_string(),
                })
                .collect(),
        });
    }
    Ok(views)
}

#[tauri::command]
pub async fn create_set_cmd(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Result<String, String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let set = chm_database::repos::profiles::create_set(&state.pool, &name, description)
        .await
        .map_err(|e| e.to_string())?;
    Ok(set.id.to_string())
}

#[tauri::command]
pub async fn delete_set_cmd(state: State<'_, AppState>, set_id: String) -> Result<(), String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let set_id = Uuid::parse_str(&set_id).map_err(|e| e.to_string())?;
    delete_set(&state.pool, set_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_set_item_cmd(
    state: State<'_, AppState>,
    set_id: String,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let set_id = Uuid::parse_str(&set_id).map_err(|e| e.to_string())?;
    let item_id = Uuid::parse_str(&item_id).map_err(|e| e.to_string())?;
    let item_type = SetItemType::parse_str(&item_type);
    if item_type == SetItemType::LaunchProfile {
        return Err(
            "configuration sets contain models, MCP servers, and skills; launch profiles are launched separately"
                .into(),
        );
    }
    add_set_item(&state.pool, set_id, item_type, item_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_set_item_cmd(
    state: State<'_, AppState>,
    set_id: String,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let pool = &state.pool;
    let sid = Uuid::parse_str(&set_id).map_err(|e| e.to_string())?;
    let iid = Uuid::parse_str(&item_id).map_err(|e| e.to_string())?;
    let itype = SetItemType::parse_str(&item_type);
    remove_set_item(pool, sid, itype, iid)
        .await
        .map_err(|e| e.to_string())
}

/// DesiredState limited to the set's members (testable core).
pub async fn set_filtered_desired(
    pool: &Pool<Sqlite>,
    set_id: &str,
) -> Result<chm_harness_sdk::adapter::plan::DesiredState, String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let sid = Uuid::parse_str(set_id).map_err(|e| e.to_string())?;
    let items = list_set_items(pool, sid).await.map_err(|e| e.to_string())?;
    if items
        .iter()
        .any(|item| item.item_type == SetItemType::LaunchProfile)
    {
        return Err(
            "this set contains a launch profile, which cannot be applied as a resource set; remove it first"
                .into(),
        );
    }
    let routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let mcp = chm_database::repos::mcp::list_mcp_servers(pool)
        .await
        .map_err(|e| e.to_string())?;
    let skills = chm_database::repos::skills::list_skills(pool)
        .await
        .map_err(|e| e.to_string())?;
    // A set can contain many entries, and each collection is filtered once.
    // Build membership indexes up front instead of scanning the full item
    // list for every route/server/skill (which made preview cost O(n*m)).
    let model_ids: HashSet<Uuid> = items
        .iter()
        .filter(|i| i.item_type == SetItemType::ModelRoute)
        .map(|i| i.item_id)
        .collect();
    let mcp_ids: HashSet<Uuid> = items
        .iter()
        .filter(|i| i.item_type == SetItemType::McpServer)
        .map(|i| i.item_id)
        .collect();
    let skill_ids: HashSet<Uuid> = items
        .iter()
        .filter(|i| i.item_type == SetItemType::Skill)
        .map(|i| i.item_id)
        .collect();

    let routes = routes
        .into_iter()
        .filter(|r| r.enabled)
        .filter(|r| model_ids.contains(&r.id))
        .collect::<Vec<_>>();
    // Set sync uses the same provider bundles as full-library sync. Keeping
    // this metadata is essential: adapters need the provider, endpoint,
    // protocol, and credential reference to write a usable native config.
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut endpoints = Vec::new();
    for provider in &providers {
        endpoints.extend(
            list_endpoints(pool, provider.id)
                .await
                .map_err(|e| e.to_string())?,
        );
    }
    let provider_routes = crate::commands::sync::group_provider_routes(
        &routes,
        &providers,
        &endpoints,
    )?;
    let routes = provider_routes
        .iter()
        .flat_map(|bundle| {
            bundle
                .models
                .iter()
                .cloned()
                .map(|route| crate::commands::sync::route_for_provider_bundle(route, bundle))
        })
        .collect();

    Ok(chm_harness_sdk::adapter::plan::DesiredState {
        provider_routes,
        routes,
        mcp_servers: mcp
            .into_iter()
            .filter(|s| s.enabled)
            .filter(|s| mcp_ids.contains(&s.id))
            .collect(),
        skills: skills
            .into_iter()
            .filter(|sk| sk.enabled)
            .filter(|sk| skill_ids.contains(&sk.id))
            .collect(),
    })
}
/// Preview applying a set: same machinery as sync_preview with set-filtered desired.
#[tauri::command]
pub async fn apply_set_preview_cmd(
    state: State<'_, AppState>,
    set_id: String,
    installation_id: String,
    mode: String,
) -> Result<crate::commands::sync::PreviewReport, String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let pool = &state.pool;
    let desired = set_filtered_desired(pool, &set_id).await?;
    let mode = crate::commands::sync::parse_mode(&mode);
    let (_, _adapter, plan, native_plan) = crate::commands::sync::build_native_plan_for_desired(
        pool,
        &installation_id,
        &mode,
        desired,
    )
    .await?;
    crate::commands::sync::preview_report(&plan, &native_plan)
}

#[tauri::command]
pub async fn apply_set_cmd(
    state: State<'_, AppState>,
    set_id: String,
    installation_id: String,
    mode: String,
    plan_hash: String,
) -> Result<crate::commands::sync::ApplyReport, String> {
    crate::commands::settings::require_profiles_and_sets_enabled()?;
    let pool = &state.pool;
    let m = crate::commands::sync::parse_mode(&mode);
    let desired = set_filtered_desired(pool, &set_id).await?;
    crate::commands::sync::execute_desired_with_plan_using_secrets(
        pool,
        &installation_id,
        &m,
        false,
        Some(&plan_hash),
        None,
        desired,
        &*state.secrets,
    )
    .await
}
