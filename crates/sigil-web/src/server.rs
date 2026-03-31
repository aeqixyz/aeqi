use anyhow::Result;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware,
    response::{IntoResponse, Response},
};
use sigil_core::config::{PeerAgentConfig, SigilConfig};
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
use crate::routes::api_routes;
use crate::ws;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub ipc: Arc<IpcClient>,
    pub auth_secret: Option<String>,
    pub agents_config: Vec<PeerAgentConfig>,
    pub ui_dist_dir: Option<PathBuf>,
}

/// Start the web server using settings from SigilConfig.
pub async fn start(config: &SigilConfig) -> Result<()> {
    let web = &config.web;
    let data_dir = config.data_dir();

    let ipc = Arc::new(IpcClient::from_data_dir(&data_dir));

    let state = AppState {
        ipc: ipc.clone(),
        auth_secret: web.auth_secret.clone(),
        agents_config: config.agents.clone(),
        ui_dist_dir: web.ui_dist_dir.as_ref().map(PathBuf::from),
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

    // Public routes (health + login + ws).
    let public = Router::new()
        .route("/api/health", axum::routing::get(health_handler))
        .route("/api/auth/login", axum::routing::post(login_handler))
        .route("/api/ws", axum::routing::get(ws::handler))
        .route("/api/chat/stream", axum::routing::get(crate::chat_ws::handler));

    let mut app = Router::new()
        .nest("/api", protected)
        .merge(public)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    if serve_ui {
        if let Some(ui_dist_dir) = ui_dist_dir.as_ref() {
            info!("sigil-web serving UI assets from {}", ui_dist_dir.display());
        }
        app = app.fallback(spa_handler);
    }

    let app = app.with_state(state);

    let listener = tokio::net::TcpListener::bind(&web.bind).await?;
    info!("sigil-web listening on {}", web.bind);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match state.ipc.cmd("ping").await {
        Ok(resp) => axum::Json(resp).into_response(),
        Err(_) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({"ok": false, "error": "daemon not reachable"})),
        )
            .into_response(),
    }
}

async fn login_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let secret = body.get("secret").and_then(|s| s.as_str()).unwrap_or("");
    let expected = state.auth_secret.as_deref().unwrap_or("");

    if expected.is_empty() || secret != expected {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({"ok": false, "error": "invalid secret"})),
        )
            .into_response();
    }

    match auth::create_token(expected, 24) {
        Ok(token) => axum::Json(serde_json::json!({
            "ok": true,
            "token": token,
            "token_type": "Bearer",
            "expires_in": 86400,
        }))
        .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        )
            .into_response(),
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
