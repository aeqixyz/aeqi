use aeqi_core::traits::Tool;
use aeqi_core::traits::{Channel, ToolResult, ToolSpec};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::agent_registry::AgentRegistry;
use crate::event_store::EventStore;
use aeqi_core::traits::{Insight, InsightCategory, InsightQuery};

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

pub struct InsightStoreTool {
    memory: Arc<dyn Insight>,
}

impl InsightStoreTool {
    pub fn new(memory: Arc<dyn Insight>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for InsightStoreTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let key = args
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing key"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing content"))?;
        let category = match args.get("category").and_then(|v| v.as_str()) {
            Some("procedure") => InsightCategory::Procedure,
            Some("preference") => InsightCategory::Preference,
            Some("context") => InsightCategory::Context,
            Some("evergreen") => InsightCategory::Evergreen,
            _ => InsightCategory::Fact,
        };
        let agent_id = args.get("agent_id").and_then(|v| v.as_str());

        match self.memory.store(key, content, category, agent_id).await {
            Ok(id) => Ok(ToolResult::success(format!("Stored memory {id} {key}"))),
            Err(e) => Ok(ToolResult::error(format!("Failed to store: {e}"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "aeqi_remember".to_string(),
            description: "Store a memory with semantic embeddings for later recall. Use for facts, preferences, patterns, and context worth remembering.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Short label for the memory (e.g. 'jwt-auth-preference')" },
                    "content": { "type": "string", "description": "The memory content to store" },
                    "category": { "type": "string", "enum": ["fact", "procedure", "preference", "context", "evergreen"], "description": "Memory category (default: fact)" },
                    "agent_id": { "type": "string", "description": "Agent ID to associate with this memory" }
                },
                "required": ["key", "content"]
            }),
        }
    }

    fn name(&self) -> &str {
        "aeqi_remember"
    }
}

pub struct InsightRecallTool {
    memory: Arc<dyn Insight>,
}

impl InsightRecallTool {
    pub fn new(memory: Arc<dyn Insight>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for InsightRecallTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let query_text = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing query"))?;
        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

        let mut query = InsightQuery::new(query_text, top_k);

        if let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()) {
            query = query.with_agent(agent_id);
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
            name: "aeqi_recall".to_string(),
            description: "Search memories using semantic similarity + keyword matching. Returns the most relevant memories ranked by hybrid score.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language search query" },
                    "top_k": { "type": "integer", "description": "Max results to return (default: 5)" },
                    "agent_id": { "type": "string", "description": "Filter to a specific agent's memories" }
                },
                "required": ["query"]
            }),
        }
    }

    fn name(&self) -> &str {
        "aeqi_recall"
    }
}

/// Format an `aeqi_quests::Quest` into a human-readable detail string.
fn format_task_detail(task: &aeqi_quests::Quest) -> String {
    let mut out = format!(
        "Task: {} \nStatus: {:?}\nPriority: {}\nSubject: {}\n",
        task.id, task.status, task.priority, task.name,
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
    out
}

/// Tool for reading full task details by ID.
pub struct QuestDetailTool {
    agent_registry: Arc<AgentRegistry>,
}

impl QuestDetailTool {
    pub fn new(agent_registry: Arc<AgentRegistry>) -> Self {
        Self { agent_registry }
    }
}

#[async_trait]
impl Tool for QuestDetailTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;

        match self.agent_registry.get_task(task_id).await {
            Ok(Some(task)) => Ok(ToolResult::success(format_task_detail(&task))),
            Ok(None) => Ok(ToolResult::error(format!("Task not found: {task_id}"))),
            Err(e) => Ok(ToolResult::error(format!("Failed to get task: {e}"))),
        }
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
    agent_registry: Arc<AgentRegistry>,
}

impl QuestCancelTool {
    pub fn new(agent_registry: Arc<AgentRegistry>) -> Self {
        Self { agent_registry }
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

        let reason_owned = reason.to_string();
        match self
            .agent_registry
            .update_task(task_id, |q| {
                q.status = aeqi_quests::QuestStatus::Cancelled;
                q.assignee = None;
                q.closed_reason = Some(reason_owned.clone());
                q.set_task_outcome(&aeqi_quests::QuestOutcomeRecord::new(
                    aeqi_quests::QuestOutcomeKind::Cancelled,
                    &reason_owned,
                ));
            })
            .await
        {
            Ok(_) => Ok(ToolResult::success(format!("Task {task_id} cancelled."))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to cancel task {task_id}: {e}"
            ))),
        }
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
    agent_registry: Arc<AgentRegistry>,
}

