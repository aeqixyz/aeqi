use anyhow::Result;
use system_tasks::{TaskStatus, TaskBoard};
use system_core::config::{ExecutionMode, ProjectTeamConfig};
use system_core::traits::{Memory, Provider, Tool};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use crate::checkpoint::AgentCheckpoint;
use crate::cost_ledger::{CostLedger, CostEntry};
use crate::emotional_state::EmotionalState;
use crate::executor::{ClaudeCodeExecutor, TaskOutcome};
use crate::metrics::SystemMetrics;
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::project::Project;
use crate::agent_worker::AgentWorker;

/// Max resolution attempts at the project level before escalating to Focal Agent.
/// Each attempt spawns a new worker to try to answer the blocker question.
const MAX_PROJECT_RESOLUTION_ATTEMPTS: u32 = 1;

/// Label prefix for tracking escalation depth on beads.
const ESCALATION_LABEL_PREFIX: &str = "escalation:";

/// A running spirit with age tracking for timeout detection.
struct TrackedWorker {
    handle: tokio::task::JoinHandle<()>,
    task_id: String,
    started_at: std::time::Instant,
}

/// Supervisor: per-rig supervisor. Runs patrol cycles, manages workers,
/// detects stuck/orphaned beads, handles escalation, reports to Focal Agent.
pub struct Supervisor {
    pub project_name: String,
    pub max_workers: u32,
    pub patrol_interval_secs: u64,
    pub dispatch_bus: Arc<DispatchBus>,
    pub tasks: Arc<Mutex<TaskBoard>>,
    /// Execution mode for this rig's workers.
    pub execution_mode: ExecutionMode,
    // Agent-mode fields (used when execution_mode == Agent).
    pub provider: Arc<dyn Provider>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub model: String,
    pub identity: system_core::Identity,
    // ClaudeCode-mode fields (used when execution_mode == ClaudeCode).
    /// Rig's repo path for Claude Code working directory.
    pub repo: Option<std::path::PathBuf>,
    /// Max turns per Claude Code execution.
    pub cc_max_turns: u32,
    /// Max budget per Claude Code execution.
    pub cc_max_budget_usd: Option<f64>,
    /// Background worker tasks with age tracking.
    running_tasks: Vec<TrackedWorker>,
    /// Timeout in seconds for spirit execution. Hung spirits are aborted after this.
    worker_timeout_secs: u64,
    /// Last reported state — only send mail on change.
    last_report: (usize, usize),
    /// Shared cost ledger for budget enforcement.
    pub cost_ledger: Option<Arc<CostLedger>>,
    /// Shared metrics registry.
    pub metrics: Option<Arc<SystemMetrics>>,
    /// Per-project team config — which agents work on this project.
    pub team: Option<ProjectTeamConfig>,
    /// Name of the project's team leader to escalate to first.
    pub escalation_target: String,
    /// Name of the system team leader to escalate to if project leader can't resolve.
    pub system_escalation_target: String,
    /// Memory instance for spirit reflection (project-scoped).
    pub memory: Option<Arc<dyn Memory>>,
    /// Provider used for post-execution reflection (cheap model).
    pub reflect_provider: Option<Arc<dyn Provider>>,
    /// Model used for reflection extraction.
    pub reflect_model: String,
    /// Fires when a quest closes (passed through to spirits).
    pub task_notify: Arc<Notify>,
    /// Emotional state tracking (trust, mood, interaction count).
    pub emotional_state: Option<Arc<Mutex<EmotionalState>>>,
    /// Path to save emotional state (agent's .sigil dir).
    pub emotional_state_path: Option<std::path::PathBuf>,
}

