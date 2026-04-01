use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use sigil_core::traits::Tool;
use sigil_core::traits::{Channel, ToolResult, ToolSpec};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::registry::ProjectRegistry;
use sigil_core::traits::{Memory, MemoryCategory, MemoryQuery, MemoryScope};

/// Tool for querying project health, task counts, and worker states.
pub struct ProjectStatusTool {
    registry: Arc<ProjectRegistry>,
}

impl ProjectStatusTool {
    pub fn new(registry: Arc<ProjectRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ProjectStatusTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let project_filter = args.get("project").and_then(|v| v.as_str());

        let status = self.registry.status().await;
        let mut output = String::new();

        for ds in &status.projects {
            if let Some(filter) = project_filter
                && ds.name != filter
            {
                continue;
            }
            output.push_str(&format!(
                "{}: {} open, {} ready | workers: {} idle, {} working, {} bonded\n",
                ds.name,
                ds.open_tasks,
                ds.ready_tasks,
                ds.workers_idle,
                ds.workers_working,
                ds.workers_bonded,
            ));
        }

        if project_filter.is_none() {
            output.push_str(&format!(
                "\nUnread dispatches: {}\n",
                status.unread_dispatches
            ));
        }

        if output.is_empty() {
            if let Some(filter) = project_filter {
                return Ok(ToolResult::error(format!("Project not found: {filter}")));
            }
            output = "No projects registered.\n".to_string();
        }

        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "project_status".to_string(),
            description: "Get project health, task counts, and worker states. Optionally filter by project name.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Optional project name to filter (omit for all projects)" }
                }
            }),
        }
    }

    fn name(&self) -> &str {
        "project_status"
    }
}

/// Tool for assigning a task to a target project.
pub struct ProjectAssignTool {
    registry: Arc<ProjectRegistry>,
}

impl ProjectAssignTool {
    pub fn new(registry: Arc<ProjectRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ProjectAssignTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let project = args
            .get("project")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing project"))?;
        let subject = args
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing subject"))?;
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match self.registry.assign(project, subject, description).await {
            Ok(task) => Ok(ToolResult::success(format!(
                "Assigned {} [{}] {} to project '{}'",
                task.id, task.priority, task.subject, project
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to assign: {e}"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "project_assign".to_string(),
            description: "Assign a task to a specific project.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Target project name. Use project_list to discover available projects." },
                    "subject": { "type": "string", "description": "Task title" },
                    "description": { "type": "string", "description": "Detailed task description" }
                },
                "required": ["project", "subject"]
            }),
        }
    }

    fn name(&self) -> &str {
        "project_assign"
    }
}

/// Tool for listing all registered projects with metadata.
pub struct ProjectListTool {
    registry: Arc<ProjectRegistry>,
}

impl ProjectListTool {
    pub fn new(registry: Arc<ProjectRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ProjectListTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let projects: Vec<serde_json::Value> = self.registry.projects_info().await;

        if projects.is_empty() {
            return Ok(ToolResult::success("No projects registered."));
        }

        let mut output = String::new();
        for d in &projects {
            output.push_str(&format!(
                "{} (prefix: {}, model: {}, max_workers: {})\n",
                d["name"].as_str().unwrap_or("?"),
                d["prefix"].as_str().unwrap_or("?"),
                d["model"].as_str().unwrap_or("?"),
                d["max_workers"].as_u64().unwrap_or(0),
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "project_list".to_string(),
            description: "List all registered projects with their prefix, model, and worker count."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str {
        "project_list"
    }
}

/// Tool for reading unread mail addressed to the agent.
pub struct MailReadTool {
    dispatch_bus: Arc<DispatchBus>,
}

impl MailReadTool {
    pub fn new(dispatch_bus: Arc<DispatchBus>) -> Self {
        Self { dispatch_bus }
    }
}

#[async_trait]
impl Tool for MailReadTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let messages = self.dispatch_bus.read("leader").await;

        if messages.is_empty() {
            return Ok(ToolResult::success("No unread mail."));
        }

        let mut output = String::new();
        for m in &messages {
            output.push_str(&format!(
                "[{}] from={} subject={}\n{}\n\n",
                m.timestamp.format("%H:%M:%S"),
                m.from,
                m.kind.subject_tag(),
                m.kind.body_text(),
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "dispatch_read".to_string(),
            description: "Read all unread dispatches addressed to the agent.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str {
        "dispatch_read"
    }
}

/// Tool for sending mail through the bus.
pub struct MailSendTool {
    dispatch_bus: Arc<DispatchBus>,
}

impl MailSendTool {
    pub fn new(dispatch_bus: Arc<DispatchBus>) -> Self {
        Self { dispatch_bus }
    }
}

#[async_trait]
impl Tool for MailSendTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let to = args
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing to"))?;
        let subject = args
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing subject"))?;
        let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");

        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let kind = match subject.to_uppercase().as_str() {
            "RESOLVED" => DispatchKind::Resolution {
                task_id,
                answer: body.to_string(),
            },
            _ => DispatchKind::Resolution {
                task_id,
                answer: format!("[{}] {}", subject, body),
            },
        };

        self.dispatch_bus
            .send(Dispatch::new_typed("leader", to, kind))
            .await;
        Ok(ToolResult::success(format!(
            "Message sent to '{to}': {subject}"
        )))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "dispatch_send".to_string(),
            description:
                "Send a dispatch message to another project or agent through the dispatch bus."
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": { "type": "string", "description": "Recipient (project name or agent name)" },
                    "subject": { "type": "string", "description": "Message subject" },
                    "body": { "type": "string", "description": "Message body" }
                },
                "required": ["to", "subject"]
            }),
        }
    }

    fn name(&self) -> &str {
        "dispatch_send"
    }
}

