use anyhow::Result;
use chrono::Utc;
use sigil_core::traits::{
    ChatRequest, LogObserver, Memory, MemoryCategory, MemoryScope, Message, MessageContent,
    Observer, Provider, Role, Tool,
};
use sigil_core::{Agent, AgentConfig, Identity};
use sigil_tasks::{Checkpoint, Task, TaskStatus};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use crate::audit::{AuditEvent, AuditLog, DecisionType};
use crate::blackboard::Blackboard;
use crate::checkpoint::AgentCheckpoint;
use crate::execution_events::{EventBroadcaster, ExecutionEvent};
use crate::executor::{ClaudeCodeExecutor, TaskOutcome};
use crate::failure_analysis::{FailureAnalysis, FailureMode};
use crate::hook::Hook;
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::middleware::{
    MiddlewareAction, MiddlewareChain, Outcome, OutcomeStatus, WorkerContext,
};

/// Worker states.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerState {
    Idle,
    Hooked,
    Working,
    Done,
    Failed(String),
}

/// How a worker executes its assigned task.
pub enum WorkerExecution {
    /// Internal Agent loop (current behavior): LLM API calls with basic tools.
    Agent {
        provider: Arc<dyn sigil_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        model: String,
    },
    /// Claude Code CLI subprocess: full Edit, Grep, Glob, context compression.
    ClaudeCode(ClaudeCodeExecutor),
}

/// An AgentWorker is an ephemeral task executor. Each worker runs as a tokio task
/// with its own identity, hook, and tool allowlist.
pub struct AgentWorker {
    /// Stable logical agent identity used for assignee, expertise, memory, and audit semantics.
    pub agent_name: String,
    /// Ephemeral worker-run identifier used for execution tracing.
    pub name: String,
    pub project_name: String,
    pub state: WorkerState,
    pub hook: Option<Hook>,
    pub execution: WorkerExecution,
    pub identity: Identity,
    pub dispatch_bus: Arc<DispatchBus>,
    pub tasks: Arc<Mutex<sigil_tasks::TaskBoard>>,
    /// Fired when a task is closed so waiters don't need to poll.
    pub task_notify: Arc<Notify>,
    pub memory: Option<Arc<dyn Memory>>,
    pub reflect_provider: Option<Arc<dyn Provider>>,
    pub reflect_model: String,
    /// Project directory path for checkpoint storage.
    pub project_dir: Option<PathBuf>,
    /// Max task retries (handoff/failure) before auto-cancel.
    pub max_task_retries: u32,
    /// Optional shared blackboard for adaptive retry strategies.
    pub blackboard: Option<Arc<Blackboard>>,
    /// Optional audit log for recording retry analysis.
    pub audit_log: Option<Arc<AuditLog>>,
    /// Whether adaptive retry is enabled for this worker.
    pub adaptive_retry: bool,
    /// Model used for failure analysis when adaptive retry is enabled.
    pub failure_analysis_model: String,
    /// Middleware chain for composable execution behavior (guardrails, cost tracking, etc.).
    pub middleware_chain: Option<MiddlewareChain>,
    /// Event broadcaster for real-time execution event streaming.
    pub event_broadcaster: Option<Arc<EventBroadcaster>>,
    /// Optional debounced write queue for batching reflection memory writes.
    pub write_queue: Option<Arc<tokio::sync::Mutex<sigil_memory::debounce::WriteQueue>>>,
}