impl Supervisor {
    pub fn new(project: &Project, provider: Arc<dyn Provider>, tools: Vec<Arc<dyn Tool>>, dispatch_bus: Arc<DispatchBus>) -> Self {
        Self {
            project_name: project.name.clone(),
            max_workers: project.max_workers,
            patrol_interval_secs: 60,
            dispatch_bus,
            tasks: project.tasks.clone(),
            execution_mode: ExecutionMode::Agent,
            provider,
            tools,
            model: project.model.clone(),
            identity: project.project_identity.clone(),
            repo: None,
            cc_max_turns: 25,
            cc_max_budget_usd: None,
            running_tasks: Vec::new(),
            worker_timeout_secs: project.worker_timeout_secs,
            last_report: (0, 0),
            team: None,
            escalation_target: "aurelia".to_string(),
            system_escalation_target: "aurelia".to_string(),
            cost_ledger: None,
            metrics: None,
            memory: None,
            reflect_provider: None,
            reflect_model: String::new(),
            task_notify: project.task_notify.clone(),
            emotional_state: None,
            emotional_state_path: None,
        }
    }

    /// Set the per-project team config and escalation targets.
    pub fn set_team(&mut self, team: ProjectTeamConfig, system_leader: &str) {
        self.escalation_target = team.leader.clone();
        self.system_escalation_target = system_leader.to_string();
        self.team = Some(team);
    }

    /// Set execution mode to Claude Code with rig-specific settings.
    pub fn set_claude_code_mode(
        &mut self,
        repo: std::path::PathBuf,
        model: String,
        max_turns: u32,
        max_budget_usd: Option<f64>,
    ) {
        self.execution_mode = ExecutionMode::ClaudeCode;
        self.repo = Some(repo);
        self.model = model;
        self.cc_max_turns = max_turns;
        self.cc_max_budget_usd = max_budget_usd;
    }

    /// Create a worker based on the rig's execution mode.
    async fn create_worker(&self, worker_name: String) -> AgentWorker {
        // Enrich identity with emotional state context if available.
        let identity = if let Some(ref emo) = self.emotional_state {
            let emo_guard = emo.lock().await;
            let mut id = self.identity.clone();
            let emo_ctx = emo_guard.as_context();
            drop(emo_guard);
            let existing = id.memory.unwrap_or_default();
            id.memory = Some(if existing.is_empty() {
                emo_ctx
            } else {
                format!("{existing}\n\n{emo_ctx}")
            });
            id
        } else {
            self.identity.clone()
        };

        let mut worker = match self.execution_mode {
            ExecutionMode::Agent => AgentWorker::new(
                worker_name,
                self.project_name.clone(),
                self.provider.clone(),
                self.tools.clone(),
                identity.clone(),
                self.model.clone(),
                self.dispatch_bus.clone(),
                self.tasks.clone(),
                self.task_notify.clone(),
            ),
            ExecutionMode::ClaudeCode => {
                let workdir = self.repo.clone().unwrap_or_default();
                let executor = ClaudeCodeExecutor::new(
                    workdir,
                    self.model.clone(),
                    self.cc_max_turns,
                    self.cc_max_budget_usd,
                );
                AgentWorker::new_claude_code(
                    worker_name,
                    self.project_name.clone(),
                    executor,
                    identity.clone(),
                    self.dispatch_bus.clone(),
                    self.tasks.clone(),
                    self.task_notify.clone(),
                )
            }
        };
        if let Some(ref mem) = self.memory {
            worker = worker.with_memory(mem.clone());
        }
        if let Some(ref provider) = self.reflect_provider {
            worker = worker.with_reflect(provider.clone(), self.reflect_model.clone());
        }
        worker
    }