/// Tool for listing all unblocked tasks across all projects.
pub struct AllReadyTool {
    registry: Arc<ProjectRegistry>,
}

impl AllReadyTool {
    pub fn new(registry: Arc<ProjectRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for AllReadyTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let ready: Vec<(String, sigil_tasks::Task)> = self.registry.all_ready().await;

        if ready.is_empty() {
            return Ok(ToolResult::success("No ready work across any project."));
        }

        let mut output = String::new();
        for (project_name, task) in &ready {
            output.push_str(&format!(
                "[{}] {} [{}] {} — {}\n",
                project_name,
                task.id,
                task.priority,
                task.subject,
                if task.description.is_empty() {
                    "(no description)"
                } else {
                    &task.description
                }
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "all_ready".to_string(),
            description: "List all unblocked tasks across all projects that are ready for work."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str {
        "all_ready"
    }
}

/// Tool for replying to a channel message (Telegram, Discord, etc.)
pub struct ChannelReplyTool {
    channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
}

impl ChannelReplyTool {
    pub fn new(channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>) -> Self {
        Self { channels }
    }
}
#[async_trait]
impl Tool for ChannelReplyTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let channel_name = args
            .get("channel")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing channel"))?;
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing text"))?;

        // Extract optional reaction emoji
        let reaction = args.get("reaction").and_then(|v| v.as_str());

        // Build metadata from args (pass through chat_id etc.)
        let mut metadata = serde_json::Map::new();
        if let Some(chat_id) = args.get("chat_id") {
            metadata.insert("chat_id".to_string(), chat_id.clone());
        }
        if let Some(message_id) = args.get("message_id") {
            metadata.insert("message_id".to_string(), message_id.clone());
        }

        let channels = self.channels.read().await;
        let channel = channels
            .get(channel_name)
            .ok_or_else(|| anyhow::anyhow!("channel not found: {channel_name}"))?;

        let outgoing = sigil_core::traits::OutgoingMessage {
            channel: channel_name.to_string(),
            recipient: String::new(),
            text: text.to_string(),
            metadata: serde_json::Value::Object(metadata),
        };

        channel.send(outgoing).await?;

        // Add reaction if specified
        if let Some(emoji) = reaction {
            let chat_id = args
                .get("chat_id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("missing chat_id for reaction"))?;
            let message_id = args
                .get("message_id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("missing message_id for reaction"))?;

            channel.react(chat_id, message_id, emoji).await?;
        }

        Ok(ToolResult::success(format!(
            "Reply sent via {channel_name}"
        )))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "channel_reply".to_string(),
            description: "Send a reply through a messaging channel (Telegram, Discord, etc.)"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": { "type": "string", "description": "Channel name (telegram, discord, slack)" },
                    "chat_id": { "type": "integer", "description": "Chat ID to reply to" },
                    "text": { "type": "string", "description": "Message text to send" }
                },
                "required": ["channel", "chat_id", "text"]
            }),
        }
    }

    fn name(&self) -> &str {
        "channel_reply"
    }
}

/// Tool that surfaces OpenRouter key usage and per-project worker execution
/// costs aggregated from `~/.sigil/usage.jsonl`.
pub struct UsageStatsTool {
    api_key: Option<String>,
}

impl UsageStatsTool {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Tool for UsageStatsTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let mut output = String::new();

        output.push_str("**OpenRouter API Key**\n");
        match &self.api_key {
            Some(key) => match collect_openrouter_usage(key).await {
                Ok(s) => output.push_str(&s),
                Err(e) => output.push_str(&format!("  Error fetching key info: {e}\n")),
            },
            None => output.push_str("  (API key not configured)\n"),
        }
        output.push('\n');

        output.push_str("**Worker Executions (all time)**\n");
        match collect_worker_usage().await {
            Ok(s) => output.push_str(&s),
            Err(_) => output.push_str("  (no executions logged yet)\n"),
        }

        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "usage_stats".to_string(),
            description:
                "Get OpenRouter API key credit usage and per-project worker execution costs."
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str {
        "usage_stats"
    }
}

