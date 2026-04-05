use aeqi_core::config::{AEQIConfig, AuthConfig, AuthMode, PeerAgentConfig};
use anyhow::Result;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware,
    response::{IntoResponse, Response},
};
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::info;

use crate::auth;
use crate::ipc::IpcClient;
use crate::routes::{api_routes, webhook_routes};
use crate::ws;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub ipc: Arc<IpcClient>,
    pub auth_secret: Option<String>,
    pub auth_mode: AuthMode,
    pub auth_config: AuthConfig,
    pub agents_config: Vec<PeerAgentConfig>,
    pub ui_dist_dir: Option<PathBuf>,
    pub user_store: Option<Arc<crate::users::UserStore>>,
}

/// Start the web server using settings from AEQIConfig.
pub async fn start(config: &AEQIConfig) -> Result<()> {
    let web = &config.web;
    let data_dir = config.data_dir();

    let ipc = Arc::new(IpcClient::from_data_dir(&data_dir));

    // Open user store for accounts mode.
    let user_store = if web.auth.mode == AuthMode::Accounts {
        let db_path = data_dir.join("agents.db");
        let store = crate::users::UserStore::open(&db_path)?;
        info!("user store initialized (accounts mode)");
        Some(Arc::new(store))
    } else {
        None
    };

    let state = AppState {
        ipc: ipc.clone(),
        auth_secret: web.auth_secret.clone(),
        auth_mode: web.auth.mode.clone(),
        auth_config: web.auth.clone(),
        agents_config: config.agents.clone(),
        ui_dist_dir: web.ui_dist_dir.as_ref().map(PathBuf::from),
        user_store,
    };
    let ui_dist_dir = state.ui_dist_dir.clone();
    let serve_ui = ui_dist_dir.is_some();

    // Build CORS layer.
    let cors = if web.cors_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<_> = web
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    };

    // Protected routes (auth required) — uses AppState for the secret.
    let protected = api_routes().route_layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_auth,
    ));

    // Public routes (health + login + ws + webhooks).
    let public = Router::new()
        .route("/api/health", axum::routing::get(health_handler))
        .route("/api/auth/mode", axum::routing::get(auth_mode_handler))
        .route("/api/auth/login", axum::routing::post(login_handler))
        .route("/api/auth/signup", axum::routing::post(signup_handler))
        .route("/api/auth/google", axum::routing::get(google_redirect_handler))
        .route(
            "/api/auth/google/callback",
            axum::routing::get(google_callback_handler),
        )
        .route("/api/ws", axum::routing::get(ws::handler))
        .route(
            "/api/chat/stream",
            axum::routing::get(crate::session_ws::handler),
        )
        .nest("/api", webhook_routes());

    // Protected /api/auth/me route.
    let auth_me = Router::new()
        .route("/api/auth/me", axum::routing::get(me_handler))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let mut app = Router::new()
        .nest("/api", protected)
        .merge(auth_me)
        .merge(public)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    if serve_ui {
        if let Some(ui_dist_dir) = ui_dist_dir.as_ref() {
            info!("aeqi-web serving UI assets from {}", ui_dist_dir.display());
        }
        app = app.fallback(spa_handler);
    } else {
        #[cfg(feature = "embed-ui")]
        {
            info!("aeqi-web serving embedded UI assets");
            app = app.fallback(embedded_spa_handler);
        }
    }

    let app = app.with_state(state);

    let listener = tokio::net::TcpListener::bind(&web.bind).await?;
    info!("aeqi-web listening on {} (auth: {:?})", web.bind, web.auth.mode);
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Handlers ────────────────────────────────────────────

async fn health_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    match state.ipc.cmd("ping").await {
        Ok(resp) => axum::Json(resp).into_response(),
        Err(_) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({"ok": false, "error": "daemon not reachable"})),
        )
            .into_response(),
    }
}

async fn auth_mode_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    let mode = match state.auth_mode {
        AuthMode::None => "none",
        AuthMode::Secret => "secret",
        AuthMode::Accounts => "accounts",
    };
    let google = state.auth_config.google_client_id.is_some();
    axum::Json(serde_json::json!({
        "mode": mode,
        "google_oauth": google,
    }))
    .into_response()
}

