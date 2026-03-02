use anyhow::Result;
use chrono::Utc;
use system_tasks::{Checkpoint, Task, TaskStatus};
use system_core::traits::{
    ChatRequest, LogObserver, Memory, MemoryCategory, MemoryScope, Message, MessageContent,
    Observer, Provider, Role, Tool,
};
use system_core::{Agent, AgentConfig, Identity};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use crate::checkpoint::AgentCheckpoint;
use crate::executor::{ClaudeCodeExecutor, TaskOutcome};
use crate::hook::Hook;
use crate::message::{Dispatch, DispatchBus, DispatchKind};

/// Worker states.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerState {
    Idle,
    Hooked,
    Working,
    Done,
    Failed(String),
}

/// How a worker executes its assigned bead.
pub enum WorkerExecution {
    /// Internal Agent loop (current behavior): LLM API calls with basic tools.
    Agent {
        provider: Arc<dyn system_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        model: String,
    },
    /// Claude Code CLI subprocess: full Edit, Grep, Glob, context compression.
    ClaudeCode(ClaudeCodeExecutor),
}

/// An AgentWorker is an ephemeral task executor. Each worker runs as a tokio task
/// with its own identity, hook, and tool allowlist.
pub struct AgentWorker {
    pub name: String,
    pub project_name: String,
    pub state: WorkerState,
    pub hook: Option<Hook>,
    pub execution: WorkerExecution,
    pub identity: Identity,
    pub dispatch_bus: Arc<DispatchBus>,
    pub tasks: Arc<Mutex<system_tasks::TaskBoard>>,
    /// Fired when a quest is closed so waiters don't need to poll.
    pub task_notify: Arc<Notify>,
    pub memory: Option<Arc<dyn Memory>>,
    pub reflect_provider: Option<Arc<dyn Provider>>,
    pub reflect_model: String,
    /// Project directory path for checkpoint storage.
    pub project_dir: Option<PathBuf>,
}