/// Query OpenRouter /api/v1/auth/key and return a formatted credit summary.
pub async fn collect_openrouter_usage(api_key: &str) -> Result<String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://openrouter.ai/api/v1/auth/key")
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .context("request failed")?;

    let v: serde_json::Value = resp.json().await.context("failed to parse response")?;
    let data = v.get("data").context("no data field in response")?;

    let usage = data.get("usage").and_then(|u| u.as_f64()).unwrap_or(0.0);
    let limit = data.get("limit").and_then(|l| l.as_f64());
    let limit_str = match limit {
        Some(l) => format!("${l:.2}"),
        None => "unlimited".to_string(),
    };

    let mut out = format!("  Spent: ${usage:.4} / {limit_str}\n");

    if let Some(rl) = data.get("rate_limit") {
        let requests = rl.get("requests").and_then(|r| r.as_u64()).unwrap_or(0);
        let interval = rl.get("interval").and_then(|i| i.as_str()).unwrap_or("?");
        out.push_str(&format!("  Rate limit: {requests} req/{interval}\n"));
    }

    Ok(out)
}

/// Read ~/.sigil/usage.jsonl and return a per-project cost summary.
pub async fn collect_worker_usage() -> Result<String> {
    let path = usage_log_path();

    let content = tokio::fs::read_to_string(&path)
        .await
        .context("no usage log yet")?;

    let mut project_totals: HashMap<String, (f64, usize)> = HashMap::new();
    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
            let project = entry
                .get("project")
                .or_else(|| entry.get("rig"))
                .and_then(|r| r.as_str())
                .unwrap_or("unknown")
                .to_string();
            let cost = entry
                .get("cost_usd")
                .and_then(|c| c.as_f64())
                .unwrap_or(0.0);
            let e = project_totals.entry(project).or_insert((0.0, 0));
            e.0 += cost;
            e.1 += 1;
        }
    }

    if project_totals.is_empty() {
        return Ok("  (no executions logged yet)\n".to_string());
    }

    let mut projects: Vec<_> = project_totals.iter().collect();
    projects.sort_by(|a, b| {
        b.1.0
            .partial_cmp(&a.1.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = String::new();
    let total_cost: f64 = projects.iter().map(|(_, (c, _))| c).sum();
    for (project, (cost, count)) in &projects {
        out.push_str(&format!("  {project}: ${cost:.4} ({count} runs)\n"));
    }
    out.push_str(&format!("  Total: ${total_cost:.4}\n"));

    Ok(out)
}

pub fn usage_log_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".sigil")
        .join("usage.jsonl")
}

pub struct MemoryStoreTool {
    memory: Arc<dyn Memory>,
}

impl MemoryStoreTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let key = args
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing key"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing content"))?;
        let scope = match args.get("scope").and_then(|v| v.as_str()) {
            Some("system") => MemoryScope::System,
            Some("entity") | Some("companion") => MemoryScope::Entity,
            _ => MemoryScope::Domain,
        };
        let category = match args.get("category").and_then(|v| v.as_str()) {
            Some("procedure") => MemoryCategory::Procedure,
            Some("preference") => MemoryCategory::Preference,
            Some("context") => MemoryCategory::Context,
            Some("evergreen") => MemoryCategory::Evergreen,
            _ => MemoryCategory::Fact,
        };
        let entity_id = args
            .get("entity_id")
            .and_then(|v| v.as_str())
            .or_else(|| args.get("companion_id").and_then(|v| v.as_str()));

        match self
            .memory
            .store(key, content, category, scope, entity_id)
            .await
        {
            Ok(id) => Ok(ToolResult::success(format!(
                "Stored memory {id} [{scope}] {key}"
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to store: {e}"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "memory_store".to_string(),
            description: "Store a memory with semantic embeddings for later recall. Use for facts, preferences, patterns, and context worth remembering.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Short label for the memory (e.g. 'jwt-auth-preference')" },
                    "content": { "type": "string", "description": "The memory content to store" },
                    "scope": { "type": "string", "enum": ["domain", "system", "entity"], "description": "Memory scope (default: domain)" },
                    "category": { "type": "string", "enum": ["fact", "procedure", "preference", "context", "evergreen"], "description": "Memory category (default: fact)" },
                    "entity_id": { "type": "string", "description": "Entity ID for entity-scoped memories" }
                },
                "required": ["key", "content"]
            }),
        }
    }

    fn name(&self) -> &str {
        "memory_store"
    }
}

pub struct MemoryRecallTool {
    memory: Arc<dyn Memory>,
}

