use aeqi_core::traits::{
    ChatRequest, Event, Insight, InsightCategory, LogObserver, LoopAction, Message, MessageContent,
    Observer, Provider, Role, Tool,
};
use aeqi_core::{Agent, AgentConfig, AssembledPrompt};
use aeqi_quests::{Quest, QuestOutcomeKind, QuestOutcomeRecord, QuestStatus};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::checkpoint::AgentCheckpoint;
use crate::event_store::EventStore;
use crate::execution_events::{EventBroadcaster, ExecutionEvent};
use crate::executor::TaskOutcome;
use crate::failure_analysis::{FailureAnalysis, FailureMode};
use crate::hook::Hook;
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::middleware::{MiddlewareAction, MiddlewareChain, Outcome, OutcomeStatus, WorkerContext};
use crate::notes::Notes;
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
    /// Native AEQI agent loop.
    Agent {
        provider: Arc<dyn aeqi_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        model: String,
    },
    /// Delegate to Claude Code CLI.
    ClaudeCode { cwd: PathBuf, max_budget_usd: f64 },
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
    pub system_prompt: String,
    pub assembled_prompt: Option<AssembledPrompt>,
    pub dispatch_bus: Arc<DispatchBus>,
    /// Snapshot of the assigned task, populated at assign() time.
    pub task_snapshot: Option<aeqi_quests::Quest>,
    /// Called once at the end of execute() with the final status and optional outcome record.
    pub on_complete: Option<Box<dyn FnOnce(QuestStatus, Option<QuestOutcomeRecord>) + Send + Sync>>,
    pub insight_store: Option<Arc<dyn Insight>>,
    pub reflect_provider: Option<Arc<dyn Provider>>,
    pub reflect_model: String,
    /// Project directory path for checkpoint storage.
    pub project_dir: Option<PathBuf>,
    /// Max task retries (handoff/failure) before auto-cancel.
    pub max_task_retries: u32,
    /// Optional shared blackboard for adaptive retry strategies.
    pub notes: Option<Arc<Notes>>,
    /// Optional event store for recording decisions (replaces audit log).
    pub event_store: Option<Arc<EventStore>>,
    /// Whether adaptive retry is enabled for this worker.
    pub adaptive_retry: bool,
    /// Model used for failure analysis when adaptive retry is enabled.
    pub failure_analysis_model: String,
    /// Middleware chain for composable execution behavior (guardrails, cost tracking, etc.).
    pub middleware_chain: Option<Arc<MiddlewareChain>>,
    /// Event broadcaster for real-time execution event streaming.
    pub event_broadcaster: Option<Arc<EventBroadcaster>>,
    /// Optional debounced write queue for batching reflection memory writes.
    pub write_queue: Option<Arc<tokio::sync::Mutex<aeqi_insights::debounce::WriteQueue>>>,
    /// Persistent agent UUID for entity-scoped memory. When set, memory queries
    /// include this agent's entity memories alongside domain/system memories.
    pub persistent_agent_id: Option<String>,
    /// Session store for recording worker transcripts.
    pub session_store: Option<Arc<crate::SessionStore>>,
}