impl QuestReprioritizeTool {
    pub fn new(agent_registry: Arc<AgentRegistry>) -> Self {
        Self { agent_registry }
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
            "low" => aeqi_quests::Priority::Low,
            "normal" => aeqi_quests::Priority::Normal,
            "high" => aeqi_quests::Priority::High,
            "critical" => aeqi_quests::Priority::Critical,
            _ => {
                return Ok(ToolResult::error(format!(
                    "Invalid priority: {priority_str}. Use: low, normal, high, critical"
                )));
            }
        };

        match self
            .agent_registry
            .update_task(task_id, |q| {
                q.priority = priority;
            })
            .await
        {
            Ok(_) => Ok(ToolResult::success(format!(
                "Task {task_id} reprioritized to {priority}."
            ))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to reprioritize task {task_id}: {e}"
            ))),
        }
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

/// Tool for posting/querying shared insights and claiming resources via quests.
///
/// post/query/get/delete operate on the insight store.
/// claim/release operate on quests via agent_registry.
pub struct NotesTool {
    insight_store: Arc<dyn Insight>,
    agent_registry: Arc<crate::agent_registry::AgentRegistry>,
    agent_name: String,
}

impl NotesTool {
    pub fn new(
        insight_store: Arc<dyn Insight>,
        agent_registry: Arc<crate::agent_registry::AgentRegistry>,
        agent_name: String,
    ) -> Self {
        Self {
            insight_store,
            agent_registry,
            agent_name,
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

                match self
                    .insight_store
                    .store(key, content, aeqi_core::traits::InsightCategory::Fact, None)
                    .await
                {
                    Ok(id) => Ok(ToolResult::success(format!(
                        "Stored insight: {key} (id: {id})"
                    ))),
                    Err(e) => Ok(ToolResult::error(format!("Failed to store: {e}"))),
                }
            }
            "query" => {
                let query_text = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .filter(|s| !s.is_empty())
                    .or_else(|| args.get("key").and_then(|v| v.as_str()).map(String::from))
                    .unwrap_or_else(|| "*".to_string());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                let q = aeqi_core::traits::InsightQuery::new(&query_text, limit);
                match self.insight_store.search(&q).await {
                    Ok(entries) if entries.is_empty() => {
                        Ok(ToolResult::success("No matching entries."))
                    }
                    Ok(entries) => {
                        let mut out = String::new();
                        for e in &entries {
                            out.push_str(&format!(
                                "{}: {} (by {})\n",
                                e.key,
                                e.content,
                                e.agent_id.as_deref().unwrap_or("system"),
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

                let q = aeqi_core::traits::InsightQuery::new(key, 5);
                match self.insight_store.search(&q).await {
                    Ok(entries) => {
                        if let Some(e) = entries.into_iter().find(|e| e.key == key) {
                            Ok(ToolResult::success(format!(
                                "{}: {} (by {})",
                                e.key,
                                e.content,
                                e.agent_id.as_deref().unwrap_or("system"),
                            )))
                        } else {
                            Ok(ToolResult::success(format!(
                                "No entry found for key: {key}"
                            )))
                        }
                    }
                    Err(e) => Ok(ToolResult::error(format!("Get failed: {e}"))),
                }
            }
            "claim" => {
                let resource = args
                    .get("resource")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing resource"))?;
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let claim_label = format!("claim:{resource}");

                // Check for existing in-progress claim quest.
                let existing = self
                    .agent_registry
                    .list_tasks(Some("in_progress"), None)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|t| t.labels.contains(&claim_label));

                match existing {
                    Some(task) => {
                        let holder = task.assignee.as_deref().unwrap_or("unknown");
                        if holder == self.agent_name {
                            Ok(ToolResult::success(format!("Renewed claim: {resource}")))
                        } else {
                            Ok(ToolResult::success(format!(
                                "BLOCKED — {resource} is claimed by {holder}: {}",
                                task.description
                            )))
                        }
                    }
                    None => {
                        let agent_id = self
                            .agent_registry
                            .resolve_by_hint(&self.agent_name)
                            .await
                            .ok()
                            .flatten()
                            .map(|a| a.name.clone())
                            .unwrap_or_else(|| self.agent_name.clone());
                        match self
                            .agent_registry
                            .create_task(
                                &agent_id,
                                &format!("claim: {resource}"),
                                content,
                                None,
                                &[claim_label],
                            )
                            .await
                        {
                            Ok(task) => {
                                let _ = self
                                    .agent_registry
                                    .update_task_status(
                                        &task.id.0,
                                        aeqi_quests::QuestStatus::InProgress,
                                    )
                                    .await;
                                Ok(ToolResult::success(format!("Claimed: {resource}")))
                            }
                            Err(e) => Ok(ToolResult::error(format!("Claim failed: {e}"))),
                        }
                    }
                }
            }
            "release" => {
                let resource = args
                    .get("resource")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing resource"))?;
                let claim_label = format!("claim:{resource}");

                let existing = self
                    .agent_registry
                    .list_tasks(Some("in_progress"), None)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|t| t.labels.contains(&claim_label));

                match existing {
                    Some(task) => {
                        match self
                            .agent_registry
                            .update_task_status(&task.id.0, aeqi_quests::QuestStatus::Done)
                            .await
                        {
                            Ok(()) => Ok(ToolResult::success(format!("Released: {resource}"))),
                            Err(e) => Ok(ToolResult::error(format!("Release failed: {e}"))),
                        }
                    }
                    None => Ok(ToolResult::success(format!(
                        "No active claim found for: {resource}"
                    ))),
                }
            }
            "delete" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing key"))?;

                let q = aeqi_core::traits::InsightQuery::new(key, 5);
                match self.insight_store.search(&q).await {
                    Ok(entries) => {
                        let mut deleted = false;
                        for e in &entries {
                            if e.key == key {
                                let _ = self.insight_store.delete(&e.id).await;
                                deleted = true;
                            }
                        }
                        if deleted {
                            Ok(ToolResult::success(format!("Deleted: {key}")))
                        } else {
                            Ok(ToolResult::success(format!("No entry found for: {key}")))
                        }
                    }
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
            name: "aeqi_notes".to_string(),
            description: "Shared coordination surface. Post discoveries, claim resources, signal state, query entries. Actions: post (store insight), query (search), get (lookup by key), claim (exclusive resource lock via quest), release (drop claim), delete (remove entry).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["post", "query", "get", "claim", "release", "delete"], "description": "Action to perform (default: query)" },
                    "key": { "type": "string", "description": "Key for post/get/delete" },
                    "resource": { "type": "string", "description": "Resource path for claim/release (e.g. src/api/auth.rs)" },
                    "content": { "type": "string", "description": "Content to post or claim description" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for filtering/categorization" },
                    "limit": { "type": "integer", "description": "Max results for query (default: 10)" },
                    "force": { "type": "boolean", "description": "Force release even if claimed by another agent" }
                }
            }),
        }
    }

    fn name(&self) -> &str {
        "aeqi_notes"
    }
}

/// Build orchestration tools for the leader agent.
///
/// NOTE: `channel_reply` is intentionally excluded. The leader agent's final text output
/// is automatically delivered to the originating channel by the daemon's polling loop.
/// Including `channel_reply` causes double-delivery: the tool sends once, and the
/// task's closed_reason (the LLM's confirmation text) gets sent again.
pub fn build_orchestration_tools(
    leader_name: String,
    _default_project: String,
    project_name: Option<String>,
    event_store: Arc<EventStore>,
    _channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    api_key: Option<String>,
    memory: Option<Arc<dyn Insight>>,
    event_broadcaster: Option<Arc<crate::EventBroadcaster>>,
    graph_db_path: Option<PathBuf>,
    session_id: Option<String>,
    provider: Option<Arc<dyn aeqi_core::traits::Provider>>,
    session_store: Option<Arc<crate::SessionStore>>,
    session_manager: Option<Arc<crate::session_manager::SessionManager>>,
    default_model: String,
    agent_registry: Arc<crate::agent_registry::AgentRegistry>,
) -> Vec<Arc<dyn Tool>> {
    let mut delegate_tool = crate::delegate::DelegateTool::new(event_store, leader_name.clone())
        .with_project(project_name)
        .with_agent_registry(agent_registry.clone());
    if let Some(broadcaster) = event_broadcaster {
        delegate_tool = delegate_tool.with_event_broadcaster(broadcaster);
    }
    if let Some(sid) = session_id {
        delegate_tool = delegate_tool.with_session_id(sid);
    }
    if let Some(ref p) = provider {
        delegate_tool = delegate_tool.with_provider(p.clone());
    }
    if let Some(ref sm) = session_manager {
        delegate_tool = delegate_tool.with_session_manager(sm.clone());
    }
    if let Some(ref ss) = session_store {
        delegate_tool = delegate_tool.with_session_store(ss.clone());
    }
    delegate_tool = delegate_tool.with_default_model(default_model);

    let detail_tool = QuestDetailTool::new(agent_registry.clone());
    let cancel_tool = QuestCancelTool::new(agent_registry.clone());
    let reprioritize_tool = QuestReprioritizeTool::new(agent_registry.clone());

    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(detail_tool),
        Arc::new(cancel_tool),
        Arc::new(reprioritize_tool),
        Arc::new(delegate_tool),
        Arc::new(UsageStatsTool::new(api_key)),
    ];

