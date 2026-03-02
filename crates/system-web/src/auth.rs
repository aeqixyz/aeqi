use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use std::sync::Arc;

use system_tenants::Tenant;
use crate::AppState;

/// Extractor that resolves a tenant from the Authorization header.
pub struct AuthTenant(pub Arc<Tenant>);

impl FromRequestParts<Arc<AppState>> for AuthTenant {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token"))?;

        let tenant = state
            .manager
            .resolve_by_session(token)
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token"))?
            .ok_or((StatusCode::UNAUTHORIZED, "tenant not found"))?;

        Ok(AuthTenant(tenant))
    }
}
