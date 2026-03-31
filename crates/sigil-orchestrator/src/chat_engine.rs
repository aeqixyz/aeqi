//! Unified Chat Engine — source-agnostic chat processing for Telegram, web, and future channels.
//!
//! Both Telegram and web chat are thin clients that delegate to this engine.
//! The engine handles: intent detection, conversation history, agent routing,
//! council invocation, task creation, and completion tracking.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use anyhow::Result;

use sigil_core::traits::{Memory, MemoryQuery, MemoryScope};

use crate::agent_router::AgentRouter;
use crate::conversation_store::{ChannelInfo, ConversationMessage, ConversationStore, ThreadEvent};
use crate::registry::ProjectRegistry;

const CHAT_COUNCIL_HOLD_REASON: &str = "awaiting_council";

// ── Types ──

/// Source of a chat message.
#[derive(Debug, Clone)]
pub enum ChatSource {
    Telegram { message_id: i64 },
    Web,
    Discord,
    Slack,
}

impl ChatSource {
    pub fn channel_type(&self) -> &str {
        match self {
            ChatSource::Telegram { .. } => "telegram",
            ChatSource::Web => "web",
            ChatSource::Discord => "discord",
            ChatSource::Slack => "slack",
        }
    }

    pub fn message_id(&self) -> i64 {
        match self {
            ChatSource::Telegram { message_id } => *message_id,
            _ => 0,
        }
    }
}

/// Incoming chat message.
pub struct ChatMessage {
    pub message: String,
    pub chat_id: i64,
    pub sender: String,
    pub source: ChatSource,
    pub project_hint: Option<String>,
    pub department_hint: Option<String>,
    pub channel_name: Option<String>,
    /// Persistent agent UUID for entity memory scoping and routing.
    pub agent_id: Option<String>,
}

impl ChatMessage {
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
pub struct ChatTaskHandle {
    pub task_id: String,
    pub chat_id: i64,
    pub project: String,
}

/// A pending task that's being processed asynchronously.
pub struct PendingChatTask {
    pub project: String,
    pub chat_id: i64,
    pub message_id: i64,
    pub source: ChatSource,
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
    pub source: ChatSource,
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
pub struct ChatEngine {
    pub conversations: Arc<ConversationStore>,
    pub registry: Arc<ProjectRegistry>,
    pub agent_router: Arc<Mutex<AgentRouter>>,
    pub council_advisors: Arc<Vec<sigil_core::config::PeerAgentConfig>>,
    /// If false, only explicit `/council` requests fan out to advisors.
    pub auto_council_enabled: bool,
    pub leader_name: String,
    pub pending_tasks: Arc<Mutex<HashMap<String, PendingChatTask>>>,
    pub task_notify: Arc<tokio::sync::Notify>,
    /// Per-project memory stores for knowledge-aware chat.
    pub memory_stores: HashMap<String, Arc<dyn Memory>>,
    /// LLM-backed intent classifier for ambiguous messages.
    pub intent_classifier: Option<Arc<crate::intent::IntentClassifier>>,
}

impl ChatEngine {
    fn set_scheduler_hold(task: &mut sigil_tasks::Task, hold: bool, reason: Option<&str>) {
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
                "sigil".to_string(),
                serde_json::json!({
                    "hold": true,
                    "hold_reason": reason.unwrap_or(CHAT_COUNCIL_HOLD_REASON),
                }),
            );
        } else if let Some(sigil_meta) = metadata.get_mut("sigil")
            && let Some(obj) = sigil_meta.as_object_mut()
        {
            obj.remove("hold");
            obj.remove("hold_reason");
            if obj.is_empty() {
                metadata.remove("sigil");
            }
        }