async fn login_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::response::Response {
    match state.auth_mode {
        AuthMode::None => {
            // No auth needed — return a token anyway for API compat.
            match auth::create_token("aeqi-dev", 8760, None, None) {
                Ok(token) => axum::Json(serde_json::json!({
                    "ok": true, "token": token, "token_type": "Bearer", "expires_in": 31536000,
                }))
                .into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        AuthMode::Secret => {
            let secret = body.get("secret").and_then(|s| s.as_str()).unwrap_or("");
            let expected = state.auth_secret.as_deref().unwrap_or("");

            if !expected.is_empty() && secret != expected {
                return (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({"ok": false, "error": "invalid secret"})),
                )
                    .into_response();
            }

            let signing_key = if expected.is_empty() {
                "aeqi-dev"
            } else {
                expected
            };
            match auth::create_token(signing_key, 24, None, None) {
                Ok(token) => axum::Json(serde_json::json!({
                    "ok": true, "token": token, "token_type": "Bearer", "expires_in": 86400,
                }))
                .into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        AuthMode::Accounts => {
            let email = body.get("email").and_then(|s| s.as_str()).unwrap_or("");
            let password = body.get("password").and_then(|s| s.as_str()).unwrap_or("");

            if email.is_empty() || password.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({"ok": false, "error": "email and password required"})),
                )
                    .into_response();
            }

            let Some(ref store) = state.user_store else {
                return (StatusCode::INTERNAL_SERVER_ERROR, "user store not available")
                    .into_response();
            };

            let user = match store.find_by_email(email) {
                Some(u) => u,
                None => {
                    return (
                        StatusCode::UNAUTHORIZED,
                        axum::Json(serde_json::json!({"ok": false, "error": "invalid email or password"})),
                    )
                        .into_response();
                }
            };

            if !store.verify_password(&user, password) {
                return (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({"ok": false, "error": "invalid email or password"})),
                )
                    .into_response();
            }

            let signing_key = state.auth_secret.as_deref().unwrap_or("aeqi-dev");
            match auth::create_token(signing_key, 24, Some(&user.id), Some(&user.email)) {
                Ok(token) => axum::Json(serde_json::json!({
                    "ok": true,
                    "token": token,
                    "token_type": "Bearer",
                    "expires_in": 86400,
                    "user": {
                        "id": user.id,
                        "email": user.email,
                        "name": user.name,
                        "avatar_url": user.avatar_url,
                    },
                }))
                .into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
    }
}

async fn signup_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::response::Response {
    if state.auth_mode != AuthMode::Accounts {
        return (StatusCode::NOT_FOUND, "signup not available").into_response();
    }

    let email = body.get("email").and_then(|s| s.as_str()).unwrap_or("");
    let password = body.get("password").and_then(|s| s.as_str()).unwrap_or("");
    let name = body.get("name").and_then(|s| s.as_str()).unwrap_or("");

    if email.is_empty() || password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"ok": false, "error": "email and password required"})),
        )
            .into_response();
    }

    if password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"ok": false, "error": "password must be at least 8 characters"})),
        )
            .into_response();
    }

    let Some(ref store) = state.user_store else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "user store not available").into_response();
    };

    if store.find_by_email(email).is_some() {
        return (
            StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"ok": false, "error": "email already registered"})),
        )
            .into_response();
    }

    let user = match store.create_user(email, password, name) {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"ok": false, "error": e.to_string()})),
            )
                .into_response();
        }
    };

    let signing_key = state.auth_secret.as_deref().unwrap_or("aeqi-dev");
    match auth::create_token(signing_key, 24, Some(&user.id), Some(&user.email)) {
        Ok(token) => axum::Json(serde_json::json!({
            "ok": true,
            "token": token,
            "token_type": "Bearer",
            "expires_in": 86400,
            "user": {
                "id": user.id,
                "email": user.email,
                "name": user.name,
                "avatar_url": user.avatar_url,
            },
        }))
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn google_redirect_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    if state.auth_mode != AuthMode::Accounts {
        return (StatusCode::NOT_FOUND, "OAuth not available").into_response();
    }

    let Some(ref client_id) = state.auth_config.google_client_id else {
        return (StatusCode::NOT_FOUND, "Google OAuth not configured").into_response();
    };

    let Some(ref store) = state.user_store else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "user store not available").into_response();
    };

    let nonce = uuid::Uuid::new_v4().to_string();
    store.save_oauth_state(&nonce);

    let base = state
        .auth_config
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:8400");
    let redirect_uri = format!("{}/api/auth/google/callback", base);

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}",
        urlencoding::encode(client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&nonce),
    );

    axum::response::Redirect::temporary(&url).into_response()
}