impl AgentWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        project_name: String,
        provider: Arc<dyn system_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        identity: Identity,
        model: String,
        dispatch_bus: Arc<DispatchBus>,
        tasks: Arc<Mutex<system_tasks::TaskBoard>>,
        task_notify: Arc<Notify>,
    ) -> Self {
        let reflect_model = model.clone();
        Self {
            name,
            project_name,
            state: WorkerState::Idle,
            hook: None,
            execution: WorkerExecution::Agent { provider, tools, model },
            identity,
            dispatch_bus,
            tasks,
            task_notify,
            memory: None,
            reflect_provider: None,
            reflect_model,
            project_dir: None,
        }
    }

    pub fn new_claude_code(
        name: String,
        project_name: String,
        executor: ClaudeCodeExecutor,
        identity: Identity,
        dispatch_bus: Arc<DispatchBus>,
        tasks: Arc<Mutex<system_tasks::TaskBoard>>,
        task_notify: Arc<Notify>,
    ) -> Self {
        let project_dir = Some(executor.workdir().to_path_buf());
        Self {
            name,
            project_name,
            state: WorkerState::Idle,
            hook: None,
            execution: WorkerExecution::ClaudeCode(executor),
            identity,
            dispatch_bus,
            tasks,
            task_notify,
            memory: None,
            reflect_provider: None,
            reflect_model: String::new(),
            project_dir,
        }
    }

    pub fn with_memory(mut self, memory: Arc<dyn Memory>) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn with_reflect(mut self, provider: Arc<dyn Provider>, model: String) -> Self {
        self.reflect_provider = Some(provider);
        self.reflect_model = model;
        self
    }

    pub fn with_project_dir(mut self, project_dir: PathBuf) -> Self {
        self.project_dir = Some(project_dir);
        self
    }

    /// Get the working directory for this spirit (from executor or project_dir).
    fn workdir(&self) -> Option<&std::path::Path> {
        match &self.execution {
            WorkerExecution::ClaudeCode(executor) => Some(executor.workdir()),
            WorkerExecution::Agent { .. } => self.project_dir.as_deref(),
        }
    }

    /// Capture an external checkpoint by inspecting git state in the spirit's workdir.
    /// Saves the checkpoint to the project's `.sigil/checkpoints/` directory.
    fn capture_and_save_checkpoint(&self, task_id: &str, progress_notes: Option<&str>) {
        let Some(workdir) = self.workdir() else {
            debug!(worker = %self.name, "no workdir — skipping checkpoint capture");
            return;
        };

        let project_dir = self.project_dir.as_deref().unwrap_or(workdir);

        match AgentCheckpoint::capture(workdir) {
            Ok(checkpoint) => {
                let checkpoint: AgentCheckpoint = checkpoint
                    .with_quest_id(task_id)
                    .with_worker_name(&self.name);

                let checkpoint = if let Some(notes) = progress_notes {
                    checkpoint.with_progress_notes(notes)
                } else {
                    checkpoint
                };

                let cp_path = AgentCheckpoint::path_for_quest(project_dir, task_id);
                if let Err(e) = checkpoint.write(&cp_path) {
                    warn!(
                        worker = %self.name,
                        task = %task_id,
                        error = %e,
                        "failed to write checkpoint"
                    );
                } else {
                    info!(
                        worker = %self.name,
                        task = %task_id,
                        files = checkpoint.modified_files.len(),
                        "external checkpoint captured"
                    );
                }
            }
            Err(e) => {
                warn!(
                    worker = %self.name,
                    task = %task_id,
                    error = %e,
                    "failed to capture git checkpoint"
                );
            }
        }
    }

    /// Assign a bead to this worker (set hook).
    pub fn assign(&mut self, bead: &Task) {
        self.hook = Some(Hook::new(bead.id.clone(), bead.subject.clone()));
        self.state = WorkerState::Hooked;
    }

    /// Save a checkpoint recording this spirit's progress on a quest.
    async fn save_checkpoint(&self, task_id: &str, progress: &str, cost: f64, turns: u32) {
        let mut store = self.tasks.lock().await;
        if let Err(e) = store.update(task_id, |q| {
            q.checkpoints.push(Checkpoint {
                timestamp: Utc::now(),
                worker: self.name.clone(),
                progress: progress.to_string(),
                cost_usd: cost,
                turns_used: turns,
            });
        }) {
            warn!(task_id, error = %e, "failed to save checkpoint to quest store");
        }
    }

    /// Execute the hooked work. Dispatches to Agent or Claude Code based on execution mode.
    /// Returns (outcome, cost_usd, turns_used) for the Supervisor to record.
    pub async fn execute(&mut self) -> Result<(TaskOutcome, f64, u32)> {
        let hook = match &self.hook {
            Some(h) => h.clone(),
            None => {
                warn!(worker = %self.name, "no hook assigned, nothing to do");
                return Ok((TaskOutcome::Done("no work assigned".to_string()), 0.0, 0));
            }
        };

        info!(
            worker = %self.name,
            bead = %hook.task_id,
            subject = %hook.subject,
            mode = match &self.execution {
                WorkerExecution::Agent { .. } => "agent",
                WorkerExecution::ClaudeCode(_) => "claude_code",
            },
            "starting work"
        );

        self.state = WorkerState::Working;

        // Mark bead as in_progress.
        {
            let mut store = self.tasks.lock().await;
            if let Err(e) = store.update(&hook.task_id.0, |b| {
                b.status = TaskStatus::InProgress;
                b.assignee = Some(self.name.clone());
            }) {
                warn!(bead = %hook.task_id, error = %e, "failed to mark quest in_progress");
            }
        }

        // Build the prompt from the bead (including any previous checkpoints).
        let quest_context = {
            let store = self.tasks.lock().await;
            match store.get(&hook.task_id.0) {
                Some(b) => {
                    let mut ctx = format!("## Task: {}\n\n", b.subject);
                    if !b.description.is_empty() {
                        ctx.push_str(&format!("{}\n\n", b.description));
                    }
                    ctx.push_str(&format!("Task ID: {}\nPriority: {}\n", b.id, b.priority));

                    // Include budgeted checkpoints from previous attempts.
                    if !b.checkpoints.is_empty() {
                        let budget = crate::context_budget::ContextBudget::default();
                        ctx.push_str(&budget.budget_checkpoints(&b.checkpoints));
                        ctx.push_str("Review the above before starting. Skip work that's already done.\n\n");
                    }

                    // Include acceptance criteria if defined.
                    if let Some(ref criteria) = b.acceptance_criteria {
                        ctx.push_str(&format!(
                            "\n## Acceptance Criteria\n\n{}\n\n\
                             Verify your work meets these criteria before marking as DONE.\n\n",
                            criteria
                        ));
                    }

                    ctx
                }
                None => format!("Task: {}", hook.subject),
            }
        };

        // Inject recalled memories into quest context for richer execution.
        let quest_context = if let Some(ref mem) = self.memory {
            let query = system_core::traits::MemoryQuery::new(&quest_context, 5)
                .with_scope(system_core::traits::MemoryScope::Domain);
            match mem.search(&query).await {
                Ok(entries) if !entries.is_empty() => {
                    let ctx = entries
                        .iter()
                        .map(|e| format!("[{}] {}: {}", e.scope, e.key, e.content))
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!("{quest_context}\n## Recalled Memory\n{ctx}\n")
                }
                _ => quest_context,
            }
        } else {
            quest_context
        };

        // Enrich identity with dynamic memory recall for richer system prompts.
        let enriched_identity = if let Some(ref mem) = self.memory {
            let query = system_core::traits::MemoryQuery::new(&quest_context, 10)
                .with_scope(MemoryScope::Domain);
            match mem.search(&query).await {
                Ok(entries) if !entries.is_empty() => {
                    let mut id = self.identity.clone();
                    let dynamic = entries
                        .iter()
                        .map(|e| format!("- [{}] {}: {}", e.scope, e.key, e.content))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let existing = id.memory.unwrap_or_default();
                    id.memory = Some(format!("{existing}\n\n## Dynamic Recall\n{dynamic}"));
                    id
                }
                _ => self.identity.clone(),
            }
        } else {
            self.identity.clone()
        };

        // Dispatch based on execution mode. Returns (text, cost_usd, turns_used).
        let raw_result = match &self.execution {
            WorkerExecution::Agent { provider, tools, model } => {
                self.execute_agent(provider.clone(), tools.clone(), model, &quest_context, &enriched_identity)
                    .await
                    .map(|agent_result| {
                        let cost = system_providers::estimate_cost(
                            &agent_result.model,
                            agent_result.total_prompt_tokens,
                            agent_result.total_completion_tokens,
                        );
                        info!(
                            worker = %self.name,
                            model = %agent_result.model,
                            prompt_tokens = agent_result.total_prompt_tokens,
                            completion_tokens = agent_result.total_completion_tokens,
                            cost_usd = cost,
                            iterations = agent_result.iterations,
                            "agent execution cost calculated"
                        );
                        (agent_result.text, cost, agent_result.iterations)
                    })
            }
            WorkerExecution::ClaudeCode(executor) => {
                self.execute_claude_code(executor, &quest_context, &enriched_identity).await
            }
        };

        // Parse into structured outcome.
        let (outcome, cost, turns) = match raw_result {
            Ok((result_text, cost, turns)) => (TaskOutcome::parse(&result_text), cost, turns),
            Err(e) => (TaskOutcome::Failed(e.to_string()), 0.0, 0),
        };

        // Process outcome: save checkpoint, update bead status, notify scout.
        match &outcome {
            TaskOutcome::Done(result_text) => {
                info!(worker = %self.name, bead = %hook.task_id, "work completed");
                // Capture external checkpoint from git state before recording completion.
                self.capture_and_save_checkpoint(
                    &hook.task_id.0,
                    Some(&format!("DONE: {}", result_text)),
                );
                self.save_checkpoint(
                    &hook.task_id.0,
                    &format!("DONE: {}", result_text),
                    cost,
                    turns,
                ).await;
                {
                    let mut store = self.tasks.lock().await;
                    let _ = store.close(&hook.task_id.0, result_text);
                }
                self.task_notify.notify_waiters();
                self.dispatch_bus
                    .send(Dispatch::new_typed(
                        &self.name,
                        &format!("witness-{}", self.project_name),
                        DispatchKind::QuestDone {
                            task_id: hook.task_id.to_string(),
                            summary: format!("{}: {}", hook.subject, result_text),
                        },
                    ))
                    .await;
                self.state = WorkerState::Done;
            }

            TaskOutcome::Blocked { question, full_text } => {
                info!(
                    worker = %self.name,
                    bead = %hook.task_id,
                    question = %question,
                    "worker blocked — needs input"
                );
                // Capture external checkpoint from git state before recording block.
                self.capture_and_save_checkpoint(
                    &hook.task_id.0,
                    Some(&format!("BLOCKED: {}\n\nWork so far:\n{}", question, full_text)),
                );
                self.save_checkpoint(
                    &hook.task_id.0,
                    &format!("BLOCKED on: {}\n\nWork done so far:\n{}", question, full_text),
                    cost,
                    turns,
                ).await;
                // Mark bead as Blocked and preserve the question for Supervisor resolution.
                {
                    let mut store = self.tasks.lock().await;
                    if let Err(e) = store.update(&hook.task_id.0, |b| {
                        b.status = TaskStatus::Blocked;
                        b.assignee = None;
                        b.closed_reason = Some(question.clone());
                    }) {
                        warn!(bead = %hook.task_id, error = %e, "failed to mark quest blocked");
                    }
                }
                self.task_notify.notify_waiters();
                self.dispatch_bus
                    .send(Dispatch::new_typed(
                        &self.name,
                        &format!("witness-{}", self.project_name),
                        DispatchKind::QuestBlocked {
                            task_id: hook.task_id.to_string(),
                            question: question.clone(),
                            context: full_text.clone(),
                        },
                    ))
                    .await;
                self.state = WorkerState::Done; // Spirit is done; bead is blocked.
            }

            TaskOutcome::Handoff { checkpoint } => {
                info!(worker = %self.name, bead = %hook.task_id, "spirit handing off — context exhaustion");
                // Capture external checkpoint from git state before recording handoff.
                self.capture_and_save_checkpoint(
                    &hook.task_id.0,
                    Some(&format!("HANDOFF: {}", checkpoint)),
                );
                self.save_checkpoint(
                    &hook.task_id.0,
                    &format!("HANDOFF: {}", checkpoint),
                    cost,
                    turns,
                ).await;
                {
                    let mut store = self.tasks.lock().await;
                    if let Err(e) = store.update(&hook.task_id.0, |b| {
                        b.retry_count += 1;
                        if b.retry_count >= 3 {
                            b.status = TaskStatus::Cancelled;
                            b.assignee = None;
                            b.closed_reason = Some(format!(
                                "Auto-cancelled after {} retries (handoff). Last: {}",
                                b.retry_count, checkpoint
                            ));
                        } else {
                            b.status = TaskStatus::Pending;
                            b.assignee = None;
                        }
                    }) {
                        warn!(bead = %hook.task_id, error = %e, "failed to re-queue quest after handoff");
                    }
                }
                // Notify focal agent if auto-cancelled.
                {
                    let store = self.tasks.lock().await;
                    if let Some(b) = store.get(&hook.task_id.0)
                        && b.status == TaskStatus::Cancelled
                    {
                        warn!(worker = %self.name, bead = %hook.task_id, retries = b.retry_count, "quest auto-cancelled after max retries");
                        self.dispatch_bus
                            .send(Dispatch::new_typed(
                                &self.name,
                                &format!("witness-{}", self.project_name),
                                DispatchKind::QuestFailed {
                                    task_id: hook.task_id.to_string(),
                                    error: format!("Auto-cancelled after {} retries (repeated handoff)", b.retry_count),
                                },
                            ))
                            .await;
                    }
                }
                self.task_notify.notify_waiters();
                self.dispatch_bus
                    .send(Dispatch::new_typed(
                        &self.name,
                        &format!("witness-{}", self.project_name),
                        DispatchKind::QuestBlocked {
                            task_id: hook.task_id.to_string(),
                            question: "Context exhaustion handoff — re-queued automatically".to_string(),
                            context: checkpoint.clone(),
                        },
                    ))
                    .await;
                self.state = WorkerState::Done;
            }

            TaskOutcome::Failed(error_text) => {
                warn!(worker = %self.name, bead = %hook.task_id, "work failed");
                // Capture external checkpoint from git state before recording failure.
                self.capture_and_save_checkpoint(
                    &hook.task_id.0,
                    Some(&format!("FAILED: {}", error_text)),
                );
                self.save_checkpoint(
                    &hook.task_id.0,
                    &format!("FAILED: {}", error_text),
                    cost,
                    turns,
                ).await;
                let auto_cancelled = {
                    let mut store = self.tasks.lock().await;
                    let mut cancelled = false;
                    if let Err(e) = store.update(&hook.task_id.0, |b| {
                        b.retry_count += 1;
                        if b.retry_count >= 3 {
                            b.status = TaskStatus::Cancelled;
                            b.assignee = None;
                            b.closed_reason = Some(format!(
                                "Auto-cancelled after {} retries. Last error: {}",
                                b.retry_count, error_text
                            ));
                            cancelled = true;
                        } else {
                            b.status = TaskStatus::Pending;
                            b.assignee = None;
                        }
                    }) {
                        warn!(bead = %hook.task_id, error = %e, "failed to re-queue quest after failure");
                    }
                    cancelled
                };
                self.task_notify.notify_waiters();
                if auto_cancelled {
                    warn!(worker = %self.name, bead = %hook.task_id, "quest auto-cancelled after 3 failed retries");
                }
                self.dispatch_bus
                    .send(Dispatch::new_typed(
                        &self.name,
                        &format!("witness-{}", self.project_name),
                        DispatchKind::QuestFailed {
                            task_id: hook.task_id.to_string(),
                            error: if auto_cancelled {
                                format!("Auto-cancelled after 3 retries. Last: {}", error_text)
                            } else {
                                error_text.clone()
                            },
                        },
                    ))
                    .await;
                self.state = WorkerState::Failed(error_text.to_string());
            }
        }

        self.hook = None;
        Ok((outcome, cost, turns))
    }

    async fn execute_agent(
        &self,
        provider: Arc<dyn system_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        model: &str,
        quest_context: &str,
        identity: &Identity,
    ) -> Result<system_core::AgentResult> {
        let observer: Arc<dyn Observer> = Arc::new(LogObserver);
        let agent_config = AgentConfig {
            model: model.to_string(),
            max_iterations: 20,
            name: self.name.clone(),
            ..Default::default()
        };

        let mut agent = Agent::new(
            agent_config,
            provider,
            tools,
            observer,
            identity.clone(),
        );

        if let Some(ref mem) = self.memory {
            agent = agent.with_memory(mem.clone());
        }

        agent.run(quest_context).await
    }

    /// Execute via Claude Code CLI subprocess. Returns (text, cost_usd, turns_used).
    async fn execute_claude_code(
        &self,
        executor: &ClaudeCodeExecutor,
        quest_context: &str,
        identity: &Identity,
    ) -> Result<(String, f64, u32)> {
        let result = executor.execute(identity, quest_context).await?;

        info!(
            worker = %self.name,
            turns = result.num_turns,
            cost_usd = result.total_cost_usd,
            duration_ms = result.duration_ms,
            "claude code execution completed"
        );

        self.reflect_on_result(quest_context, &result.result_text).await;

        Ok((result.result_text, result.total_cost_usd, result.num_turns))
    }

    async fn reflect_on_result(&self, quest_context: &str, result_text: &str) {
        let Some(ref mem) = self.memory else { return };
        let Some(ref provider) = self.reflect_provider else { return };

        let transcript = format!("User: {}\n\nAssistant: {}", quest_context, result_text);
        if transcript.len() < 100 {
            return;
        }

        let max_len = 8000;
        let truncated = if transcript.len() > max_len {
            &transcript[..max_len]
        } else {
            &transcript
        };

        let reflection_prompt = format!(
            "You are a memory extraction system. Analyze this conversation and extract ONLY \
             genuinely important insights worth remembering long-term. Output NOTHING if the \
             conversation is trivial.\n\n\
             For each insight, output exactly one line in this format:\n\
             SCOPE CATEGORY: key-slug | The insight content\n\n\
             Scopes (choose the most appropriate):\n\
             - DOMAIN: Technical facts about this specific project/codebase\n\
             - REALM: Insights about the Emperor (preferences, decisions, patterns that span projects)\n\
             - SELF: Your own observations, reflections, learnings as a companion\n\n\
             Categories:\n\
             - FACT: Factual information (technical details, architecture decisions, numbers)\n\
             - PROCEDURE: How something works or should be done\n\
             - PREFERENCE: User preferences, opinions, behavioral patterns\n\
             - CONTEXT: Decisions made, strategic shifts, project state changes\n\n\
             Rules:\n\
             - Maximum 5 insights per conversation\n\
             - Each insight must be self-contained\n\
             - key-slug: 2-4 lowercase hyphenated words\n\
             - Content: one concise sentence\n\
             - If nothing is worth remembering, output exactly: NONE\n\n\
             ## Conversation\n\n{}",
            truncated
        );

        let request = ChatRequest {
            model: self.reflect_model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text(&reflection_prompt),
            }],
            tools: vec![],
            max_tokens: 512,
            temperature: 0.0,
        };

        match provider.chat(&request).await {
            Ok(response) => {
                if let Some(text) = response.content {
                    self.store_routed_insights(&text, mem).await;
                }
            }
            Err(e) => warn!(worker = %self.name, "reflection failed: {e}"),
        }
    }

    async fn store_routed_insights(&self, text: &str, mem: &Arc<dyn Memory>) {
        for line in text.lines() {
            let line = line.trim();
            if line == "NONE" || line.is_empty() {
                continue;
            }

            let (scope, rest) = if let Some(r) = line.strip_prefix("DOMAIN ") {
                (MemoryScope::Domain, r)
            } else if let Some(r) = line.strip_prefix("REALM ") {
                (MemoryScope::Realm, r)
            } else if let Some(r) = line.strip_prefix("SELF ") {
                (MemoryScope::Companion, r)
            } else if let Some((cat_str, _rest)) = line.split_once(':') {
                let cat_str = cat_str.trim();
                if matches!(cat_str, "FACT" | "PROCEDURE" | "PREFERENCE" | "CONTEXT") {
                    (MemoryScope::Domain, line)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let Some((cat_str, rest)) = rest.split_once(':') else {
                continue;
            };
            let Some((key, content)) = rest.split_once('|') else {
                continue;
            };

            let category = match cat_str.trim().to_uppercase().as_str() {
                "FACT" => MemoryCategory::Fact,
                "PROCEDURE" => MemoryCategory::Procedure,
                "PREFERENCE" => MemoryCategory::Preference,
                "CONTEXT" => MemoryCategory::Context,
                _ => continue,
            };

            let key = key.trim();
            let content = content.trim();
            if key.is_empty() || content.is_empty() {
                continue;
            }

            let companion_id = if scope == MemoryScope::Companion {
                Some(self.name.as_str())
            } else {
                None
            };

            match mem.store(key, content, category, scope, companion_id).await {
                Ok(id) => {
                    debug!(worker = %self.name, id = %id, key = %key, scope = %scope, "insight stored")
                }
                Err(e) => {
                    warn!(worker = %self.name, key = %key, "failed to store insight: {e}")
                }
            }
        }
    }
}
