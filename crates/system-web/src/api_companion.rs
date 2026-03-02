use axum::extract::{Json, Path};
use axum::http::StatusCode;

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
