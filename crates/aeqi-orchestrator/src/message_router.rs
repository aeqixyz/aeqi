//! Unified Message Router — source-agnostic message processing for Telegram, web, and future channels.
//!
//! Both Telegram and web interfaces are thin clients that delegate to this router.
//! The router handles: conversation history, agent routing, task creation,
//! and completion tracking.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use anyhow::Result;

use aeqi_core::traits::{Insight, InsightQuery};

use crate::agent_registry::AgentRegistry;
use crate::agent_router::AgentRouter;
use crate::session_store::{ChannelInfo, SessionMessage, SessionStore, ThreadEvent};

const CHAT_COUNCIL_HOLD_REASON: &str = "awaiting_council";

// ── Types ──

/// Source of a chat message.
#[derive(Debug, Clone)]
pub enum MessageSource {
    Telegram { message_id: i64 },
    Web,
    Discord,
    Slack,
}

impl MessageSource {
    pub fn channel_type(&self) -> &str {
        match self {
            MessageSource::Telegram { .. } => "telegram",
            MessageSource::Web => "web",
            MessageSource::Discord => "discord",
            MessageSource::Slack => "slack",
        }
    }

    pub fn message_id(&self) -> i64 {
        match self {
            MessageSource::Telegram { message_id } => *message_id,
            _ => 0,
        }
    }
}

/// Incoming chat message.
pub struct IncomingMessage {
    pub message: String,
    pub chat_id: i64,
    pub sender: String,
    pub source: MessageSource,
    pub project_hint: Option<String>,
    pub department_hint: Option<String>,
    pub channel_name: Option<String>,
    /// Persistent agent UUID for entity memory scoping and routing.
    pub agent_id: Option<String>,
}

impl IncomingMessage {
    fn conversation_channel_type(&self) -> String {
        match (
            self.source.channel_type(),
            self.project_hint.as_deref(),
            self.department_hint.as_deref(),
        ) {
            (base, _, Some(_)) => format!("{base}_department"),
            (base, Some(_), None) => format!("{base}_project"),
            (base, None, None) => base.to_string(),
        }
    }

    fn conversation_channel_name(&self) -> String {
        if let Some(name) = &self.channel_name {
            return name.clone();
        }
        if let Some(project) = &self.project_hint {
            if let Some(department) = &self.department_hint {
                return format!("{project}/{department}");
            }
            return project.clone();
        }
        self.sender.clone()
    }

    fn scope_label(&self) -> String {
        match (&self.project_hint, &self.department_hint) {
            (Some(project), Some(department)) => {
                format!("project={project}, department={department}")
            }
            (Some(project), None) => format!("project={project}"),
            (None, _) => "global".to_string(),
        }
    }
}

/// Response from the chat engine (quick path).
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub ok: bool,
    pub context: String,
    pub action: Option<String>,
    pub task: Option<serde_json::Value>,
    pub projects: Option<Vec<serde_json::Value>>,
    pub cost: Option<serde_json::Value>,
    pub workers: Option<u32>,
}

impl ChatResponse {
    pub fn error(msg: &str) -> Self {
        Self {
            ok: false,
            context: msg.to_string(),
            action: None,
            task: None,
            projects: None,
            cost: None,
            workers: None,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "ok": self.ok,
            "context": self.context,
        });
        if let Some(ref action) = self.action {
            v["action"] = serde_json::json!(action);
        }
        if let Some(ref task) = self.task {
            v["task"] = task.clone();
        }
        if let Some(ref projects) = self.projects {
            v["projects"] = serde_json::json!(projects);
        }
        if let Some(ref cost) = self.cost {
            v["cost"] = cost.clone();
        }
        if let Some(workers) = self.workers {
            v["workers"] = serde_json::json!(workers);
        }
        v
    }
}

/// Handle returned when a full (async) chat task is created.
#[derive(Debug, Clone)]
pub struct TaskHandle {
    pub task_id: String,
    pub chat_id: i64,
    pub project: String,
}

/// A pending task that's being processed asynchronously.
pub struct PendingTask {
    pub project: String,
    pub chat_id: i64,
    pub message_id: i64,
    pub source: MessageSource,
    pub channel_type: String,
    pub created_at: std::time::Instant,
    pub phase1_reaction: Option<String>,
    pub sent_slow_notice: bool,
}

/// Result of a completed chat task.
#[derive(Debug, Clone)]
pub struct ChatCompletion {
    pub task_id: String,
    pub chat_id: i64,
    pub message_id: i64,
    pub source: MessageSource,
    pub status: CompletionStatus,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum CompletionStatus {
    Done,
    Blocked,
    Cancelled,
    TimedOut,
}

// ── Engine ──

/// The unified chat engine.
pub struct MessageRouter {
    pub conversations: Arc<SessionStore>,
    pub agent_registry: Arc<AgentRegistry>,
    pub agent_router: Arc<Mutex<AgentRouter>>,
    pub council_advisors: Arc<Vec<aeqi_core::config::PeerAgentConfig>>,
    /// If false, only explicit `/council` requests fan out to advisors.
    pub auto_council_enabled: bool,
    pub leader_name: String,
    /// Default project/agent to route messages to when no project_hint is given.
    pub default_project: String,
    pub pending_tasks: Arc<Mutex<HashMap<String, PendingTask>>>,
    pub task_notify: Arc<tokio::sync::Notify>,
    /// Per-project memory stores for knowledge-aware chat (keyed by project name).
    pub insight_stores: HashMap<String, Arc<dyn Insight>>,
    /// Per-project memory stores keyed by project UUID for id-based lookups.
    pub insight_stores_by_id: HashMap<String, Arc<dyn Insight>>,
    /// LLM-backed intent classifier for ambiguous messages.
    pub intent_classifier: Option<Arc<crate::intent::IntentClassifier>>,
    /// Wake signal for the global Scheduler.
    pub scheduler_wake: Arc<tokio::sync::Notify>,
}

impl MessageRouter {
    fn set_scheduler_hold(task: &mut aeqi_quests::Quest, hold: bool, reason: Option<&str>) {
        let mut metadata = match std::mem::take(&mut task.metadata) {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("_legacy".to_string(), other);
                map
            }
        };

