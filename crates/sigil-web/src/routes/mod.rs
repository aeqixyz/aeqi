use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::server::AppState;

/// Build protected API routes (auth required).
pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/projects", get(projects))
        .route("/tasks", get(tasks).post(create_task))
        .route("/tasks/{id}/close", post(close_task))
        .route("/missions", get(missions))
        .route("/agents", get(agents))
        .route("/audit", get(audit))
        .route("/blackboard", get(blackboard).post(post_blackboard_entry))
        .route("/expertise", get(expertise))
        .route("/cost", get(cost))
        .route("/dashboard", get(dashboard))
        .route("/worker/events", get(worker_events))
        .route("/chat", post(chat))
        .route("/chat/full", post(chat_full))
        .route("/chat/poll/{task_id}", get(chat_poll))
        .route("/chat/history", get(chat_history))
        .route("/chat/timeline", get(chat_timeline))
        .route("/chat/channels", get(chat_channels))
        .route("/brief", get(brief))
        .route("/crons", get(crons))
        .route("/memories", get(memories))
        .route("/memory/profile", get(memory_profile))
        .route("/memory/graph", get(memory_graph))
        .route("/skills", get(skills))
        .route("/pipelines", get(pipelines))
        .route("/projects/{name}/knowledge", get(project_knowledge))
        .route("/knowledge/channel", get(channel_knowledge))
        .route("/knowledge/store", post(knowledge_store))
        .route("/knowledge/delete", post(knowledge_delete))
        .route("/rate-limit", get(rate_limit))
        .route("/agents/{name}/identity", get(agent_identity))
        .route("/agents/{name}/files", post(save_agent_file))
}

// --- Status ---

async fn status(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "status", serde_json::Value::Null).await
}

// --- Projects ---

async fn projects(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "projects", serde_json::Value::Null).await
}

// --- Tasks ---

#[derive(Deserialize, Default)]
struct TasksQuery {
    project: Option<String>,
    status: Option<String>,
}

async fn tasks(State(state): State<AppState>, Query(q): Query<TasksQuery>) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(status) = &q.status {
        params["status"] = serde_json::Value::String(status.clone());
    }
    ipc_proxy(state, "tasks", params).await
}

// --- Missions ---

#[derive(Deserialize, Default)]
struct MissionsQuery {
    project: Option<String>,
}

async fn missions(State(state): State<AppState>, Query(q): Query<MissionsQuery>) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    ipc_proxy(state, "missions", params).await
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

// --- Audit ---

#[derive(Deserialize, Default)]
struct AuditQuery {
    project: Option<String>,
    last: Option<u32>,
}

async fn audit(State(state): State<AppState>, Query(q): Query<AuditQuery>) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(last) = q.last {
        params["last"] = serde_json::json!(last);
    }
    ipc_proxy(state, "audit", params).await
}

// --- Blackboard ---

#[derive(Deserialize, Default)]
struct BlackboardQuery {
    project: Option<String>,
    limit: Option<u32>,
}

async fn blackboard(State(state): State<AppState>, Query(q): Query<BlackboardQuery>) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::Value::String(project.clone());
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    ipc_proxy(state, "blackboard", params).await
}

// --- Expertise ---

#[derive(Deserialize, Default)]
struct ExpertiseQuery {
    domain: Option<String>,
}

async fn expertise(State(state): State<AppState>, Query(q): Query<ExpertiseQuery>) -> Response {
    let mut params = serde_json::json!({});
    if let Some(domain) = &q.domain {
        params["domain"] = serde_json::Value::String(domain.clone());
    }
    ipc_proxy(state, "expertise", params).await
}

// --- Cost ---

async fn cost(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "cost", serde_json::Value::Null).await
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
    Query(q): Query<WorkerEventsQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(cursor) = q.cursor {
        params["cursor"] = serde_json::json!(cursor);
    }
    ipc_proxy(state, "worker_events", params).await
}

// --- Memories ---

#[derive(Deserialize, Default)]
struct MemoriesQuery {
    project: Option<String>,
    query: Option<String>,
    limit: Option<u64>,
}

async fn memories(State(state): State<AppState>, Query(q): Query<MemoriesQuery>) -> Response {
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
    ipc_proxy(state, "memories", params).await
}

// --- Memory Profile ---

#[derive(Deserialize, Default)]
struct MemoryProfileQuery {
    project: Option<String>,
}

async fn memory_profile(
    State(state): State<AppState>,
    Query(q): Query<MemoryProfileQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::json!(project);
    }
    ipc_proxy(state, "memory_profile", params).await
}

// --- Memory Graph ---

#[derive(Deserialize, Default)]
struct MemoryGraphQuery {
    project: Option<String>,
    limit: Option<u64>,
}

async fn memory_graph(
    State(state): State<AppState>,
    Query(q): Query<MemoryGraphQuery>,
) -> Response {
    let mut params = serde_json::json!({});
    if let Some(project) = &q.project {
        params["project"] = serde_json::json!(project);
    }
    if let Some(limit) = q.limit {
        params["limit"] = serde_json::json!(limit);
    }
    ipc_proxy(state, "memory_graph", params).await
}

// --- Skills ---

async fn skills(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "skills", serde_json::Value::Null).await
}

// --- Pipelines ---

async fn pipelines(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "pipelines", serde_json::Value::Null).await
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
    ipc_proxy(state, "channel_knowledge", params).await
}

// --- Knowledge CRUD ---

async fn knowledge_store(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    ipc_proxy(state, "knowledge_store", body).await
}

async fn knowledge_delete(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    ipc_proxy(state, "knowledge_delete", body).await
}

// --- Agent Identity ---

async fn agent_identity(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    ipc_proxy(state, "agent_identity", serde_json::json!({"name": name})).await
}

async fn save_agent_file(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut params = body;
    params["name"] = serde_json::Value::String(name);
    ipc_proxy(state, "save_agent_file", params).await
}

// --- Rate Limit ---

async fn rate_limit(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "rate_limit", serde_json::Value::Null).await
}

// --- Brief ---

async fn brief(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "brief", serde_json::Value::Null).await
}

// --- Crons ---

async fn crons(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "crons", serde_json::Value::Null).await
}

// --- Chat ---

async fn chat(State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> Response {
    ipc_proxy(state, "chat", body).await
}

async fn chat_full(State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> Response {
    ipc_proxy(state, "chat_full", body).await
}

async fn chat_poll(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Response {
    ipc_proxy(state, "chat_poll", serde_json::json!({"task_id": task_id})).await
}

#[derive(Deserialize, Default)]
struct ChatHistoryQuery {
    chat_id: Option<i64>,
    project: Option<String>,
    department: Option<String>,
    channel_name: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn chat_history(
    State(state): State<AppState>,
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
    ipc_proxy(state, "chat_history", params).await
}

async fn chat_timeline(
    State(state): State<AppState>,
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
    ipc_proxy(state, "chat_timeline", params).await
}

async fn chat_channels(State(state): State<AppState>) -> Response {
    ipc_proxy(state, "chat_channels", serde_json::Value::Null).await
}

// --- Write: Create Task ---

async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    ipc_proxy(state, "create_task", body).await
}

// --- Write: Close Task ---

async fn close_task(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut params = body;
    params["task_id"] = serde_json::Value::String(id);
    ipc_proxy(state, "close_task", params).await
}

// --- Write: Post to Blackboard ---

async fn post_blackboard_entry(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    ipc_proxy(state, "post_blackboard", body).await
}

// --- Helper ---

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
