pub mod routes;
pub mod auth;
pub mod ws;
pub mod api_chat;
pub mod api_gacha;
pub mod api_companion;
pub mod api_party;
pub mod api_user;
pub mod api_admin;
pub mod api_project;
pub mod api_stripe;
pub mod types;

use std::sync::Arc;
use anyhow::Result;
use tracing::info;

use system_tenants::TenantManager;
use system_tenants::config::PlatformConfig;

/// Application state shared across all handlers.
pub struct AppState {
    pub manager: Arc<TenantManager>,
    pub platform: PlatformConfig,
}

/// Start the web server.
pub async fn start_server(manager: Arc<TenantManager>, platform: PlatformConfig) -> Result<()> {
    let state = Arc::new(AppState {
        manager,
        platform: platform.clone(),
    });

    let app = routes::build_router(state);
    let bind = &platform.web.bind;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(bind = %bind, "realm-web server started");
    axum::serve(listener, app).await?;
    Ok(())
}
