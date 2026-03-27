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
use crate::executor::TaskOutcome;
use crate::failure_analysis::{FailureAnalysis, FailureMode};
use crate::hook::Hook;
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::middleware::{MiddlewareAction, MiddlewareChain, Outcome, OutcomeStatus, WorkerContext};
use crate::runtime::{
    Artifact, ArtifactKind, RuntimeExecution, RuntimeOutcome, RuntimePhase, RuntimeSession,
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
    /// Native Sigil agent loop.
    Agent {
        provider: Arc<dyn sigil_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        model: String,
    },
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

    /// Get the working directory for this worker.
    fn workdir(&self) -> Option<&std::path::Path> {
        self.project_dir.as_deref()
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

    /// Execute the hooked work through the native Sigil agent runtime.
    /// Returns (outcome, cost_usd, turns_used) for the Supervisor to record.
    pub async fn execute(&mut self) -> Result<(TaskOutcome, RuntimeExecution, f64, u32)> {
        let hook = match &self.hook {
            Some(h) => h.clone(),
            None => {
                warn!(worker = %self.name, "no hook assigned, nothing to do");
                let outcome = TaskOutcome::Done("no work assigned".to_string());
                let mut session = RuntimeSession::new(
                    "unassigned",
                    self.name.clone(),
                    self.project_name.clone(),
                    self.execution_model(),
                );
                session.mark_phase(RuntimePhase::Prime, "Worker had no hook assigned");
                let runtime_outcome = RuntimeOutcome::from_task_outcome(&outcome, Vec::new());
                session.finish(&runtime_outcome);
                return Ok((
                    outcome,
                    RuntimeExecution {
                        session,
                        outcome: runtime_outcome,
                    },
                    0.0,
                    0,
                ));
            }
        };

        let execution_start = std::time::Instant::now();
        let mut runtime_session = RuntimeSession::new(
            hook.task_id.0.clone(),
            self.name.clone(),
            self.project_name.clone(),
            self.execution_model(),
        );
        runtime_session.mark_phase(RuntimePhase::Prime, "Loaded task hook and worker identity");

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
                    self.hook = None;
                    runtime_session.mark_phase(
                        RuntimePhase::Frame,
                        "Middleware halted execution before run",
                    );
                    let outcome = TaskOutcome::Failed(format!("Middleware halted: {reason}"));
                    let runtime_outcome = RuntimeOutcome::from_task_outcome(&outcome, Vec::new());
                    runtime_session.finish(&runtime_outcome);
                    let runtime_execution = RuntimeExecution {
                        session: runtime_session,
                        outcome: runtime_outcome,
                    };
                    self.persist_runtime_execution(&hook.task_id.0, &runtime_execution)
                        .await;
                    if let Some(ref broadcaster) = self.event_broadcaster {
                        broadcaster.publish(ExecutionEvent::TaskFailed {
                            task_id: hook.task_id.0.clone(),
                            reason: reason.clone(),
                            artifacts_preserved: false,
                            runtime: Some(runtime_execution.clone()),
                        });
                    }
                    return Ok((outcome, runtime_execution, 0.0, 0));
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
                runtime_session: Some(runtime_session.clone()),
            });
        }

        info!(
            worker = %self.name,
            task = %hook.task_id,
            subject = %hook.subject,
            mode = "agent",
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
        runtime_session.mark_phase(
            RuntimePhase::Frame,
            "Prepared task context, checkpoints, and resume brief",
        );

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
        runtime_session.mark_phase(
            RuntimePhase::Act,
            format!(
                "Executing native Sigil agent loop{}",
                self.execution_model()
                    .map(|model| format!(" with model {model}"))
                    .unwrap_or_default()
            ),
        );
        self.persist_runtime_session(&hook.task_id.0, &runtime_session)
            .await;

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
        };

        // Fire-and-forget reflection so the worker slot does not wait on memory extraction.
        if let Ok((ref result_text, _, _)) = raw_result
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
                (TaskOutcome::Failed(e.to_string()), 0.0, 0)
            }
        };
        let mut runtime_outcome =
            RuntimeOutcome::from_task_outcome(&outcome, self.collect_runtime_artifacts());
        let runtime_artifact_refs = runtime_outcome.artifact_refs();
        runtime_session.mark_phase(
            RuntimePhase::Verify,
            "Captured runtime artifacts and prepared structured outcome",
        );

        // Run middleware on_complete with structured outcome.
        let duration_ms = execution_start.elapsed().as_millis() as u64;
        if let Some(ref chain) = self.middleware_chain {
            let mw_outcome = match &outcome {
                TaskOutcome::Done(_) => Outcome {
                    status: OutcomeStatus::Done,
                    confidence: 1.0,
                    artifacts: runtime_artifact_refs.clone(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: None,
                    runtime: Some(runtime_outcome.clone()),
                },
                TaskOutcome::Blocked { question, .. } => Outcome {
                    status: OutcomeStatus::Blocked,
                    confidence: 0.5,
                    artifacts: runtime_artifact_refs.clone(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: Some(question.clone()),
                    runtime: Some(runtime_outcome.clone()),
                },
                TaskOutcome::Handoff { checkpoint } => Outcome {
                    status: OutcomeStatus::NeedsContext,
                    confidence: 0.3,
                    artifacts: runtime_artifact_refs.clone(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: Some(checkpoint.clone()),
                    runtime: Some(runtime_outcome.clone()),
                },
                TaskOutcome::Failed(error) => Outcome {
                    status: OutcomeStatus::Failed,
                    confidence: 0.0,
                    artifacts: runtime_artifact_refs.clone(),
                    cost_usd: cost,
                    turns,
                    duration_ms,
                    reason: Some(error.clone()),
                    runtime: Some(runtime_outcome.clone()),
                },
            };
            worker_ctx.cost_usd = cost;
            chain.run_on_complete(&mut worker_ctx, &mw_outcome).await;
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

        if let Some(checkpoint_path) = self.checkpoint_path_for_task(&hook.task_id.0)
            && checkpoint_path.exists()
        {
            let checkpoint_ref = checkpoint_path.display().to_string();
            runtime_session.add_checkpoint_ref(checkpoint_ref.clone());
            runtime_outcome.artifacts.push(Artifact::new(
                ArtifactKind::Checkpoint,
                "checkpoint",
                checkpoint_ref,
            ));
        }
        runtime_session.finish(&runtime_outcome);
        let runtime_execution = RuntimeExecution {
            session: runtime_session.clone(),
            outcome: runtime_outcome.clone(),
        };
        self.persist_runtime_execution(&hook.task_id.0, &runtime_execution)
            .await;

        // Publish outcome-specific execution events with the finalized runtime state.
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
                        runtime: Some(runtime_execution.clone()),
                    });
                }
                TaskOutcome::Blocked { question, .. } => {
                    broadcaster.publish(ExecutionEvent::ClarificationNeeded {
                        task_id: hook.task_id.0.clone(),
                        question: question.clone(),
                        options: Vec::new(),
                        runtime: Some(runtime_execution.clone()),
                    });
                }
                TaskOutcome::Handoff { checkpoint } => {
                    broadcaster.publish(ExecutionEvent::CheckpointCreated {
                        task_id: hook.task_id.0.clone(),
                        message: format!(
                            "HANDOFF: {}",
                            checkpoint.chars().take(500).collect::<String>()
                        ),
                        runtime: Some(runtime_execution.clone()),
                    });
                }
                TaskOutcome::Failed(reason) => {
                    broadcaster.publish(ExecutionEvent::TaskFailed {
                        task_id: hook.task_id.0.clone(),
                        reason: reason.chars().take(500).collect(),
                        artifacts_preserved: !runtime_execution.outcome.artifacts.is_empty(),
                        runtime: Some(runtime_execution.clone()),
                    });
                }
            }
        }

        self.hook = None;
        Ok((outcome, runtime_execution, cost, turns))
    }

    fn execution_model(&self) -> Option<String> {
        match &self.execution {
            WorkerExecution::Agent { model, .. } => Some(model.clone()),
        }
    }

    async fn persist_runtime_session(&self, task_id: &str, session: &RuntimeSession) {
        self.persist_runtime_value(
            task_id,
            serde_json::json!({
                "session": session,
                "outcome": serde_json::Value::Null,
            }),
        )
        .await;
    }

    async fn persist_runtime_execution(&self, task_id: &str, runtime: &RuntimeExecution) {
        match serde_json::to_value(runtime) {
            Ok(value) => self.persist_runtime_value(task_id, value).await,
            Err(error) => warn!(
                worker = %self.name,
                task = %task_id,
                error = %error,
                "failed to serialize runtime execution for task metadata"
            ),
        }
    }

    async fn persist_runtime_value(&self, task_id: &str, runtime: serde_json::Value) {
        let mut store = self.tasks.lock().await;
        if let Err(error) = store.update(task_id, |task| {
            Self::set_task_sigil_metadata(task, "runtime", runtime);
        }) {
            warn!(
                worker = %self.name,
                task = %task_id,
                error = %error,
                "failed to persist runtime metadata"
            );
        }
    }

    fn set_task_sigil_metadata(task: &mut sigil_tasks::Task, key: &str, value: serde_json::Value) {
        let mut metadata = match std::mem::take(&mut task.metadata) {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("_legacy".to_string(), other);
                map
            }
        };

        let sigil_value = metadata
            .entry("sigil".to_string())
            .or_insert_with(|| serde_json::json!({}));

        if !sigil_value.is_object() {
            *sigil_value = serde_json::json!({});
        }

        if let Some(sigil_meta) = sigil_value.as_object_mut() {
            sigil_meta.insert(key.to_string(), value);
        }

        task.metadata = if metadata.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(metadata)
        };
    }

    fn checkpoint_path_for_task(&self, task_id: &str) -> Option<PathBuf> {
        self.project_dir
            .as_deref()
            .or(self.workdir())
            .map(|project_dir| AgentCheckpoint::path_for_task(project_dir, task_id))
    }

    fn collect_runtime_artifacts(&self) -> Vec<Artifact> {
        let Some(workdir) = self.workdir() else {
            return Vec::new();
        };

        let checkpoint = match AgentCheckpoint::capture(workdir) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                debug!(
                    worker = %self.name,
                    error = %error,
                    "failed to collect runtime artifacts from git state"
                );
                return Vec::new();
            }
        };

        let mut artifacts = Vec::new();

        if let Some(ref worktree) = checkpoint.worktree_path {
            artifacts.push(Artifact::new(ArtifactKind::Worktree, "worktree", worktree));
        }
        if let Some(ref branch) = checkpoint.branch {
            artifacts.push(Artifact::new(ArtifactKind::GitBranch, "branch", branch));
        }
        if let Some(ref commit) = checkpoint.last_commit {
            artifacts.push(Artifact::new(ArtifactKind::GitCommit, "head", commit));
        }
        for file in checkpoint.modified_files {
            artifacts.push(Artifact::new(ArtifactKind::File, file.clone(), file));
        }

        artifacts
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
            let should_store_action = async {
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
            .await;

            let should_store = match &should_store_action {
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

            // Capture dedup relation targets for edge creation.
            let supersede_target = match &should_store_action {
                Ok(DedupAction::Supersede(id)) => Some(("supersedes", id.clone())),
                Ok(DedupAction::Merge(id)) => Some(("derived_from", id.clone())),
                _ => None,
            };

            match mem.store(key, content, category, scope, entity_id).await {
                Ok(id) if !id.is_empty() => {
                    debug!(worker = %worker_name, id = %id, key = %key, scope = %scope, "insight stored");

                    // Create memory graph edge if dedup detected a relationship.
                    if let Some((relation, target_id)) = supersede_target {
                        if let Err(e) = mem.store_memory_edge(&id, &target_id, relation, 0.8).await
                        {
                            debug!(worker = %worker_name, "failed to store edge: {e}");
                        } else {
                            debug!(
                                worker = %worker_name,
                                source = %id,
                                target = %target_id,
                                relation = %relation,
                                "memory edge created"
                            );
                        }
                    }
                }
                Ok(_) => {
                    debug!(worker = %worker_name, key = %key, "store returned empty id (likely dedup)")
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

#[cfg(test)]
mod tests {
    use super::AgentWorker;
    use sigil_tasks::{Task, TaskId};

    #[test]
    fn set_task_sigil_metadata_preserves_existing_sigil_fields() {
        let mut task = Task::new(TaskId::from("sg-001"), "Runtime persistence");
        task.metadata = serde_json::json!({
            "sigil": {
                "hold": true,
                "hold_reason": "awaiting_review",
            }
        });

        AgentWorker::set_task_sigil_metadata(
            &mut task,
            "runtime",
            serde_json::json!({
                "session": {
                    "phase": "act",
                }
            }),
        );

        assert_eq!(
            task.metadata
                .pointer("/sigil/hold")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            task.metadata
                .pointer("/sigil/hold_reason")
                .and_then(|value| value.as_str()),
            Some("awaiting_review")
        );
        assert_eq!(
            task.metadata
                .pointer("/sigil/runtime/session/phase")
                .and_then(|value| value.as_str()),
            Some("act")
        );
    }

    #[test]
    fn set_task_sigil_metadata_preserves_legacy_metadata() {
        let mut task = Task::new(TaskId::from("sg-002"), "Legacy metadata");
        task.metadata = serde_json::json!("legacy");

        AgentWorker::set_task_sigil_metadata(
            &mut task,
            "runtime",
            serde_json::json!({
                "session": {
                    "phase": "verify",
                }
            }),
        );

        assert_eq!(
            task.metadata
                .pointer("/_legacy")
                .and_then(|value| value.as_str()),
            Some("legacy")
        );
        assert_eq!(
            task.metadata
                .pointer("/sigil/runtime/session/phase")
                .and_then(|value| value.as_str()),
            Some("verify")
        );
    }
}
