use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::scope::RequestScope;
use crate::server::AppState;

/// Build the public webhook route (no auth required).
pub fn webhook_routes() -> Router<AppState> {
    Router::new().route("/webhooks/{public_id}", post(webhook_handler))
}

/// Build protected API routes (auth required).
pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/companies", get(projects).post(create_company))
        .route("/tasks", get(tasks).post(create_task))
        .route("/tasks/{id}/close", post(close_task))
        .route("/agents", get(agents))
        .route("/agents/registry", get(agents_registry))
        .route("/agents/spawn", post(agents_spawn))
        .route("/agents/{id}/retire", post(agent_retire))
        .route("/agents/{id}/activate", post(agent_activate))
        .route("/triggers", get(triggers))
        .route("/audit", get(audit))
        .route("/notes", get(notes).post(post_note_entry))
        .route("/expertise", get(expertise))
        .route("/cost", get(cost))
        .route("/dashboard", get(dashboard))
        .route("/worker/events", get(worker_events))
        .route("/chat", post(chat)) // Deprecated: use /session/send
        .route("/chat/full", post(chat_full)) // Deprecated: use /session/send
        .route("/session/send", post(session_send))
        .route("/chat/poll/{task_id}", get(chat_poll)) // Deprecated: use /session/send polling
        .route("/chat/history", get(chat_history)) // Deprecated: kept for backwards compat
        .route("/chat/timeline", get(chat_timeline)) // Deprecated: kept for backwards compat
        .route("/chat/channels", get(chat_channels)) // Deprecated: kept for backwards compat
        .route("/memories", get(memories))
        .route("/memory/profile", get(memory_profile))
        .route("/memory/graph", get(memory_graph))
        .route("/skills", get(skills))
        .route("/pipelines", get(pipelines))
        .route("/companies/{name}/knowledge", get(project_knowledge))
        .route("/knowledge/channel", get(channel_knowledge))
        .route("/knowledge/store", post(knowledge_store))
        .route("/knowledge/delete", post(knowledge_delete))
        .route("/rate-limit", get(rate_limit))
        .route("/agents/{name}/identity", get(agent_identity))
        .route("/agents/{name}/prompts", get(agent_prompts))
        .route("/agents/{name}/files", post(save_agent_file))
        .route("/departments", get(departments))
        .route("/approvals", get(approvals))
        .route("/approvals/{id}/resolve", post(resolve_approval))
        .route("/sessions", get(sessions).post(create_session))
        .route("/sessions/{id}/close", post(close_session))
        .route("/sessions/{id}/messages", get(session_messages))
        .route("/sessions/{id}/children", get(session_children))
        .route("/vfs", get(vfs_list))
        .route("/vfs/search", get(vfs_search))
        .route("/vfs/{*path}", get(vfs_read))
}

// --- Status ---

async fn status(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "status", serde_json::Value::Null).await
}

// --- Companies ---

async fn projects(State(state): State<AppState>, req: Request) -> Response {
    let user_id = crate::auth::extract_user_id(&state, &req);

    // In accounts mode with a user, filter by membership.
    if let Some(ref uid) = user_id
        && let Some(ref store) = state.user_store
    {
        let user_companies = store.get_user_companies(uid);

        let ipc_resp = ipc_proxy(state, "companies", serde_json::Value::Null).await;
        let body_bytes = axum::body::to_bytes(ipc_resp.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();

        if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            if let Some(companies) = json.get_mut("companies").and_then(|v| v.as_array_mut()) {
                companies.retain(|c| {
                    c.get("name")
                        .and_then(|n| n.as_str())
                        .map(|n| user_companies.contains(&n.to_string()))
                        .unwrap_or(false)
                });
            }
            return axum::Json(json).into_response();
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to parse companies",
        )
            .into_response();
    }

    // Non-accounts mode or no user: return all.
    ipc_proxy(state, "companies", serde_json::Value::Null).await
}

async fn create_company(State(state): State<AppState>, req: Request) -> Response {
    let user_id = crate::auth::extract_user_id(&state, &req);

    // Extract body.
    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 64)
        .await
        .unwrap_or_default();
    let body: serde_json::Value =
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);

    let resp = ipc_proxy(state.clone(), "create_company", body.clone()).await;

    // Associate company with user in accounts mode.
    if let Some(uid) = user_id
        && let Some(name) = body.get("name").and_then(|n| n.as_str())
        && let Some(ref store) = state.user_store
    {
        store.add_user_company(&uid, name, "owner");
    }

    resp
}