impl MemoryRecallTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryRecallTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let query_text = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing query"))?;
        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

        let mut query = MemoryQuery::new(query_text, top_k);

        if let Some(scope) = args.get("scope").and_then(|v| v.as_str()) {
            query.scope = Some(match scope {
                "system" => MemoryScope::System,
                "entity" | "companion" => MemoryScope::Entity,
                _ => MemoryScope::Domain,
            });
        }
        if let Some(eid) = args
            .get("entity_id")
            .and_then(|v| v.as_str())
            .or_else(|| args.get("companion_id").and_then(|v| v.as_str()))
        {
            query = query.with_entity(eid);
        }

        match self.memory.search(&query).await {
            Ok(results) if results.is_empty() => Ok(ToolResult::success(format!(
                "No memories found for: {query_text}"
            ))),
            Ok(results) => {
                let mut output = String::new();
                for (i, entry) in results.iter().enumerate() {
                    let age = chrono::Utc::now() - entry.created_at;
                    let age_str = if age.num_days() > 0 {
                        format!("{}d ago", age.num_days())
                    } else if age.num_hours() > 0 {
                        format!("{}h ago", age.num_hours())
                    } else {
                        format!("{}m ago", age.num_minutes())
                    };
                    output.push_str(&format!(
                        "{}. [{}] ({:.2}) {} — {}\n",
                        i + 1,
                        age_str,
                        entry.score,
                        entry.key,
                        entry.content,
                    ));
                }
                Ok(ToolResult::success(output))
            }
            Err(e) => Ok(ToolResult::error(format!("Search failed: {e}"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "memory_recall".to_string(),
            description: "Search memories using semantic similarity + keyword matching. Returns the most relevant memories ranked by hybrid score.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language search query" },
                    "top_k": { "type": "integer", "description": "Max results to return (default: 5)" },
                    "scope": { "type": "string", "enum": ["domain", "system", "entity"], "description": "Filter by scope" },
                    "entity_id": { "type": "string", "description": "Filter to specific entity's memories" }
                },
                "required": ["query"]
            }),
        }
    }

    fn name(&self) -> &str {
        "memory_recall"
    }
}

/// Tool for reading full task details by ID.
pub struct QuestDetailTool {
    registry: Arc<ProjectRegistry>,
}

impl QuestDetailTool {
    pub fn new(registry: Arc<ProjectRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for QuestDetailTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;

        let projects = self.registry.project_names().await;
        for project_name in &projects {
            if let Some(project) = self.registry.get_project(project_name).await {
                let store = project.tasks.lock().await;
                if let Some(task) = store.get(task_id) {
                    let mut out = format!(
                        "Task: {} ({})\nStatus: {:?}\nPriority: {}\nSubject: {}\n",
                        task.id, project_name, task.status, task.priority, task.subject,
                    );
                    if !task.description.is_empty() {
                        out.push_str(&format!("Description: {}\n", task.description));
                    }
                    if let Some(ref assignee) = task.assignee {
                        out.push_str(&format!("Assignee: {}\n", assignee));
                    }
                    if let Some(outcome) = task.task_outcome() {
                        out.push_str(&format!("Outcome: {}\n", outcome.kind));
                        out.push_str(&format!("Outcome summary: {}\n", outcome.summary));
                        if let Some(reason) = outcome.reason {
                            out.push_str(&format!("Outcome reason: {}\n", reason));
                        }
                    }
                    if let Some(ref reason) = task.closed_reason {
                        out.push_str(&format!("Closed reason: {}\n", reason));
                    }
                    if task.retry_count > 0 {
                        out.push_str(&format!("Retries: {}\n", task.retry_count));
                    }
                    if !task.checkpoints.is_empty() {
                        out.push_str(&format!("Checkpoints: {}\n", task.checkpoints.len()));
                    }
                    return Ok(ToolResult::success(out));
                }
            }
        }

        Ok(ToolResult::error(format!("Task not found: {task_id}")))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_detail".to_string(),
            description: "Read full details of a task by its ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID (e.g. 'as-001')" }
                },
                "required": ["task_id"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_detail"
    }
}

/// Tool for cancelling a task by ID.
pub struct QuestCancelTool {
    registry: Arc<ProjectRegistry>,
}

impl QuestCancelTool {
    pub fn new(registry: Arc<ProjectRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for QuestCancelTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;
        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Cancelled by leader agent");

        let projects = self.registry.project_names().await;
        for project_name in &projects {
            if let Some(project) = self.registry.get_project(project_name).await {
                let mut store = project.tasks.lock().await;
                if store.get(task_id).is_some() {
                    match store.update(task_id, |q| {
                        q.status = sigil_tasks::TaskStatus::Cancelled;
                        q.assignee = None;
                        q.closed_reason = Some(reason.to_string());
                        q.set_task_outcome(&sigil_tasks::TaskOutcomeRecord::new(
                            sigil_tasks::TaskOutcomeKind::Cancelled,
                            reason,
                        ));
                    }) {
                        Ok(_) => {
                            return Ok(ToolResult::success(format!("Task {task_id} cancelled.")));
                        }
                        Err(e) => return Ok(ToolResult::error(format!("Failed to cancel: {e}"))),
                    }
                }
            }
        }

        Ok(ToolResult::error(format!("Task not found: {task_id}")))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_cancel".to_string(),
            description: "Cancel a task by its ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID to cancel" },
                    "reason": { "type": "string", "description": "Reason for cancellation" }
                },
                "required": ["task_id"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_cancel"
    }
}

