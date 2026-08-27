//! Launch profile commands.

use chm_core::domain::harness::HarnessType;
use chm_core::domain::profiles::{LaunchProfile, RoleMapping};
use chm_database::repos::models::list_routes;
use chm_database::repos::profiles::{create_profile, list_profiles};
use chm_database::repos::providers::{list_endpoints, list_providers};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub harness_type: String,
    pub model_route_id: Option<String>,
    pub provider_endpoint_id: Option<String>,
    pub provider_name: Option<String>,
    pub model_display: Option<String>,
    pub env: serde_json::Value,
    pub role_mappings: Vec<RoleMappingView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleMappingView {
    pub role: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    pub name: String,
    pub harness_type: String,
    pub model_route_id: Option<String>,
    pub provider_endpoint_id: Option<String>,
    pub env: serde_json::Map<String, serde_json::Value>,
    pub role_mappings: Vec<RoleMappingInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleMappingInput {
    pub role: String,
    pub model: String,
}

pub async fn profile_views(pool: &Pool<Sqlite>) -> Result<Vec<ProfileView>, String> {
    let profiles = list_profiles(pool).await.map_err(|e| e.to_string())?;
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let mut views = Vec::new();
    for p in profiles {
        let mut provider_name = None;
        if let Some(ep_id) = p.provider_endpoint_id {
            for prov in &providers {
                if let Ok(ends) = list_endpoints(pool, prov.id).await
                    && ends.iter().any(|e| e.id == ep_id)
                {
                    provider_name = Some(prov.display_name.clone());
                    break;
                }
            }
        }
        let model_display = routes
            .iter()
            .find(|r| Some(r.id) == p.model_route_id)
            .map(|r| r.display_name.clone());
        views.push(ProfileView {
            id: p.id.to_string(),
            name: p.name,
            harness_type: p.harness_type.as_str().to_string(),
            model_route_id: p.model_route_id.map(|i| i.to_string()),
            provider_endpoint_id: p.provider_endpoint_id.map(|i| i.to_string()),
            provider_name,
            model_display,
            env: serde_json::to_value(&p.env).unwrap_or_default(),
            role_mappings: p
                .role_mappings
                .iter()
                .map(|rm| RoleMappingView {
                    role: rm.role.clone(),
                    model: rm.model.clone(),
                })
                .collect(),
        });
    }
    Ok(views)
}

#[tauri::command]
pub async fn list_profiles_cmd(state: State<'_, AppState>) -> Result<Vec<ProfileView>, String> {
    profile_views(&state.pool).await
}

#[tauri::command]
pub async fn create_profile_cmd(
    state: State<'_, AppState>,
    input: ProfileInput,
) -> Result<String, String> {
    let now = chrono::Utc::now();
    let profile = LaunchProfile {
        id: Uuid::new_v4(),
        name: input.name,
        harness_type: HarnessType::parse_str(&input.harness_type),
        model_route_id: parse_opt(input.model_route_id)?,
        provider_endpoint_id: parse_opt(input.provider_endpoint_id)?,
        env: input.env,
        role_mappings: input
            .role_mappings
            .into_iter()
            .map(|rm| RoleMapping {
                role: rm.role,
                model: rm.model,
            })
            .collect(),
        native_overrides: serde_json::json!({}),
        created_at: now,
        updated_at: now,
    };
    create_profile(&state.pool, &profile)
        .await
        .map_err(|e| e.to_string())?;
    Ok(profile.id.to_string())
}

#[tauri::command]
pub async fn delete_profile_cmd(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let pid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM launch_profiles WHERE id = ?")
        .bind(pid.to_string())
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_opt(s: Option<String>) -> Result<Option<Uuid>, String> {
    match s {
        Some(v) => Ok(Some(Uuid::parse_str(&v).map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}
use std::collections::HashMap;

#[tauri::command]
pub async fn launch_profile_cmd(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<crate::commands::launcher::LaunchResult, String> {
    let pid = Uuid::parse_str(&profile_id).map_err(|e| e.to_string())?;
    let profiles = list_profiles(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let profile = profiles
        .into_iter()
        .find(|p| p.id == pid)
        .ok_or("profile not found")?;
    let installs = chm_database::repos::harness::list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let inst = installs
        .into_iter()
        .find(|i| i.harness_type == profile.harness_type)
        .ok_or(format!(
            "no installed harness of type {}",
            profile.harness_type.as_str()
        ))?;
    let exe = inst
        .executable_path
        .clone()
        .ok_or("harness executable not found")?;

    let inherited: HashMap<String, String> = std::env::vars().collect();
    let resolved = crate::commands::launcher::resolve_profile_env(
        &profile.env,
        state.secrets.as_ref(),
        &inherited,
    );
    let role_env = crate::commands::launcher::role_env_for(
        profile.harness_type.as_str(),
        &profile.role_mappings,
    );
    let mut all_env = inherited.clone();
    for (k, v) in role_env {
        all_env.insert(k, v);
    }
    for (k, v) in resolved {
        all_env.insert(k, v);
    }

    let mut cmd = tokio::process::Command::new(exe.clone());
    cmd.env_clear().envs(all_env).kill_on_drop(false);
    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to launch {exe}: {e}"))?;
    let pid_num = child.id();
    Ok(crate::commands::launcher::LaunchResult {
        pid: pid_num,
        executable: exe,
    })
}
