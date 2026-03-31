//! Streaming chat WebSocket endpoint.
//!
//! Accepts a user message, submits it to the daemon via chat_full, then
//! streams ChatStreamEvents back in real-time until the agent completes.
//!
//! Protocol:
//! - Client sends: `{"message": "...", "project": "...", "agent": "..."}`
//! - Server sends: sequence of ChatStreamEvent JSON objects
//! - Server sends final `{"type": "Complete", ...}` and closes

use axum::{
    extract::{Query, State, WebSocketUpgrade},
    response::Response,
};
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::auth;
use crate::server::AppState;

#[derive(Deserialize, Default)]
pub struct ChatWsQuery {
    token: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    Query(q): Query<ChatWsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
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

    ws.on_upgrade(move |socket| handle_chat_socket(socket, state))
}

async fn handle_chat_socket(mut socket: axum::extract::ws::WebSocket, state: AppState) {
    use axum::extract::ws::Message;

    info!("Chat WebSocket client connected");

    // Wait for the client's first message (the chat request).
    let request = match socket.recv().await {
        Some(Ok(Message::Text(text))) => {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => v,
                Err(e) => {
                    let _ = socket
                        .send(Message::Text(
                            serde_json::json!({"type": "Error", "message": e.to_string(), "recoverable": false}).to_string().into(),
                        ))
                        .await;
                    return;
                }
            }
        }
        _ => return,
    };

    let message = request
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if message.is_empty() {
        let _ = socket
            .send(Message::Text(
                serde_json::json!({"type": "Error", "message": "empty message", "recoverable": false}).to_string().into(),
            ))
            .await;
        return;
    }

    // Submit to daemon via chat_full.
    let chat_req = serde_json::json!({
        "message": message,
        "source": "cli",
        "project": request.get("project").and_then(|v| v.as_str()).unwrap_or(""),
        "agent": request.get("agent").and_then(|v| v.as_str()).unwrap_or(""),
    });

    let task_handle = match state.ipc.cmd_with("chat_full", chat_req).await {
        Ok(resp) => resp,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type": "Error", "message": e.to_string(), "recoverable": false}).to_string().into(),
                ))
                .await;
            return;
        }
    };

    let task_id = task_handle
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if task_id.is_empty() {
        // Quick-path response (no task created) — send as text and close.
        if let Some(context) = task_handle.get("context").and_then(|v| v.as_str()) {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type": "TextDelta", "text": context}).to_string().into(),
                ))
                .await;
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type": "Complete", "stop_reason": "quick_path", "total_prompt_tokens": 0, "total_completion_tokens": 0, "iterations": 0, "cost_usd": 0.0}).to_string().into(),
                ))
                .await;
        }
        return;
    }

    let task_id_owned = task_id.to_string();

    // Send task_id to client so they can track it.
    let _ = socket
        .send(Message::Text(
            serde_json::json!({"type": "Status", "message": format!("Task {} started", task_id_owned)}).to_string().into(),
        ))
        .await;

    // Poll for worker events that match this task_id.
    // Using 500ms polling interval for low-latency streaming.
    let poll_interval = std::time::Duration::from_millis(500);
    let mut interval = tokio::time::interval(poll_interval);
    let mut worker_cursor: Option<u64> = None;
    let mut completed = false;
    let timeout = tokio::time::Instant::now() + std::time::Duration::from_secs(600);

    while !completed && tokio::time::Instant::now() < timeout {
        tokio::select! {
            _ = interval.tick() => {
                // Poll worker events from daemon.
                let req = match worker_cursor {
                    Some(c) => serde_json::json!({"cursor": c}),
                    None => serde_json::json!({}),
                };

                if let Ok(resp) = state.ipc.cmd_with("worker_events", req).await {
                    if let Some(next) = resp.get("next_cursor").and_then(|v| v.as_u64()) {
                        worker_cursor = Some(next);
                    }

                    if let Some(events) = resp.get("events").and_then(|v| v.as_array()) {
                        for event in events {
                            // Filter for our task_id.
                            let event_task = event.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                            if event_task != task_id_owned {
                                continue;
                            }

                            let event_type = event.get("event_type").and_then(|v| v.as_str()).unwrap_or("");

                            match event_type {
                                "ChatStream" => {
                                    // Forward the inner ChatStreamEvent to the client.
                                    if let Some(inner) = event.get("event") {
                                        let json = serde_json::to_string(inner).unwrap_or_default();
                                        if socket.send(Message::Text(json.into())).await.is_err() {
                                            return;
                                        }
                                        // Check if this is the Complete event.
                                        if inner.get("type").and_then(|v| v.as_str()) == Some("Complete") {
                                            completed = true;
                                        }
                                    }
                                }
                                "TaskCompleted" | "TaskFailed" => {
                                    completed = true;
                                    // If we didn't get a ChatStream Complete, synthesize one.
                                    let _ = socket.send(Message::Text(
                                        serde_json::to_string(event).unwrap_or_default().into(),
                                    )).await;
                                }
                                _ => {
                                    // Forward other execution events too (Progress, ToolCallStarted, etc.)
                                    let json = serde_json::to_string(event).unwrap_or_default();
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("chat client disconnected");
                        return;
                    }
                    _ => {}
                }
            }
        }
    }

    if !completed {
        warn!(task_id = %task_id_owned, "chat stream timed out");
        let _ = socket
            .send(Message::Text(
                serde_json::json!({"type": "Error", "message": "timeout", "recoverable": false}).to_string().into(),
            ))
            .await;
    }

    info!(task_id = %task_id_owned, "chat stream completed");
}