// --- Tasks ---

#[derive(Deserialize, Default)]
struct TasksQuery {
    project: Option<String>,
    status: Option<String>,
}

async fn tasks(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<TasksQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(status) = &q.status {
        params["status"] = serde_json::Value::String(status.clone());
    }
    scoped_ipc(state, &scope, "tasks", params).await
}

// --- Agents ---

async fn agents(State(state): State<AppState>) -> Response {
    let agents_config: Vec<serde_json::Value> = state
        .agents_config
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name,
                "prefix": a.prefix,
                "model": a.model,
                "role": a.role,
                "expertise": a.expertise,
            })
        })
        .collect();

    let expertise = state.ipc.cmd("expertise").await.ok();
    let scores = expertise
        .as_ref()
        .and_then(|e| e.get("scores"))
        .and_then(|s| s.as_array());

    let enriched: Vec<serde_json::Value> = agents_config
        .into_iter()
        .map(|mut agent| {
            if let (Some(name), Some(scores)) = (agent.get("name").and_then(|n| n.as_str()), scores)
            {
                let agent_scores: Vec<&serde_json::Value> = scores
                    .iter()
                    .filter(|s| s.get("agent").and_then(|a| a.as_str()) == Some(name))
                    .collect();
                if !agent_scores.is_empty() {
                    agent["expertise_scores"] = serde_json::json!(agent_scores);
                }
            }
            agent
        })
        .collect();

    Json(serde_json::json!({"ok": true, "agents": enriched})).into_response()
}

// --- Agent Registry ---

#[derive(Deserialize, Default)]
struct RegistryQuery {
    project: Option<String>,
    status: Option<String>,
}

async fn agents_registry(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<RegistryQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(status) = &q.status {
        params["status"] = serde_json::Value::String(status.clone());
    }
    scoped_ipc(state, &scope, "agents_registry", params).await
}

async fn agents_spawn(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "agent_spawn", body).await
}

async fn agent_retire(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "agent_set_status",
        serde_json::json!({"name": id, "status": "retired"}),
    )
    .await
}

async fn agent_activate(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "agent_set_status",
        serde_json::json!({"name": id, "status": "active"}),
    )
    .await
}

// --- Triggers ---

async fn triggers(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "triggers", serde_json::Value::Null).await
}

// --- Audit ---

#[derive(Deserialize, Default)]
struct AuditQuery {
    project: Option<String>,
    last: Option<u32>,
}

async fn audit(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<AuditQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(last) = q.last {
        params["last"] = serde_json::json!(last);
    }
    scoped_ipc(state, &scope, "audit", params).await
}

// --- Notes ---

#[derive(Deserialize, Default)]
struct NotesQuery {
    project: Option<String>,
    limit: Option<u32>,
}

async fn notes(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<NotesQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    scoped_ipc(state, &scope, "notes", params).await
}

// --- Expertise ---

#[derive(Deserialize, Default)]
struct ExpertiseQuery {
    domain: Option<String>,
}

async fn expertise(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<ExpertiseQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(domain) = &q.domain {
        params["domain"] = serde_json::Value::String(domain.clone());
    }
    scoped_ipc(state, &scope, "expertise", params).await
}

// --- Cost ---

async fn cost(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "cost", serde_json::Value::Null).await
}

// --- Dashboard (aggregate) ---

async fn dashboard(State(state): State<AppState>) -> Response {
    let status = state.ipc.cmd("status").await.ok();
    let audit = state
        .ipc
        .cmd_with("audit", serde_json::json!({"last": 10}))
        .await
        .ok();
    let cost = state.ipc.cmd("cost").await.ok();

    Json(serde_json::json!({
        "ok": true,
        "status": status,
        "recent_audit": audit.as_ref().and_then(|a| a.get("events")),
        "cost": cost,
    }))
    .into_response()
}

// --- Worker Events ---

#[derive(Deserialize, Default)]
struct WorkerEventsQuery {
    cursor: Option<u64>,
}

async fn worker_events(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<WorkerEventsQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(cursor) = q.cursor {
        params["cursor"] = serde_json::json!(cursor);
    }
    scoped_ipc(state, &scope, "worker_events", params).await
}

// --- Memories ---

#[derive(Deserialize, Default)]
struct MemoriesQuery {
    project: Option<String>,
    query: Option<String>,
    limit: Option<u64>,
}