impl AgentWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent_name: String,
        name: String,
        project_name: String,
        provider: Arc<dyn aeqi_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        system_prompt: String,
        model: String,
        dispatch_bus: Arc<DispatchBus>,
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
            system_prompt,
            assembled_prompt: None,
            dispatch_bus,
            task_snapshot: None,
            on_complete: None,
            insight_store: None,
            reflect_provider: None,
            reflect_model,
            project_dir: None,
            max_task_retries: 3,
            notes: None,
            event_store: None,
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            middleware_chain: None,
            event_broadcaster: None,
            write_queue: None,
            persistent_agent_id: None,
            session_store: None,
        }
    }

    pub fn new_claude_code(
        agent_name: String,
        name: String,
        project_name: String,
        cwd: PathBuf,
        max_budget_usd: f64,
        system_prompt: String,
        dispatch_bus: Arc<DispatchBus>,
    ) -> Self {
        Self {
            agent_name,
            name,
            project_name,
            state: WorkerState::Idle,
            hook: None,
            execution: WorkerExecution::ClaudeCode {
                cwd,
                max_budget_usd,
            },
            system_prompt,
            assembled_prompt: None,
            dispatch_bus,
            task_snapshot: None,
            on_complete: None,
            insight_store: None,
            reflect_provider: None,
            reflect_model: String::new(),
            project_dir: None,
            max_task_retries: 3,
            notes: None,
            event_store: None,
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            middleware_chain: None,
            event_broadcaster: None,
            write_queue: None,
            persistent_agent_id: None,
            session_store: None,
        }
    }

    pub fn with_insight_store(mut self, insight_store: Arc<dyn Insight>) -> Self {
        self.insight_store = Some(insight_store);
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

    /// Set the persistent agent UUID for entity-scoped memory.
    pub fn with_persistent_agent(mut self, agent_id: String) -> Self {
        self.persistent_agent_id = Some(agent_id);
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
        self.middleware_chain = Some(Arc::new(chain));
    }

    /// Set the event broadcaster for real-time execution event streaming.
    pub fn set_broadcaster(&mut self, broadcaster: Arc<EventBroadcaster>) {
        self.event_broadcaster = Some(broadcaster);
    }

    /// Set the debounced write queue for batching reflection memory writes.
    pub fn set_write_queue(
        &mut self,
        queue: Arc<tokio::sync::Mutex<aeqi_insights::debounce::WriteQueue>>,
    ) {
        self.write_queue = Some(queue);
    }

    /// Get the working directory for this worker.
    fn workdir(&self) -> Option<&std::path::Path> {
        self.project_dir.as_deref()
    }

    /// Capture an external checkpoint by inspecting git state in the worker's workdir.
    /// Saves the checkpoint to the project's `.aeqi/checkpoints/` directory.
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

    /// Assign a quest to this worker (set hook and snapshot the full quest).
    pub fn assign(&mut self, quest: &Quest) {
        self.hook = Some(Hook::new(quest.id.clone(), quest.name.clone()));
        self.task_snapshot = Some(quest.clone());
        self.state = WorkerState::Hooked;
    }

    /// Save a checkpoint recording this worker's progress on a task.
    /// Now a no-op — the scheduler handles task state through AgentRegistry;
    /// checkpoints written to a throwaway board were lost anyway.
    async fn save_checkpoint(&self, task_id: &str, progress: &str, _cost: f64, _turns: u32) {
        debug!(
            worker = %self.name,
            task = %task_id,
            progress = %progress,
            "checkpoint save skipped (scheduler manages task state)"
        );
    }

    async fn build_resume_brief(&self, quest: &Quest) -> String {
        let mut sections = Vec::new();

        if let Some(ref es) = self.event_store {
            let filter = crate::event_store::EventFilter {
                event_type: Some("decision".to_string()),
                quest_id: Some(quest.id.0.clone()),
                ..Default::default()
            };
            if let Ok(events) = es.query(&filter, 6, 0).await {
                if !events.is_empty() {
                    let lines = events
                        .iter()
                        .map(|event| {
                            let decision_type = event
                                .content
                                .get("decision_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let reasoning = event
                                .content
                                .get("reasoning")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            format!(
                                "- {} [{}] {}",
                                event.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
                                decision_type,
                                truncate_for_prompt(reasoning, 220),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    sections.push(format!("### Audit trail\n{lines}"));
                }
            }
        }

        if let Some(ref blackboard) = self.notes {
            let entries = blackboard
                .query(&self.project_name, &quest.labels, 5)
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
            .filter(|dispatch| is_relevant_dispatch(dispatch, &self.project_name, &quest.id.0))
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

    /// Execute the hooked work through the native AEQI agent runtime.
    /// Returns (outcome, cost_usd, turns_used) for the Scheduler to record.
    pub async fn execute(&mut self) -> Result<(TaskOutcome, RuntimeExecution, f64, u32)> {
        let hook = match &self.hook {
            Some(h) => h.clone(),
            None => {
                warn!(worker = %self.name, "no hook assigned, nothing to do");
                let runtime_outcome = RuntimeOutcome::done("no work assigned", Vec::new());
                let outcome = TaskOutcome::from_runtime_outcome(&runtime_outcome);
                let mut session = RuntimeSession::new(
                    "unassigned",
                    self.name.clone(),
                    self.project_name.clone(),
                    self.execution_model(),
                );
                session.mark_phase(RuntimePhase::Prime, "Worker had no hook assigned");
                session.finish(&runtime_outcome);
                // Fire on_complete for this early exit path.
                if let Some(cb) = self.on_complete.take() {
                    let outcome_record = Self::build_task_outcome_record(&runtime_outcome);
                    cb(QuestStatus::Done, Some(outcome_record));
                }
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

        // Extract parent_session_id from task labels (set by dispatch consumption).
        let parent_session_id = self.task_snapshot.as_ref().and_then(|t| {
            t.labels
                .iter()
                .find_map(|l| l.strip_prefix("parent_session_id:"))
                .map(String::from)
        });

        // Create a DB session for this worker execution.
        let worker_session_id = if let Some(ref ss) = self.session_store {
            let task_id_str = hook.task_id.0.clone();
            ss.create_session(
                &self.agent_name,
                "task",
                &task_id_str,
                parent_session_id.as_deref(),
                Some(&task_id_str),
            )
            .await
            .ok()
        } else {
            None
        };

        // Build WorkerContext for middleware chain.
        let task_description_for_ctx = self
            .task_snapshot
            .as_ref()
            .map(|t| t.description.clone())
            .unwrap_or_else(|| hook.subject.clone());
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
                    let runtime_outcome =
                        RuntimeOutcome::failed(format!("Middleware halted: {reason}"), Vec::new());
                    let outcome = TaskOutcome::from_runtime_outcome(&runtime_outcome);
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
                    // Fire on_complete for this early exit path.
                    if let Some(cb) = self.on_complete.take() {
                        let outcome_record =
                            Self::build_task_outcome_record(&runtime_execution.outcome);
                        cb(QuestStatus::Pending, Some(outcome_record));
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

        // The scheduler already marks in_progress before spawning — no need to do it here.

        // Use the task snapshot populated at assign() time.
        let task_snapshot = self.task_snapshot.clone();

        let mut task_context = match task_snapshot.as_ref() {
            Some(b) => {
                let mut ctx = format!("## Task: {}\n\n", b.name);
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

        // Record the task context into the worker session.
        if let (Some(ss), Some(sid)) = (&self.session_store, &worker_session_id) {
            let _ = ss
                .record_by_session(sid, "user", &task_context, Some("worker"))
                .await;
        }

        // Enrich system prompt with dynamic memory recall via query planner.
        // Primers are already assembled into self.system_prompt by assemble_prompts().
        // When a persistent agent UUID is set, also recall entity-scoped memories.
        let enriched_system_prompt = if let Some(ref mem) = self.insight_store {
            // Try query planner first — generates typed, prioritized queries.
            let entries = match std::panic::catch_unwind(|| {
                aeqi_insights::query_planner::QueryPlanner::plan(
                    &task_context,
                    Some(&self.project_name),
                )
            }) {
                Ok(plan) => {
                    let mut all_entries = Vec::new();
                    for typed_query in &plan.queries {
                        let query = aeqi_core::traits::InsightQuery::new(
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
                    let query = aeqi_core::traits::InsightQuery::new(&task_context, 30);
                    mem.search(&query).await.unwrap_or_default()
                }
            };

            // Also recall entity-scoped memories for persistent agents.
            let mut all = entries;
            if let Some(ref agent_id) = self.persistent_agent_id {
                let eq = aeqi_core::traits::InsightQuery::new(&task_context, 10)
                    .with_agent(agent_id.clone());
                if let Ok(entity_entries) = mem.search(&eq).await {
                    debug!(
                        worker = %self.name,
                        agent_id = %agent_id,
                        entity_memories = entity_entries.len(),
                        "entity memory recall for persistent agent"
                    );
                    all.extend(entity_entries);
                }
            }

            if !all.is_empty() {
                let dynamic = all
                    .iter()
                    .map(|e| {
                        format!(
                            "- [{}] {}: {}",
                            e.agent_id.as_deref().unwrap_or("global"),
                            e.key,
                            e.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "{}\n\n---\n\n## Dynamic Recall\n{dynamic}",
                    self.system_prompt
                )
            } else {
                self.system_prompt.clone()
            }
        } else {
            self.system_prompt.clone()
        };
        runtime_session.mark_phase(
            RuntimePhase::Act,
            format!(
                "Executing native AEQI agent loop{}",
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
                    &enriched_system_prompt,
                )
                .await
                .map(|agent_result| {
                    let cost = aeqi_providers::estimate_cost(
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
            WorkerExecution::ClaudeCode {
                cwd,
                max_budget_usd,
            } => {
                info!(
                    worker = %self.name,
                    cwd = %cwd.display(),
                    budget = max_budget_usd,
                    "dispatching to claude code"
                );
                let executor = crate::claude_code::ClaudeCodeExecutor::new(cwd.clone())
                    .with_budget(*max_budget_usd);
                // Pass the enriched system prompt (persona, memory, blackboard,
                // resume brief) to Claude Code.
                executor
                    .execute_with_identity(&enriched_system_prompt, &task_context)
                    .await
                    .map(|cc_result| {
                        info!(
                            worker = %self.name,
                            model = %cc_result.model,
                            cost_usd = cc_result.cost_usd,
                            turns = cc_result.num_turns,
                            session = %cc_result.session_id,
                            "claude code execution complete"
                        );
                        (cc_result.text, cc_result.cost_usd, cc_result.num_turns)
                    })
            }
        };

        // Record the agent result into the worker session.
        if let (Some(ss), Some(sid)) = (&self.session_store, &worker_session_id) {
            let content = match &raw_result {
                Ok((text, _, _)) => text.clone(),
                Err(e) => format!("ERROR: {e}"),
            };
            let _ = ss
                .record_by_session(sid, "assistant", &content, Some("worker"))
                .await;
        }

        // Fire-and-forget reflection so the worker slot does not wait on memory extraction.
        if let Ok((ref result_text, _, _)) = raw_result
            && let (Some(mem), Some(provider)) =
                (self.insight_store.clone(), self.reflect_provider.clone())
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
        let runtime_artifacts = self.collect_runtime_artifacts();
        let (outcome, mut runtime_outcome, cost, turns) = match raw_result {
            Ok((result_text, cost, turns)) => {
                let runtime_outcome =
                    RuntimeOutcome::from_agent_response(&result_text, runtime_artifacts);
                let outcome = TaskOutcome::from_runtime_outcome(&runtime_outcome);
                (outcome, runtime_outcome, cost, turns)
            }
            Err(e) => {
                // Run middleware on_error.
                if let Some(ref chain) = self.middleware_chain {
                    let error_str = e.to_string();
                    chain.run_on_error(&mut worker_ctx, &error_str).await;
                }
                let runtime_outcome = RuntimeOutcome::failed(e.to_string(), runtime_artifacts);
                let outcome = TaskOutcome::from_runtime_outcome(&runtime_outcome);
                (outcome, runtime_outcome, 0.0, 0)
            }
        };
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

        // Process outcome: save checkpoint, determine final task status.
        // `final_task_status` is used by the on_complete callback below.
        let mut final_task_status = QuestStatus::Pending;
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
                final_task_status = QuestStatus::Done;
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
                final_task_status = QuestStatus::Blocked;
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
                // Check if this handoff would exceed max retries.
                let current_retry = self
                    .task_snapshot
                    .as_ref()
                    .map(|t| t.retry_count)
                    .unwrap_or(0);
                if current_retry + 1 >= self.max_task_retries {
                    final_task_status = QuestStatus::Cancelled;
                    warn!(
                        worker = %self.name,
                        task = %hook.task_id,
                        retries = current_retry + 1,
                        "task auto-cancelled after max retries (handoff)"
                    );
                } else {
                    final_task_status = QuestStatus::Pending;
                }
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

                // Attempt LLM-based failure analysis.
                let failure_result: Option<(String, FailureMode)> = if self.adaptive_retry
                    && let Some(ref provider) = self.reflect_provider
                {
                    let fa_model = if self.failure_analysis_model.is_empty() {
                        self.reflect_model.clone()
                    } else {
                        self.failure_analysis_model.clone()
                    };
                    if !fa_model.is_empty() {
                        let (task_desc, task_labels) = self
                            .task_snapshot
                            .as_ref()
                            .map(|t| (t.description.clone(), t.labels.clone()))
                            .unwrap_or_default();
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

                                // Record decision event.
                                if let Some(ref es) = self.event_store {
                                    let _ = es
                                        .emit(
                                            "decision",
                                            None,
                                            None,
                                            Some(&hook.task_id.0),
                                            &serde_json::json!({
                                                "decision_type": "FailureAnalyzed",
                                                "agent": self.agent_name,
                                                "reasoning": format!(
                                                    "Mode: {:?}, Reasoning: {}",
                                                    analysis.mode, analysis.reasoning
                                                ),
                                            }),
                                        )
                                        .await;
                                }

                                // Mode-specific: query blackboard for MissingContext.
                                let mut enrichment = analysis.enrich_description();
                                if analysis.mode == FailureMode::MissingContext
                                    && let Some(ref bb) = self.notes
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

                // Determine auto-cancel from task snapshot retry_count.
                let current_retry = self
                    .task_snapshot
                    .as_ref()
                    .map(|t| t.retry_count)
                    .unwrap_or(0);
                let auto_cancelled = current_retry + 1 >= self.max_task_retries;
                let failure_mode = failure_result.as_ref().map(|(_, m)| *m);
                // Also treat ExternalBlocker / BudgetExhausted as terminal.
                let is_blocker = matches!(
                    failure_mode,
                    Some(FailureMode::ExternalBlocker) | Some(FailureMode::BudgetExhausted)
                );

                if auto_cancelled {
                    final_task_status = QuestStatus::Cancelled;
                    warn!(
                        worker = %self.name,
                        task = %hook.task_id,
                        retries = current_retry + 1,
                        "task auto-cancelled after max retries"
                    );
                    if let Some(ref es) = self.event_store {
                        let _ = es.emit(
                            "decision",
                            None,
                            None,
                            Some(&hook.task_id.0),
                            &serde_json::json!({
                                "decision_type": "TaskCancelled",
                                "agent": self.agent_name,
                                "reasoning": format!("Auto-cancelled after max retries: {}", error_text),
                            }),
                        ).await;
                    }
                } else if is_blocker {
                    final_task_status = QuestStatus::Blocked;
                    warn!(
                        worker = %self.name,
                        task = %hook.task_id,
                        mode = ?failure_mode,
                        "task blocked by failure analysis"
                    );
                } else {
                    final_task_status = QuestStatus::Pending;
                }
                self.state = WorkerState::Failed(error_text.to_string());
            }
        }

        // Close the worker session now that the outcome is determined.
        if let (Some(ss), Some(sid)) = (&self.session_store, &worker_session_id) {
            let _ = ss.close_session(sid).await;
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

        // Fire the on_complete callback with the final status and outcome record.
        if let Some(cb) = self.on_complete.take() {
            let final_record = if final_task_status == QuestStatus::Done {
                Some(Self::build_task_outcome_record(&runtime_execution.outcome))
            } else {
                None
            };
            cb(final_task_status, final_record);
        }

        self.hook = None;
        Ok((outcome, runtime_execution, cost, turns))
    }

    fn execution_model(&self) -> Option<String> {
        match &self.execution {
            WorkerExecution::Agent { model, .. } => Some(model.clone()),
            WorkerExecution::ClaudeCode { .. } => Some("claude-code".to_string()),
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
            Ok(value) => {
                self.persist_runtime_value(task_id, value).await;
                self.persist_task_outcome(task_id, &runtime.outcome).await;
            }
            Err(error) => warn!(
                worker = %self.name,
                task = %task_id,
                error = %error,
                "failed to serialize runtime execution for task metadata"
            ),
        }
    }

    async fn persist_runtime_value(&self, task_id: &str, _runtime: serde_json::Value) {
        // No-op — was writing to a throwaway board. Runtime metadata now flows
        // through the on_complete callback and the scheduler.
        debug!(
            worker = %self.name,
            task = %task_id,
            "runtime value persist skipped (scheduler manages task state)"
        );
    }

    /// Build a QuestOutcomeRecord from a RuntimeOutcome (used by on_complete callback).
    fn build_task_outcome_record(outcome: &RuntimeOutcome) -> QuestOutcomeRecord {
        QuestOutcomeRecord {
            kind: Self::task_outcome_kind(outcome),
            summary: outcome.summary.clone(),
            reason: outcome.reason.clone(),
            next_action: outcome.next_action.clone(),
        }
    }

    async fn persist_task_outcome(&self, task_id: &str, outcome: &RuntimeOutcome) {
        // The outcome record now flows through the on_complete callback.
        debug!(
            worker = %self.name,
            task = %task_id,
            kind = ?Self::task_outcome_kind(outcome),
            "task outcome persist skipped (delivered via on_complete callback)"
        );
    }

    fn task_outcome_kind(outcome: &RuntimeOutcome) -> QuestOutcomeKind {
        match outcome.status {
            crate::runtime::RuntimeOutcomeStatus::Done => QuestOutcomeKind::Done,
            crate::runtime::RuntimeOutcomeStatus::Blocked => QuestOutcomeKind::Blocked,
            crate::runtime::RuntimeOutcomeStatus::Handoff => QuestOutcomeKind::Handoff,
            crate::runtime::RuntimeOutcomeStatus::Failed => QuestOutcomeKind::Failed,
        }
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
        provider: Arc<dyn aeqi_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        model: &str,
        task_context: &str,
        system_prompt: &str,
    ) -> Result<aeqi_core::AgentResult> {
        let observer: Arc<dyn Observer> = if let Some(ref chain) = self.middleware_chain {
            let mut worker_ctx = crate::middleware::WorkerContext::new(
                self.hook
                    .as_ref()
                    .map(|h| h.task_id.0.as_str())
                    .unwrap_or("unknown"),
                task_context.chars().take(500).collect::<String>(),
                &self.agent_name,
                &self.project_name,
            );
            // Signal to ContextCompressionMiddleware that the agent loop handles compaction.
            worker_ctx.agent_compaction_active = true;
            if let WorkerExecution::Agent { ref model, .. } = self.execution {
                worker_ctx.model = model.clone();
            }
            Arc::new(MiddlewareObserver::from_arc(
                Arc::clone(chain),
                worker_ctx,
                Arc::new(LogObserver),
            ))
        } else {
            Arc::new(LogObserver)
        };

        // Resolve context window from model name.
        let context_window = aeqi_providers::context_window_for_model(model);

        // Resolve persist_dir: use project dir's .aeqi/persist/{worker}, or temp on demand.
        let persist_dir = self.project_dir.as_ref().map(|dir| {
            let p = dir.join(".aeqi").join("persist").join(&self.name);
            if !p.exists() {
                let _ = std::fs::create_dir_all(&p);
            }
            p
        });

        // Resolve session file for checkpoint/resume.
        let session_file = self.project_dir.as_ref().map(|dir| {
            let task_id = self
                .hook
                .as_ref()
                .map(|h| h.task_id.0.as_str())
                .unwrap_or("unknown");
            dir.join(".aeqi")
                .join("sessions")
                .join(format!("{}.json", task_id))
        });

        let agent_config = AgentConfig {
            model: model.to_string(),
            max_iterations: 20,
            name: self.agent_name.clone(),
            context_window,
            persist_dir,
            session_file,
            ..Default::default()
        };

        let mut agent = Agent::new(
            agent_config,
            provider,
            tools,
            observer,
            system_prompt.to_string(),
        );

        if let Some(ref mem) = self.insight_store {
            agent = agent.with_memory(mem.clone());
        }

        // Wire chat stream: create sender, subscribe in background task to
        // forward ChatStreamEvents to the EventBroadcaster as ChatStream events.
        if let Some(ref broadcaster) = self.event_broadcaster {
            let task_id = self
                .hook
                .as_ref()
                .map(|h| h.task_id.0.clone())
                .unwrap_or_default();
            let (sender, mut rx) = aeqi_core::ChatStreamSender::new(512);
            let bc = Arc::clone(broadcaster);
            let tid = task_id.clone();
            tokio::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    bc.publish(crate::execution_events::ExecutionEvent::ChatStream {
                        task_id: tid.clone(),
                        chat_id: 0, // Filled by MessageRouter when routing
                        event,
                    });
                }
            });
            agent = agent.with_chat_stream(sender);
        }

        agent.run(task_context).await
    }

    /// Detached reflection — runs in a separate tokio task, no &self needed.
    async fn reflect_detached(
        worker_name: String,
        task_context: String,
        result_text: String,
        model: String,
        mem: Arc<dyn Insight>,
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

    async fn store_routed_insights_static(worker_name: &str, text: &str, mem: &Arc<dyn Insight>) {
        use aeqi_insights::dedup::{DedupAction, DedupCandidate, DedupPipeline, SimilarMemory};

        let dedup = DedupPipeline::default();

        for line in text.lines() {
            let line = line.trim();
            if line == "NONE" || line.is_empty() {
                continue;
            }

            // Determine whether memory is agent-scoped (SELF) or global (DOMAIN/SYSTEM).
            let (is_self, rest) = if let Some(r) = line.strip_prefix("DOMAIN ") {
                (false, r)
            } else if let Some(r) = line.strip_prefix("SYSTEM ") {
                (false, r)
            } else if let Some(r) = line.strip_prefix("SELF ") {
                (true, r)
            } else if let Some((cat_str, _rest)) = line.split_once(':') {
                let cat_str = cat_str.trim();
                if matches!(cat_str, "FACT" | "PROCEDURE" | "PREFERENCE" | "CONTEXT") {
                    (false, line)
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
                "FACT" => InsightCategory::Fact,
                "PROCEDURE" => InsightCategory::Procedure,
                "PREFERENCE" => InsightCategory::Preference,
                "CONTEXT" => InsightCategory::Context,
                _ => continue,
            };

            let key = key.trim();
            let content = content.trim();
            if key.is_empty() || content.is_empty() {
                continue;
            }

            // ── Dedup check: search for similar existing memories ──
            let should_store_action = async {
                let query = aeqi_core::traits::InsightQuery::new(key, 5);
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

            let agent_id_for_store: Option<&str> = if is_self { Some(worker_name) } else { None };

            // Capture dedup relation targets for edge creation.
            let supersede_target = match &should_store_action {
                Ok(DedupAction::Supersede(id)) => Some(("supersedes", id.clone())),
                Ok(DedupAction::Merge(id)) => Some(("derived_from", id.clone())),
                _ => None,
            };

            match mem.store(key, content, category, agent_id_for_store).await {
                Ok(id) if !id.is_empty() => {
                    debug!(worker = %worker_name, id = %id, key = %key, "insight stored");

                    // Create memory graph edge if dedup detected a relationship.
                    if let Some((relation, target_id)) = supersede_target {
                        if let Err(e) = mem.store_insight_edge(&id, &target_id, relation, 0.8).await
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

                    // Infer additional edges: search scoped to same agent
                    let mut edge_query = aeqi_core::traits::InsightQuery::new(content, 3);
                    if let Some(aid) = agent_id_for_store {
                        edge_query = edge_query.with_agent(aid.to_string());
                    }
                    if let Ok(related) = mem.search(&edge_query).await {
                        for entry in &related {
                            if entry.id == id {
                                continue;
                            }
                            if aeqi_insights::dedup::is_support(content, &entry.content) {
                                let _ = mem
                                    .store_insight_edge(&id, &entry.id, "supports", 0.7)
                                    .await;
                                debug!(
                                    worker = %worker_name,
                                    source = %id, target = %entry.id,
                                    "supports edge created"
                                );
                            } else if entry.score > 0.7 && entry.key != key {
                                let _ = mem
                                    .store_insight_edge(&id, &entry.id, "related_to", 0.5)
                                    .await;
                                debug!(
                                    worker = %worker_name,
                                    source = %id, target = %entry.id,
                                    "related_to edge created"
                                );
                            }
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
        DispatchKind::DelegateRequest { reply_to, .. } => reply_to.as_deref() == Some(task_id),
        DispatchKind::DelegateResponse { reply_to, .. } => reply_to == task_id,
        DispatchKind::HumanEscalation {
            project: dispatch_project,
            task_id: id,
            ..
        } => dispatch_project == project && id == task_id,
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

// ---------------------------------------------------------------------------
// MiddlewareObserver — bridges the middleware chain into the agent loop
// ---------------------------------------------------------------------------

use crate::middleware::{ToolCall as MwToolCall, ToolResult as MwToolResult};

struct MiddlewareObserver {
    chain: Arc<MiddlewareChain>,
    ctx: tokio::sync::Mutex<WorkerContext>,
    inner: Arc<dyn Observer>,
}

impl MiddlewareObserver {
    fn from_arc(chain: Arc<MiddlewareChain>, ctx: WorkerContext, inner: Arc<dyn Observer>) -> Self {
        Self {
            chain,
            ctx: tokio::sync::Mutex::new(ctx),
            inner,
        }
    }

    fn map_action(action: MiddlewareAction) -> LoopAction {
        match action {
            MiddlewareAction::Continue | MiddlewareAction::Skip => LoopAction::Continue,
            MiddlewareAction::Halt(reason) => LoopAction::Halt(reason),
            MiddlewareAction::Inject(msgs) => LoopAction::Inject(msgs),
        }
    }
}

#[async_trait::async_trait]
impl Observer for MiddlewareObserver {
    async fn record(&self, event: Event) {
        self.inner.record(event).await;
    }

    fn name(&self) -> &str {
        "middleware-bridge"
    }

    async fn before_model(&self, _iteration: u32) -> LoopAction {
        let mut ctx = self.ctx.lock().await;
        Self::map_action(self.chain.run_before_model(&mut ctx).await)
    }

    async fn after_model(
        &self,
        _iteration: u32,
        prompt_tokens: u32,
        completion_tokens: u32,
    ) -> LoopAction {
        let mut ctx = self.ctx.lock().await;
        ctx.cost_usd += aeqi_providers::estimate_cost(&ctx.model, prompt_tokens, completion_tokens);
        Self::map_action(self.chain.run_after_model(&mut ctx).await)
    }

    async fn before_tool(&self, tool_name: &str, input: &serde_json::Value) -> LoopAction {
        let mut ctx = self.ctx.lock().await;
        let call = MwToolCall {
            name: tool_name.to_string(),
            input: input.to_string(),
        };
        Self::map_action(self.chain.run_before_tool(&mut ctx, &call).await)
    }

    async fn after_tool(&self, tool_name: &str, output: &str, is_error: bool) -> LoopAction {
        let mut ctx = self.ctx.lock().await;
        let call = MwToolCall {
            name: tool_name.to_string(),
            input: String::new(),
        };
        let result = MwToolResult {
            success: !is_error,
            output: output.chars().take(500).collect(),
        };
        ctx.tool_call_history.push(call.clone());
        Self::map_action(self.chain.run_after_tool(&mut ctx, &call, &result).await)
    }

    async fn on_error(&self, _iteration: u32, error: &str) -> LoopAction {
        let mut ctx = self.ctx.lock().await;
        Self::map_action(self.chain.run_on_error(&mut ctx, error).await)
    }

    async fn after_turn(
        &self,
        _iteration: u32,
        response_text: &str,
        stop_reason: &str,
    ) -> LoopAction {
        let mut ctx = self.ctx.lock().await;
        Self::map_action(
            self.chain
                .run_after_turn(&mut ctx, response_text, stop_reason)
                .await,
        )
    }

    async fn collect_attachments(
        &self,
        _iteration: u32,
    ) -> Vec<aeqi_core::traits::ContextAttachment> {
        let mut ctx = self.ctx.lock().await;
        self.chain.run_collect_enrichments(&mut ctx).await
    }
}
