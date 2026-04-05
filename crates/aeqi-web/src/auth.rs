use aeqi_core::config::AuthMode;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::server::AppState;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub iat: usize,
    pub exp: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Create a JWT token with optional user identity.
pub fn create_token(
    secret: &str,
    expiry_hours: u64,
    user_id: Option<&str>,
    email: Option<&str>,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id.unwrap_or("operator").to_string(),
        iat: now,
        exp: now + (expiry_hours * 3600) as usize,
        user_id: user_id.map(|s| s.to_string()),
        email: email.map(|s| s.to_string()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Validate a JWT token and return claims.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

/// Extract Bearer token from Authorization header.
fn extract_bearer(req: &Request) -> Option<&str> {
    req.headers()
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

pub fn signing_secret(state: &AppState) -> &str {
    match state.auth_secret.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => "aeqi-dev",
    }
}

/// Extract user_id from a validated request (for data scoping).
pub fn extract_user_id(state: &AppState, req: &Request) -> Option<String> {
    if state.auth_mode != AuthMode::Accounts {
        return None;
    }
    let token = extract_bearer(req)?;
    let secret = signing_secret(state);
    let claims = validate_token(token, secret).ok()?;
    claims.user_id
}

/// Axum middleware — dispatches by auth mode.
pub async fn require_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    match state.auth_mode {
        AuthMode::None => next.run(req).await,
        AuthMode::Secret => {
            let secret = signing_secret(&state);
            let Some(token) = extract_bearer(&req) else {
                return (StatusCode::UNAUTHORIZED, "missing authorization header").into_response();
            };
            match validate_token(token, secret) {
                Ok(claims) => {
                    req.extensions_mut().insert(claims);
                    next.run(req).await
                }
                Err(_) => (StatusCode::UNAUTHORIZED, "invalid or expired token").into_response(),
            }
        }
        AuthMode::Accounts => {
            let secret = signing_secret(&state);
            let Some(token) = extract_bearer(&req) else {
                return (StatusCode::UNAUTHORIZED, "missing authorization header").into_response();
            };
            match validate_token(token, secret) {
                Ok(claims) => {
                    // Check email_verified for accounts mode.
                    if let Some(ref uid) = claims.user_id
                        && let Some(ref store) = state.user_store
                        && !store.is_email_verified(uid)
                    {
                        // Allow only auth endpoints and companies for unverified users.
                        let path = req.uri().path();
                        // Nested routers strip /api prefix, so check both forms.
                        let allowed = path.starts_with("/api/auth/")
                            || path.starts_with("/auth/")
                            || path.starts_with("/api/companies")
                            || path.starts_with("/companies");
                        if !allowed {
                            return (StatusCode::FORBIDDEN, "email not verified").into_response();
                        }
                    }
                    req.extensions_mut().insert(claims);
                    next.run(req).await
                }
                Err(_) => (StatusCode::UNAUTHORIZED, "invalid or expired token").into_response(),
            }
        }
    }
}
