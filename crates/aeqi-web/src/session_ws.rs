//! Streaming session WebSocket endpoint.
//!
//! Accepts user messages, submits them to the daemon via session_send with
//! streaming mode, forwarding ChatStreamEvents to the client in real-time.
//!
//! Protocol:
//! - Client sends: `{"message": "...", "agent": "...", "agent_id": "...", "session_id": "..."}`
//! - Server streams: `{"type": "TextDelta", "text": "..."}` per token
//! - Server streams: `{"type": "ToolStart", ...}`, `{"type": "ToolComplete", ...}`
//! - Server sends final: `{"type": "Complete", "done": true, ...}`
//! - Connection stays open for next message (persistent session)

use aeqi_core::config::AuthMode;
use axum::{
    extract::{Query, State, WebSocketUpgrade},
    response::Response,
};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use crate::auth;
use crate::server::AppState;

#[derive(Deserialize, Default)]
pub struct SessionWsQuery {
    token: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    Query(q): Query<SessionWsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    // Validate token from query param, dispatching by auth mode.
    match state.auth_mode {
        AuthMode::None => { /* allow without validation */ }
        AuthMode::Secret | AuthMode::Accounts => {
            let secret = auth::signing_secret(&state);
            let token = q.token.as_deref().unwrap_or("");
            if auth::validate_token(token, secret).is_err() {
                return axum::response::IntoResponse::into_response((
                    axum::http::StatusCode::UNAUTHORIZED,
                    "invalid or missing token",
                ));
            }
        }
    }

    ws.on_upgrade(move |socket| handle_session_socket(socket, state))
}

async fn handle_session_socket(mut socket: axum::extract::ws::WebSocket, state: AppState) {
    use axum::extract::ws::Message;

    info!("Session WebSocket client connected");

    let mut session_id: Option<String> = None;

    loop {
        let request = match socket.recv().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = socket
                            .send(Message::Text(
                                serde_json::json!({"type": "Error", "message": e.to_string(), "recoverable": true}).to_string().into(),
                            ))
                            .await;
                        continue;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => break,
            _ => continue,
        };

        let message = request
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if message.is_empty() {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type": "Error", "message": "empty message", "recoverable": true}).to_string().into(),
                ))
                .await;
            continue;
        }

        let agent = request.get("agent").and_then(|v| v.as_str()).unwrap_or("");

        let agent_id = request
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let req_session_id = request
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| session_id.clone());

        let mut session_req = serde_json::json!({
            "cmd": "session_send",
            "message": message,
            "agent": agent,
            "stream": true,
        });
        if !agent_id.is_empty() {
            session_req["agent_id"] = serde_json::json!(agent_id);
        }
        if let Some(ref sid) = req_session_id {
            session_req["session_id"] = serde_json::json!(sid);
        }

        // Open a raw IPC connection and stream events directly to WebSocket.
        match stream_ipc_to_ws(
            state.ipc.socket_path(),
            &session_req,
            &mut socket,
            &mut session_id,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::json!({"type": "Error", "message": e.to_string(), "recoverable": true})
                            .to_string()
                            .into(),
                    ))
                    .await;
            }
        }
    }

    info!("Session WebSocket client disconnected");
}

/// Open a raw IPC connection, send the session request, and forward each JSON line to the WebSocket.
async fn stream_ipc_to_ws(
    socket_path: &std::path::Path,
    request: &serde_json::Value,
    ws: &mut axum::extract::ws::WebSocket,
    session_id: &mut Option<String>,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message;

    let stream = tokio::net::UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    let mut req_bytes = serde_json::to_vec(request)?;
    req_bytes.push(b'\n');
    writer.write_all(&req_bytes).await?;

    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let event: serde_json::Value = serde_json::from_str(&line)?;

        // Capture session_id.
        if let Some(sid) = event.get("session_id").and_then(|v| v.as_str()) {
            *session_id = Some(sid.to_string());
        }

        let is_done = event.get("done").and_then(|v| v.as_bool()).unwrap_or(false);

        // Forward to WebSocket.
        if ws.send(Message::Text(line.into())).await.is_err() {
            break;
        }

        if is_done {
            break;
        }
    }

    Ok(())
}