        task.metadata = if metadata.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(metadata)
        };
    }

    fn task_completion_reason(task: &sigil_tasks::Task) -> Option<String> {
        match task.status {
            sigil_tasks::TaskStatus::Blocked => task.blocker_context(),
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

    async fn ensure_channel_registered(&self, msg: &ChatMessage) {
        let channel_type = msg.conversation_channel_type();
        let channel_name = msg.conversation_channel_name();
        let _ = self
            .conversations
            .ensure_channel(msg.chat_id, &channel_type, &channel_name)
            .await;
    }

    pub async fn record_exchange(&self, msg: &ChatMessage, reply_text: &str) {
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

    async fn record_response_action_event(&self, msg: &ChatMessage, response: &ChatResponse) {
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
    ) -> Result<sigil_tasks::Task> {
        let project = self
            .registry
            .get_project(project_name)
            .await
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;

        let task = project.create_task(subject).await?;
        let mut store = project.tasks.lock().await;
        let task = store.update(&task.id.0, |entry| {
            if !description.is_empty() {
                entry.description = description.to_string();
            }
            Self::set_scheduler_hold(
                entry,
                hold_for_council,
                hold_for_council.then_some(CHAT_COUNCIL_HOLD_REASON),
            );
        })?;

        info!(
            project = %project_name,
            task = %task.id,
            hold_for_council,
            subject = %subject,
            "chat task created"
        );

        if !hold_for_council {
            self.registry.wake.notify_one();
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

    /// Handle a chat message (quick path): intent detection + status queries.
    /// Returns immediately. For messages that don't match an intent, returns None
    /// to signal the caller should use `handle_message_full` instead.
    ///
    /// Uses keyword fast path first, then LLM classifier for ambiguous messages.
    pub async fn handle_message(&self, msg: &ChatMessage) -> Option<ChatResponse> {
        if msg.message.is_empty() {
            return Some(ChatResponse::error("message is required"));
        }

        // Register channel.
        self.ensure_channel_registered(msg).await;

        let msg_lower = msg.message.to_lowercase();

        // ── Fast path: keyword matching (no API call) ──

        // Intent: create task (explicit prefix).
        if msg_lower.starts_with("create task")
            || msg_lower.starts_with("new task")
            || msg_lower.starts_with("add task")
        {
            let response = self.handle_create_task(msg).await;
            self.record_exchange(msg, &response.context).await;
            self.record_response_action_event(msg, &response).await;
            return Some(response);
        }

        // Intent: close task (explicit prefix).
        if msg_lower.starts_with("close task") || msg_lower.starts_with("done with") {
            let response = self.handle_close_task(msg).await;
            self.record_exchange(msg, &response.context).await;
            self.record_response_action_event(msg, &response).await;
            return Some(response);
        }

        // Intent: blackboard post (explicit prefix).
        if msg_lower.starts_with("note:")
            || msg_lower.starts_with("remember:")
            || msg_lower.starts_with("blackboard:")
        {
            let response = self.handle_blackboard_post(msg).await;
            self.record_exchange(msg, &response.context).await;
            self.record_response_action_event(msg, &response).await;
            return Some(response);
        }

        // ── Slow path: LLM classification for ambiguous messages ──

        if let Some(ref classifier) = self.intent_classifier {
            use crate::intent::ChatIntent;
            let intent = classifier.classify(&msg.message).await;
            match intent {
                ChatIntent::CreateTask => {
                    let response = self.handle_create_task(msg).await;
                    self.record_exchange(msg, &response.context).await;
                    self.record_response_action_event(msg, &response).await;
                    return Some(response);
                }
                ChatIntent::CloseTask => {
                    let response = self.handle_close_task(msg).await;
                    self.record_exchange(msg, &response.context).await;
                    self.record_response_action_event(msg, &response).await;
                    return Some(response);
                }
                ChatIntent::BlackboardPost => {
                    let response = self.handle_blackboard_post(msg).await;
                    self.record_exchange(msg, &response.context).await;
                    self.record_response_action_event(msg, &response).await;
                    return Some(response);
                }
                ChatIntent::StatusQuery => {
                    // Status queries go to full path for comprehensive response.
                    return None;
                }
                ChatIntent::FullPath | ChatIntent::Unknown => {
                    // Complex or ambiguous — proceed to full path.
                    return None;
                }
            }
        }

        // No classifier available — fall through to full path.
        None
    }

    /// Handle a chat message (full path): conversation context + task creation.
    /// Council enrichment, when enabled, is performed asynchronously after the
    /// handle is returned and before the task is released to the scheduler.
    pub async fn handle_message_full(
        &self,
        msg: &ChatMessage,
        phase1_reaction: Option<String>,
    ) -> Result<ChatTaskHandle> {
        let source_tag = msg.conversation_channel_type();
        let scoped_project = msg
            .project_hint
            .clone()
            .unwrap_or_else(|| self.leader_name.clone());

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
            PendingChatTask {
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
            let registry = self.registry.clone();
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

            tokio::spawn(async move {
                ChatEngine::finish_council_enrichment(
                    registry,
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
                )
                .await;
            });
        }

        Ok(ChatTaskHandle {
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

        for (qid, project) in pending {
            let status = {
                if let Some(rig) = self.registry.get_project(&project).await {
                    let store = rig.tasks.lock().await;
                    store
                        .get(&qid)
                        .map(|b| (b.status, Self::task_completion_reason(b)))
                } else {
                    None
                }
            };

            match status {
                Some((sigil_tasks::TaskStatus::Done, reason)) => {
                    if let Some(completion) = self
                        .consume_pending_completion(&qid, CompletionStatus::Done, reason)
                        .await
                    {
                        completions.push(completion);
                    }
                }
                Some((sigil_tasks::TaskStatus::Blocked, reason)) => {
                    if let Some(completion) = self
                        .consume_pending_completion(&qid, CompletionStatus::Blocked, reason)
                        .await
                    {
                        completions.push(completion);
                    }
                }
                Some((sigil_tasks::TaskStatus::Cancelled, reason)) => {
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
    pub async fn get_slow_tasks(&self) -> Vec<(String, i64, i64, ChatSource)> {
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
        let project = {
            let pending = self.pending_tasks.lock().await;
            pending.get(task_id).map(|task| task.project.clone())?
        };
        let status = {
            if let Some(rig) = self.registry.get_project(&project).await {
                let store = rig.tasks.lock().await;
                store.get(task_id).map(|b| (b.status, Self::task_completion_reason(b)))
            } else {
                None
            }
        };

        match status {
            Some((sigil_tasks::TaskStatus::Done, reason)) => {
                self.consume_pending_completion(task_id, CompletionStatus::Done, reason)
                    .await
            }
            Some((sigil_tasks::TaskStatus::Blocked, reason)) => {
                self.consume_pending_completion(task_id, CompletionStatus::Blocked, reason)
                    .await
            }
            Some((sigil_tasks::TaskStatus::Cancelled, reason)) => {
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
    ) -> Result<Vec<ConversationMessage>> {
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
            for (name, mem) in &self.memory_stores {
                let mq = MemoryQuery::new(q, 3).with_scope(MemoryScope::Domain);
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

        let summaries = self.registry.list_project_summaries().await;
        let (spent, budget, remaining) = self.registry.cost_ledger.budget_status();
        let worker_count = self.registry.total_max_workers().await;

        let recent_audit = match &self.registry.audit_log {
            Some(audit) => audit.query_recent(5).unwrap_or_default(),
            None => Vec::new(),
        };

        let project_summaries: Vec<_> = if let Some(p) = project_hint {
            summaries.iter().filter(|s| s.name == p).collect()
        } else {
            summaries.iter().collect()
        };

        let mut context = String::new();

        if let Some(p) = project_hint {
            if let Some(s) = project_summaries.first() {
                context.push_str(&format!(
                    "{}: {} open tasks ({} pending, {} in progress, {} done), {} missions\n",
                    s.name,
                    s.open_tasks,
                    s.pending_tasks,
                    s.in_progress_tasks,
                    s.done_tasks,
                    s.active_missions
                ));
                if let Some(t) = &s.team {
                    context.push_str(&format!(
                        "Team: {} (lead), agents: {}\n",
                        t.leader,
                        t.agents.join(", ")
                    ));
                }
                if !s.departments.is_empty() {
                    context.push_str("Departments:\n");
                    for d in &s.departments {
                        context.push_str(&format!(
                            "  {} — lead: {}, agents: {}\n",
                            d.name,
                            d.lead.as_deref().unwrap_or("-"),
                            d.agents.join(", ")
                        ));
                    }
                }
            } else {
                context.push_str(&format!("Project '{}' not found.\n", p));
            }
        } else {
            for s in &project_summaries {
                context.push_str(&format!(
                    "{}: {} open/{} total tasks, {} missions\n",
                    s.name, s.open_tasks, s.total_tasks, s.active_missions
                ));
            }
        }

        context.push_str(&format!(
            "\nWorkers: {}, Cost: ${:.3}/${:.2}, Remaining: ${:.3}\n",
            worker_count, spent, budget, remaining
        ));

        if !recent_audit.is_empty() {
            context.push_str("\nRecent:\n");
            for e in &recent_audit {
                context.push_str(&format!(
                    "  [{}] {} — {}\n",
                    e.project,
                    e.decision_type,
                    e.reasoning.chars().take(80).collect::<String>()
                ));
            }
        }

        // Prepend memory context if available.
        if let Some(ref mem_ctx) = memory_context {
            context = format!("{}\n\n{}", mem_ctx, context);
        }

        ChatResponse {
            ok: true,
            context: context.trim().to_string(),
            action: None,
            task: None,
            projects: Some(
                project_summaries
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "name": s.name,
                            "open_tasks": s.open_tasks,
                            "total_tasks": s.total_tasks,
                            "active_missions": s.active_missions,
                        })
                    })
                    .collect(),
            ),
            cost: Some(serde_json::json!({
                "spent": spent,
                "budget": budget,
                "remaining": remaining,
            })),
            workers: Some(worker_count),
        }
    }

    /// Search memory for context relevant to a query in a specific project.
    pub async fn build_memory_context(&self, project: &str, query: &str) -> Option<String> {
        let mem = self.memory_stores.get(project)?;
        let mq = MemoryQuery::new(query, 5).with_scope(MemoryScope::Domain);
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
            .memory_stores
            .get(project)
            .ok_or_else(|| anyhow::anyhow!("no memory store for project: {project}"))?;
        let id = mem
            .store(
                key,
                content,
                sigil_core::traits::MemoryCategory::Fact,
                MemoryScope::Domain,
                None,
            )
            .await?;
        Ok(id)
    }

    // ── Private helpers ──

    async fn handle_create_task(&self, msg: &ChatMessage) -> ChatResponse {
        let msg_lower = msg.message.to_lowercase();

        let project = if let Some(p) = &msg.project_hint {
            p.clone()
        } else {
            let mut found = String::new();
            for s in self.registry.list_project_summaries().await {
                if msg_lower.contains(&s.name.to_lowercase()) {
                    found = s.name.clone();
                    break;
                }
            }
            if found.is_empty() {
                self.registry
                    .list_project_summaries()
                    .await
                    .first()
                    .map(|s| s.name.clone())
                    .unwrap_or_default()
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

        match self.registry.assign(&project, &subject, "").await {
            Ok(task) => ChatResponse {
                ok: true,
                context: format!(
                    "Done. Created task {} in {} — \"{}\"",
                    task.id, project, subject
                ),
                action: Some("task_created".to_string()),
                task: Some(serde_json::json!({
                    "id": task.id.0,
                    "subject": task.subject,
                    "project": project,
                })),
                projects: None,
                cost: None,
                workers: None,
            },
            Err(e) => ChatResponse::error(&format!("Failed to create task: {}", e)),
        }
    }

    async fn handle_close_task(&self, msg: &ChatMessage) -> ChatResponse {
        let task_id: String = msg
            .message
            .split_whitespace()
            .find(|w| w.contains('-') && w.chars().any(|c| c.is_ascii_digit()))
            .unwrap_or("")
            .to_string();

        if task_id.is_empty() {
            return ChatResponse::error("I need a task ID to close (e.g., 'close task as-001').");
        }

        for name in self.registry.project_names().await {
            if let Some(board) = self.registry.get_task_board(&name).await {
                let mut board = board.lock().await;
                if board.get(&task_id).is_some() && board.close(&task_id, "closed via chat").is_ok()
                {
                    return ChatResponse {
                        ok: true,
                        context: format!("Done. Task {} is now closed.", task_id),
                        action: Some("task_closed".to_string()),
                        task: None,
                        projects: None,
                        cost: None,
                        workers: None,
                    };
                }
            }
        }

        ChatResponse::error(&format!("Couldn't find task {}.", task_id))
    }

    async fn handle_blackboard_post(&self, msg: &ChatMessage) -> ChatResponse {
        let content = msg
            .message
            .split_once(':')
            .map(|x| x.1)
            .unwrap_or("")
            .trim();
        let project = msg.project_hint.as_deref().unwrap_or("*");
        let key = format!("chat-note-{}", chrono::Utc::now().timestamp());

        // Store to memory (permanent knowledge).
        let memory_result = if project != "*" {
            self.store_note(project, &key, content).await.ok()
        } else {
            None
        };

        // Also store to blackboard (shared ephemeral knowledge).
        match &self.registry.blackboard {
            Some(bb) => {
                match bb.post(
                    &key,
                    content,
                    &self.leader_name,
                    project,
                    &[],
                    crate::blackboard::EntryDurability::Durable,
                ) {
                    Ok(_) => {
                        let stored_where = if memory_result.is_some() {
                            format!("Noted. Stored as knowledge in {}.", project)
                        } else {
                            format!("Noted. Saved to blackboard for {}.", project)
                        };
                        ChatResponse {
                            ok: true,
                            context: stored_where,
                            action: Some("knowledge_stored".to_string()),
                            task: None,
                            projects: None,
                            cost: None,
                            workers: None,
                        }
                    }
                    Err(e) => ChatResponse::error(&format!("Failed to save note: {}", e)),
                }
            }
            None => ChatResponse::error("Blackboard not initialized."),
        }
    }

    #[cfg(test)]
    async fn scoped_advisor_names(
        &self,
        project_hint: Option<&str>,
        department_hint: Option<&str>,
    ) -> Option<HashSet<String>> {
        Self::scoped_advisor_names_with(&self.registry, project_hint, department_hint).await
    }

    async fn classify_advisors_with(
        registry: &Arc<ProjectRegistry>,
        agent_router: &Arc<Mutex<AgentRouter>>,
        council_advisors: &Arc<Vec<sigil_core::config::PeerAgentConfig>>,
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
            Self::scoped_advisor_names_with(registry, project_hint, department_hint).await;
        let advisor_refs: Vec<&sigil_core::config::PeerAgentConfig> = match &scoped_names {
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
        registry: &Arc<ProjectRegistry>,
        project_hint: Option<&str>,
        department_hint: Option<&str>,
    ) -> Option<HashSet<String>> {
        let project_name = project_hint?;
        let summaries = registry.list_project_summaries().await;
        let summary = summaries.into_iter().find(|s| s.name == project_name)?;

        let mut allowed = HashSet::new();

        if let Some(department_name) = department_hint {
            if let Some(department) = summary
                .departments
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(department_name))
            {
                if let Some(lead) = &department.lead {
                    allowed.insert(lead.clone());
                }
                allowed.extend(department.agents.iter().cloned());
            } else {
                warn!(
                    project = %project_name,
                    department = %department_name,
                    "department-scoped chat referenced unknown department, falling back to project team"
                );
            }
        }

        if allowed.is_empty()
            && let Some(team) = summary.team
        {
            allowed.insert(team.leader);
            allowed.extend(team.agents);
        }

        Some(allowed)
    }

    async fn gather_council_input_with(
        registry: Arc<ProjectRegistry>,
        conversations: Arc<ConversationStore>,
        advisors: &[String],
        clean_text: &str,
        conv_context: &str,
        chat_id: i64,
        source_tag: &str,
    ) -> Vec<(String, String)> {
        info!(advisors = ?advisors, "invoking council advisors");

        let mut handles = Vec::new();
        for advisor_name in advisors {
            let project_name = advisor_name.clone();
            let adv_name = advisor_name.clone();
            let adv_msg = clean_text.to_string();
            let adv_history = conv_context.to_string();
            let reg = registry.clone();

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

                let task_id = match reg.assign(&project_name, &task_subject, &task_desc).await {
                    Ok(b) => b.id.0.clone(),
                    Err(e) => {
                        warn!(agent = %adv_name, error = %e, "failed to create advisor task");
                        return None;
                    }
                };

                let notify = reg
                    .get_project(&project_name)
                    .await
                    .map(|d| d.task_notify.clone());
                let timeout = tokio::time::sleep(std::time::Duration::from_secs(60));
                tokio::pin!(timeout);
                loop {
                    tokio::select! {
                        _ = async {
                            match &notify {
                                Some(n) => n.notified().await,
                                None => std::future::pending::<()>().await,
                            }
                        } => {}
                        _ = &mut timeout => {
                            warn!(agent = %adv_name, "advisor task timed out");
                            return None;
                        }
                    }
                    let done = {
                        if let Some(rig) = reg.get_project(&project_name).await {
                            let store = rig.tasks.lock().await;
                            store.get(&task_id).map(|b| {
                                (
                                    b.status == sigil_tasks::TaskStatus::Done,
                                    b.outcome_summary(),
                                )
                            })
                        } else {
                            None
                        }
                    };
                    if let Some((true, reason)) = done {
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
        registry: Arc<ProjectRegistry>,
        conversations: Arc<ConversationStore>,
        agent_router: Arc<Mutex<AgentRouter>>,
        council_advisors: Arc<Vec<sigil_core::config::PeerAgentConfig>>,
        task_id: String,
        project_name: String,
        clean_text: String,
        is_council: bool,
        conv_context: String,
        chat_id: i64,
        source_tag: String,
        project_hint: Option<String>,
        department_hint: Option<String>,
    ) {
        let advisors_to_invoke = Self::classify_advisors_with(
            &registry,
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
                registry.clone(),
                conversations.clone(),
                &advisors_to_invoke,
                &clean_text,
                &conv_context,
                chat_id,
                &source_tag,
            )
            .await
        };

        let Some(project) = registry.get_project(&project_name).await else {
            warn!(
                project = %project_name,
                task = %task_id,
                "chat council enrichment could not find project"
            );
            return;
        };

        let update_result = {
            let mut store = project.tasks.lock().await;
            store.update(&task_id, |task| {
                Self::append_council_input(&mut task.description, &council_input);
                Self::set_scheduler_hold(task, false, None);
            })
        };

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
                registry.wake.notify_one()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Supervisor;
    use crate::message::DispatchBus;
    use crate::project::Project;
    use crate::registry::ProjectRegistry;
    use anyhow::Result;
    use async_trait::async_trait;
    use chrono::Utc;
    use sigil_core::config::{
        AgentRole, DepartmentConfig, ExecutionMode, PeerAgentConfig, ProjectConfig,
        ProjectTeamConfig,
    };
    use sigil_core::traits::{
        ChatRequest, ChatResponse as ProviderChatResponse, Provider, StopReason, Usage,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn temp_project(name: &str, prefix: &str) -> anyhow::Result<(Arc<Project>, TempDir)> {
        let dir = TempDir::new()?;
        std::fs::create_dir_all(dir.path().join(".tasks"))?;
        let config = ProjectConfig {
            name: name.to_string(),
            prefix: prefix.to_string(),
            repo: dir.path().display().to_string(),
            model: Some("test-model".to_string()),
            runtime: None,
            max_workers: 1,
            worktree_root: None,
            execution_mode: ExecutionMode::Agent,
            max_turns: Some(1),
            max_budget_usd: None,
            worker_timeout_secs: 60,
            max_cost_per_day_usd: None,
            team: None,
            orchestrator: None,
            missions: Vec::new(),
            departments: Vec::new(),
        };
        let project = Project::from_config(&config, dir.path(), "test-model")?;
        Ok((Arc::new(project), dir))
    }

    async fn test_engine() -> (
        ChatEngine,
        Arc<Project>,
        Arc<ProjectRegistry>,
        TempDir,
        TempDir,
        PathBuf,
    ) {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let registry = Arc::new(ProjectRegistry::new(
            dispatch_bus.clone(),
            "leader".to_string(),
        ));
        let (project, project_dir) = temp_project("leader", "ld").unwrap();
        registry.register_project_only(project.clone()).await;

        let conv_dir = TempDir::new().unwrap();
        let conv_path = conv_dir.path().join("conv.db");
        let conversations = Arc::new(ConversationStore::open(&conv_path).unwrap());

        let engine = ChatEngine {
            conversations,
            registry: registry.clone(),
            agent_router: Arc::new(Mutex::new(AgentRouter::new(String::new(), 0))),
            council_advisors: Arc::new(Vec::new()),
            auto_council_enabled: true,
            leader_name: "leader".to_string(),
            pending_tasks: Arc::new(Mutex::new(HashMap::new())),
            task_notify: Arc::new(tokio::sync::Notify::new()),
            memory_stores: HashMap::new(),
            intent_classifier: None,
        };

        (engine, project, registry, project_dir, conv_dir, conv_path)
    }

    struct DoneProvider;

    #[async_trait]
    impl Provider for DoneProvider {
        async fn chat(&self, _request: &ChatRequest) -> Result<ProviderChatResponse> {
            Ok(ProviderChatResponse {
                content: Some("DONE: fixed".to_string()),
                tool_calls: Vec::new(),
                usage: Usage::default(),
                stop_reason: StopReason::EndTurn,
            })
        }

        fn name(&self) -> &str {
            "done-provider"
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn quick_path_records_user_and_reply() {
        let (engine, _project, _registry, _project_dir, _conv_dir, _conv_path) =
            test_engine().await;
        let msg = ChatMessage {
            message: "create task review the patrol loop".to_string(),
            chat_id: 7,
            sender: "alice".to_string(),
            source: ChatSource::Web,
            project_hint: None,
            department_hint: None,
            channel_name: None,
            agent_id: None,
        };

        let response = engine.handle_message(&msg).await.unwrap();
        assert!(response.ok);

        let history = engine.get_history(7, 10, 0).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "User");
        assert_eq!(history[0].content, msg.message);
        assert_eq!(history[1].role, "leader");
        assert_eq!(history[1].content, response.context);
    }

    #[tokio::test]
    async fn poll_completion_records_reply_in_history() {
        let (engine, project, _registry, _project_dir, _conv_dir, _conv_path) = test_engine().await;
        let msg = ChatMessage {
            message: "Give me a deployment update".to_string(),
            chat_id: 11,
            sender: "alice".to_string(),
            source: ChatSource::Web,
            project_hint: None,
            department_hint: None,
            channel_name: None,
            agent_id: None,
        };

        let handle = engine.handle_message_full(&msg, None).await.unwrap();
        {
            let mut store = project.tasks.lock().await;
            store.close(&handle.task_id, "All green.").unwrap();
        }

        let completion = engine.poll_completion(&handle.task_id).await.unwrap();
        assert_eq!(completion.text, "All green.");

        let history = engine.get_history(11, 10, 0).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "User");
        assert_eq!(history[0].content, msg.message);
        assert_eq!(history[1].role, "leader");
        assert_eq!(history[1].content, "All green.");
    }

    #[tokio::test]
    async fn full_path_records_timeline_lifecycle_events() {
        let (engine, project, _registry, _project_dir, _conv_dir, _conv_path) = test_engine().await;
        let msg = ChatMessage {
            message: "Investigate the patrol loop".to_string(),
            chat_id: 12,
            sender: "alice".to_string(),
            source: ChatSource::Web,
            project_hint: None,
            department_hint: None,
            channel_name: None,
            agent_id: None,
        };

        let handle = engine.handle_message_full(&msg, None).await.unwrap();

        let timeline = engine.get_timeline(12, 10, 0).await.unwrap();
        let event_types: Vec<&str> = timeline
            .iter()
            .map(|event| event.event_type.as_str())
            .collect();
        assert_eq!(
            event_types,
            vec!["message", "task_created", "task_released"]
        );
        assert_eq!(timeline[0].role, "User");
        assert_eq!(timeline[0].content, msg.message);
        assert_eq!(timeline[1].role, "system");
        assert_eq!(
            timeline[1]
                .metadata
                .as_ref()
                .and_then(|m| m.get("task_id"))
                .and_then(|v| v.as_str()),
            Some(handle.task_id.as_str())
        );

        project
            .tasks
            .lock()
            .await
            .close(&handle.task_id, "Patrol loop fixed.")
            .unwrap();

        let completion = engine.poll_completion(&handle.task_id).await.unwrap();
        assert_eq!(completion.text, "Patrol loop fixed.");

        let timeline = engine.get_timeline(12, 10, 0).await.unwrap();
        let event_types: Vec<&str> = timeline
            .iter()
            .map(|event| event.event_type.as_str())
            .collect();
        assert_eq!(
            event_types,
            vec![
                "message",
                "task_created",
                "task_released",
                "task_completed",
                "message",
            ]
        );
        assert_eq!(timeline[3].role, "system");
        assert_eq!(timeline[4].role, "leader");
        assert_eq!(timeline[4].content, "Patrol loop fixed.");
        assert_eq!(
            timeline[3]
                .metadata
                .as_ref()
                .and_then(|m| m.get("task_id"))
                .and_then(|v| v.as_str()),
            Some(handle.task_id.as_str())
        );
    }

    #[tokio::test]
    async fn full_path_preserves_older_history() {
        let (engine, _project, _registry, _project_dir, _conv_dir, conv_path) = test_engine().await;
        engine
            .conversations
            .record_with_source(21, "User", "Earlier context", Some("web"))
            .await
            .unwrap();

        let conn = rusqlite::Connection::open(&conv_path).unwrap();
        conn.execute(
            "UPDATE conversations SET timestamp = ?1 WHERE chat_id = ?2",
            rusqlite::params![(Utc::now() - chrono::TimeDelta::hours(3)).to_rfc3339(), 21],
        )
        .unwrap();

        let msg = ChatMessage {
            message: "What should we do next?".to_string(),
            chat_id: 21,
            sender: "alice".to_string(),
            source: ChatSource::Web,
            project_hint: None,
            department_hint: None,
            channel_name: None,
            agent_id: None,
        };

        let _handle = engine.handle_message_full(&msg, None).await.unwrap();

        let history = engine.get_history(21, 10, 0).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Earlier context");
        assert_eq!(history[1].content, msg.message);
    }

    #[tokio::test]
    async fn full_path_routes_scoped_chat_to_project_and_completes_there() {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let registry = Arc::new(ProjectRegistry::new(dispatch_bus, "leader".to_string()));
        let (leader_project, _leader_dir) = temp_project("leader", "ld").unwrap();
        let (app_project, _app_dir) = temp_project("app", "ap").unwrap();
        registry.register_project_only(leader_project.clone()).await;
        registry.register_project_only(app_project.clone()).await;

        let conv_dir = TempDir::new().unwrap();
        let conv_path = conv_dir.path().join("conv.db");
        let conversations = Arc::new(ConversationStore::open(&conv_path).unwrap());

        let engine = ChatEngine {
            conversations,
            registry: registry.clone(),
            agent_router: Arc::new(Mutex::new(AgentRouter::new(String::new(), 0))),
            council_advisors: Arc::new(Vec::new()),
            auto_council_enabled: true,
            leader_name: "leader".to_string(),
            pending_tasks: Arc::new(Mutex::new(HashMap::new())),
            task_notify: Arc::new(tokio::sync::Notify::new()),
            memory_stores: HashMap::new(),
            intent_classifier: None,
        };

        let msg = ChatMessage {
            message: "Ship the release checklist".to_string(),
            chat_id: 55,
            sender: "alice".to_string(),
            source: ChatSource::Web,
            project_hint: Some("app".to_string()),
            department_hint: None,
            channel_name: Some("app-ops".to_string()),
            agent_id: None,
        };

        let handle = engine.handle_message_full(&msg, None).await.unwrap();
        assert_eq!(handle.project, "app");
        assert!(
            app_project
                .tasks
                .lock()
                .await
                .get(&handle.task_id)
                .is_some()
        );
        assert!(
            leader_project
                .tasks
                .lock()
                .await
                .get(&handle.task_id)
                .is_none()
        );

        app_project
            .tasks
            .lock()
            .await
            .close(&handle.task_id, "Release checklist completed.")
            .unwrap();

        let completion = engine.poll_completion(&handle.task_id).await.unwrap();
        assert_eq!(completion.text, "Release checklist completed.");
    }

    #[tokio::test]
    async fn create_chat_task_hold_blocks_scheduler_until_released() {
        let (engine, project, _registry, _project_dir, _conv_dir, _conv_path) = test_engine().await;

        let task = engine
            .create_chat_task("leader", "[web] alice (77)", "Draft the answer", true)
            .await
            .unwrap();

        {
            let store = project.tasks.lock().await;
            let held = store.get(&task.id.0).unwrap();
            assert!(held.is_scheduler_held());
            assert!(store.ready().is_empty());
        }

        {
            let mut store = project.tasks.lock().await;
            store
                .update(&task.id.0, |entry| {
                    ChatEngine::set_scheduler_hold(entry, false, None);
                })
                .unwrap();
            let ready = store.ready();
            assert_eq!(ready.len(), 1);
            assert_eq!(ready[0].id, task.id);
        }
    }

    #[tokio::test]
    async fn scoped_advisor_names_follow_department_before_project_team() {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let registry = Arc::new(ProjectRegistry::new(
            dispatch_bus.clone(),
            "leader".to_string(),
        ));

        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".tasks")).unwrap();
        let config = ProjectConfig {
            name: "app".to_string(),
            prefix: "ap".to_string(),
            repo: dir.path().display().to_string(),
            model: Some("test-model".to_string()),
            runtime: None,
            max_workers: 1,
            worktree_root: None,
            execution_mode: ExecutionMode::Agent,
            max_turns: Some(1),
            max_budget_usd: None,
            worker_timeout_secs: 60,
            max_cost_per_day_usd: None,
            team: Some(ProjectTeamConfig {
                org: None,
                unit: None,
                leader: "leader".to_string(),
                agents: vec!["researcher".to_string(), "reviewer".to_string()],
            }),
            orchestrator: None,
            missions: Vec::new(),
            departments: vec![DepartmentConfig {
                name: "backend".to_string(),
                lead: Some("reviewer".to_string()),
                agents: vec!["reviewer".to_string()],
                description: None,
            }],
        };
        let project = Arc::new(Project::from_config(&config, dir.path(), "test-model").unwrap());
        let provider: Arc<dyn Provider> = Arc::new(DoneProvider);
        let mut supervisor = Supervisor::new(&project, provider, Vec::new(), dispatch_bus.clone());
        supervisor.execution_mode = sigil_core::ExecutionMode::Agent;
        supervisor.set_team(
            ProjectTeamConfig {
                org: None,
                unit: None,
                leader: "leader".to_string(),
                agents: vec!["researcher".to_string(), "reviewer".to_string()],
            },
            "leader",
        );
        registry.register_project(project, supervisor).await;

        let conv_dir = TempDir::new().unwrap();
        let conv_path = conv_dir.path().join("conv.db");
        let conversations = Arc::new(ConversationStore::open(&conv_path).unwrap());

        let engine = ChatEngine {
            conversations,
            registry,
            agent_router: Arc::new(Mutex::new(AgentRouter::new(String::new(), 0))),
            council_advisors: Arc::new(vec![
                PeerAgentConfig {
                    name: "researcher".to_string(),
                    prefix: "rs".to_string(),
                    model: None,
                    runtime: None,
                    role: AgentRole::Advisor,
                    voice: Default::default(),
                    execution_mode: ExecutionMode::Agent,
                    max_workers: 1,
                    max_turns: None,
                    max_budget_usd: None,
                    default_repo: None,
                    expertise: vec!["research".to_string()],
                    capabilities: Vec::new(),
                    telegram_token_secret: None,
                },
                PeerAgentConfig {
                    name: "reviewer".to_string(),
                    prefix: "rv".to_string(),
                    model: None,
                    runtime: None,
                    role: AgentRole::Advisor,
                    voice: Default::default(),
                    execution_mode: ExecutionMode::Agent,
                    max_workers: 1,
                    max_turns: None,
                    max_budget_usd: None,
                    default_repo: None,
                    expertise: vec!["review".to_string()],
                    capabilities: Vec::new(),
                    telegram_token_secret: None,
                },
                PeerAgentConfig {
                    name: "outsider".to_string(),
                    prefix: "ot".to_string(),
                    model: None,
                    runtime: None,
                    role: AgentRole::Advisor,
                    voice: Default::default(),
                    execution_mode: ExecutionMode::Agent,
                    max_workers: 1,
                    max_turns: None,
                    max_budget_usd: None,
                    default_repo: None,
                    expertise: vec!["ops".to_string()],
                    capabilities: Vec::new(),
                    telegram_token_secret: None,
                },
            ]),
            auto_council_enabled: true,
            leader_name: "leader".to_string(),
            pending_tasks: Arc::new(Mutex::new(HashMap::new())),
            task_notify: Arc::new(tokio::sync::Notify::new()),
            memory_stores: HashMap::new(),
            intent_classifier: None,
        };

        let project_scoped = engine
            .scoped_advisor_names(Some("app"), None)
            .await
            .unwrap();
        assert!(project_scoped.contains("researcher"));
        assert!(project_scoped.contains("reviewer"));
        assert!(!project_scoped.contains("outsider"));

        let department_scoped = engine
            .scoped_advisor_names(Some("app"), Some("backend"))
            .await
            .unwrap();
        assert!(department_scoped.contains("reviewer"));
        assert!(!department_scoped.contains("researcher"));
        assert!(!department_scoped.contains("outsider"));
    }

    #[tokio::test]
    async fn auto_council_can_be_disabled_without_breaking_explicit_council() {
        let (mut engine, project, _registry, _project_dir, _conv_dir, _conv_path) =
            test_engine().await;
        engine.council_advisors = Arc::new(vec![PeerAgentConfig {
            name: "reviewer".to_string(),
            prefix: "rv".to_string(),
            model: None,
            runtime: None,
            role: AgentRole::Advisor,
            voice: Default::default(),
            execution_mode: ExecutionMode::Agent,
            max_workers: 1,
            max_turns: None,
            max_budget_usd: None,
            default_repo: None,
            expertise: vec!["review".to_string()],
            capabilities: Vec::new(),
            telegram_token_secret: None,
        }]);
        engine.auto_council_enabled = false;

        let normal = ChatMessage {
            message: "check the chat tests".to_string(),
            chat_id: 88,
            sender: "alice".to_string(),
            source: ChatSource::Web,
            project_hint: None,
            department_hint: None,
            channel_name: None,
            agent_id: None,
        };
        let handle = engine.handle_message_full(&normal, None).await.unwrap();
        let released = engine.get_timeline(88, 10, 0).await.unwrap();
        assert!(released.iter().any(|e| e.event_type == "task_released"));
        assert!(!released.iter().any(|e| e.event_type == "council_pending"));
        {
            let store = project.tasks.lock().await;
            let stored = store.get(&handle.task_id).unwrap();
            assert!(!stored.is_scheduler_held());
        }

        let explicit = ChatMessage {
            message: "/council check the chat tests".to_string(),
            chat_id: 89,
            sender: "alice".to_string(),
            source: ChatSource::Web,
            project_hint: None,
            department_hint: None,
            channel_name: None,
            agent_id: None,
        };
        let explicit_handle = engine.handle_message_full(&explicit, None).await.unwrap();
        let held = engine.get_timeline(89, 10, 0).await.unwrap();
        assert!(held.iter().any(|e| e.event_type == "council_pending"));
        {
            let store = project.tasks.lock().await;
            let stored = store.get(&explicit_handle.task_id).unwrap();
            assert!(stored.is_scheduler_held());
        }
    }
}