/// Tool for reprioritizing a task.
pub struct QuestReprioritizeTool {
    registry: Arc<ProjectRegistry>,
}

impl QuestReprioritizeTool {
    pub fn new(registry: Arc<ProjectRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for QuestReprioritizeTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;
        let priority_str = args
            .get("priority")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing priority"))?;

        let priority = match priority_str.to_lowercase().as_str() {
            "low" => sigil_tasks::Priority::Low,
            "normal" => sigil_tasks::Priority::Normal,
            "high" => sigil_tasks::Priority::High,
            "critical" => sigil_tasks::Priority::Critical,
            _ => {
                return Ok(ToolResult::error(format!(
                    "Invalid priority: {priority_str}. Use: low, normal, high, critical"
                )));
            }
        };

        let projects = self.registry.project_names().await;
        for project_name in &projects {
            if let Some(project) = self.registry.get_project(project_name).await {
                let mut store = project.tasks.lock().await;
                if store.get(task_id).is_some() {
                    match store.update(task_id, |q| {
                        q.priority = priority;
                    }) {
                        Ok(_) => {
                            return Ok(ToolResult::success(format!(
                                "Task {task_id} reprioritized to {priority}."
                            )));
                        }
                        Err(e) => {
                            return Ok(ToolResult::error(format!("Failed to reprioritize: {e}")));
                        }
                    }
                }
            }
        }

        Ok(ToolResult::error(format!("Task not found: {task_id}")))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_reprioritize".to_string(),
            description: "Change the priority of a task.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID to reprioritize" },
                    "priority": { "type": "string", "enum": ["low", "normal", "high", "critical"], "description": "New priority level" }
                },
                "required": ["task_id", "priority"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_reprioritize"
    }
}

/// Tool for posting/querying the inter-agent blackboard.
pub struct BlackboardTool {
    blackboard: Arc<crate::blackboard::Blackboard>,
    agent_name: String,
    project_name: String,
}

impl BlackboardTool {
    pub fn new(
        blackboard: Arc<crate::blackboard::Blackboard>,
        agent_name: String,
        project_name: String,
    ) -> Self {
        Self {
            blackboard,
            agent_name,
            project_name,
        }
    }
}

#[async_trait]
impl Tool for BlackboardTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("query");

        match action {
            "post" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing key"))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing content"))?;
                let tags: Vec<String> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let durability = match args.get("durability").and_then(|v| v.as_str()) {
                    Some("durable") => crate::blackboard::EntryDurability::Durable,
                    _ => crate::blackboard::EntryDurability::Transient,
                };

                match self.blackboard.post(
                    key,
                    content,
                    &self.agent_name,
                    &self.project_name,
                    &tags,
                    durability,
                ) {
                    Ok(entry) => Ok(ToolResult::success(format!(
                        "Posted to blackboard: {} (expires {})",
                        entry.key,
                        entry.expires_at.format("%Y-%m-%d %H:%M"),
                    ))),
                    Err(e) => Ok(ToolResult::error(format!("Failed to post: {e}"))),
                }
            }
            "query" => {
                let tags: Vec<String> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as u32;

                match self.blackboard.query(&self.project_name, &tags, limit) {
                    Ok(entries) if entries.is_empty() => {
                        Ok(ToolResult::success("No matching blackboard entries."))
                    }
                    Ok(entries) => {
                        let mut out = String::new();
                        for e in &entries {
                            out.push_str(&format!(
                                "{}: {} (by {}, tags: {})\n",
                                e.key,
                                e.content,
                                e.agent,
                                if e.tags.is_empty() {
                                    "-".to_string()
                                } else {
                                    e.tags.join(", ")
                                },
                            ));
                        }
                        Ok(ToolResult::success(out))
                    }
                    Err(e) => Ok(ToolResult::error(format!("Query failed: {e}"))),
                }
            }
            "get" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing key"))?;

                match self.blackboard.get_by_key(&self.project_name, key) {
                    Ok(Some(entry)) => Ok(ToolResult::success(format!(
                        "{}: {} (by {}, expires {})",
                        entry.key,
                        entry.content,
                        entry.agent,
                        entry.expires_at.format("%Y-%m-%d %H:%M"),
                    ))),
                    Ok(None) => Ok(ToolResult::success(format!(
                        "No entry found for key: {key}"
                    ))),
                    Err(e) => Ok(ToolResult::error(format!("Get failed: {e}"))),
                }
            }
            "claim" => {
                let resource = args
                    .get("resource")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing resource"))?;
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

                match self
                    .blackboard
                    .claim(resource, &self.agent_name, &self.project_name, content)
                {
                    Ok(crate::blackboard::ClaimResult::Acquired) => {
                        Ok(ToolResult::success(format!("Claimed: {resource}")))
                    }
                    Ok(crate::blackboard::ClaimResult::Renewed) => {
                        Ok(ToolResult::success(format!("Renewed claim: {resource}")))
                    }
                    Ok(crate::blackboard::ClaimResult::Held { holder, content }) => {
                        Ok(ToolResult::success(format!(
                            "BLOCKED — {resource} is claimed by {holder}: {content}"
                        )))
                    }
                    Err(e) => Ok(ToolResult::error(format!("Claim failed: {e}"))),
                }
            }
            "release" => {
                let resource = args
                    .get("resource")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing resource"))?;
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

                match self
                    .blackboard
                    .release(resource, &self.agent_name, &self.project_name, force)
                {
                    Ok(true) => Ok(ToolResult::success(format!("Released: {resource}"))),
                    Ok(false) => Ok(ToolResult::success(format!(
                        "No active claim found for: {resource}"
                    ))),
                    Err(e) => Ok(ToolResult::error(format!("Release failed: {e}"))),
                }
            }
            "delete" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing key"))?;

                match self.blackboard.delete_by_key(&self.project_name, key) {
                    Ok(true) => Ok(ToolResult::success(format!("Deleted: {key}"))),
                    Ok(false) => Ok(ToolResult::success(format!("No entry found for: {key}"))),
                    Err(e) => Ok(ToolResult::error(format!("Delete failed: {e}"))),
                }
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown action: {action}. Use: post, query, get, claim, release, delete"
            ))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "blackboard".to_string(),
            description: "Shared coordination surface. Post discoveries, claim resources, signal state, query entries. Key prefixes: claim: (locks), signal: (broadcasts), finding: (results), decision: (choices).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["post", "query", "get", "claim", "release", "delete"], "description": "Action to perform (default: query)" },
                    "key": { "type": "string", "description": "Key for post/get/delete" },
                    "resource": { "type": "string", "description": "Resource path for claim/release (e.g. src/api/auth.rs)" },
                    "content": { "type": "string", "description": "Content to post or claim description" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for filtering/categorization" },
                    "durability": { "type": "string", "enum": ["transient", "durable"], "description": "How long the entry persists (default: transient)" },
                    "limit": { "type": "integer", "description": "Max results for query (default: 10)" },
                    "force": { "type": "boolean", "description": "Force release even if claimed by another agent" }
                }
            }),
        }
    }

    fn name(&self) -> &str {
        "blackboard"
    }
}