    if let Some(mem) = memory {
        tools.push(Arc::new(InsightStoreTool::new(mem.clone())));
        tools.push(Arc::new(InsightRecallTool::new(mem.clone())));
        tools.push(Arc::new(NotesTool::new(mem, agent_registry, leader_name)));
    }

    if let Some(gp) = graph_db_path {
        tools.push(Arc::new(GraphTool::new(gp)));
    }

    tools
}

// ---------------------------------------------------------------------------
// GraphTool — code intelligence via aeqi-graph
// ---------------------------------------------------------------------------

/// Tool exposing code graph queries: search symbols, get context, analyze impact.
pub struct GraphTool {
    db_path: PathBuf,
}

impl GraphTool {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

#[async_trait]
impl Tool for GraphTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("stats");

        let store = match aeqi_graph::GraphStore::open(&self.db_path) {
            Ok(s) => s,
            Err(e) => return Ok(ToolResult::error(format!("graph DB not available: {e}"))),
        };

        let result = match action {
            "search" => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                let results = store.search_nodes(query, limit)?;
                serde_json::json!({
                    "count": results.len(),
                    "nodes": results,
                })
            }
            "context" => {
                let node_id = args.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
                let ctx = store.context(node_id)?;
                serde_json::json!({
                    "node": ctx.node,
                    "callers": ctx.callers,
                    "callees": ctx.callees,
                    "implementors": ctx.implementors,
                })
            }
            "impact" => {
                let node_id = args.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
                let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
                let entries = store.impact(&[node_id], depth)?;
                let affected: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "node": e.node.name,
                            "file": e.node.file_path,
                            "depth": e.depth,
                        })
                    })
                    .collect();
                serde_json::json!({"affected": affected})
            }
            "file" => {
                let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = store.nodes_in_file(file_path)?;
                serde_json::json!({
                    "file": file_path,
                    "count": nodes.len(),
                    "symbols": nodes,
                })
            }
            "stats" => {
                let stats = store.stats()?;
                serde_json::json!({"stats": format!("{stats:?}")})
            }
            _ => {
                return Ok(ToolResult::error(format!("unknown graph action: {action}")));
            }
        };

        Ok(ToolResult::success(serde_json::to_string_pretty(&result)?))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "aeqi_graph".to_string(),
            description: "Query the code intelligence graph. Search symbols, get 360° context (callers/callees/implementors), analyze blast radius, list symbols in a file.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["search", "context", "impact", "file", "stats"],
                        "description": "search=FTS symbol search, context=360° view, impact=blast radius, file=symbols in a file, stats=graph statistics"
                    },
                    "query": {"type": "string", "description": "Search query (for search action)"},
                    "node_id": {"type": "string", "description": "Node ID (for context/impact actions)"},
                    "file_path": {"type": "string", "description": "File path (for file action)"},
                    "depth": {"type": "integer", "description": "Impact depth (default 3)"},
                    "limit": {"type": "integer", "description": "Max results (default 10)"}
                },
                "required": ["action"]
            }),
        }
    }

    fn name(&self) -> &str {
        "aeqi_graph"
    }
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
                        "quest_completed" => crate::trigger::EventPattern::QuestCompleted {
                            project: args
                                .get("project_filter")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                        },
                        "quest_failed" => crate::trigger::EventPattern::QuestFailed {
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
                        "enum": ["quest_completed", "quest_failed", "tool_call_completed"],
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
// DelegateTool via the "dept:<name>" routing pattern.

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
