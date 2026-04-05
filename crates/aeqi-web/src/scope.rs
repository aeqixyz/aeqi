use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use crate::auth::Claims;
use crate::server::AppState;
use aeqi_core::config::AuthMode;

/// Resolved tenancy scope for the current request.
/// In `none`/`secret` mode, `allowed_companies` is None (unrestricted).
/// In `accounts` mode, contains the user's company memberships.
#[derive(Debug, Clone)]
pub struct RequestScope {
    pub user_id: Option<String>,
    pub allowed_companies: Option<Vec<String>>,
}

impl RequestScope {
    /// Inject scope into IPC params. Adds `allowed_companies` array
    /// when the scope is restricted (accounts mode).
    pub fn inject(&self, params: &mut serde_json::Value) {
        if let Some(ref companies) = self.allowed_companies {
            if params.is_null() {
                *params = serde_json::json!({});
            }
            if let Some(obj) = params.as_object_mut() {
                obj.insert(
                    "allowed_companies".to_string(),
                    serde_json::json!(companies),
                );
            }
        }
        if let Some(ref uid) = self.user_id {
            if params.is_null() {
                *params = serde_json::json!({});
            }
            if let Some(obj) = params.as_object_mut() {
                obj.insert("user_id".to_string(), serde_json::json!(uid));
            }
        }
    }
}

impl<S> FromRequestParts<S> for RequestScope
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // In non-accounts mode, no scoping.
        if app_state.auth_mode != AuthMode::Accounts {
            return Ok(RequestScope {
                user_id: None,
                allowed_companies: None,
            });
        }

        // Read Claims from request extensions (stashed by require_auth middleware).
        let claims = parts.extensions.get::<Claims>().cloned();

        let user_id = claims.as_ref().and_then(|c| c.user_id.clone());

        let allowed_companies = if let Some(ref uid) = user_id {
            app_state
                .user_store
                .as_ref()
                .map(|store| store.get_user_companies(uid))
        } else {
            None
        };

        Ok(RequestScope {
            user_id,
            allowed_companies,
        })
    }
}