async fn google_callback_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    let base = state
        .auth_config
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:8400");

    let error_redirect = |msg: &str| -> Response {
        axum::response::Redirect::temporary(&format!("{}/#/login?error={}", base, urlencoding::encode(msg)))
            .into_response()
    };

    let Some(code) = params.get("code") else {
        return error_redirect("missing code");
    };
    let Some(returned_state) = params.get("state") else {
        return error_redirect("missing state");
    };

    let Some(ref store) = state.user_store else {
        return error_redirect("server error");
    };

    if !store.consume_oauth_state(returned_state) {
        return error_redirect("invalid state");
    }

    let Some(ref client_id) = state.auth_config.google_client_id else {
        return error_redirect("not configured");
    };
    let Some(ref client_secret) = state.auth_config.google_client_secret else {
        return error_redirect("not configured");
    };

    let redirect_uri = format!("{}/api/auth/google/callback", base);

    // Exchange code for token.
    let client = reqwest::Client::new();
    let token_resp = match client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return error_redirect(&format!("token exchange failed: {e}")),
    };

    let token_json: serde_json::Value = match token_resp.json().await {
        Ok(j) => j,
        Err(e) => return error_redirect(&format!("token parse failed: {e}")),
    };

    let Some(access_token) = token_json.get("access_token").and_then(|v| v.as_str()) else {
        return error_redirect("no access token");
    };

    // Get user info from Google.
    let userinfo_resp = match client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return error_redirect(&format!("userinfo failed: {e}")),
    };

    let userinfo: serde_json::Value = match userinfo_resp.json().await {
        Ok(j) => j,
        Err(e) => return error_redirect(&format!("userinfo parse failed: {e}")),
    };

    let email = userinfo
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let name = userinfo
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let avatar = userinfo
        .get("picture")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let sub = userinfo
        .get("sub")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if email.is_empty() {
        return error_redirect("no email from Google");
    }

    let user = store.find_or_create_oauth(email, name, avatar, "google", sub);

    let signing_key = state.auth_secret.as_deref().unwrap_or("aeqi-dev");
    match auth::create_token(signing_key, 24, Some(&user.id), Some(&user.email)) {
        Ok(token) => {
            let redirect_url = format!("{}/#/auth/callback?token={}", base, token);
            axum::response::Redirect::temporary(&redirect_url).into_response()
        }
        Err(_) => error_redirect("token creation failed"),
    }
}

async fn me_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: Request,
) -> axum::response::Response {
    // Extract claims from the validated token.
    let secret = state.auth_secret.as_deref().unwrap_or("aeqi-dev");
    let token = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "no token").into_response();
    };

    match auth::validate_token(token, secret) {
        Ok(claims) => {
            // In accounts mode, look up full user.
            if let Some(ref store) = state.user_store {
                if let Some(ref uid) = claims.user_id {
                    if let Some(user) = store.find_by_id(uid) {
                        let companies = store.get_user_companies(&user.id);
                        return axum::Json(serde_json::json!({
                            "id": user.id,
                            "email": user.email,
                            "name": user.name,
                            "avatar_url": user.avatar_url,
                            "provider": user.provider,
                            "companies": companies,
                        }))
                        .into_response();
                    }
                }
            }

            // Fallback for secret/none mode.
            axum::Json(serde_json::json!({
                "id": claims.sub,
                "email": claims.email,
                "name": "Operator",
            }))
            .into_response()
        }
        Err(_) => (StatusCode::UNAUTHORIZED, "invalid token").into_response(),
    }
}

// ── SPA Handlers ────────────────────────────────────────

#[cfg(feature = "embed-ui")]
async fn embedded_spa_handler(req: Request) -> Response {
    use crate::embedded_ui::Assets;

    if req.method() != Method::GET && req.method() != Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }

    let path = req.uri().path();
    if path.starts_with("/api") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let file_path = path.trim_start_matches('/');

    let file = Assets::get(file_path).or_else(|| Assets::get("index.html"));

    match file {
        Some(content) => {
            let mime = mime_guess::from_path(file_path)
                .first_or_octet_stream()
                .to_string();
            Response::builder()
                .header("content-type", mime)
                .body(Body::from(content.data.to_vec()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn spa_handler(State(state): State<AppState>, req: Request) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }

    let path = req.uri().path();
    if path.starts_with("/api") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(ui_dist_dir) = state.ui_dist_dir.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let last_segment = path.rsplit('/').next().unwrap_or_default();
    let response = if !last_segment.contains('.') {
        ServeDir::new(ui_dist_dir.clone())
            .fallback(ServeFile::new(ui_dist_dir.join("index.html")))
            .oneshot(req)
            .await
    } else {
        ServeDir::new(ui_dist_dir).oneshot(req).await
    };

    match response {
        Ok(response) => response.map(Body::new).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to serve UI asset: {err}"),
        )
            .into_response(),
    }
}
