use axum::extract::{Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::sync::Arc;

use crate::AppState;
use crate::auth::AuthTenant;
use crate::types::*;

pub async fn list_companions(
    AuthTenant(tenant): AuthTenant,
) -> Result<Json<Vec<CompanionInfo>>, (StatusCode, String)> {
    let companions = tenant
        .companion_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let infos: Vec<CompanionInfo> = companions.iter().map(CompanionInfo::from_companion).collect();
    Ok(Json(infos))
}

pub async fn get_companion(
    AuthTenant(tenant): AuthTenant,
    Path(name): Path<String>,
) -> Result<Json<CompanionInfo>, (StatusCode, String)> {
    let companions = tenant
        .companion_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let companion = companions.iter()
        .find(|c| c.name == name)
        .ok_or((StatusCode::NOT_FOUND, "companion not found".to_string()))?;

    Ok(Json(CompanionInfo::from_companion(companion)))
}

pub async fn set_familiar(
    AuthTenant(tenant): AuthTenant,
    Json(req): Json<SetFamiliarRequest>,
) -> Result<Json<CompanionInfo>, (StatusCode, String)> {
    let companions = tenant
        .companion_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let companion = companions.iter()
        .find(|c| c.name == req.name)
        .ok_or((StatusCode::NOT_FOUND, "companion not found".to_string()))?;

    tenant
        .companion_store
        .set_familiar(&companion.id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut updated = companion.clone();
    updated.is_familiar = true;
    Ok(Json(CompanionInfo::from_companion(&updated)))
}

pub async fn get_familiar(
    AuthTenant(tenant): AuthTenant,
) -> Result<Json<CompanionInfo>, (StatusCode, String)> {
    let familiar = tenant
        .companion_store
        .get_familiar()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "no familiar set".to_string()))?;

    Ok(Json(CompanionInfo::from_companion(&familiar)))
}

#[derive(serde::Deserialize)]
pub struct PortraitQuery {
    pub token: Option<String>,
}

/// Serve portrait PNG. Accepts auth via `?token=` query param
/// (needed because `<img src>` can't send Authorization headers).
pub async fn get_portrait(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<PortraitQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let token = query
        .token
        .ok_or((StatusCode::UNAUTHORIZED, "missing token query param".to_string()))?;

    let tenant = state
        .manager
        .resolve_by_session(&token)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token".to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "tenant not found".to_string()))?;

    let portrait_path = tenant.data_dir.join("agents").join(&name).join("portrait.png");

    if !portrait_path.exists() {
        return Err((StatusCode::NOT_FOUND, "portrait not found".to_string()));
    }

    let bytes = std::fs::read(&portrait_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/png"),
         (axum::http::header::CACHE_CONTROL, "public, max-age=86400")],
        bytes,
    ))
}

/// Backfill portraits for all companions missing one.
pub async fn backfill_portraits(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let companions = tenant
        .companion_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut spawned = 0u32;
    for companion in &companions {
        let portrait_path = tenant.data_dir.join("agents").join(&companion.name).join("portrait.png");
        if portrait_path.exists() {
            continue;
        }

        let data_dir = tenant.data_dir.clone();
        let companion_clone = companion.clone();
        let platform = state.platform.clone();
        let store = tenant.companion_store.clone();
        tokio::spawn(async move {
            if let Err(e) = system_tenants::provision::materialize_companion_portrait(
                &data_dir, &companion_clone, &platform, &store,
            ).await {
                tracing::warn!(
                    companion = %companion_clone.name,
                    error = %e,
                    "backfill portrait generation failed"
                );
            }
        });
        spawned += 1;
    }

    Ok(Json(serde_json::json!({ "spawned": spawned })))
}

pub async fn get_relationships(
    AuthTenant(tenant): AuthTenant,
    Path(name): Path<String>,
) -> Result<Json<Vec<RelationshipInfo>>, (StatusCode, String)> {
    let companions = tenant
        .companion_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let target = companions.iter()
        .find(|c| c.name == name)
        .ok_or((StatusCode::NOT_FOUND, "companion not found".to_string()))?;

    // Build relationships with all other companions (lazy seed).
    let mut relationships = Vec::new();
    for other in &companions {
        if other.id == target.id {
            continue;
        }
        let rel = tenant
            .companion_store
            .get_or_seed_relationship(target, other)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        relationships.push(RelationshipInfo {
            companion_a: rel.agent_a.clone(),
            companion_b: rel.agent_b.clone(),
            respect: rel.respect,
            affinity: rel.affinity,
            trust: rel.trust,
            rivalry: rel.rivalry,
            synergy: rel.synergy,
            label: rel.relationship_label().to_string(),
            compatibility: rel.overall_compatibility(),
        });
    }

    Ok(Json(relationships))
}