async fn memories(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<MemoriesQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::json!(project);
    }
    if let Some(query) = &q.query {
        params["query"] = serde_json::json!(query);
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    scoped_ipc(state, &scope, "memories", params).await
}

// --- Memory Profile ---

#[derive(Deserialize, Default)]
struct MemoryProfileQuery {
    project: Option<String>,
}

async fn memory_profile(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<MemoryProfileQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::json!(project);
    }
    scoped_ipc(state, &scope, "memory_profile", params).await
}

// --- Memory Graph ---

#[derive(Deserialize, Default)]
struct MemoryGraphQuery {
    project: Option<String>,
    limit: Option<u64>,
}

async fn memory_graph(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<MemoryGraphQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::json!(project);
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    scoped_ipc(state, &scope, "memory_graph", params).await
}

// --- Skills ---

async fn skills(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "skills", serde_json::Value::Null).await
}

// --- Pipelines ---

async fn pipelines(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "pipelines", serde_json::Value::Null).await
}

// --- Project Knowledge ---

async fn project_knowledge(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    ipc_proxy(
        state,
        "project_knowledge",
        serde_json::json!({"project": name}),
    )
    .await
}

// --- Channel Knowledge ---

#[derive(Deserialize, Default)]
struct ChannelKnowledgeQuery {
    project: Option<String>,
    query: Option<String>,
    limit: Option<u64>,
}

async fn channel_knowledge(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<ChannelKnowledgeQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::json!(project);
    }
    if let Some(query) = &q.query {
        params["query"] = serde_json::json!(query);
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    scoped_ipc(state, &scope, "channel_knowledge", params).await
}

// --- Knowledge CRUD ---

async fn knowledge_store(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "knowledge_store", body).await
}

async fn knowledge_delete(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "knowledge_delete", body).await
}

// --- Agent Identity ---

async fn agent_identity(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "agent_identity",
        serde_json::json!({"name": name}),
    )
    .await
}

async fn agent_prompts(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "agent_info",
        serde_json::json!({"name": name}),
    )
    .await
}

async fn save_agent_file(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut params = body;
    params["name"] = serde_json::Value::String(name);
    scoped_ipc(state, &scope, "save_agent_file", params).await
}

// --- Rate Limit ---

async fn rate_limit(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "rate_limit", serde_json::Value::Null).await
}

// --- Chat ---

async fn chat(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "chat", body).await
}

async fn chat_full(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "chat_full", body).await
}

async fn session_send(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "session_send", body).await
}

async fn chat_poll(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "chat_poll",
        serde_json::json!({"task_id": task_id}),
    )
    .await
}

#[derive(Deserialize, Default)]
struct ChatHistoryQuery {
    chat_id: Option<i64>,
    project: Option<String>,
    department: Option<String>,
    channel_name: Option<String>,
    agent_id: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn chat_history(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<ChatHistoryQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(chat_id) = q.chat_id {
        params["chat_id"] = serde_json::json!(chat_id);
    }
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(department) = &q.department {
        params["department"] = serde_json::Value::String(department.clone());
    }
    if let Some(channel_name) = &q.channel_name {
        params["channel_name"] = serde_json::Value::String(channel_name.clone());
    }
    if let Some(agent_id) = &q.agent_id {
        params["agent_id"] = serde_json::Value::String(agent_id.clone());
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    if let Some(offset) = q.offset {
        params["offset"] = serde_json::json!(offset);
    }
    scoped_ipc(state, &scope, "chat_history", params).await
}

async fn chat_timeline(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<ChatHistoryQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(chat_id) = q.chat_id {
        params["chat_id"] = serde_json::json!(chat_id);
    }
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(department) = &q.department {
        params["department"] = serde_json::Value::String(department.clone());
    }
    if let Some(channel_name) = &q.channel_name {
        params["channel_name"] = serde_json::Value::String(channel_name.clone());
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    if let Some(offset) = q.offset {
        params["offset"] = serde_json::json!(offset);
    }
    scoped_ipc(state, &scope, "chat_timeline", params).await
}

async fn chat_channels(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "chat_channels", serde_json::Value::Null).await
}

// --- Write: Create Task ---

async fn create_task(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "create_task", body).await
}

// --- Write: Close Task ---

async fn close_task(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut params = body;
    params["task_id"] = serde_json::Value::String(id);
    scoped_ipc(state, &scope, "close_task", params).await
}

// --- Write: Post Note ---

async fn post_note_entry(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "post_notes", body).await
}