        if hold {
            metadata.insert(
                "aeqi".to_string(),
                serde_json::json!({
                    "hold": true,
                    "hold_reason": reason.unwrap_or(CHAT_COUNCIL_HOLD_REASON),
                }),
            );
        } else if let Some(aeqi_meta) = metadata.get_mut("aeqi")
            && let Some(obj) = aeqi_meta.as_object_mut()
        {
            obj.remove("hold");
            obj.remove("hold_reason");
            if obj.is_empty() {
                metadata.remove("aeqi");
            }
        }

        task.metadata = if metadata.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(metadata)
        };
    }

    fn task_completion_reason(task: &aeqi_quests::Quest) -> Option<String> {
        match task.status {
            aeqi_quests::QuestStatus::Blocked => task.blocker_context(),
            _ => task.outcome_summary(),
        }
    }

    fn append_council_input(description: &mut String, council_input: &[(String, String)]) {
        if council_input.is_empty() {
            return;
        }

        description.push_str("\n\n## Council Input\n\n");
        for (name, text) in council_input {
            description.push_str(&format!("### {} advises:\n{}\n\n", name, text));
        }
        description.push_str(
            "Synthesize the council's input into your response. Attribute key insights where relevant.\n",
        );
    }

    async fn record_reply_text(&self, chat_id: i64, source_tag: &str, text: &str) {
        if text.trim().is_empty() {
            return;
        }

        let _ = self
            .conversations
            .record_with_source(chat_id, &self.leader_name, text, Some(source_tag))
            .await;
    }

    async fn ensure_channel_registered(&self, msg: &IncomingMessage) {
        let channel_type = msg.conversation_channel_type();
        let channel_name = msg.conversation_channel_name();
        let _ = self
            .conversations
            .ensure_channel(msg.chat_id, &channel_type, &channel_name)
            .await;
    }

    pub async fn record_exchange(&self, msg: &IncomingMessage, reply_text: &str) {
        let source_tag = msg.conversation_channel_type();
        self.ensure_channel_registered(msg).await;
        let _ = self
            .conversations
            .record_with_source(msg.chat_id, "User", &msg.message, Some(&source_tag))
            .await;
        self.record_reply_text(msg.chat_id, &source_tag, reply_text)
            .await;
    }

    async fn record_thread_event(
        &self,
        chat_id: i64,
        source_tag: &str,
        event_type: &str,
        role: &str,
        content: &str,
        metadata: Option<serde_json::Value>,
    ) {
        let _ = self
            .conversations
            .record_event(
                chat_id,
                event_type,
                role,
                content,
                Some(source_tag),
                metadata.as_ref(),
            )
            .await;
    }

    async fn record_response_action_event(&self, msg: &IncomingMessage, response: &ChatResponse) {
        let Some(action) = response.action.as_deref() else {
            return;
        };

        let source_tag = msg.conversation_channel_type();
        let event_type = match action {
            "task_created" => "task_created",
            "task_closed" => "task_closed",
            "knowledge_stored" => "knowledge_stored",
            _ => return,
        };
        let metadata = response.task.clone();

        self.record_thread_event(
            msg.chat_id,
            &source_tag,
            event_type,
            "system",
            &response.context,
            metadata,
        )
        .await;
    }

    async fn create_chat_task(
        &self,
        project_name: &str,
        subject: &str,
        description: &str,
        hold_for_council: bool,
    ) -> Result<aeqi_quests::Quest> {
        let agent = self
            .agent_registry
            .resolve_by_hint(project_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("agent not found for hint: {project_name}"))?;

        let labels: Vec<String> = if hold_for_council {
            vec!["chat".to_string(), "council_pending".to_string()]
        } else {
            vec!["chat".to_string()]
        };

        let mut task = self
            .agent_registry
            .create_task(&agent.id, subject, description, None, &labels)
            .await?;

        if hold_for_council {
            Self::set_scheduler_hold(&mut task, true, Some(CHAT_COUNCIL_HOLD_REASON));
            self.agent_registry
                .update_task(&task.id.0, |entry| {
                    Self::set_scheduler_hold(entry, true, Some(CHAT_COUNCIL_HOLD_REASON));
                })
                .await?;
        }

        info!(
            project = %project_name,
            agent = %agent.name,
            task = %task.id,
            hold_for_council,
            subject = %subject,
            "chat task created"
        );

        if !hold_for_council {
            self.scheduler_wake.notify_one();
        }

        Ok(task)
    }

    fn prefix_if_missing(text: String, prefix: &str) -> String {
        if text
            .trim_start()
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase())
        {
            text
        } else {
            format!("{prefix}{text}")
        }
    }

    fn completion_text(status: &CompletionStatus, reason: Option<String>) -> String {
        match status {
            CompletionStatus::Done => reason
                .filter(|r| !r.trim().is_empty())
                .unwrap_or_else(|| "Done.".to_string()),
            CompletionStatus::Blocked => Self::prefix_if_missing(
                reason.unwrap_or_else(|| "Needs input.".to_string()),
                "Blocked: ",
            ),
            CompletionStatus::Cancelled => Self::prefix_if_missing(
                reason.unwrap_or_else(|| "Task cancelled.".to_string()),
                "Failed: ",
            ),
            CompletionStatus::TimedOut => {
                "Sorry, this one took too long. Try again or simplify the request.".to_string()
            }
        }
    }

    async fn consume_pending_completion(
        &self,
        task_id: &str,
        status: CompletionStatus,
        reason: Option<String>,
    ) -> Option<ChatCompletion> {
        let pending = {
            let mut map = self.pending_tasks.lock().await;
            map.remove(task_id)?
        };

        let text = Self::completion_text(&status, reason);
        let event_type = match status {
            CompletionStatus::Done => "task_completed",
            CompletionStatus::Blocked => "task_blocked",
            CompletionStatus::Cancelled => "task_cancelled",
            CompletionStatus::TimedOut => "task_timed_out",
        };
        self.record_thread_event(
            pending.chat_id,
            &pending.channel_type,
            event_type,
            "system",
            &format!("Task {task_id} {event_type}."),
            Some(serde_json::json!({
                "task_id": task_id,
                "status": format!("{status:?}"),
                "reply_text": text.clone(),
                "project": pending.project.clone(),
            })),
        )
        .await;
        self.record_reply_text(pending.chat_id, &pending.channel_type, &text)
            .await;

        Some(ChatCompletion {
            task_id: task_id.to_string(),
            chat_id: pending.chat_id,
            message_id: pending.message_id,
            source: pending.source,
            status,
            text,
        })
    }

    /// Handle explicit command shortcuts (no LLM call).
    /// Returns None for normal messages — caller should use `handle_message_full`.
    pub async fn handle_message(&self, msg: &IncomingMessage) -> Option<ChatResponse> {
        if msg.message.is_empty() {
            return Some(ChatResponse::error("message is required"));
        }

        self.ensure_channel_registered(msg).await;

        let msg_lower = msg.message.to_lowercase();

        // Keyword shortcuts — explicit prefixes only, no guessing.
        if msg_lower.starts_with("create task")
            || msg_lower.starts_with("new task")
            || msg_lower.starts_with("add task")
        {
            let response = self.handle_create_task(msg).await;
            self.record_exchange(msg, &response.context).await;
            self.record_response_action_event(msg, &response).await;
            return Some(response);
        }

        if msg_lower.starts_with("close task") || msg_lower.starts_with("done with") {
            let response = self.handle_close_task(msg).await;
            self.record_exchange(msg, &response.context).await;
            self.record_response_action_event(msg, &response).await;
            return Some(response);
        }

        // Everything else goes to the agent via full path.
        None
    }

    /// Handle a chat message (full path): conversation context + task creation.
    /// Council enrichment, when enabled, is performed asynchronously after the
    /// handle is returned and before the task is released to the scheduler.
    pub async fn handle_message_full(
        &self,
        msg: &IncomingMessage,
        phase1_reaction: Option<String>,
    ) -> Result<TaskHandle> {
        let source_tag = msg.conversation_channel_type();
        let scoped_project = msg
            .project_hint
            .clone()
            .unwrap_or_else(|| self.default_project.clone());

        // Register channel.
        self.ensure_channel_registered(msg).await;

        // Fetch recent messages for context.
        let recent = self
            .conversations
            .recent(msg.chat_id, 20)
            .await
            .unwrap_or_default();

        // Build conversation context for task description.
        let ctx = self
            .conversations
            .context_string(msg.chat_id, 20)
            .await
            .unwrap_or_default();

        // Build compact context for advisor tasks.
        let conv_context_for_advisors = if recent.is_empty() {
            String::new()
        } else {
            let mut s = String::from("Recent conversation:\n");
            for msg_item in recent
                .iter()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                let truncated = if msg_item.content.len() > 200 {
                    let mut end = 200;
                    while !msg_item.content.is_char_boundary(end) {
                        end -= 1;
                    }
                    &msg_item.content[..end]
                } else {
                    msg_item.content.as_str()
                };
                s.push_str(&format!("  {}: {}\n", msg_item.role, truncated));
            }
            s
        };

        // Record user message.
        let _ = self
            .conversations
            .record_with_source(msg.chat_id, "User", &msg.message, Some(&source_tag))
            .await;

        // Build task description with conversation context.
        let routing = format!(
            "[transport: {} | scope: {} | channel: {} | chat_id: {} | reply: auto-delivered by daemon]",
            msg.source.channel_type(),
            msg.scope_label(),
            msg.conversation_channel_name(),
            msg.chat_id
        );
        let response_protocol = "**RESPONSE PROTOCOL**: Write your reply directly — in character, in voice. Your output text IS the reply. The daemon delivers it automatically. Do NOT call any tools to send the reply. Do NOT write meta-commentary like \"I've sent your reply\" or \"Done.\".";
        let mut description = if ctx.is_empty() {
            format!("{}\n\n---\n{}\n{}", msg.message, routing, response_protocol)
        } else {
            format!(
                "{}\n## Current Message\n\n{}\n\n---\n{}\n{}",
                ctx, msg.message, routing, response_protocol
            )
        };

        // Inject Phase 1 reaction if available.
        if let Some(ref reaction) = phase1_reaction {
            description = format!(
                "{}\n\n---\n## Your Immediate Reaction (already sent)\n\n\
                 You already reacted with this stage direction:\n\
                 {}\n\n\
                 Continue from this energy. Your full reply should feel like the natural \
                 next beat after this reaction — same emotional tone, same intensity. \
                 Don't repeat or reference the reaction itself, just carry its momentum.\n",
                description, reaction
            );
        }

        if msg.project_hint.is_some() || msg.department_hint.is_some() || msg.channel_name.is_some()
        {
            let mut lines = Vec::new();
            if let Some(name) = &msg.channel_name {
                lines.push(format!("Channel: {name}"));
            }
            if let Some(project) = &msg.project_hint {
                lines.push(format!("Project scope: {project}"));
            }
            if let Some(department) = &msg.department_hint {
                lines.push(format!("Department scope: {department}"));
            }
            description.push_str("\n\n---\n## Channel Context\n\n");
            description.push_str(&lines.join("\n"));
            description.push('\n');
        }

        let is_council = msg.message.starts_with("/council");
        let clean_text = if is_council {
            msg.message
                .strip_prefix("/council")
                .unwrap_or(&msg.message)
                .trim()
                .to_string()
        } else {
            msg.message.clone()
        };
        let hold_for_council =
            !self.council_advisors.is_empty() && (is_council || self.auto_council_enabled);

        // Create the task.
        let subject = format!("[{}] {} ({})", source_tag, msg.sender, msg.chat_id);
        let task = self
            .create_chat_task(&scoped_project, &subject, &description, hold_for_council)
            .await?;
        let task_id = task.id.0.clone();
        self.record_thread_event(
            msg.chat_id,
            &source_tag,
            "task_created",
            "system",
            &format!("Task {task_id} created in {scoped_project}."),
            Some(serde_json::json!({
                "task_id": task_id.clone(),
                "project": scoped_project.clone(),
                "held_for_council": hold_for_council,
            })),
        )
        .await;
        if hold_for_council {
            self.record_thread_event(
                msg.chat_id,
                &source_tag,
                "council_pending",
                "system",
                "Gathering advisor input.",
                None,
            )
            .await;
        } else {
            self.record_thread_event(
                msg.chat_id,
                &source_tag,
                "task_released",
                "system",
                "Task released to the project scheduler.",
                Some(serde_json::json!({
                    "task_id": task_id.clone(),
                    "project": scoped_project.clone(),
                })),
            )
            .await;
        }

        // Register pending task for completion tracking.
        self.pending_tasks.lock().await.insert(
            task_id.clone(),
            PendingTask {
                project: scoped_project.clone(),
                chat_id: msg.chat_id,
                message_id: msg.source.message_id(),
                source: msg.source.clone(),
                channel_type: source_tag.clone(),
                created_at: std::time::Instant::now(),
                phase1_reaction,
                sent_slow_notice: false,
            },
        );

        if hold_for_council {
            let agent_registry = self.agent_registry.clone();
            let conversations = self.conversations.clone();
            let agent_router = self.agent_router.clone();
            let council_advisors = self.council_advisors.clone();
            let task_id_for_spawn = task_id.clone();
            let project_name = scoped_project.clone();
            let clean_text_for_spawn = clean_text.clone();
            let conv_context_for_spawn = conv_context_for_advisors.clone();
            let source_tag_for_spawn = source_tag.clone();
            let project_hint = msg.project_hint.clone();
            let department_hint = msg.department_hint.clone();
            let chat_id = msg.chat_id;
            let scheduler_wake = self.scheduler_wake.clone();

            tokio::spawn(async move {
                MessageRouter::finish_council_enrichment(
                    agent_registry,
                    conversations,
                    agent_router,
                    council_advisors,
                    task_id_for_spawn,
                    project_name,
                    clean_text_for_spawn,
                    is_council,
                    conv_context_for_spawn,
                    chat_id,
                    source_tag_for_spawn,
                    project_hint,
                    department_hint,
                    scheduler_wake,
                )
                .await;
            });
        }

        Ok(TaskHandle {
            task_id,
            chat_id: msg.chat_id,
            project: scoped_project,
        })
    }

    /// Check pending tasks for completions. Returns completed tasks and removes them from pending.
    pub async fn check_completions(&self) -> Vec<ChatCompletion> {
        let mut completions = Vec::new();
        let pending: Vec<(String, String)> = self
            .pending_tasks
            .lock()
            .await
            .iter()
            .map(|(task_id, pending)| (task_id.clone(), pending.project.clone()))
            .collect();

        for (qid, _project) in pending {
            let status = match self.agent_registry.get_task(&qid).await {
                Ok(Some(task)) => Some((task.status, Self::task_completion_reason(&task))),
                _ => None,
            };

            match status {
                Some((aeqi_quests::QuestStatus::Done, reason)) => {
                    if let Some(completion) = self
                        .consume_pending_completion(&qid, CompletionStatus::Done, reason)
                        .await
                    {
                        completions.push(completion);
                    }
                }
                Some((aeqi_quests::QuestStatus::Blocked, reason)) => {
                    if let Some(completion) = self
                        .consume_pending_completion(&qid, CompletionStatus::Blocked, reason)
                        .await
                    {
                        completions.push(completion);
                    }
                }
                Some((aeqi_quests::QuestStatus::Cancelled, reason)) => {
                    if let Some(completion) = self
                        .consume_pending_completion(&qid, CompletionStatus::Cancelled, reason)
                        .await
                    {
                        completions.push(completion);
                    }
                }
                _ => {
                    let elapsed = {
                        let map = self.pending_tasks.lock().await;
                        map.get(&qid).map(|pq| pq.created_at.elapsed())
                    };
                    if elapsed.is_some_and(|age| age > std::time::Duration::from_secs(1800)) {
                        warn!(task = %qid, "chat task hard-timed out after 30min");
                        if let Some(completion) = self
                            .consume_pending_completion(&qid, CompletionStatus::TimedOut, None)
                            .await
                        {
                            completions.push(completion);
                        }
                    }
                }
            }
        }

        completions
    }

    /// Get pending tasks that need a slow-progress notice (elapsed > 2min).
    pub async fn get_slow_tasks(&self) -> Vec<(String, i64, i64, MessageSource)> {
        let mut slow = Vec::new();
        let mut map = self.pending_tasks.lock().await;
        for (qid, pq) in map.iter_mut() {
            let elapsed = pq.created_at.elapsed();
            if elapsed > std::time::Duration::from_secs(120) && !pq.sent_slow_notice {
                pq.sent_slow_notice = true;
                self.record_thread_event(
                    pq.chat_id,
                    &pq.channel_type,
                    "task_slow",
                    "system",
                    "Still working.",
                    Some(serde_json::json!({
                        "task_id": qid,
                        "project": pq.project,
                        "elapsed_secs": elapsed.as_secs(),
                    })),
                )
                .await;
                slow.push((qid.clone(), pq.chat_id, pq.message_id, pq.source.clone()));
            }
        }
        slow
    }

    /// Poll a specific task for completion.
    pub async fn poll_completion(&self, task_id: &str) -> Option<ChatCompletion> {
        let _project = {
            let pending = self.pending_tasks.lock().await;
            pending.get(task_id).map(|task| task.project.clone())?
        };

        let status = match self.agent_registry.get_task(task_id).await {
            Ok(Some(task)) => Some((task.status, Self::task_completion_reason(&task))),
            _ => None,
        };

        match status {
            Some((aeqi_quests::QuestStatus::Done, reason)) => {
                self.consume_pending_completion(task_id, CompletionStatus::Done, reason)
                    .await
            }
            Some((aeqi_quests::QuestStatus::Blocked, reason)) => {
                self.consume_pending_completion(task_id, CompletionStatus::Blocked, reason)
                    .await
            }
            Some((aeqi_quests::QuestStatus::Cancelled, reason)) => {
                self.consume_pending_completion(task_id, CompletionStatus::Cancelled, reason)
                    .await
            }
            _ => None,
        }
    }

    /// Get conversation history.
    pub async fn get_history(
        &self,
        chat_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SessionMessage>> {
        self.conversations
            .recent_with_offset(chat_id, limit, offset)
            .await
    }

    /// Get typed thread timeline events.
    pub async fn get_timeline(
        &self,
        chat_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ThreadEvent>> {
        self.conversations
            .timeline_with_offset(chat_id, limit, offset)
            .await
    }

    /// List all known channels.
    pub async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        self.conversations.list_channels().await
    }

    /// Build a status response enriched with relevant memories.
    pub async fn status_response(
        &self,
        project_hint: Option<&str>,
        query: Option<&str>,
    ) -> ChatResponse {
        // Search memory for relevant context if we have a query.
        let memory_context = if let (Some(project), Some(q)) = (project_hint, query) {
            self.build_memory_context(project, q).await
        } else if let Some(q) = query {
            // Global query — search all projects.
            let mut all_ctx = Vec::new();
            for (name, mem) in &self.insight_stores {
                let mq = InsightQuery::new(q, 3);
                if let Ok(results) = mem.search(&mq).await {
                    for entry in results {
                        all_ctx.push(format!("  • [{}] {}: {}", name, entry.key, entry.content));
                    }
                }
            }
            if all_ctx.is_empty() {
                None
            } else {
                Some(format!("Relevant knowledge:\n{}", all_ctx.join("\n")))
            }
        } else {
            None
        };

        // Gather agent and task info from AgentRegistry.
        let agents = self.agent_registry.list_active().await.unwrap_or_default();
        let all_tasks = self
            .agent_registry
            .list_tasks(None, None)
            .await
            .unwrap_or_default();

        let agent_summaries: Vec<serde_json::Value> = if let Some(hint) = project_hint {
            agents
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case(hint) || a.id == hint)
                .map(|a| {
                    let agent_tasks: Vec<_> = all_tasks
                        .iter()
                        .filter(|t| t.agent_id.as_deref() == Some(&a.id))
                        .collect();
                    let open = agent_tasks
                        .iter()
                        .filter(|t| {
                            !matches!(
                                t.status,
                                aeqi_quests::QuestStatus::Done
                                    | aeqi_quests::QuestStatus::Cancelled
                            )
                        })
                        .count();
                    let total = agent_tasks.len();
                    serde_json::json!({
                        "name": a.name,
                        "open_tasks": open,
                        "total_tasks": total,
                    })
                })
                .collect()
        } else {
            agents
                .iter()
                .map(|a| {
                    let agent_tasks: Vec<_> = all_tasks
                        .iter()
                        .filter(|t| t.agent_id.as_deref() == Some(&a.id))
                        .collect();
                    let open = agent_tasks
                        .iter()
                        .filter(|t| {
                            !matches!(
                                t.status,
                                aeqi_quests::QuestStatus::Done
                                    | aeqi_quests::QuestStatus::Cancelled
                            )
                        })
                        .count();
                    let total = agent_tasks.len();
                    serde_json::json!({
                        "name": a.name,
                        "open_tasks": open,
                        "total_tasks": total,
                    })
                })
                .collect()
        };

        let total_open = all_tasks
            .iter()
            .filter(|t| {
                !matches!(
                    t.status,
                    aeqi_quests::QuestStatus::Done | aeqi_quests::QuestStatus::Cancelled
                )
            })
            .count();

        let mut context = String::new();

        if let Some(hint) = project_hint {
            if let Some(summary) = agent_summaries.first() {
                let name = summary["name"].as_str().unwrap_or(hint);
                let open = summary["open_tasks"].as_u64().unwrap_or(0);
                let total = summary["total_tasks"].as_u64().unwrap_or(0);
                context.push_str(&format!("{}: {} open/{} total tasks\n", name, open, total));
            } else {
                context.push_str(&format!("Agent '{}' not found.\n", hint));
            }
        } else {
            for summary in &agent_summaries {
                let name = summary["name"].as_str().unwrap_or("?");
                let open = summary["open_tasks"].as_u64().unwrap_or(0);
                let total = summary["total_tasks"].as_u64().unwrap_or(0);
                context.push_str(&format!("{}: {} open/{} total tasks\n", name, open, total));
            }
        }

        context.push_str(&format!(
            "\nAgents: {}, Total open tasks: {}\n",
            agents.len(),
            total_open
        ));

        // Prepend memory context if available.
        if let Some(ref mem_ctx) = memory_context {
            context = format!("{}\n\n{}", mem_ctx, context);
        }

        ChatResponse {
            ok: true,
            context: context.trim().to_string(),
            action: None,
            task: None,
            projects: Some(agent_summaries),
            cost: None,
            workers: Some(agents.len() as u32),
        }
    }

    /// Search memory for context relevant to a query in a specific project.
    pub async fn build_memory_context(&self, project: &str, query: &str) -> Option<String> {
        let mem = self.insight_stores.get(project)?;
        let mq = InsightQuery::new(query, 5);
        let results = mem.search(&mq).await.ok()?;
        if results.is_empty() {
            return None;
        }
        let mut ctx = String::from("Relevant knowledge:\n");
        for entry in &results {
            ctx.push_str(&format!("  • {}: {}\n", entry.key, entry.content));
        }
        Some(ctx)
    }

    /// Store a note to the project's memory.
    pub async fn store_note(&self, project: &str, key: &str, content: &str) -> Result<String> {
        let mem = self
            .insight_stores
            .get(project)
            .ok_or_else(|| anyhow::anyhow!("no memory store for project: {project}"))?;
        let id = mem
            .store(key, content, aeqi_core::traits::InsightCategory::Fact, None)
            .await?;
        Ok(id)
    }

    // ── Private helpers ──

    async fn handle_create_task(&self, msg: &IncomingMessage) -> ChatResponse {
        let msg_lower = msg.message.to_lowercase();

        let project = if let Some(p) = &msg.project_hint {
            p.clone()
        } else {
            // Try to match an agent name from the message text.
            let agents = self.agent_registry.list_active().await.unwrap_or_default();
            let mut found = String::new();
            for agent in &agents {
                if msg_lower.contains(&agent.name.to_lowercase()) {
                    found = agent.name.clone();
                    break;
                }
            }
            if found.is_empty() {
                agents
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| self.default_project.clone())
            } else {
                found
            }
        };

        let subject = msg_lower
            .replace("create a task", "")
            .replace("create task", "")
            .replace("new task", "")
            .replace("add a task", "")
            .replace("add task", "")
            .replace(&format!("in {}", project.to_lowercase()), "")
            .replace(&format!("for {}", project.to_lowercase()), "")
            .replace(" to ", " ")
            .trim()
            .trim_start_matches(':')
            .trim()
            .to_string();

        let subject = if subject.is_empty() {
            msg.message.clone()
        } else {
            let start = msg.message.to_lowercase().find(&subject).unwrap_or(0);
            if start + subject.len() <= msg.message.len() {
                msg.message[start..start + subject.len()].to_string()
            } else {
                subject
            }
        };

        let create_result = match self.agent_registry.resolve_by_hint(&project).await {
            Ok(Some(agent)) => {
                self.agent_registry
                    .create_task(&agent.id, &subject, "", None, &["chat".to_string()])
                    .await
            }
            Ok(None) => Err(anyhow::anyhow!("agent not found for hint: {project}")),
            Err(e) => Err(e),
        };

        match create_result {
            Ok(task) => ChatResponse {
                ok: true,
                context: format!(
                    "Done. Created task {} in {} — \"{}\"",
                    task.id, project, subject
                ),
                action: Some("task_created".to_string()),
                task: Some(serde_json::json!({
                    "id": task.id.0,
                    "subject": task.name,
                    "project": project,
                })),
                projects: None,
                cost: None,
                workers: None,
            },
            Err(e) => ChatResponse::error(&format!("Failed to create task: {}", e)),
        }
    }

    async fn handle_close_task(&self, msg: &IncomingMessage) -> ChatResponse {
        let task_id: String = msg
            .message
            .split_whitespace()
            .find(|w| w.contains('-') && w.chars().any(|c| c.is_ascii_digit()))
            .unwrap_or("")
            .to_string();

        if task_id.is_empty() {
            return ChatResponse::error("I need a task ID to close (e.g., 'close task as-001').");
        }

        // Close via AgentRegistry: update the task status to Done.
        match self.agent_registry.get_task(&task_id).await {
            Ok(Some(_)) => {
                match self
                    .agent_registry
                    .update_task(&task_id, |task| {
                        task.status = aeqi_quests::QuestStatus::Done;
                        task.closed_at = Some(chrono::Utc::now());
                        task.closed_reason = Some("closed via chat".to_string());
                    })
                    .await
                {
                    Ok(_) => ChatResponse {
                        ok: true,
                        context: format!("Done. Task {} is now closed.", task_id),
                        action: Some("task_closed".to_string()),
                        task: None,
                        projects: None,
                        cost: None,
                        workers: None,
                    },
                    Err(e) => {
                        ChatResponse::error(&format!("Failed to close task {}: {}", task_id, e))
                    }
                }
            }
            Ok(None) => ChatResponse::error(&format!("Couldn't find task {}.", task_id)),
            Err(e) => ChatResponse::error(&format!("Error looking up task {}: {}", task_id, e)),
        }
    }

    async fn classify_advisors_with(
        agent_registry: &Arc<AgentRegistry>,
        agent_router: &Arc<Mutex<AgentRouter>>,
        council_advisors: &Arc<Vec<aeqi_core::config::PeerAgentConfig>>,
        clean_text: &str,
        is_council: bool,
        chat_id: i64,
        project_hint: Option<&str>,
        department_hint: Option<&str>,
    ) -> Vec<String> {
        if council_advisors.is_empty() {
            return Vec::new();
        }

        let scoped_names =
            Self::scoped_advisor_names_with(agent_registry, project_hint, department_hint).await;
        let advisor_refs: Vec<&aeqi_core::config::PeerAgentConfig> = match &scoped_names {
            Some(names) => council_advisors
                .iter()
                .filter(|advisor| names.contains(&advisor.name))
                .collect(),
            None => council_advisors.iter().collect(),
        };
        if advisor_refs.is_empty() {
            return Vec::new();
        }

        let route = {
            let mut router = agent_router.lock().await;
            if scoped_names.is_some() {
                router
                    .classify_for_project(clean_text, &advisor_refs, chat_id)
                    .await
            } else {
                router.classify(clean_text, &advisor_refs, chat_id).await
            }
        };
        match route {
            Ok(decision) => {
                if is_council && decision.advisors.is_empty() {
                    advisor_refs.iter().map(|c| c.name.clone()).collect()
                } else {
                    decision.advisors
                }
            }
            Err(e) => {
                warn!(error = %e, "classifier failed");
                if is_council {
                    advisor_refs.iter().map(|c| c.name.clone()).collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    async fn scoped_advisor_names_with(
        agent_registry: &Arc<AgentRegistry>,
        project_hint: Option<&str>,
        department_hint: Option<&str>,
    ) -> Option<HashSet<String>> {
        // Department scoping: find agents that are children of the project agent.
        let project_name = project_hint?;
        let _department_name = department_hint?;

        // Resolve the project agent and collect its subtree names.
        let agent = agent_registry.resolve_by_hint(project_name).await.ok()??;
        let children = agent_registry
            .get_children(&agent.id)
            .await
            .ok()
            .unwrap_or_default();

        if children.is_empty() {
            return None;
        }

        let mut allowed = HashSet::new();
        for child in children {
            allowed.insert(child.name);
        }
        Some(allowed)
    }

    async fn gather_council_input_with(
        agent_registry: Arc<AgentRegistry>,
        conversations: Arc<SessionStore>,
        advisors: &[String],
        clean_text: &str,
        conv_context: &str,
        chat_id: i64,
        source_tag: &str,
    ) -> Vec<(String, String)> {
        info!(advisors = ?advisors, "invoking council advisors");

        let mut handles = Vec::new();
        for advisor_name in advisors {
            let adv_name = advisor_name.clone();
            let adv_msg = clean_text.to_string();
            let adv_history = conv_context.to_string();
            let ar = agent_registry.clone();

            let handle = tokio::spawn(async move {
                let task_subject = "[council] Advisor input requested".to_string();
                let task_desc = if adv_history.is_empty() {
                    format!(
                        "The user said:\n\n{}\n\n\
                         Provide your specialist perspective on this in character. \
                         Be concise (2-5 sentences). Focus on your domain expertise.",
                        adv_msg
                    )
                } else {
                    format!(
                        "{}\n\nThe user now says:\n\n{}\n\n\
                         Provide your specialist perspective on this in character. \
                         Be concise (2-5 sentences). Focus on your domain expertise.",
                        adv_history, adv_msg
                    )
                };

                // Resolve the advisor agent and create a task via AgentRegistry.
                let agent = match ar.resolve_by_hint(&adv_name).await {
                    Ok(Some(a)) => a,
                    Ok(None) => {
                        warn!(agent = %adv_name, "advisor agent not found");
                        return None;
                    }
                    Err(e) => {
                        warn!(agent = %adv_name, error = %e, "failed to resolve advisor agent");
                        return None;
                    }
                };

                let task_id = match ar
                    .create_task(
                        &agent.id,
                        &task_subject,
                        &task_desc,
                        None,
                        &["council".to_string()],
                    )
                    .await
                {
                    Ok(t) => t.id.0.clone(),
                    Err(e) => {
                        warn!(agent = %adv_name, error = %e, "failed to create advisor task");
                        return None;
                    }
                };

                // Poll for completion with a 60-second timeout.
                let timeout = tokio::time::sleep(std::time::Duration::from_secs(60));
                tokio::pin!(timeout);
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {}
                        _ = &mut timeout => {
                            warn!(agent = %adv_name, "advisor task timed out");
                            return None;
                        }
                    }
                    let done = match ar.get_task(&task_id).await {
                        Ok(Some(task)) => {
                            if task.status == aeqi_quests::QuestStatus::Done {
                                Some(task.outcome_summary())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(reason) = done {
                        let text = reason.unwrap_or_default();
                        return Some((adv_name, text));
                    }
                }
            });
            handles.push(handle);
        }

        // Record advisor responses in conversation history.
        let mut responses = Vec::new();
        for handle in handles {
            if let Ok(Some((name, text))) = handle.await
                && !text.trim().is_empty()
            {
                let capitalized = {
                    let mut c = name.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().to_string() + c.as_str(),
                    }
                };
                let _ = conversations
                    .record_event(
                        chat_id,
                        "council_advice",
                        &capitalized,
                        text.trim(),
                        Some(source_tag),
                        Some(&serde_json::json!({
                            "advisor": name,
                        })),
                    )
                    .await;
                responses.push((name, text.trim().to_string()));
            }
        }

        responses
    }

    async fn finish_council_enrichment(
        agent_registry: Arc<AgentRegistry>,
        conversations: Arc<SessionStore>,
        agent_router: Arc<Mutex<AgentRouter>>,
        council_advisors: Arc<Vec<aeqi_core::config::PeerAgentConfig>>,
        task_id: String,
        project_name: String,
        clean_text: String,
        is_council: bool,
        conv_context: String,
        chat_id: i64,
        source_tag: String,
        project_hint: Option<String>,
        department_hint: Option<String>,
        scheduler_wake: Arc<tokio::sync::Notify>,
    ) {
        let advisors_to_invoke = Self::classify_advisors_with(
            &agent_registry,
            &agent_router,
            &council_advisors,
            &clean_text,
            is_council,
            chat_id,
            project_hint.as_deref(),
            department_hint.as_deref(),
        )
        .await;

        let council_input = if advisors_to_invoke.is_empty() {
            Vec::new()
        } else {
            let _ = conversations
                .record_event(
                    chat_id,
                    "council_started",
                    "system",
                    "Consulting advisors.",
                    Some(&source_tag),
                    Some(&serde_json::json!({
                        "task_id": task_id.clone(),
                        "advisors": advisors_to_invoke.clone(),
                    })),
                )
                .await;
            Self::gather_council_input_with(
                agent_registry.clone(),
                conversations.clone(),
                &advisors_to_invoke,
                &clean_text,
                &conv_context,
                chat_id,
                &source_tag,
            )
            .await
        };

        // Update task with council input.
        let update_result: Result<()> = agent_registry
            .update_task(&task_id, |task| {
                Self::append_council_input(&mut task.description, &council_input);
                Self::set_scheduler_hold(task, false, None);
            })
            .await
            .map(|_| ());

        match update_result {
            Ok(_) => {
                if !council_input.is_empty() {
                    let _ = conversations
                        .record_event(
                            chat_id,
                            "council_ready",
                            "system",
                            "Council input attached to the task.",
                            Some(&source_tag),
                            Some(&serde_json::json!({
                                "task_id": task_id.clone(),
                                "advisor_count": council_input.len(),
                            })),
                        )
                        .await;
                }
                let _ = conversations
                    .record_event(
                        chat_id,
                        "task_released",
                        "system",
                        "Task released to the project scheduler.",
                        Some(&source_tag),
                        Some(&serde_json::json!({
                            "task_id": task_id.clone(),
                            "project": project_name.clone(),
                        })),
                    )
                    .await;
                scheduler_wake.notify_one();
            }
            Err(e) => warn!(
                project = %project_name,
                task = %task_id,
                error = %e,
                "failed to finalize chat council enrichment"
            ),
        }
    }
}