/// Build orchestration tools for the leader agent.
///
/// NOTE: `channel_reply` is intentionally excluded. The leader agent's final text output
/// is automatically delivered to the originating channel by the daemon's polling loop.
/// Including `channel_reply` causes double-delivery: the tool sends once, and the
/// task's closed_reason (the LLM's confirmation text) gets sent again.
pub fn build_orchestration_tools(
    registry: Arc<ProjectRegistry>,
    dispatch_bus: Arc<DispatchBus>,
    _channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    api_key: Option<String>,
    memory: Option<Arc<dyn Memory>>,
    blackboard: Option<Arc<crate::blackboard::Blackboard>>,
) -> Vec<Arc<dyn Tool>> {
    let leader_name = registry.leader_agent_name.clone();
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ProjectStatusTool::new(registry.clone())),
        Arc::new(ProjectAssignTool::new(registry.clone())),
        Arc::new(ProjectListTool::new(registry.clone())),
        Arc::new(QuestDetailTool::new(registry.clone())),
        Arc::new(QuestCancelTool::new(registry.clone())),
        Arc::new(QuestReprioritizeTool::new(registry.clone())),
        Arc::new(MailReadTool::new(dispatch_bus.clone())),
        Arc::new(MailSendTool::new(dispatch_bus)),
        Arc::new(AllReadyTool::new(registry)),
        Arc::new(UsageStatsTool::new(api_key)),
    ];

    if let Some(mem) = memory {
        tools.push(Arc::new(MemoryStoreTool::new(mem.clone())));
        tools.push(Arc::new(MemoryRecallTool::new(mem)));
    }

    if let Some(bb) = blackboard {
        tools.push(Arc::new(BlackboardTool::new(
            bb,
            leader_name,
            "*".to_string(),
        )));
    }

    tools
}

// ---------------------------------------------------------------------------
// TriggerManageTool — CRUD for agent-owned triggers
// ---------------------------------------------------------------------------

/// Tool for creating, listing, enabling, disabling, and deleting triggers.
/// Scoped to the calling agent's own triggers.
pub struct TriggerManageTool {
    trigger_store: Arc<crate::trigger::TriggerStore>,
    agent_id: String,
}

impl TriggerManageTool {
    pub fn new(trigger_store: Arc<crate::trigger::TriggerStore>, agent_id: String) -> Self {
        Self {
            trigger_store,
            agent_id,
        }
    }
}

#[async_trait]
impl Tool for TriggerManageTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match action {
            "create" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
                let skill = args
                    .get("skill")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'skill' is required"))?;
                let max_budget_usd = args.get("max_budget_usd").and_then(|v| v.as_f64());

