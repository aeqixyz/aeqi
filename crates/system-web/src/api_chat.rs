use axum::extract::{Json, Query};
use axum::http::StatusCode;

use crate::auth::AuthTenant;
use crate::types::*;

pub async fn history(
    AuthTenant(tenant): AuthTenant,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Vec<ChatHistoryEntry>>, (StatusCode, String)> {
    let messages = tenant
        .conversation_store
        .recent(0, params.limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<ChatHistoryEntry> = messages
        .into_iter()
        .map(|m| ChatHistoryEntry {
            role: m.role,
            content: m.content,
            timestamp: m.timestamp,
        })
        .collect();

    Ok(Json(entries))
}
