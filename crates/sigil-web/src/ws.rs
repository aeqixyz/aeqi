use axum::{
    extract::{Query, State, WebSocketUpgrade},
    response::Response,
};
use serde::Deserialize;
use tracing::info;

use crate::auth;
use crate::server::AppState;

#[derive(Deserialize, Default)]
pub struct WsQuery {
    token: Option<String>,
}

/// WebSocket upgrade handler.
pub async fn handler(
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    // Validate token from query param.
    let secret = state.auth_secret.as_deref().unwrap_or("");
    if !secret.is_empty() {
        let token = q.token.as_deref().unwrap_or("");
        if auth::validate_token(token, secret).is_err() {
            return axum::response::IntoResponse::into_response((
                axum::http::StatusCode::UNAUTHORIZED,
                "invalid or missing token",
            ));
        }
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: axum::extract::ws::WebSocket, state: AppState) {
    use axum::extract::ws::Message;

    info!("WebSocket client connected");

    let poll_interval = std::time::Duration::from_secs(5);
    let mut interval = tokio::time::interval(poll_interval);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Poll daemon for status + worker progress.
                let status = state.ipc.cmd("status").await;
                let workers = state.ipc.cmd("worker_progress").await;
                let msg = match (status, workers) {
                    (Ok(data), Ok(wp)) => serde_json::json!({
                        "event": "status",
                        "data": data,
                        "workers": wp.get("workers").cloned().unwrap_or(serde_json::json!([])),
                    }),
                    (Ok(data), Err(_)) => serde_json::json!({"event": "status", "data": data}),
                    (Err(e), _) => serde_json::json!({"event": "error", "data": {"error": e.to_string()}}),
                };

                if let Ok(text) = serde_json::to_string(&msg)
                    && socket.send(Message::Text(text.into())).await.is_err()
                {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle client requests.
                        if let Ok(req) = serde_json::from_str::<serde_json::Value>(&text) {
                            let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
                            let result = state.ipc.request(&req).await;
                            let resp = match result {
                                Ok(data) => serde_json::json!({"event": cmd, "data": data}),
                                Err(e) => serde_json::json!({"event": "error", "data": {"error": e.to_string()}}),
                            };
                            if let Ok(text) = serde_json::to_string(&resp)
                                && socket.send(Message::Text(text.into())).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}