                // Determine trigger type from args.
                let trigger_type = if let Some(schedule) =
                    args.get("schedule").and_then(|v| v.as_str())
                {
                    crate::trigger::TriggerType::Schedule {
                        expr: schedule.to_string(),
                    }
                } else if let Some(event) = args.get("event_pattern").and_then(|v| v.as_str()) {
                    let cooldown = args
                        .get("cooldown_secs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(300);
                    if cooldown < 60 {
                        return Ok(ToolResult {
                            output: "cooldown_secs must be >= 60".to_string(),
                            is_error: true,
                        });
                    }
                    let pattern = match event {
                        "task_completed" => crate::trigger::EventPattern::TaskCompleted {
                            project: args
                                .get("project_filter")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                        },
                        "task_failed" => crate::trigger::EventPattern::TaskFailed {
                            project: args
                                .get("project_filter")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                        },
                        "tool_call_completed" => crate::trigger::EventPattern::ToolCallCompleted {
                            tool: args
                                .get("tool_filter")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                        },
                        other => {
                            return Ok(ToolResult {
                                output: format!("unknown event pattern: {other}"),
                                is_error: true,
                            });
                        }
                    };
                    crate::trigger::TriggerType::Event {
                        pattern,
                        cooldown_secs: cooldown,
                    }
                } else {
                    return Ok(ToolResult {
                        output: "provide 'schedule' or 'event_pattern'".to_string(),
                        is_error: true,
                    });
                };

                match self
                    .trigger_store
                    .create(&crate::trigger::NewTrigger {
                        agent_id: self.agent_id.clone(),
                        name: name.to_string(),
                        trigger_type,
                        skill: skill.to_string(),
                        max_budget_usd,
                    })
                    .await
                {
                    Ok(trigger) => Ok(ToolResult {
                        output: format!(
                            "Trigger '{}' created (id: {}, skill: {}, type: {})",
                            trigger.name,
                            trigger.id,
                            trigger.skill,
                            trigger.trigger_type.type_str()
                        ),
                        is_error: false,
                    }),
                    Err(e) => Ok(ToolResult {
                        output: format!("Failed to create trigger: {e}"),
                        is_error: true,
                    }),
                }
            }

            "list" => {
                let triggers = self
                    .trigger_store
                    .list_for_agent(&self.agent_id)
                    .await
                    .unwrap_or_default();
                let items: Vec<String> = triggers
                    .iter()
                    .map(|t| {
                        format!(
                            "- {} (id: {}, type: {}, skill: {}, enabled: {}, fires: {})",
                            t.name,
                            t.id,
                            t.trigger_type.type_str(),
                            t.skill,
                            t.enabled,
                            t.fire_count
                        )
                    })
                    .collect();
                Ok(ToolResult {
                    output: if items.is_empty() {
                        "No triggers.".to_string()
                    } else {
                        items.join("\n")
                    },
                    is_error: false,
                })
            }

            "enable" | "disable" => {
                let id = args
                    .get("trigger_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'trigger_id' is required"))?;
                let enabled = action == "enable";
                match self.trigger_store.update_enabled(id, enabled).await {
                    Ok(()) => Ok(ToolResult {
                        output: format!(
                            "Trigger {id} {}.",
                            if enabled { "enabled" } else { "disabled" }
                        ),
                        is_error: false,
                    }),
                    Err(e) => Ok(ToolResult {
                        output: format!("Failed: {e}"),
                        is_error: true,
                    }),
                }
            }

            "delete" => {
                let id = args
                    .get("trigger_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'trigger_id' is required"))?;
                match self.trigger_store.delete(id).await {
                    Ok(()) => Ok(ToolResult {
                        output: format!("Trigger {id} deleted."),
                        is_error: false,
                    }),
                    Err(e) => Ok(ToolResult {
                        output: format!("Failed: {e}"),
                        is_error: true,
                    }),
                }
            }

            other => Ok(ToolResult {
                output: format!(
                    "Unknown action: {other}. Use: create, list, enable, disable, delete"
                ),
                is_error: true,
            }),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "manage_triggers".to_string(),
            description: "Create, list, enable, disable, or delete triggers for this agent. Triggers automate recurring tasks on a schedule or in response to events.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["create", "list", "enable", "disable", "delete"],
                        "description": "Action to perform"
                    },
                    "name": {
                        "type": "string",
                        "description": "Trigger name (for create)"
                    },
                    "schedule": {
                        "type": "string",
                        "description": "Cron expression or interval (e.g., '0 9 * * *' or 'every 1h')"
                    },
                    "event_pattern": {
                        "type": "string",
                        "enum": ["task_completed", "task_failed", "tool_call_completed"],
                        "description": "Event to react to"
                    },
                    "cooldown_secs": {
                        "type": "integer",
                        "description": "Minimum seconds between event trigger fires (>= 60)"
                    },
                    "skill": {
                        "type": "string",
                        "description": "Skill to run when triggered"
                    },
                    "max_budget_usd": {
                        "type": "number",
                        "description": "Maximum budget per execution in USD"
                    },
                    "trigger_id": {
                        "type": "string",
                        "description": "Trigger ID (for enable/disable/delete)"
                    },
                    "project_filter": {
                        "type": "string",
                        "description": "Filter events by project (optional)"
                    },
                    "tool_filter": {
                        "type": "string",
                        "description": "Filter tool_call_completed events by tool name (optional)"
                    }
                },
                "required": ["action"]
            }),
        }
    }

    fn name(&self) -> &str {
        "manage_triggers"
    }

    fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// ChannelPostTool — post messages to conversation channels