impl AgentWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent_name: String,
        name: String,
        project_name: String,
        provider: Arc<dyn sigil_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        identity: Identity,
        model: String,
        dispatch_bus: Arc<DispatchBus>,
        tasks: Arc<Mutex<sigil_tasks::TaskBoard>>,
        task_notify: Arc<Notify>,
    ) -> Self {
        let reflect_model = model.clone();
        Self {
            agent_name,
            name,
            project_name,
            state: WorkerState::Idle,
            hook: None,
            execution: WorkerExecution::Agent {
                provider,
                tools,
                model,
            },
            identity,
            dispatch_bus,
            tasks,
            task_notify,
            memory: None,
            reflect_provider: None,
            reflect_model,
            project_dir: None,
            max_task_retries: 3,
            blackboard: None,
            audit_log: None,
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            middleware_chain: None,
            event_broadcaster: None,
            write_queue: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_claude_code(
        agent_name: String,
        name: String,
        project_name: String,
        executor: ClaudeCodeExecutor,
        identity: Identity,
        dispatch_bus: Arc<DispatchBus>,
        tasks: Arc<Mutex<sigil_tasks::TaskBoard>>,
        task_notify: Arc<Notify>,
    ) -> Self {
        let project_dir = Some(executor.workdir().to_path_buf());
        Self {
            agent_name,
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
            max_task_retries: 3,
            blackboard: None,
            audit_log: None,
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            middleware_chain: None,
            event_broadcaster: None,
            write_queue: None,
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

    pub fn with_max_task_retries(mut self, max_retries: u32) -> Self {
        self.max_task_retries = max_retries;
        self
    }

    pub fn with_adaptive_retry(mut self, model: String) -> Self {
        self.adaptive_retry = true;
        self.failure_analysis_model = model;
        self
    }

    /// Set the middleware chain for this worker.
    pub fn set_middleware(&mut self, chain: MiddlewareChain) {
        self.middleware_chain = Some(chain);
    }

    /// Set the event broadcaster for real-time execution event streaming.
    pub fn set_broadcaster(&mut self, broadcaster: Arc<EventBroadcaster>) {
        self.event_broadcaster = Some(broadcaster);
    }

    /// Set the debounced write queue for batching reflection memory writes.
    pub fn set_write_queue(
        &mut self,
        queue: Arc<tokio::sync::Mutex<sigil_memory::debounce::WriteQueue>>,
    ) {
        self.write_queue = Some(queue);
    }

    /// Get the child PID tracker (for process group kill on timeout).
    /// Returns a zero-valued AtomicU32 for Agent mode.
    pub fn child_pid(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        match &self.execution {
            WorkerExecution::ClaudeCode(executor) => executor.child_pid.clone(),
            WorkerExecution::Agent { .. } => {
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))
            }
        }
    }

    /// Get the working directory for this worker (from executor or project_dir).
    fn workdir(&self) -> Option<&std::path::Path> {
        match &self.execution {
            WorkerExecution::ClaudeCode(executor) => Some(executor.workdir()),
            WorkerExecution::Agent { .. } => self.project_dir.as_deref(),
        }
    }

    /// Capture an external checkpoint by inspecting git state in the worker's workdir.
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
                    .with_task_id(task_id)
                    .with_worker_name(&self.agent_name);

                let checkpoint = if let Some(notes) = progress_notes {
                    checkpoint.with_progress_notes(notes)
                } else {
                    checkpoint
                };

                let cp_path = AgentCheckpoint::path_for_task(project_dir, task_id);
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

    /// Assign a task to this worker (set hook).
    pub fn assign(&mut self, task: &Task) {
        self.hook = Some(Hook::new(task.id.clone(), task.subject.clone()));
        self.state = WorkerState::Hooked;
    }

    /// Save a checkpoint recording this worker's progress on a task.
    async fn save_checkpoint(&self, task_id: &str, progress: &str, cost: f64, turns: u32) {
        let mut store = self.tasks.lock().await;
        if let Err(e) = store.update(task_id, |q| {
            q.checkpoints.push(Checkpoint {
                timestamp: Utc::now(),
                worker: self.agent_name.clone(),
                progress: progress.to_string(),
                cost_usd: cost,
                turns_used: turns,
            });
        }) {
            warn!(task_id, error = %e, "failed to save checkpoint to task store");
        }
    }

    async fn build_resume_brief(&self, task: &Task) -> String {
        let mut sections = Vec::new();

        if let Some(ref audit) = self.audit_log {
            let mut events = audit.query_by_task(&task.id.0).unwrap_or_default();
            if !events.is_empty() {
                if events.len() > 6 {
                    events = events.split_off(events.len() - 6);
                }
                let lines = events
                    .iter()
                    .map(|event| {
                        format!(
                            "- {} [{}] {}",
                            event.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                            event.decision_type,
                            truncate_for_prompt(&event.reasoning, 220),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                sections.push(format!("### Audit trail\n{lines}"));
            }
        }

        if let Some(ref blackboard) = self.blackboard {
            let entries = blackboard
                .query(&self.project_name, &task.labels, 5)
                .unwrap_or_default();
            if !entries.is_empty() {
                let lines = entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "- [{}] {}: {}",
                            entry.agent,
                            entry.key,
                            truncate_for_prompt(&entry.content, 220),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                sections.push(format!("### Blackboard\n{lines}"));
            }
        }

        let mut dispatches = self
            .dispatch_bus
            .all()
            .await
            .into_iter()
            .filter(|dispatch| is_relevant_dispatch(dispatch, &self.project_name, &task.id.0))
            .collect::<Vec<_>>();
        if !dispatches.is_empty() {
            if dispatches.len() > 6 {
                dispatches = dispatches.split_off(dispatches.len() - 6);
            }
            let lines = dispatches
                .iter()
                .map(format_dispatch_for_prompt)
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("### Control plane\n{lines}"));
        }

        if sections.is_empty() {
            String::new()
        } else {
            format!(
                "\n## Resume Brief\n\n{}\n\nUse this to avoid repeating earlier failures or redundant work.\n",
                sections.join("\n\n")
            )
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

        let execution_start = std::time::Instant::now();

        // Build WorkerContext for middleware chain.
        let task_description_for_ctx = {
            let store = self.tasks.lock().await;
            store
                .get(&hook.task_id.0)
                .map(|t| t.description.clone())
                .unwrap_or_else(|| hook.subject.clone())
        };
        let mut worker_ctx = WorkerContext::new(
            &hook.task_id.0,
            &task_description_for_ctx,
            &self.agent_name,
            &self.project_name,
        );

        // Run middleware on_start — check for Halt before proceeding.
        if let Some(ref chain) = self.middleware_chain {
            let action = chain.run_on_start(&mut worker_ctx).await;
            match action {
                MiddlewareAction::Halt(reason) => {
                    warn!(
                        worker = %self.name,
                        task = %hook.task_id,
                        reason = %reason,
                        "middleware halted execution on start"
                    );
                    if let Some(ref broadcaster) = self.event_broadcaster {
                        broadcaster.publish(ExecutionEvent::TaskFailed {
                            task_id: hook.task_id.0.clone(),
                            reason: reason.clone(),
                            artifacts_preserved: false,
                        });
                    }
                    self.hook = None;
                    return Ok((
                        TaskOutcome::Failed(format!("Middleware halted: {reason}")),
                        0.0,
                        0,
                    ));
                }
                MiddlewareAction::Continue
                | MiddlewareAction::Inject(_)
                | MiddlewareAction::Skip => {}
            }
        }

        // Publish TaskStarted event.
        if let Some(ref broadcaster) = self.event_broadcaster {
            broadcaster.publish(ExecutionEvent::TaskStarted {
                task_id: hook.task_id.0.clone(),
                agent: self.agent_name.clone(),
                project: self.project_name.clone(),
            });
        }

        info!(
            worker = %self.name,
            task = %hook.task_id,
            subject = %hook.subject,
            mode = match &self.execution {
                WorkerExecution::Agent { .. } => "agent",
                WorkerExecution::ClaudeCode(_) => "claude_code",
            },
            "starting work"
        );

        self.state = WorkerState::Working;

        // Mark task as in_progress.
        {
            let mut store = self.tasks.lock().await;
            if let Err(e) = store.update(&hook.task_id.0, |b| {
                b.status = TaskStatus::InProgress;
                b.assignee = Some(self.agent_name.clone());
            }) {
                warn!(task = %hook.task_id, error = %e, "failed to mark task in_progress");
            }
        }

        // Build the prompt from the task (including any previous checkpoints).
        let task_snapshot = {
            let store = self.tasks.lock().await;
            store.get(&hook.task_id.0).cloned()
        };

        let mut task_context = match task_snapshot.as_ref() {
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
                    ctx.push_str(
                        "Review the above before starting. Skip work that's already done.\n\n",
                    );
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
        };
        if let Some(task) = task_snapshot.as_ref() {
            let resume_brief = self.build_resume_brief(task).await;
            if !resume_brief.is_empty() {
                task_context.push_str(&resume_brief);
            }
        }

        // Enrich identity with dynamic memory recall via query planner.
        let enriched_identity = if let Some(ref mem) = self.memory {
            // Try query planner first — generates typed, prioritized queries.
            let entries = match std::panic::catch_unwind(|| {
                sigil_memory::query_planner::QueryPlanner::plan(
                    &task_context,
                    Some(&self.project_name),
                )
            }) {
                Ok(plan) => {
                    let mut all_entries = Vec::new();
                    for typed_query in &plan.queries {
                        let query = sigil_core::traits::MemoryQuery::new(
                            &typed_query.query_text,
                            plan.max_results_per_query,
                        );
                        if let Ok(results) = mem.search(&query).await {
                            all_entries.extend(results);
                        }
                    }
                    // Deduplicate by id, keep highest score.
                    all_entries.sort_by(|a, b| {
                        b.score
                            .partial_cmp(&a.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    all_entries.dedup_by(|a, b| a.id == b.id);
                    all_entries.truncate(30);
                    debug!(
                        worker = %self.name,
                        queries = plan.queries.len(),
                        results = all_entries.len(),
                        "query planner memory recall"
                    );
                    all_entries
                }
                Err(_) => {
                    // Fallback: single flat search if query planner fails.
                    warn!(worker = %self.name, "query planner failed, falling back to flat search");
                    let query = sigil_core::traits::MemoryQuery::new(&task_context, 30)
                        .with_scope(MemoryScope::Domain);
                    mem.search(&query).await.unwrap_or_default()
                }
            };

            if !entries.is_empty() {
                let mut id = self.identity.clone();
                let dynamic = entries
                    .iter()
                    .map(|e| format!("- [{}] {}: {}", e.scope, e.key, e.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                let existing = id.memory.unwrap_or_default();
                id.memory = Some(format!("{existing}\n\n## Dynamic Recall\n{dynamic}"));
                id
            } else {
                self.identity.clone()
            }
        } else {
            self.identity.clone()
        };

        // Dispatch based on execution mode. Returns (text, cost_usd, turns_used).
        let raw_result = match &self.execution {
            WorkerExecution::Agent {
                provider,
                tools,
                model,
            } => self
                .execute_agent(
                    provider.clone(),
                    tools.clone(),
                    model,
                    &task_context,
                    &enriched_identity,
                )
                .await
                .map(|agent_result| {
                    let cost = sigil_providers::estimate_cost(
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
                }),
            WorkerExecution::ClaudeCode(executor) => {
                self.execute_claude_code(executor, &task_context, &enriched_identity)
                    .await
            }
        };

        // Fire-and-forget reflection for Agent mode (ClaudeCode mode triggers its own).
        if matches!(self.execution, WorkerExecution::Agent { .. })
            && let Ok((ref result_text, _, _)) = raw_result
            && let (Some(mem), Some(provider)) =
                (self.memory.clone(), self.reflect_provider.clone())
        {
            let task_ctx = task_context.clone();
            let text = result_text.clone();
            let model = self.reflect_model.clone();
            let name = self.agent_name.clone();
            tokio::spawn(async move {
                Self::reflect_detached(name, task_ctx, text, model, mem, provider).await;
            });
        }

        // Parse into structured outcome.
        let (outcome, cost, turns) = match raw_result {
            Ok((result_text, cost, turns)) => (TaskOutcome::parse(&result_text), cost, turns),
            Err(e) => {
                // Run middleware on_error.
                if let Some(ref chain) = self.middleware_chain {
                    let error_str = e.to_string();
                    chain.run_on_error(&mut worker_ctx, &error_str).await;
                }
                // Publish TaskFailed event.
                if let Some(ref broadcaster) = self.event_broadcaster {
                    broadcaster.publish(ExecutionEvent::TaskFailed {
                        task_id: hook.task_id.0.clone(),
                        reason: e.to_string(),
                        artifacts_preserved: false,
                    });
                }
                (TaskOutcome::Failed(e.to_string()), 0.0, 0)
            }
        };

        // Run middleware on_complete with structured outcome.
        let duration_ms = execution_start.elapsed().as_millis() as u64;
        if let Some(ref chain) = self.middleware_chain {
            let mw_outcome = match &outcome {
                TaskOutcome::Done(_) => Outcome {
                    status: OutcomeStatus::Done,
                    confidence: 1.0,
                    artifacts: Vec::new(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: None,
                },
                TaskOutcome::Blocked { question, .. } => Outcome {
                    status: OutcomeStatus::Blocked,
                    confidence: 0.5,
                    artifacts: Vec::new(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: Some(question.clone()),
                },
                TaskOutcome::Handoff { checkpoint } => Outcome {
                    status: OutcomeStatus::NeedsContext,
                    confidence: 0.3,
                    artifacts: Vec::new(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: Some(checkpoint.clone()),
                },
                TaskOutcome::Failed(error) => Outcome {
                    status: OutcomeStatus::Failed,
                    confidence: 0.0,
                    artifacts: Vec::new(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: Some(error.clone()),
                },
            };
            worker_ctx.cost_usd = cost;
            chain.run_on_complete(&mut worker_ctx, &mw_outcome).await;
        }

        // Publish outcome-specific execution events.
        if let Some(ref broadcaster) = self.event_broadcaster {
            match &outcome {
                TaskOutcome::Done(summary) => {
                    broadcaster.publish(ExecutionEvent::TaskCompleted {
                        task_id: hook.task_id.0.clone(),
                        outcome: summary.chars().take(500).collect(),
                        confidence: 1.0,
                        cost_usd: cost,
                        turns,
                        duration_ms,
                    });
                }
                TaskOutcome::Blocked { question, .. } => {
                    broadcaster.publish(ExecutionEvent::ClarificationNeeded {
                        task_id: hook.task_id.0.clone(),
                        question: question.clone(),
                        options: Vec::new(),
                    });
                }
                TaskOutcome::Handoff { checkpoint } => {
                    broadcaster.publish(ExecutionEvent::CheckpointCreated {
                        task_id: hook.task_id.0.clone(),
                        message: format!(
                            "HANDOFF: {}",
                            checkpoint.chars().take(500).collect::<String>()
                        ),
                    });
                }
                TaskOutcome::Failed(reason) => {
                    // TaskFailed event may already be published in the Err arm above,
                    // but if the failure came from TaskOutcome::parse on a successful
                    // execution (FAILED: prefix in response text), publish here.
                    broadcaster.publish(ExecutionEvent::TaskFailed {
                        task_id: hook.task_id.0.clone(),
                        reason: reason.chars().take(500).collect(),
                        artifacts_preserved: false,
                    });
                }
            }
        }

        // Process outcome: save checkpoint, update task status, notify supervisor.
        match &outcome {
            TaskOutcome::Done(result_text) => {
                info!(worker = %self.name, task = %hook.task_id, "work completed");
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
                )
                .await;
                {
                    let mut store = self.tasks.lock().await;
                    let _ = store.close(&hook.task_id.0, result_text);
                }
                self.task_notify.notify_waiters();
                self.state = WorkerState::Done;
            }

            TaskOutcome::Blocked {
                question,
                full_text,
            } => {
                info!(
                    worker = %self.name,
                    task = %hook.task_id,
                    question = %question,
                    "worker blocked — needs input"
                );
                // Capture external checkpoint from git state before recording block.
                self.capture_and_save_checkpoint(
                    &hook.task_id.0,
                    Some(&format!(
                        "BLOCKED: {}\n\nWork so far:\n{}",
                        question, full_text
                    )),
                );
                self.save_checkpoint(
                    &hook.task_id.0,
                    &format!(
                        "BLOCKED on: {}\n\nWork done so far:\n{}",
                        question, full_text
                    ),
                    cost,
                    turns,
                )
                .await;
                // Mark task as Blocked and preserve the question for Supervisor resolution.
                {
                    let mut store = self.tasks.lock().await;
                    if let Err(e) = store.update(&hook.task_id.0, |b| {
                        b.status = TaskStatus::Blocked;
                        b.assignee = None;
                        b.closed_reason = Some(question.clone());
                    }) {
                        warn!(task = %hook.task_id, error = %e, "failed to mark task blocked");
                    }
                }
                self.task_notify.notify_waiters();
                self.state = WorkerState::Done; // Worker is done; task is blocked.
            }

            TaskOutcome::Handoff { checkpoint } => {
                info!(worker = %self.name, task = %hook.task_id, "worker handing off — context exhaustion");
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
                )
                .await;
                {
                    let mut store = self.tasks.lock().await;
                    let max_retries = self.max_task_retries;
                    if let Err(e) = store.update(&hook.task_id.0, |b| {
                        b.retry_count += 1;
                        if b.retry_count >= max_retries {
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
                        warn!(task = %hook.task_id, error = %e, "failed to re-queue task after handoff");
                    }
                }
                // Log if auto-cancelled.
                {
                    let store = self.tasks.lock().await;
                    if let Some(b) = store.get(&hook.task_id.0)
                        && b.status == TaskStatus::Cancelled
                    {
                        warn!(worker = %self.name, task = %hook.task_id, retries = b.retry_count, "task auto-cancelled after max retries");
                    }
                }
                self.task_notify.notify_waiters();
                self.state = WorkerState::Done;
            }

            TaskOutcome::Failed(error_text) => {
                warn!(worker = %self.name, task = %hook.task_id, "work failed");
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
                )
                .await;

                // Attempt LLM-based failure analysis before locking the task store.
                let failure_result: Option<(String, FailureMode)> = if self.adaptive_retry
                    && let Some(ref provider) = self.reflect_provider
                {
                    let fa_model = if self.failure_analysis_model.is_empty() {
                        self.reflect_model.clone()
                    } else {
                        self.failure_analysis_model.clone()
                    };
                    if !fa_model.is_empty() {
                        let (task_desc, task_labels) = {
                            let store = self.tasks.lock().await;
                            store
                                .get(&hook.task_id.0)
                                .map(|t| (t.description.clone(), t.labels.clone()))
                                .unwrap_or_default()
                        };
                        let prompt =
                            FailureAnalysis::analysis_prompt(&hook.subject, &task_desc, error_text);
                        let request = ChatRequest {
                            model: fa_model,
                            messages: vec![Message {
                                role: Role::User,
                                content: MessageContent::text(&prompt),
                            }],
                            tools: vec![],
                            max_tokens: 256,
                            temperature: 0.0,
                        };
                        match provider.chat(&request).await {
                            Ok(response) if response.content.is_some() => {
                                let analysis =
                                    FailureAnalysis::parse(response.content.as_deref().unwrap());
                                info!(
                                    worker = %self.name,
                                    task = %hook.task_id,
                                    mode = ?analysis.mode,
                                    "failure analysis completed"
                                );

                                // Record audit event.
                                if let Some(ref audit) = self.audit_log {
                                    let _ = audit.record(
                                        &AuditEvent::new(
                                            &self.project_name,
                                            DecisionType::FailureAnalyzed,
                                            format!(
                                                "Mode: {:?}, Reasoning: {}",
                                                analysis.mode, analysis.reasoning
                                            ),
                                        )
                                        .with_task(&hook.task_id.0)
                                        .with_agent(&self.agent_name),
                                    );
                                }

                                // Mode-specific: query blackboard for MissingContext.
                                let mut enrichment = analysis.enrich_description();
                                if analysis.mode == FailureMode::MissingContext
                                    && let Some(ref bb) = self.blackboard
                                {
                                    let bb_entries = bb
                                        .query(&self.project_name, &task_labels, 5)
                                        .unwrap_or_default();
                                    if !bb_entries.is_empty() {
                                        let bb_ctx = bb_entries
                                            .iter()
                                            .map(|e| {
                                                format!("- [{}] {}: {}", e.agent, e.key, e.content)
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n");
                                        enrichment.push_str(&format!(
                                            "\n### Blackboard context\n{bb_ctx}\n"
                                        ));
                                    }
                                }

                                let mode = analysis.mode;
                                Some((enrichment, mode))
                            }
                            Ok(_) | Err(_) => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let auto_cancelled = {
                    let mut store = self.tasks.lock().await;
                    let mut cancelled = false;
                    let max_retries = self.max_task_retries;
                    let enrichment = failure_result.as_ref().map(|(e, _)| e.clone());
                    let failure_mode = failure_result.as_ref().map(|(_, m)| *m);
                    if let Err(e) = store.update(&hook.task_id.0, |b| {
                        b.retry_count += 1;
                        if b.retry_count >= max_retries {
                            b.status = TaskStatus::Cancelled;
                            b.assignee = None;
                            b.closed_reason = Some(format!(
                                "Auto-cancelled after {} retries. Last error: {}",
                                b.retry_count, error_text
                            ));
                            cancelled = true;
                        } else {
                            // Mode-specific status overrides.
                            match failure_mode {
                                Some(FailureMode::ExternalBlocker) => {
                                    b.status = TaskStatus::Blocked;
                                    b.assignee = None;
                                    b.closed_reason =
                                        Some("Blocked: external blocker detected".to_string());
                                    cancelled = true;
                                }
                                Some(FailureMode::BudgetExhausted) => {
                                    b.status = TaskStatus::Blocked;
                                    b.assignee = None;
                                    b.closed_reason =
                                        Some("Blocked: budget exhausted".to_string());
                                    b.labels.push("budget-blocked".to_string());
                                    cancelled = true;
                                }
                                _ => {
                                    let failure_context =
                                        enrichment.clone().unwrap_or_else(|| {
                                            format!(
                                                "\n\n---\n## Previous Failure (attempt {})\n\n\
                                                 The previous worker failed with this error. \
                                                 Try a different approach to avoid the same failure.\n\n\
                                                 **Error:**\n{}\n",
                                                b.retry_count, error_text
                                            )
                                        });
                                    b.description.push_str(&failure_context);
                                    b.status = TaskStatus::Pending;
                                    b.assignee = None;
                                }
                            }
                        }
                    }) {
                        warn!(task = %hook.task_id, error = %e, "failed to re-queue task after failure");
                    }
                    cancelled
                };
                self.task_notify.notify_waiters();
                if auto_cancelled {
                    warn!(worker = %self.name, task = %hook.task_id, "task auto-cancelled after 3 failed retries");
                    if let Some(ref audit) = self.audit_log {
                        let _ = audit.record(
                            &AuditEvent::new(
                                &self.project_name,
                                DecisionType::TaskCancelled,
                                format!("Auto-cancelled after max retries: {}", error_text),
                            )
                            .with_task(&hook.task_id.0)
                            .with_agent(&self.agent_name),
                        );
                    }
                }
                self.state = WorkerState::Failed(error_text.to_string());
            }
        }

        self.hook = None;
        Ok((outcome, cost, turns))
    }

    async fn execute_agent(
        &self,
        provider: Arc<dyn sigil_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        model: &str,
        task_context: &str,
        identity: &Identity,
    ) -> Result<sigil_core::AgentResult> {
        let observer: Arc<dyn Observer> = Arc::new(LogObserver);
        let agent_config = AgentConfig {
            model: model.to_string(),
            max_iterations: 20,
            name: self.agent_name.clone(),
            ..Default::default()
        };

        let mut agent = Agent::new(agent_config, provider, tools, observer, identity.clone());

        if let Some(ref mem) = self.memory {
            agent = agent.with_memory(mem.clone());
        }

        agent.run(task_context).await
    }

    /// Execute via Claude Code CLI subprocess. Returns (text, cost_usd, turns_used).
    async fn execute_claude_code(
        &self,
        executor: &ClaudeCodeExecutor,
        task_context: &str,
        identity: &Identity,
    ) -> Result<(String, f64, u32)> {
        let result = executor.execute(identity, task_context).await?;

        info!(
            worker = %self.name,
            turns = result.num_turns,
            cost_usd = result.total_cost_usd,
            duration_ms = result.duration_ms,
            "claude code execution completed"
        );

        // Persist latest rate limit info for dashboard visibility.
        if let Some(ref rl) = result.rate_limit {
            let rl_json = serde_json::json!({
                "status": rl.status,
                "resets_at": rl.resets_at,
                "rate_limit_type": rl.rate_limit_type,
                "overage_status": rl.overage_status,
                "updated_at": chrono::Utc::now().to_rfc3339(),
            });
            if let Ok(data_dir) =
                std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".sigil"))
            {
                let _ = std::fs::write(
                    data_dir.join("rate_limit.json"),
                    serde_json::to_string_pretty(&rl_json).unwrap_or_default(),
                );
            }
        }

        // Fire-and-forget reflection — don't block the worker slot.
        if let (Some(mem), Some(provider)) = (self.memory.clone(), self.reflect_provider.clone()) {
            let task_ctx = task_context.to_string();
            let result_text = result.result_text.clone();
            let model = self.reflect_model.clone();
            let name = self.agent_name.clone();
            tokio::spawn(async move {
                Self::reflect_detached(name, task_ctx, result_text, model, mem, provider).await;
            });
        }

        Ok((result.result_text, result.total_cost_usd, result.num_turns))
    }

    /// Detached reflection — runs in a separate tokio task, no &self needed.
    async fn reflect_detached(
        worker_name: String,
        task_context: String,
        result_text: String,
        model: String,
        mem: Arc<dyn Memory>,
        provider: Arc<dyn Provider>,
    ) {
        let transcript = format!("User: {}\n\nAssistant: {}", task_context, result_text);
        if transcript.len() < 30 {
            return;
        }

        let max_len = 8000;
        let truncated = if transcript.chars().count() > max_len {
            Self::char_prefix(&transcript, max_len)
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
             - SYSTEM: Insights about the user (preferences, decisions, patterns that span projects)\n\
             - SELF: Your own observations, reflections, learnings as an agent\n\n\
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
            model,
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
                    Self::store_routed_insights_static(&worker_name, &text, &mem).await;
                }
            }
            Err(e) => warn!(worker = %worker_name, "reflection failed: {e}"),
        }
    }

    async fn store_routed_insights_static(worker_name: &str, text: &str, mem: &Arc<dyn Memory>) {
        use sigil_memory::dedup::{DedupAction, DedupCandidate, DedupPipeline, SimilarMemory};

        let dedup = DedupPipeline::default();

        for line in text.lines() {
            let line = line.trim();
            if line == "NONE" || line.is_empty() {
                continue;
            }

            let (scope, rest) = if let Some(r) = line.strip_prefix("DOMAIN ") {
                (MemoryScope::Domain, r)
            } else if let Some(r) = line.strip_prefix("SYSTEM ") {
                (MemoryScope::System, r)
            } else if let Some(r) = line.strip_prefix("SELF ") {
                (MemoryScope::Entity, r)
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

            // ── Dedup check: search for similar existing memories ──
            let should_store = match async {
                let query = sigil_core::traits::MemoryQuery::new(key, 5);
                let existing = mem.search(&query).await.unwrap_or_default();
                let similar: Vec<SimilarMemory> = existing
                    .iter()
                    .map(|e| SimilarMemory {
                        id: e.id.clone(),
                        key: e.key.clone(),
                        content: e.content.clone(),
                        similarity: e.score as f32,
                    })
                    .collect();
                let candidate = DedupCandidate {
                    key: key.to_string(),
                    content: content.to_string(),
                    embedding: None,
                };
                Ok::<DedupAction, anyhow::Error>(dedup.decide(&candidate, &similar))
            }
            .await
            {
                Ok(DedupAction::Skip) => {
                    debug!(worker = %worker_name, key = %key, "dedup: skipping duplicate memory");
                    false
                }
                Ok(DedupAction::Create) => true,
                Ok(DedupAction::Merge(id)) => {
                    debug!(worker = %worker_name, key = %key, merge_with = %id, "dedup: merging with existing memory");
                    true
                }
                Ok(DedupAction::Supersede(id)) => {
                    debug!(worker = %worker_name, key = %key, supersedes = %id, "dedup: superseding existing memory");
                    true
                }
                Err(e) => {
                    debug!(worker = %worker_name, key = %key, "dedup check failed, proceeding with store: {e}");
                    true
                }
            };

            if !should_store {
                continue;
            }

            let entity_id = if scope == MemoryScope::Entity {
                Some(worker_name)
            } else {
                None
            };

            match mem.store(key, content, category, scope, entity_id).await {
                Ok(id) => {
                    debug!(worker = %worker_name, id = %id, key = %key, scope = %scope, "insight stored")
                }
                Err(e) => {
                    warn!(worker = %worker_name, key = %key, "failed to store insight: {e}")
                }
            }
        }
    }

    fn char_prefix(text: &str, max_chars: usize) -> &str {
        let cut = text
            .char_indices()
            .nth(max_chars)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len());
        &text[..cut]
    }
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    let mut out = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn is_relevant_dispatch(dispatch: &Dispatch, project: &str, task_id: &str) -> bool {
    match &dispatch.kind {
        DispatchKind::TaskDone { task_id: id, .. }
        | DispatchKind::TaskBlocked { task_id: id, .. }
        | DispatchKind::TaskFailed { task_id: id, .. }
        | DispatchKind::Resolution { task_id: id, .. } => id == task_id,
        DispatchKind::Escalation {
            project: dispatch_project,
            task_id: id,
            ..
        }
        | DispatchKind::HumanEscalation {
            project: dispatch_project,
            task_id: id,
            ..
        } => dispatch_project == project && id == task_id,
        DispatchKind::DependencySuggestion {
            project: dispatch_project,
            from_task,
            to_task,
            ..
        } => dispatch_project == project && (from_task == task_id || to_task == task_id),
        _ => false,
    }
}

fn format_dispatch_for_prompt(dispatch: &Dispatch) -> String {
    format!(
        "- {} [{} -> {}] {}",
        dispatch.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
        dispatch.from,
        dispatch.to,
        truncate_for_prompt(&dispatch.kind.body_text(), 220),
    )
}
