use axum::{Router, routing::{get, post, put}};
use axum::http::HeaderValue;
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;

use crate::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = if state.platform.web.cors_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<HeaderValue> = state
            .platform
            .web
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    };

    Router::new()
        // Auth (legacy anonymous)
        .route("/api/auth/register", post(crate::api_user::register))
        .route("/api/auth/refresh", post(crate::api_user::refresh))
        // Auth (email+password)
        .route("/api/auth/register-email", post(crate::api_user::register_email))
        .route("/api/auth/login", post(crate::api_user::login))
        .route("/api/auth/verify-email", post(crate::api_user::verify_email))
        .route("/api/auth/request-password-reset", post(crate::api_user::request_password_reset))
        .route("/api/auth/reset-password", post(crate::api_user::reset_password))
        // Companions
        .route("/api/companions", get(crate::api_companion::list_companions))
        .route("/api/companions/{name}", get(crate::api_companion::get_companion))
        .route("/api/companions/familiar", post(crate::api_companion::set_familiar).get(crate::api_companion::get_familiar))
        .route("/api/companions/{name}/relationships", get(crate::api_companion::get_relationships))
        // Party
        .route("/api/party", get(crate::api_party::get_party))
        .route("/api/party/squad", put(crate::api_party::set_squad))
        .route("/api/party/leader", put(crate::api_party::set_leader))
        // Gacha
        .route("/api/gacha/pull", post(crate::api_gacha::pull))
        .route("/api/gacha/pull10", post(crate::api_gacha::pull10))
        .route("/api/gacha/fuse", post(crate::api_gacha::fuse))
        // Chat
        .route("/api/chat/history", get(crate::api_chat::history))
        .route("/api/chat/ws", get(crate::ws::ws_handler))
        // User
        .route("/api/user/profile", get(crate::api_user::profile))
        .route("/api/user/usage", get(crate::api_user::usage))
        .route("/api/user/economy", get(crate::api_user::economy))
        .route("/api/user/change-password", post(crate::api_user::change_password))
        .route("/api/user/totp/setup", post(crate::api_user::setup_totp))
        .route("/api/user/totp/verify", post(crate::api_user::verify_totp))
        .route("/api/user/totp/disable", post(crate::api_user::disable_totp))
        // Stripe
        .route("/api/stripe/checkout", post(crate::api_stripe::checkout))
        .route("/api/stripe/webhook", post(crate::api_stripe::webhook))
        .route("/api/stripe/portal", post(crate::api_stripe::portal))
        // Projects
        .route("/api/projects", get(crate::api_project::list_projects))
        .route("/api/projects/{name}", get(crate::api_project::get_project))
        .route("/api/projects/{name}/missions", get(crate::api_project::list_missions).post(crate::api_project::create_mission))
        .route("/api/projects/{name}/missions/{id}", get(crate::api_project::get_mission))
        .route("/api/projects/{name}/tasks", get(crate::api_project::list_tasks).post(crate::api_project::create_task))
        .route("/api/projects/{name}/tasks/{id}", get(crate::api_project::get_task).patch(crate::api_project::update_task))
        // Admin
        .route("/api/admin/users", get(crate::api_admin::list_users))
        .route("/api/admin/stats", get(crate::api_admin::stats))
        // Layers
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