// ---------------------------------------------------------------------------

/// Tool for agents to post messages to department/project conversation channels.
pub struct ChannelPostTool {
    conversation_store: Arc<crate::ConversationStore>,
    event_broadcaster: Arc<crate::EventBroadcaster>,
    agent_name: String,
}

impl ChannelPostTool {
    pub fn new(
        conversation_store: Arc<crate::ConversationStore>,
        event_broadcaster: Arc<crate::EventBroadcaster>,
        agent_name: String,
    ) -> Self {
        Self {
            conversation_store,
            event_broadcaster,
            agent_name,
        }
    }
}

#[async_trait]
impl Tool for ChannelPostTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let channel = args
            .get("channel")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'channel' is required"))?;
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'message' is required"))?;

        // Resolve channel to chat_id.
        let (chat_id, channel_name) = if let Some(rest) = channel.strip_prefix("dept:") {
            // Format: "dept:project:department" or "dept:department" (project from context)
            let parts: Vec<&str> = rest.splitn(2, ':').collect();
            if parts.len() == 2 {
                let cid = crate::conversation_store::department_chat_id(parts[0], parts[1]);
                (cid, format!("{}/{}", parts[0], parts[1]))
            } else {
                let cid = crate::conversation_store::named_channel_chat_id(channel);
                (cid, channel.to_string())
            }
        } else if let Some(project) = channel.strip_prefix("project:") {
            let cid = crate::conversation_store::project_chat_id(project);
            (cid, project.to_string())
        } else {
            let cid = crate::conversation_store::named_channel_chat_id(channel);
            (cid, channel.to_string())
        };

        // Ensure channel exists in conversation store.
        let _ = self
            .conversation_store
            .ensure_channel(chat_id, "agent", &channel_name)
            .await;

        // Record the message.
        let _ = self
            .conversation_store
            .record_with_source(chat_id, &self.agent_name, message, Some("agent"))
            .await;

        // Emit event for trigger system.
        let preview: String = message.chars().take(100).collect();
        self.event_broadcaster
            .publish(crate::execution_events::ExecutionEvent::ChannelMessage {
                channel_name: channel_name.clone(),
                chat_id,
                from_agent: self.agent_name.clone(),
                content_preview: preview,
            });

        Ok(ToolResult {
            output: format!("Posted to channel '{channel_name}' (chat_id: {chat_id})"),
            is_error: false,
        })
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "channel_post".to_string(),
            description: "Post a message to a conversation channel. Use 'dept:project:name' for department channels, 'project:name' for project channels, or a plain channel name.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel target: 'dept:project:department', 'project:name', or channel name"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message content to post"
                    }
                },
                "required": ["channel", "message"]
            }),
        }
    }

    fn name(&self) -> &str {
        "channel_post"
    }

    fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// TranscriptSearchTool — FTS search across past session transcripts
// ---------------------------------------------------------------------------

/// Tool for agents to search past session transcripts via FTS5.
pub struct TranscriptSearchTool {
    conversation_store: Arc<crate::ConversationStore>,
}

impl TranscriptSearchTool {
    pub fn new(conversation_store: Arc<crate::ConversationStore>) -> Self {
        Self { conversation_store }
    }
}

#[async_trait]
impl Tool for TranscriptSearchTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'query' is required"))?;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        match self
            .conversation_store
            .search_transcripts(query, limit)
            .await
        {
            Ok(messages) => {
                if messages.is_empty() {
                    return Ok(ToolResult {
                        output: "No transcript matches found.".to_string(),
                        is_error: false,
                    });
                }
                let results: Vec<String> = messages
                    .iter()
                    .map(|m| {
                        let preview: String = m.content.chars().take(200).collect();
                        format!(
                            "[{}] {}: {}",
                            m.timestamp.format("%Y-%m-%d %H:%M"),
                            m.role,
                            preview
                        )
                    })
                    .collect();
                Ok(ToolResult {
                    output: format!("{} matches:\n{}", results.len(), results.join("\n\n")),
                    is_error: false,
                })
            }
            Err(e) => Ok(ToolResult {
                output: format!("Transcript search failed: {e}"),
                is_error: true,
            }),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "transcript_search".to_string(),
            description: "Search past session transcripts. Returns matching messages from previous agent sessions. Use when you need to recall HOW you solved something.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (FTS5 syntax: words, phrases in quotes, OR/AND/NOT)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results (default 10)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    fn name(&self) -> &str {
        "transcript_search"
    }

    fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
        true
    }
}