// --- Approvals ---

// --- Departments ---

async fn departments(State(state): State<AppState>, scope: RequestScope) -> Response {
    scoped_ipc(state, &scope, "departments", serde_json::json!({})).await
}

// --- Approvals ---

#[derive(Deserialize, Default)]
struct ApprovalsQuery {
    status: Option<String>,
}

async fn approvals(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<ApprovalsQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(status) = &q.status {
        params["status"] = serde_json::Value::String(status.clone());
    }
    scoped_ipc(state, &scope, "approvals", params).await
}

async fn resolve_approval(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut params = body;
    params["approval_id"] = serde_json::Value::String(id);
    scoped_ipc(state, &scope, "resolve_approval", params).await
}
// --- Sessions ---

#[derive(Deserialize, Default)]
struct SessionsQuery {
    agent_id: Option<String>,
}

async fn sessions(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<SessionsQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(agent_id) = &q.agent_id {
        params["agent_id"] = serde_json::Value::String(agent_id.clone());
    }
    scoped_ipc(state, &scope, "list_sessions", params).await
}

async fn create_session(
    State(state): State<AppState>,
    scope: RequestScope,
    Json(body): Json<serde_json::Value>,
) -> Response {
    scoped_ipc(state, &scope, "create_session", body).await
}

async fn close_session(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "close_session",
        serde_json::json!({"session_id": id}),
    )
    .await
}

#[derive(Deserialize, Default)]
struct SessionMessagesQuery {
    limit: Option<u64>,
}

async fn session_messages(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(q): Query<SessionMessagesQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(50);
    scoped_ipc(
        state,
        &scope,
        "session_messages",
        serde_json::json!({"session_id": id, "limit": limit}),
    )
    .await
}

async fn session_children(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "session_children",
        serde_json::json!({"session_id": id}),
    )
    .await
}

// --- Webhook (public, no auth) ---

async fn webhook_handler(
    State(state): State<AppState>,
    axum::extract::Path(public_id): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let signature = headers
        .get("x-signature-256")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // Encode body as base64 for IPC transport.
    use base64::Engine;
    let body_b64 = base64::engine::general_purpose::STANDARD.encode(&body);

    let mut params = serde_json::json!({
        "public_id": public_id,
        "body_b64": body_b64,
    });
    if let Some(sig) = signature {
        params["signature"] = serde_json::Value::String(sig);
    }

    let result = state.ipc.cmd_with("webhook_fire", params).await;

    match result {
        Ok(resp) => {
            if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                Json(resp).into_response()
            } else {
                let error = resp
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error");
                let status = if error.contains("not found") {
                    StatusCode::NOT_FOUND
                } else if error.contains("signature") {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                (status, Json(resp)).into_response()
            }
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        )
            .into_response(),
    }
}

// --- VFS (Virtual Filesystem) ---

#[derive(Deserialize, Default)]
struct VfsListQuery {
    path: Option<String>,
}

async fn vfs_list(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<VfsListQuery>,
) -> Response {
    let path = q.path.unwrap_or_else(|| "/".to_string());
    scoped_ipc(state, &scope, "vfs_list", serde_json::json!({"path": path})).await
}

async fn vfs_read(
    State(state): State<AppState>,
    scope: RequestScope,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    scoped_ipc(state, &scope, "vfs_read", serde_json::json!({"path": path})).await
}

#[derive(Deserialize, Default)]
struct VfsSearchQuery {
    query: String,
}

async fn vfs_search(
    State(state): State<AppState>,
    scope: RequestScope,
    Query(q): Query<VfsSearchQuery>,
) -> Response {
    scoped_ipc(
        state,
        &scope,
        "vfs_search",
        serde_json::json!({"query": q.query}),
    )
    .await
}

// --- Helper ---

/// IPC proxy with tenancy scope injection. All new endpoints should use this.
async fn scoped_ipc(
    state: AppState,
    scope: &RequestScope,
    cmd: &str,
    mut params: serde_json::Value,
) -> Response {
    scope.inject(&mut params);
    ipc_proxy(state, cmd, params).await
}

async fn ipc_proxy(state: AppState, cmd: &str, params: serde_json::Value) -> Response {
    let result = if params.is_null() || params.as_object().is_some_and(|m| m.is_empty()) {
        state.ipc.cmd(cmd).await
    } else {
        state.ipc.cmd_with(cmd, params).await
    };

    match result {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        )
            .into_response(),
    }
}
