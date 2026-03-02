use axum::extract::Json;
use axum::http::StatusCode;

use crate::auth::AuthTenant;
use crate::types::*;

pub async fn get_party(
    AuthTenant(tenant): AuthTenant,
) -> Result<Json<PartyResponse>, (StatusCode, String)> {
    let roster = tenant
        .companion_store
        .get_roster()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let leader = tenant
        .companion_store
        .get_leader()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(PartyResponse {
        leader: leader.as_ref().map(CompanionInfo::from_companion),
        squad: roster.iter().map(CompanionInfo::from_companion).collect(),
        max_size: 4,
    }))
}

pub async fn set_squad(
    AuthTenant(tenant): AuthTenant,
    Json(req): Json<SetSquadRequest>,
) -> Result<Json<PartyResponse>, (StatusCode, String)> {
    if req.members.len() > 4 {
        return Err((StatusCode::BAD_REQUEST, "squad cannot exceed 4 members".to_string()));
    }

    // Resolve companion names → IDs.
    let all = tenant
        .companion_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut ids = Vec::new();
    for name in &req.members {
        let companion = all
            .iter()
            .find(|c| c.name == *name)
            .ok_or((StatusCode::NOT_FOUND, format!("companion not found: {name}")))?;
        ids.push(companion.id.clone());
    }

    tenant
        .companion_store
        .set_roster(&ids)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Return updated party state.
    get_party(AuthTenant(tenant)).await
}

pub async fn set_leader(
    AuthTenant(tenant): AuthTenant,
    Json(req): Json<SetLeaderRequest>,
) -> Result<Json<PartyResponse>, (StatusCode, String)> {
    // Resolve name → ID.
    let all = tenant
        .companion_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let companion = all
        .iter()
        .find(|c| c.name == req.name)
        .ok_or((StatusCode::NOT_FOUND, "companion not found".to_string()))?;

    tenant
        .companion_store
        .set_leader(&companion.id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Return updated party state.
    get_party(AuthTenant(tenant)).await
}