    /// Run one patrol cycle: reap finished tasks, detect timeouts,
    /// assign + launch ready work, handle blocked beads, report status.
    ///
    /// Spirit execution is fully non-blocking — each worker runs as a background
    /// tokio task. The daemon loop never stalls waiting for workers.
    pub async fn patrol(&mut self) -> Result<()> {
        let patrol_start = std::time::Instant::now();
        debug!(project = %self.project_name, "patrol cycle");

        // 0. Reload beads from disk to pick up externally-created beads
        //    (e.g., from `sg assign` CLI or Claude Code workers).
        {
            let mut store = self.tasks.lock().await;
            if let Err(e) = store.reload() {
                warn!(project = %self.project_name, error = %e, "failed to reload beads from disk");
            }
        }

        // 1. Reap completed tasks + detect timed-out spirits.
        let timeout = std::time::Duration::from_secs(self.worker_timeout_secs);
        let mut timed_out = Vec::new();
        self.running_tasks.retain(|t| {
            if t.handle.is_finished() {
                return false;
            }
            if t.started_at.elapsed() > timeout {
                t.handle.abort();
                timed_out.push(t.task_id.clone());
                return false;
            }
            true
        });

        // Reset timed-out quests back to Pending and notify shadow.
        if !timed_out.is_empty()
            && let Some(ref m) = self.metrics {
                m.workers_timed_out.inc_by(timed_out.len() as u64);
            }
        for task_id in timed_out {
            warn!(
                project = %self.project_name,
                task = %task_id,
                timeout_secs = self.worker_timeout_secs,
                "spirit timed out, aborting and resetting quest"
            );

            // Capture external checkpoint before resetting — preserve git state evidence.
            if let Some(ref repo) = self.repo {
                match AgentCheckpoint::capture(repo) {
                    Ok(checkpoint) => {
                        let checkpoint: AgentCheckpoint = checkpoint
                            .with_quest_id(&task_id)
                            .with_worker_name(format!("timeout-{}", task_id))
                            .with_progress_notes(format!(
                                "Spirit timed out after {}s — checkpoint captured externally by scout",
                                self.worker_timeout_secs
                            ));

                        let cp_path = AgentCheckpoint::path_for_quest(repo, &task_id);
                        if let Err(e) = checkpoint.write(&cp_path) {
                            warn!(project = %self.project_name, task = %task_id, error = %e, "failed to write timeout checkpoint");
                        } else {
                            info!(project = %self.project_name, task = %task_id, files = checkpoint.modified_files.len(), "timeout checkpoint captured");
                        }
                    }
                    Err(e) => {
                        warn!(project = %self.project_name, task = %task_id, error = %e, "failed to capture timeout checkpoint");
                    }
                }
            }

            {
                let mut store = self.tasks.lock().await;
                let _ = store.update(&task_id, |q| {
                    q.status = TaskStatus::Pending;
                    q.assignee = None;
                });
            }
            self.dispatch_bus
                .send(Dispatch::new_typed(
                    &format!("scout-{}", self.project_name),
                    &self.escalation_target,
                    DispatchKind::WorkerCrashed {
                        project: self.project_name.clone(),
                        worker: format!("timeout-{}", task_id),
                        error: format!(
                            "Spirit timed out after {}s on quest {}",
                            self.worker_timeout_secs, task_id
                        ),
                    },
                ))
                .await;
        }

        // 2. Handle blocked beads — attempt resolution or escalate.
        self.handle_blocked_tasks().await;

        // 3. Assign + launch ready beads as background tasks.
        let ready_tasks = {
            let store = self.tasks.lock().await;
            store.ready().into_iter().cloned().collect::<Vec<_>>()
        };

        for bead in ready_tasks {
            if self.running_tasks.len() >= self.max_workers as usize {
                break;
            }

            if bead.assignee.is_some() {
                continue;
            }

            // Budget check: don't spawn if global daily budget or project budget is exhausted.
            if let Some(ref ledger) = self.cost_ledger
                && !ledger.can_afford_project(&self.project_name) {
                    let (spent, budget, _) = ledger.project_budget_status(&self.project_name);
                    warn!(
                        project = %self.project_name,
                        spent_usd = spent,
                        budget_usd = budget,
                        "budget exhausted for project, skipping quest"
                    );
                    break;
                }

            let spirit_idx = self.running_tasks.len() + 1;
            let spirit_name = format!("{}-worker-{}", self.project_name, spirit_idx);
            info!(
                project = %self.project_name,
                worker = %spirit_name,
                bead = %bead.id,
                subject = %bead.subject,
                mode = ?self.execution_mode,
                "assigning work"
            );

            let mut worker = self.create_worker(spirit_name).await;

            // If there's a previous external checkpoint for this quest, inject it into the
            // quest description so the new spirit has context about the prior attempt's git state.
            if let Some(ref repo) = self.repo {
                let cp_path = AgentCheckpoint::path_for_quest(repo, &bead.id.0);
                match AgentCheckpoint::read(&cp_path) {
                    Ok(Some(checkpoint)) => {
                        let checkpoint_ctx: String = checkpoint.as_context();
                        let mut store = self.tasks.lock().await;
                        let _ = store.update(&bead.id.0, |b| {
                            b.description.push_str(&format!(
                                "\n\n---\n{checkpoint_ctx}"
                            ));
                        });
                        info!(
                            project = %self.project_name,
                            task = %bead.id,
                            "injected external checkpoint context from previous spirit"
                        );
                        // Remove the checkpoint file — it's been consumed.
                        let _ = AgentCheckpoint::remove(&cp_path);
                    }
                    Ok(None) => {} // No checkpoint — normal launch.
                    Err(e) => {
                        warn!(
                            project = %self.project_name,
                            task = %bead.id,
                            error = %e,
                            "failed to read checkpoint — launching without it"
                        );
                    }
                }
            }

            worker.assign(&bead);

            let task_id = bead.id.0.clone();

            // Fire-and-forget: worker handles its own bead updates + mail notifications.
            let project_name_task = self.project_name.clone();
            let quest_id_task = task_id.clone();
            let cost_ledger = self.cost_ledger.clone();
            let metrics = self.metrics.clone();
            if let Some(ref m) = self.metrics {
                m.workers_spawned.inc();
            }
            let emo_state = self.emotional_state.clone();
            let emo_path = self.emotional_state_path.clone();
            let handle = tokio::spawn(async move {
                let start = std::time::Instant::now();
                match worker.execute().await {
                    Ok((outcome, cost_usd, turns)) => {
                        let duration_secs = start.elapsed().as_secs_f64();
                        debug!(
                            project = %project_name_task,
                            outcome = ?std::mem::discriminant(&outcome),
                            cost_usd,
                            turns,
                            duration_secs,
                            "worker finished"
                        );

                        // Record to cost ledger.
                        if let Some(ref ledger) = cost_ledger {
                            let _ = ledger.record(CostEntry {
                                project: project_name_task.clone(),
                                task_id: quest_id_task.clone(),
                                worker: "worker".to_string(),
                                cost_usd,
                                turns,
                                timestamp: chrono::Utc::now(),
                            });
                        }

                        // Record metrics.
                        if let Some(ref m) = metrics {
                            m.worker_duration_seconds.observe(duration_secs);
                            m.task_cost_usd.observe(cost_usd);
                            match &outcome {
                                TaskOutcome::Done(_) => m.tasks_completed.inc(),
                                TaskOutcome::Blocked { .. } | TaskOutcome::Handoff { .. } => m.tasks_blocked.inc(),
                                TaskOutcome::Failed(_) => m.tasks_failed.inc(),
                            }
                        }

                        // Update emotional state based on outcome.
                        if let Some(ref emo) = emo_state {
                            let mut state = emo.lock().await;
                            match &outcome {
                                TaskOutcome::Done(_) => state.record_positive(),
                                TaskOutcome::Blocked { .. } | TaskOutcome::Handoff { .. } => state.record_interaction(),
                                TaskOutcome::Failed(_) => state.record_negative(),
                            }
                            if let Some(ref path) = emo_path
                                && let Err(e) = state.save(path)
                            {
                                warn!(error = %e, "failed to save emotional state");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            project = %project_name_task,
                            error = %e,
                            "worker execution error"
                        );
                        if let Some(ref m) = metrics {
                            m.tasks_failed.inc();
                        }
                        // Record failure in emotional state.
                        if let Some(ref emo) = emo_state {
                            let mut state = emo.lock().await;
                            state.record_negative();
                            if let Some(ref path) = emo_path {
                                let _  = state.save(path);
                            }
                        }
                    }
                }
            });
            self.running_tasks.push(TrackedWorker {
                handle,
                task_id,
                started_at: std::time::Instant::now(),
            });
        }

        // 4. Report to Focal Agent (only on state change).
        let active = self.running_tasks.len();
        let pending = {
            let store = self.tasks.lock().await;
            store.ready().len()
        };

        let current = (active, pending);
        if current != self.last_report && (active > 0 || pending > 0) {
            self.last_report = current;
            self.dispatch_bus
                .send(Dispatch::new_typed(
                    &format!("scout-{}", self.project_name),
                    &self.escalation_target,
                    DispatchKind::PatrolReport {
                        project: self.project_name.clone(),
                        active,
                        pending,
                    },
                ))
                .await;
        } else if current != self.last_report {
            self.last_report = current;
        }

        // 5. Record patrol metrics.
        if let Some(ref m) = self.metrics {
            m.patrol_cycles.inc();
            m.patrol_cycle_seconds.observe(patrol_start.elapsed().as_secs_f64());
            m.workers_active.set(active as f64);
            m.tasks_pending.set(pending as f64);
        }

        Ok(())
    }

    /// Handle blocked tasks: attempt project-level resolution or escalate to Focal Agent.
    ///
    /// Escalation chain:
    ///   1. Spirit BLOCKED → Supervisor spawns resolver worker (same project, has full codebase access)
    ///   2. Resolver answers → Supervisor appends answer to bead, resets to Pending for re-attempt
    ///   3. Resolver also blocked → Supervisor escalates to Focal Agent via mail
    ///   4. Focal Agent tries (has KNOWLEDGE.md + cross-project context)
    ///   5. Focal Agent resolves → sends RESOLVED mail back → Supervisor re-opens bead
    ///   6. Focal Agent stuck → routes to human via Telegram
    async fn handle_blocked_tasks(&mut self) {
        let blocked_tasks = {
            let store = self.tasks.lock().await;
            store.all().into_iter()
                .filter(|b| b.status == TaskStatus::Blocked)
                .cloned()
                .collect::<Vec<_>>()
        };

        for bead in blocked_tasks {
            let escalation_depth = Self::get_escalation_depth(&bead.labels);

            if escalation_depth >= MAX_PROJECT_RESOLUTION_ATTEMPTS {
                // Already tried project-level resolution. Escalate to team leader.
                self.escalate_to_leader(&bead).await;
            } else {
                // Attempt project-level resolution: re-open as Pending with resolution context.
                self.attempt_project_resolution(&bead, escalation_depth).await;
            }
        }
    }

    /// Attempt to resolve a blocker at the project level.
    ///
    /// Increments escalation depth, appends the blocker question to the bead
    /// description as resolution context, and resets to Pending so a new worker
    /// picks it up with the full context.
    async fn attempt_project_resolution(
        &self,
        bead: &system_tasks::Task,
        current_depth: u32,
    ) {
        let new_depth = current_depth + 1;
        let new_label = format!("{ESCALATION_LABEL_PREFIX}{new_depth}");

        // Extract the blocker question from closed_reason or description.
        let blocker_context = bead.closed_reason.as_deref()
            .unwrap_or("(no blocker details captured)");

        info!(
            project = %self.project_name,
            bead = %bead.id,
            depth = new_depth,
            "attempting project-level resolution"
        );

        // Update bead: append resolution context, increment depth, reset to Pending.
        let mut store = self.tasks.lock().await;
        let _ = store.update(&bead.id.0, |b| {
            // Append blocker context to description so the next worker sees it.
            b.description.push_str(&format!(
                "\n\n---\n## Resolution Attempt {new_depth}\n\n\
                 A previous worker was blocked on this task. \
                 Before continuing the original task, first try to answer this question \
                 using the codebase, documentation, and your knowledge. \
                 If you can answer it, proceed with the original task using that answer. \
                 If you genuinely cannot determine the answer, respond with BLOCKED: again.\n\n\
                 **Blocker question:**\n{blocker_context}\n"
            ));

            // Track escalation depth.
            b.labels.retain(|l| !l.starts_with(ESCALATION_LABEL_PREFIX));
            b.labels.push(new_label);

            // Reset to Pending so patrol picks it up for a new worker.
            b.status = TaskStatus::Pending;
            b.assignee = None;
        });
    }

    /// Escalate a blocked bead through the escalation chain:
    ///   1. Project leader — first escalation
    ///   2. System leader (orchestrator) — if project leader can't resolve
    ///   3. Human (Telegram) — last resort
    async fn escalate_to_leader(&self, bead: &system_tasks::Task) {
        // Determine escalation target based on current state.
        let already_escalated_project = bead.labels.iter().any(|l| l == "escalated");
        let already_escalated_system = bead.labels.iter().any(|l| l == "escalated-system");

        if already_escalated_system {
            return; // Already at highest level, waiting for human.
        }

        // If project leader == system leader, skip to system escalation.
        let project_leader_is_system = self.escalation_target == self.system_escalation_target;

        let (target, label) = if !already_escalated_project && !project_leader_is_system {
            // First escalation → project leader.
            (&self.escalation_target, "escalated")
        } else {
            // Second escalation → system leader.
            (&self.system_escalation_target, "escalated-system")
        };

        info!(
            project = %self.project_name,
            bead = %bead.id,
            target = %target,
            "escalating blocked task"
        );

        if let Some(ref m) = self.metrics {
            m.escalations_total.inc();
        }
        {
            let mut store = self.tasks.lock().await;
            let _ = store.update(&bead.id.0, |b| {
                b.labels.push(label.to_string());
            });
        }

        self.dispatch_bus
            .send(Dispatch::new_typed(
                &format!("scout-{}", self.project_name),
                target,
                DispatchKind::Escalation {
                    project: self.project_name.clone(),
                    task_id: bead.id.to_string(),
                    subject: bead.subject.clone(),
                    description: format!(
                        "Priority: {}\n\nFull description:\n{}\n\n\
                         This quest has been blocked after {} resolution attempt(s). \
                         Escalated to: {target}. \
                         Please try to resolve using your cross-project knowledge (KNOWLEDGE.md). \
                         If you can answer the blocker question, send a RESOLVED whisper back to \
                         scout-{} with the answer. If you cannot resolve it, escalate to the \
                         human operator via Telegram.",
                        bead.priority,
                        bead.description,
                        Self::get_escalation_depth(&bead.labels),
                        self.project_name,
                    ),
                    attempts: Self::get_escalation_depth(&bead.labels),
                },
            ))
            .await;
    }

    /// Process a RESOLVED mail from the Focal Agent: re-open the blocked bead
    /// with the answer appended to the description.
    pub async fn handle_resolution(&self, task_id: &str, answer: &str) {
        info!(
            project = %self.project_name,
            bead = %task_id,
            "received resolution from familiar"
        );

        let mut store = self.tasks.lock().await;
        let _ = store.update(task_id, |b| {
            b.description.push_str(&format!(
                "\n\n---\n## Resolution (from Focal Agent)\n\n{answer}\n\n\
                 **Now proceed with the original task using this answer.**\n"
            ));
            b.status = TaskStatus::Pending;
            b.assignee = None;
            // Remove escalation labels — fresh start with the answer.
            b.labels.retain(|l| {
                !l.starts_with(ESCALATION_LABEL_PREFIX) && l != "escalated" && l != "escalated-system"
            });
        });
    }

    /// Get escalation depth from bead labels.
    fn get_escalation_depth(labels: &[String]) -> u32 {
        labels.iter()
            .filter_map(|l| l.strip_prefix(ESCALATION_LABEL_PREFIX))
            .filter_map(|n| n.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
    }

    /// Get worker count by state: (idle, running, 0).
    /// Spirits launch immediately as background tasks — no "hooked" state.
    pub fn worker_counts(&self) -> (usize, usize, usize) {
        let running = self.running_tasks.iter().filter(|t| !t.handle.is_finished()).count();
        let capacity = self.max_workers as usize;
        let idle = capacity.saturating_sub(running);
        (idle, running, 0)
    }

    /// Cancel a quest by ID. Marks it as Cancelled and aborts any running worker task.
    pub async fn cancel_quest(&mut self, task_id: &str) -> Result<bool> {
        let mut store = self.tasks.lock().await;
        let task = store.get(task_id);
        if task.is_none() {
            return Ok(false);
        }

        let _ = store.update(task_id, |q| {
            q.status = system_tasks::TaskStatus::Cancelled;
            q.assignee = None;
            q.closed_reason = Some("Cancelled by user".to_string());
        });

        // Abort the running task if one exists for this quest.
        self.running_tasks.retain(|t| {
            if t.task_id == task_id {
                t.handle.abort();
                info!(task_id, "cancelled running worker task");
                false
            } else {
                true
            }
        });

        info!(task_id, "quest cancelled");
        Ok(true)
    }
}
