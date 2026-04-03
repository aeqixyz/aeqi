use aeqi_core::traits::Tool;
use aeqi_core::traits::{Channel, ToolResult, ToolSpec};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::message::DispatchBus;
use crate::registry::CompanyRegistry;
use aeqi_core::traits::{Memory, MemoryCategory, MemoryQuery, MemoryScope};

/// Tool that surfaces OpenRouter key usage and per-project worker execution
/// costs aggregated from `~/.aeqi/usage.jsonl`.
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

/// Read ~/.aeqi/usage.jsonl and return a per-project cost summary.
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
        .join(".aeqi")
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
    registry: Arc<CompanyRegistry>,
}

impl QuestDetailTool {
    pub fn new(registry: Arc<CompanyRegistry>) -> Self {
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

        let projects = self.registry.company_names().await;
        for project_name in &projects {
            if let Some(project) = self.registry.get_company(project_name).await {
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
    registry: Arc<CompanyRegistry>,
}

impl QuestCancelTool {
    pub fn new(registry: Arc<CompanyRegistry>) -> Self {
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

        let projects = self.registry.company_names().await;
        for project_name in &projects {
            if let Some(project) = self.registry.get_company(project_name).await {
                let mut store = project.tasks.lock().await;
                if store.get(task_id).is_some() {
                    match store.update(task_id, |q| {
                        q.status = aeqi_tasks::TaskStatus::Cancelled;
                        q.assignee = None;
                        q.closed_reason = Some(reason.to_string());
                        q.set_task_outcome(&aeqi_tasks::TaskOutcomeRecord::new(
                            aeqi_tasks::TaskOutcomeKind::Cancelled,
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
    registry: Arc<CompanyRegistry>,
}

impl QuestReprioritizeTool {
    pub fn new(registry: Arc<CompanyRegistry>) -> Self {
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
            "low" => aeqi_tasks::Priority::Low,
            "normal" => aeqi_tasks::Priority::Normal,
            "high" => aeqi_tasks::Priority::High,
            "critical" => aeqi_tasks::Priority::Critical,
            _ => {
                return Ok(ToolResult::error(format!(
                    "Invalid priority: {priority_str}. Use: low, normal, high, critical"
                )));
            }
        };

        let projects = self.registry.company_names().await;
        for project_name in &projects {
            if let Some(project) = self.registry.get_company(project_name).await {
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

/// Tool for posting/querying inter-agent notes.
pub struct NotesTool {
    notes: Arc<crate::notes::Notes>,
    agent_name: String,
    project_name: String,
}

impl NotesTool {
    pub fn new(notes: Arc<crate::notes::Notes>, agent_name: String, project_name: String) -> Self {
        Self {
            notes,
            agent_name,
            project_name,
        }
    }
}

#[async_trait]
impl Tool for NotesTool {
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
                    Some("durable") => crate::notes::EntryDurability::Durable,
                    _ => crate::notes::EntryDurability::Transient,
                };

                match self.notes.post(
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

                match self.notes.query(&self.project_name, &tags, limit) {
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

                match self.notes.get_by_key(&self.project_name, key) {
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
                    .notes
                    .claim(resource, &self.agent_name, &self.project_name, content)
                {
                    Ok(crate::notes::ClaimResult::Acquired) => {
                        Ok(ToolResult::success(format!("Claimed: {resource}")))
                    }
                    Ok(crate::notes::ClaimResult::Renewed) => {
                        Ok(ToolResult::success(format!("Renewed claim: {resource}")))
                    }
                    Ok(crate::notes::ClaimResult::Held { holder, content }) => {
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
                    .notes
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

                match self.notes.delete_by_key(&self.project_name, key) {
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
            name: "notes".to_string(),
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
        "notes"
    }
}

/// Build orchestration tools for the leader agent.
///
/// NOTE: `channel_reply` is intentionally excluded. The leader agent's final text output
/// is automatically delivered to the originating channel by the daemon's polling loop.
/// Including `channel_reply` causes double-delivery: the tool sends once, and the
/// task's closed_reason (the LLM's confirmation text) gets sent again.
pub fn build_orchestration_tools(
    registry: Arc<CompanyRegistry>,
    dispatch_bus: Arc<DispatchBus>,
    _channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    api_key: Option<String>,
    memory: Option<Arc<dyn Memory>>,
    notes: Option<Arc<crate::notes::Notes>>,
    event_broadcaster: Option<Arc<crate::EventBroadcaster>>,
) -> Vec<Arc<dyn Tool>> {
    let leader_name = registry.leader_agent_name.clone();
    let mut delegate_tool =
        crate::unified_delegate::UnifiedDelegateTool::new(dispatch_bus, leader_name.clone());
    if let Some(broadcaster) = event_broadcaster {
        delegate_tool = delegate_tool.with_event_broadcaster(broadcaster);
    }
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(QuestDetailTool::new(registry.clone())),
        Arc::new(QuestCancelTool::new(registry.clone())),
        Arc::new(QuestReprioritizeTool::new(registry)),
        Arc::new(delegate_tool),
        Arc::new(UsageStatsTool::new(api_key)),
    ];

    if let Some(mem) = memory {
        tools.push(Arc::new(MemoryStoreTool::new(mem.clone())));
        tools.push(Arc::new(MemoryRecallTool::new(mem)));
    }

    if let Some(bb) = notes {
        tools.push(Arc::new(NotesTool::new(bb, leader_name, "*".to_string())));
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
                            context_modifier: None,
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
                                context_modifier: None,
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
                        context_modifier: None,
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
                        context_modifier: None,
                    }),
                    Err(e) => Ok(ToolResult {
                        output: format!("Failed to create trigger: {e}"),
                        is_error: true,
                        context_modifier: None,
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
                    context_modifier: None,
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
                        context_modifier: None,
                    }),
                    Err(e) => Ok(ToolResult {
                        output: format!("Failed: {e}"),
                        is_error: true,
                        context_modifier: None,
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
                        context_modifier: None,
                    }),
                    Err(e) => Ok(ToolResult {
                        output: format!("Failed: {e}"),
                        is_error: true,
                        context_modifier: None,
                    }),
                }
            }

            other => Ok(ToolResult {
                output: format!(
                    "Unknown action: {other}. Use: create, list, enable, disable, delete"
                ),
                is_error: true,
                context_modifier: None,
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

// ChannelPostTool removed — department channel posting is now handled by
// UnifiedDelegateTool via the "dept:<name>" routing pattern.

// ---------------------------------------------------------------------------
// TranscriptSearchTool — FTS search across past session transcripts
// ---------------------------------------------------------------------------

/// Tool for agents to search past session transcripts via FTS5.
pub struct TranscriptSearchTool {
    session_store: Arc<crate::SessionStore>,
}

impl TranscriptSearchTool {
    pub fn new(session_store: Arc<crate::SessionStore>) -> Self {
        Self { session_store }
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

        match self.session_store.search_transcripts(query, limit).await {
            Ok(messages) => {
                if messages.is_empty() {
                    return Ok(ToolResult {
                        output: "No transcript matches found.".to_string(),
                        is_error: false,
                        context_modifier: None,
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
                    context_modifier: None,
                })
            }
            Err(e) => Ok(ToolResult {
                output: format!("Transcript search failed: {e}"),
                is_error: true,
                context_modifier: None,
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
