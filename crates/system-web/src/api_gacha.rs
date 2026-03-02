use axum::extract::{Json, State};
use axum::http::StatusCode;
use std::sync::Arc;

use crate::AppState;
use crate::auth::AuthTenant;
use crate::types::*;
use system_companions::GachaEngine;
use system_tenants::provision;

pub async fn pull(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<PullResponse>, (StatusCode, String)> {
    // Check economy: spend 1 summon.
    {
        let db = state.manager.db().await;
        let can_spend = system_tenants::economy::spend_summons(&db, &tenant.id.0, 1, &tenant.tier)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if !can_spend {
            return Err((StatusCode::PAYMENT_REQUIRED, "insufficient summons".to_string()));
        }
    }

    // Check companion limit.
    let stats = tenant.companion_store.collection_stats()
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if stats.total_companions >= tenant.tier.max_companions {
        return Err((StatusCode::FORBIDDEN, "companion limit reached".to_string()));
    }

    // Load pity state and pull.
    let mut pity = tenant.companion_store.load_pity()
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let engine = GachaEngine::default();
    let companion = engine.pull(&mut pity);
    let pity_count = pity.pulls_since_s_or_above;

    // Check if new (not already in collection).
    let existing = tenant.companion_store.get_companion(&companion.id)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let is_new = existing.is_none();

    // Save companion + pity.
    tenant.companion_store.save_companion(&companion)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tenant.companion_store.save_pity(&pity)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tenant.companion_store.record_pull(&companion)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Materialize on disk (sync — fast).
    if is_new {
        let _ = provision::materialize_companion(
            &tenant.data_dir, &state.platform.template_dir(), &companion,
        );

        // Spawn async persona generation in background.
        let data_dir = tenant.data_dir.clone();
        let companion_clone = companion.clone();
        let platform = state.platform.clone();
        tokio::spawn(async move {
            if let Err(e) = provision::materialize_companion_persona(
                &data_dir, &companion_clone, &platform, None,
            ).await {
                tracing::warn!(
                    companion = %companion_clone.name,
                    error = %e,
                    "async persona generation failed"
                );
            }
        });
    }

    Ok(Json(PullResponse {
        companion: CompanionInfo::from_companion(&companion),
        is_new,
        pity_count,
    }))
}

pub async fn pull10(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Pull10Response>, (StatusCode, String)> {
    // Check economy: spend 10 summons.
    {
        let db = state.manager.db().await;
        let can_spend = system_tenants::economy::spend_summons(&db, &tenant.id.0, 10, &tenant.tier)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if !can_spend {
            return Err((StatusCode::PAYMENT_REQUIRED, "insufficient summons (need 10)".to_string()));
        }
    }

    let mut results = Vec::with_capacity(10);
    let mut pity = tenant.companion_store.load_pity()
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let engine = GachaEngine::default();

    for _ in 0..10 {
        let companion = engine.pull(&mut pity);
        let existing = tenant.companion_store.get_companion(&companion.id)
            .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let is_new = existing.is_none();

        tenant.companion_store.save_companion(&companion)
            .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tenant.companion_store.record_pull(&companion)
            .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if is_new {
            let _ = provision::materialize_companion(
                &tenant.data_dir, &state.platform.template_dir(), &companion,
            );

            // Spawn async persona generation.
            let data_dir = tenant.data_dir.clone();
            let companion_clone = companion.clone();
            let platform = state.platform.clone();
            tokio::spawn(async move {
                if let Err(e) = provision::materialize_companion_persona(
                    &data_dir, &companion_clone, &platform, None,
                ).await {
                    tracing::warn!(
                        companion = %companion_clone.name,
                        error = %e,
                        "async persona generation failed"
                    );
                }
            });
        }

        results.push(PullResponse {
            companion: CompanionInfo::from_companion(&companion),
            is_new,
            pity_count: pity.pulls_since_s_or_above,
        });
    }

    tenant.companion_store.save_pity(&pity)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(Pull10Response { results }))
}

pub async fn fuse(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
    Json(req): Json<FuseRequest>,
) -> Result<Json<FuseResponse>, (StatusCode, String)> {
    if req.names.len() != 4 {
        return Err((StatusCode::BAD_REQUEST, "fusion requires exactly 4 companions".into()));
    }

    // Load all 4 companions
    let mut companions = Vec::new();
    for name in &req.names {
        let c = tenant.companion_store.get_companion_by_name(name)
            .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, format!("companion '{}' not found", name)))?;
        companions.push(c);
    }

    let refs: Vec<&system_companions::Companion> = companions.iter().collect();
    let result = system_companions::fuse_multi(&refs)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Save the new fused companion.
    tenant.companion_store.save_companion(&result)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Record fusion using first two as "parents" for lineage tracking.
    tenant.companion_store.record_fusion(&companions[0], &companions[1], &result)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Clone parents before removal.
    let parent_a = companions[0].clone();
    let parent_b = companions[1].clone();

    // Remove all 4 source companions (consumed by fusion).
    for c in &companions {
        tenant.companion_store.remove_companion(&c.id)
            .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Materialize fused companion on disk.
    let _ = provision::materialize_companion(
        &tenant.data_dir, &state.platform.template_dir(), &result,
    );

    // Spawn async persona generation with fusion lineage context.
    let data_dir = tenant.data_dir.clone();
    let result_clone = result.clone();
    let platform = state.platform.clone();
    tokio::spawn(async move {
        if let Err(e) = provision::materialize_companion_persona(
            &data_dir, &result_clone, &platform,
            Some((parent_a, parent_b)),
        ).await {
            tracing::warn!(
                companion = %result_clone.name,
                error = %e,
                "async persona generation failed for fused companion"
            );
        }
    });

    Ok(Json(FuseResponse {
        companion: CompanionInfo::from_companion(&result),
    }))
}
