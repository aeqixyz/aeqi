use anyhow::Result;
use sigil_core::config::{ExecutionMode, ProjectTeamConfig};
use sigil_core::traits::{
    Channel, ChatRequest, Memory, Message, MessageContent, OutgoingMessage, Provider, Role, Tool,
};
use sigil_tasks::{TaskBoard, TaskStatus};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use crate::agent_worker::AgentWorker;
use crate::audit::{AuditEvent, AuditLog, DecisionType};
use crate::blackboard::Blackboard;
use crate::checkpoint::AgentCheckpoint;
use crate::cost_ledger::{CostEntry, CostLedger};
use crate::decomposition::DecompositionResult;
use crate::emotional_state::EmotionalState;
use crate::executor::{ClaudeCodeExecutor, TaskOutcome};
use crate::expertise::{ExpertiseLedger, ExpertiseRecord, TaskOutcomeKind};
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::metrics::SigilMetrics;
use crate::preflight::{PreflightAssessment, PreflightVerdict};
use crate::project::Project;
use std::collections::HashMap;

/// Label prefix for tracking escalation depth on tasks.
const ESCALATION_LABEL_PREFIX: &str = "escalation:";

/// A running worker with age tracking for timeout detection.
struct TrackedWorker {
    handle: tokio::task::JoinHandle<()>,
    task_id: String,
    started_at: std::time::Instant,
    /// PID of the Claude Code child process (for process group kill on timeout).
    child_pid: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// Effective timeout for the running worker.
    timeout_secs: u64,
    /// Real-time progress from the Claude Code executor.
    progress_rx: Option<tokio::sync::watch::Receiver<crate::executor::ExecutionProgress>>,
}

/// Supervisor: per-rig supervisor. Runs patrol cycles, manages workers,
/// detects stuck/orphaned tasks, handles escalation, reports to Leader Agent.
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
    pub identity: sigil_core::Identity,
    // ClaudeCode-mode fields (used when execution_mode == ClaudeCode).
    /// Rig's repo path for Claude Code working directory.
    pub repo: Option<std::path::PathBuf>,
    /// Max turns per Claude Code execution.
    pub cc_max_turns: u32,
    /// Max budget per Claude Code execution.
    pub cc_max_budget_usd: Option<f64>,
    /// Background worker tasks with age tracking.
    running_tasks: Vec<TrackedWorker>,
    /// Timeout in seconds for worker execution. Hung workers are aborted after this.
    worker_timeout_secs: u64,
    /// Last reported state — only send mail on change.
    last_report: (usize, usize),
    /// Shared cost ledger for budget enforcement.
    pub cost_ledger: Option<Arc<CostLedger>>,
    /// Shared metrics registry.
    pub metrics: Option<Arc<SigilMetrics>>,
    /// Per-project team config — which agents work on this project.
    pub team: Option<ProjectTeamConfig>,
    /// Name of the project's team leader to escalate to first.
    pub escalation_target: String,
    /// Name of the system team leader to escalate to if project leader can't resolve.
    pub system_escalation_target: String,
    /// Memory instance for worker reflection (project-scoped).
    pub memory: Option<Arc<dyn Memory>>,
    /// Provider used for post-execution reflection (cheap model).
    pub reflect_provider: Option<Arc<dyn Provider>>,
    /// Model used for reflection extraction.
    pub reflect_model: String,
    /// Fires when a task closes (passed through to workers).
    pub task_notify: Arc<Notify>,
    /// Emotional state tracking (trust, mood, interaction count).
    pub emotional_state: Option<Arc<Mutex<EmotionalState>>>,
    /// Path to save emotional state (agent's .sigil dir).
    pub emotional_state_path: Option<std::path::PathBuf>,
    /// Max resolution attempts at the project level before escalating to leader.
    pub max_resolution_attempts: u32,
    /// Max task description size in chars before truncation.
    pub max_description_chars: usize,
    /// Max task retries (handoff/failure) before auto-cancel.
    pub max_task_retries: u32,
    /// Gate channels for human escalation notifications (Telegram, Discord, Slack).
    pub gate_channels: Vec<Arc<dyn Channel>>,
    /// Decision audit log (Phase 1).
    pub audit_log: Option<Arc<AuditLog>>,
    /// Agent expertise ledger for smart routing (Phase 2).
    pub expertise_ledger: Option<Arc<ExpertiseLedger>>,
    /// Inter-agent blackboard for shared knowledge (Phase 3).
    pub blackboard: Option<Arc<Blackboard>>,
    /// Enable expertise-based routing for task assignment.
    pub expertise_routing: bool,
    /// Enable pre-flight assessment before worker spawn.
    pub preflight_enabled: bool,
    pub preflight_model: String,
    pub preflight_max_cost_usd: f64,
    /// Enable adaptive retry with failure analysis.
    pub adaptive_retry: bool,
    pub failure_analysis_model: String,
    /// Paused by watchdog — skip task assignment.
    pub paused: bool,
    /// Enable auto-redecomposition of stalled missions.
    pub auto_redecompose: bool,
    /// Directories to search for skill TOML files (project skills + shared skills).
    pub skills_dirs: Vec<std::path::PathBuf>,
    /// Model for mission decomposition / redecomposition.
    pub decomposition_model: String,
    /// Threshold for inferring dependencies between tasks (0.0 = disabled).
    pub infer_deps_threshold: f64,
}

impl Supervisor {
    fn candidate_agents(&self) -> Vec<String> {
        self.team
            .as_ref()
            .map(ProjectTeamConfig::effective_agents)
            .filter(|agents| !agents.is_empty())
            .unwrap_or_else(|| vec![self.escalation_target.clone()])
    }

    async fn select_agent_for_task(
        &self,
        task: &sigil_tasks::Task,
    ) -> (String, Option<String>, Vec<String>) {
        let candidates = self.candidate_agents();
        if candidates.is_empty() {
            return (
                self.escalation_target.clone(),
                None,
                Vec::new(),
            );
        }

        let domain = ExpertiseLedger::extract_domain(&task.labels, &task.subject);
        let mut ranking_info = Vec::new();

        if self.expertise_routing
            && let Some(ref ledger) = self.expertise_ledger
        {
            let rankings = ledger.rank_for_domain(&domain).unwrap_or_default();
            ranking_info = rankings
                .iter()
                .filter(|score| candidates.contains(&score.agent_name))
                .take(3)
                .map(|score| format!("{}({:.0}%)", score.agent_name, score.confidence * 100.0))
                .collect();

            for score in rankings {
                if candidates.contains(&score.agent_name)
                    && !ledger
                        .is_deprioritized(&score.agent_name, &domain)
                        .unwrap_or(false)
                {
                    return (score.agent_name, Some(domain), ranking_info);
                }
            }
        }

        let store = self.tasks.lock().await;
        let mut load_by_agent: HashMap<String, usize> =
            candidates.iter().cloned().map(|agent| (agent, 0)).collect();
        for queued_task in store.all() {
            if queued_task.is_closed() {
                continue;
            }
            if let Some(assignee) = &queued_task.assignee
                && let Some(load) = load_by_agent.get_mut(assignee)
            {
                *load += 1;
            }
        }
        drop(store);

        let selected = candidates
            .iter()
            .min_by_key(|agent| (load_by_agent.get(*agent).copied().unwrap_or(0), agent.as_str()))
            .cloned()
            .unwrap_or_else(|| self.escalation_target.clone());
        (selected, Some(domain), ranking_info)
    }

    pub fn new(
        project: &Project,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        dispatch_bus: Arc<DispatchBus>,
    ) -> Self {
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
            escalation_target: "leader".to_string(),
            system_escalation_target: "leader".to_string(),
            cost_ledger: None,
            metrics: None,
            memory: None,
            reflect_provider: None,
            reflect_model: String::new(),
            task_notify: project.task_notify.clone(),
            emotional_state: None,
            emotional_state_path: None,
            max_resolution_attempts: 1,
            max_description_chars: 8000,
            max_task_retries: 3,
            gate_channels: Vec::new(),
            audit_log: None,
            expertise_ledger: None,
            blackboard: None,
            expertise_routing: false,
            preflight_enabled: false,
            preflight_model: String::new(),
            preflight_max_cost_usd: 0.01,
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            paused: false,
            auto_redecompose: false,
            decomposition_model: String::new(),
            infer_deps_threshold: 0.0,
            skills_dirs: Vec::new(),
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

    /// Look up a skill's system prompt by name from skills directories.
    fn load_skill_prompt(&self, skill_name: &str) -> Option<String> {
        for dir in &self.skills_dirs {
            let path = dir.join(format!("{skill_name}.toml"));
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(value) = content.parse::<toml::Value>()
                && let Some(system) = value
                    .get("prompt")
                    .and_then(|p| p.get("system"))
                    .and_then(|s| s.as_str())
            {
                return Some(system.to_string());
            }
        }
        None
    }

    /// Create a worker based on the rig's execution mode.
    /// Returns the worker and an optional progress receiver (ClaudeCode mode only).
    async fn create_worker(
        &self,
        agent_name: String,
        worker_name: String,
        task: &sigil_tasks::Task,
    ) -> (
        AgentWorker,
        Option<tokio::sync::watch::Receiver<crate::executor::ExecutionProgress>>,
    ) {
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

        let mut progress_rx = None;
        let mut worker = match self.execution_mode {
            ExecutionMode::Agent => AgentWorker::new(
                agent_name.clone(),
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
                // Use one generous turn budget for the adaptive pipeline.
                let adaptive_turns = self.cc_max_turns.max(50);
                let executor = ClaudeCodeExecutor::new(
                    workdir,
                    self.model.clone(),
                    adaptive_turns,
                    self.cc_max_budget_usd,
                );
                let (executor, rx) = executor.with_progress_channel();
                progress_rx = Some(rx);
                AgentWorker::new_claude_code(
                    agent_name,
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

        // Pass adaptive retry config to the worker.
        if self.adaptive_retry {
            worker = worker.with_adaptive_retry(self.failure_analysis_model.clone());
        }

        // Pass blackboard + audit log for failure analysis mode-specific strategies.
        worker.blackboard = self.blackboard.clone();
        worker.audit_log = self.audit_log.clone();

        // Inject relevant blackboard entries into worker identity preamble.
        if let Some(ref bb) = self.blackboard {
            let tags: Vec<String> = task.labels.clone();
            let entries = bb.query(&self.project_name, &tags, 5).unwrap_or_default();
            if !entries.is_empty() {
                let bb_context = entries
                    .iter()
                    .map(|e| format!("- [{}] {}: {}", e.agent, e.key, e.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                let existing = worker.identity.memory.clone().unwrap_or_default();
                worker.identity.memory = Some(format!(
                    "{existing}\n\n## Blackboard (shared knowledge)\n{bb_context}"
                ));
            }
        }

        // Inject skill system prompt if task specifies a skill.
        if let Some(ref skill_name) = task.skill {
            if let Some(prompt) = self.load_skill_prompt(skill_name) {
                info!(
                    project = %self.project_name,
                    skill = %skill_name,
                    "injecting skill prompt into worker identity"
                );
                worker.identity.skill_prompt = Some(prompt);
            } else {
                warn!(
                    project = %self.project_name,
                    skill = %skill_name,
                    "skill not found in skills directories"
                );
            }
        }

        // Inject domain knowledge hints based on task labels/subject.
        let hints = Self::resolve_domain_hints(&task.labels, &task.subject, &self.skills_dirs);
        if !hints.is_empty() {
            let existing = worker.identity.memory.clone().unwrap_or_default();
            worker.identity.memory = Some(format!("{existing}\n\n{hints}"));
        }

        (worker.with_max_task_retries(self.max_task_retries), progress_rx)
    }

    /// Run one patrol cycle: reap finished tasks, detect timeouts,
    /// assign + launch ready work, handle blocked tasks, report status.
    ///
    /// Worker execution is fully non-blocking — each worker runs as a background
    /// tokio task. The daemon loop never stalls waiting for workers.
    pub async fn patrol(&mut self) -> Result<()> {
        let patrol_start = std::time::Instant::now();
        debug!(project = %self.project_name, "patrol cycle");

        // 0. Reload tasks from disk to pick up externally-created tasks
        //    (e.g., from `sg assign` CLI or Claude Code workers).
        {
            let mut store = self.tasks.lock().await;
            if let Err(e) = store.reload() {
                warn!(project = %self.project_name, error = %e, "failed to reload tasks from disk");
            }

            // Reset orphaned InProgress tasks (e.g. from daemon restart or worker panic).
            // On first patrol after restart, running_tasks is empty so all InProgress reset.
            let running_ids: std::collections::HashSet<&str> = self
                .running_tasks
                .iter()
                .map(|t| t.task_id.as_str())
                .collect();
            let orphaned: Vec<String> = store
                .all()
                .iter()
                .filter(|t| {
                    t.status == TaskStatus::InProgress && !running_ids.contains(t.id.0.as_str())
                })
                .map(|t| t.id.0.clone())
                .collect();
            for id in orphaned {
                warn!(
                    project = %self.project_name,
                    task = %id,
                    "resetting orphaned InProgress task to Pending"
                );
                let _ = store.update(&id, |t| {
                    t.status = TaskStatus::Pending;
                    t.assignee = None;
                });
            }
        }

        // 1. Reap completed tasks + detect timed-out workers.
        let mut timed_out = Vec::new();
        self.running_tasks.retain(|t| {
            if t.handle.is_finished() {
                return false;
            }
            let timeout = std::time::Duration::from_secs(t.timeout_secs);
            if t.started_at.elapsed() > timeout {
                // Kill the entire process group first, then abort the tokio task.
                let pid = t.child_pid.load(std::sync::atomic::Ordering::Relaxed);
                ClaudeCodeExecutor::kill_process_group(pid);
                t.handle.abort();
                timed_out.push(t.task_id.clone());
                return false;
            }
            true
        });

        // Reset timed-out tasks back to Pending and notify shadow.
        if !timed_out.is_empty()
            && let Some(ref m) = self.metrics
        {
            m.workers_timed_out.inc_by(timed_out.len() as u64);
        }
        for task_id in timed_out {
            warn!(
                project = %self.project_name,
                task = %task_id,
                timeout_secs = self.worker_timeout_secs,
                "worker timed out, aborting and resetting task"
            );

            // Record audit: WorkerTimedOut.
            if let Some(ref audit) = self.audit_log {
                let _ = audit.record(
                    &AuditEvent::new(
                        &self.project_name,
                        DecisionType::WorkerTimedOut,
                        format!("Timed out after {}s", self.worker_timeout_secs),
                    )
                    .with_task(&task_id),
                );
            }

            // Capture external checkpoint before resetting — preserve git state evidence.
            if let Some(ref repo) = self.repo {
                match AgentCheckpoint::capture(repo) {
                    Ok(checkpoint) => {
                        let checkpoint: AgentCheckpoint = checkpoint
                            .with_task_id(&task_id)
                            .with_worker_name(format!("timeout-{}", task_id))
                            .with_progress_notes(format!(
                                "Worker timed out after {}s — checkpoint captured externally by supervisor",
                                self.worker_timeout_secs
                            ));

                        let cp_path = AgentCheckpoint::path_for_task(repo, &task_id);
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
                    &format!("supervisor-{}", self.project_name),
                    &self.escalation_target,
                    DispatchKind::WorkerCrashed {
                        project: self.project_name.clone(),
                        worker: format!("timeout-{}", task_id),
                        error: format!(
                            "Worker timed out after {}s on task {}",
                            self.worker_timeout_secs, task_id
                        ),
                    },
                ))
                .await;
        }

        // 2. Handle blocked tasks — attempt resolution or escalate.
        self.handle_blocked_tasks().await;

        // 3. Assign + launch ready tasks.
        if self.paused {
            debug!(project = %self.project_name, "project paused — skipping task assignment");
            // Skip straight to reporting.
            let active = self.running_tasks.len();
            let pending = {
                let store = self.tasks.lock().await;
                store.ready().len()
            };
            let current = (active, pending);
            if current != self.last_report {
                self.last_report = current;
            }
            if let Some(ref m) = self.metrics {
                m.patrol_cycles.inc();
                m.patrol_cycle_seconds
                    .observe(patrol_start.elapsed().as_secs_f64());
                m.workers_active.set(active as f64);
                m.tasks_pending.set(pending as f64);
            }
            return Ok(());
        }

        let ready_tasks = {
            let store = self.tasks.lock().await;
            store.ready().into_iter().cloned().collect::<Vec<_>>()
        };

        for task in ready_tasks {
            if self.running_tasks.len() >= self.max_workers as usize {
                break;
            }

            if task.assignee.is_some() {
                continue;
            }

            // Budget check: don't spawn if global daily budget or project budget is exhausted.
            if let Some(ref ledger) = self.cost_ledger
                && !ledger.can_afford_project(&self.project_name)
            {
                let (spent, budget, _) = ledger.project_budget_status(&self.project_name);
                warn!(
                    project = %self.project_name,
                    spent_usd = spent,
                    budget_usd = budget,
                    "budget exhausted for project, skipping task"
                );
                // Record audit: BudgetBlocked.
                if let Some(ref audit) = self.audit_log {
                    let _ = audit.record(
                        &AuditEvent::new(
                            &self.project_name,
                            DecisionType::BudgetBlocked,
                            format!("Budget exhausted: ${spent:.2}/${budget:.2}"),
                        )
                        .with_task(&task.id.0),
                    );
                }
                break;
            }

            let worker_idx = self.running_tasks.len() + 1;
            let (agent_name, domain, ranking_info) = self.select_agent_for_task(&task).await;
            let worker_name = format!("{}:{}:{}", self.project_name, agent_name, worker_idx);

            if let Some(ref audit) = self.audit_log {
                let ranking_info = if ranking_info.is_empty() {
                    "no expertise data".to_string()
                } else {
                    ranking_info.join(", ")
                };
                let routing_summary = if let Some(domain) = domain {
                    format!("Domain '{domain}' → {agent_name} [rankings: {ranking_info}]")
                } else {
                    format!("Selected {agent_name} [rankings: {ranking_info}]")
                };
                let _ = audit.record(
                    &AuditEvent::new(
                        &self.project_name,
                        DecisionType::RouteDecision,
                        routing_summary,
                    )
                    .with_task(&task.id.0)
                    .with_agent(&agent_name),
                );
            }
            // Preflight assessment: evaluate task before committing resources.
            if self.preflight_enabled && !self.preflight_model.is_empty() {
                let pf_provider = self.reflect_provider.as_ref().unwrap_or(&self.provider);
                let prompt =
                    PreflightAssessment::assessment_prompt(&task.subject, &task.description);
                let request = ChatRequest {
                    model: self.preflight_model.clone(),
                    messages: vec![Message {
                        role: Role::User,
                        content: MessageContent::text(&prompt),
                    }],
                    tools: vec![],
                    max_tokens: 256,
                    temperature: 0.0,
                };
                if let Ok(response) = pf_provider.chat(&request).await
                    && let Some(ref text) = response.content
                {
                    let assessment: PreflightAssessment = PreflightAssessment::parse(text);

                    // Auto-inject the unified adaptive execution skill.
                    let mut store = self.tasks.lock().await;
                    let skill_name = assessment.adaptive_pipeline_skill().to_string();
                    let _ = store.update(&task.id.0, |b| {
                        if !b.labels.iter().any(|label| label == "execution:adaptive") {
                            b.labels.push("execution:adaptive".to_string());
                        }
                        if b.skill.is_none() {
                            b.skill = Some(skill_name.clone());
                            info!(
                                project = %self.project_name,
                                task = %b.id,
                                skill = %skill_name,
                                "auto-injected adaptive pipeline skill"
                            );
                        }
                    });

                    let budget_remaining = self
                        .cost_ledger
                        .as_ref()
                        .map(|l| l.project_budget_status(&self.project_name).2)
                        .unwrap_or(10.0);
                    let agent_success_rate = self
                        .expertise_ledger
                        .as_ref()
                        .and_then(|l| {
                            let domain =
                                ExpertiseLedger::extract_domain(&task.labels, &task.subject);
                            l.rank_for_domain(&domain)
                                .ok()
                                .and_then(|scores| scores.first().map(|s| s.success_rate))
                        })
                        .unwrap_or(1.0);

                    let verdict = assessment.evaluate(budget_remaining, agent_success_rate);
                    match &verdict {
                        PreflightVerdict::Reject { reason } => {
                            info!(
                                project = %self.project_name,
                                task = %task.id,
                                reason = %reason,
                                "preflight rejected task"
                            );
                            if let Some(ref audit) = self.audit_log {
                        let _ = audit.record(
                            &AuditEvent::new(
                                &self.project_name,
                                DecisionType::PreflightRejected,
                                format!("Rejected: {reason}"),
                            )
                            .with_task(&task.id.0)
                            .with_agent(&agent_name),
                        );
                    }
                            // Store assessment in task metadata.
                            let mut store = self.tasks.lock().await;
                            let _ = store.update(&task.id.0, |b| {
                                b.labels.push(format!(
                                    "preflight:rejected:{}",
                                    reason.chars().take(50).collect::<String>()
                                ));
                            });
                            continue;
                        }
                        PreflightVerdict::Reroute { reason } => {
                            info!(
                                project = %self.project_name,
                                task = %task.id,
                                reason = %reason,
                                "preflight rerouted task"
                            );
                            if let Some(ref audit) = self.audit_log {
                        let _ = audit.record(
                            &AuditEvent::new(
                                &self.project_name,
                                DecisionType::PreflightRejected,
                                format!("Rerouted: {reason}"),
                            )
                            .with_task(&task.id.0)
                            .with_agent(&agent_name),
                        );
                    }
                            continue;
                        }
                        PreflightVerdict::Proceed => {}
                    }
                }
            }

            info!(
                project = %self.project_name,
                worker = %worker_name,
                agent = %agent_name,
                task = %task.id,
                subject = %task.subject,
                mode = ?self.execution_mode,
                "assigning work"
            );

            // Record audit: TaskAssigned.
            if let Some(ref audit) = self.audit_log {
                let _ = audit.record(
                    &AuditEvent::new(
                        &self.project_name,
                        DecisionType::TaskAssigned,
                        format!("Assigned to {agent_name} via {worker_name}"),
                    )
                    .with_task(&task.id.0)
                    .with_agent(&agent_name),
                );
            }

            let (mut worker, worker_progress_rx) =
                self.create_worker(agent_name.clone(), worker_name.clone(), &task).await;

            // If there's a previous external checkpoint for this task, inject it into the
            // task description so the new worker has context about the prior attempt's git state.
            if let Some(ref repo) = self.repo {
                let cp_path = AgentCheckpoint::path_for_task(repo, &task.id.0);
                match AgentCheckpoint::read(&cp_path) {
                    Ok(Some(checkpoint)) => {
                        let checkpoint_ctx: String = checkpoint.as_context();
                        let max_desc = self.max_description_chars;
                        let mut store = self.tasks.lock().await;
                        let _ = store.update(&task.id.0, |b| {
                            b.description
                                .push_str(&format!("\n\n---\n{checkpoint_ctx}"));
                            Self::cap_description_with_limit(&mut b.description, max_desc);
                        });
                        info!(
                            project = %self.project_name,
                            task = %task.id,
                            "injected external checkpoint context from previous worker"
                        );
                        // Remove the checkpoint file — it's been consumed.
                        let _ = AgentCheckpoint::remove(&cp_path);
                    }
                    Ok(None) => {} // No checkpoint — normal launch.
                    Err(e) => {
                        warn!(
                            project = %self.project_name,
                            task = %task.id,
                            error = %e,
                            "failed to read checkpoint — launching without it"
                        );
                    }
                }
            }

            worker.assign(&task);

            let child_pid_tracker = worker.child_pid();
            let task_id = task.id.0.clone();

            // Fire-and-forget: worker handles its own task updates + dispatch notifications.
            let project_name_task = self.project_name.clone();
            let task_id_clone = task_id.clone();
            let cost_ledger = self.cost_ledger.clone();
            let metrics = self.metrics.clone();
            if let Some(ref m) = self.metrics {
                m.workers_spawned.inc();
            }
            if let Some(ref audit) = self.audit_log {
                let _ = audit.record(
                    &AuditEvent::new(
                        &self.project_name,
                        DecisionType::WorkerSpawned,
                        format!("Worker {} spawned for task {}", worker_name, task.id),
                    )
                    .with_task(&task.id.0)
                    .with_agent(&agent_name),
                );
            }
            let emo_state = self.emotional_state.clone();
            let emo_path = self.emotional_state_path.clone();
            let tasks_for_err = self.tasks.clone();
            let expertise_ledger = self.expertise_ledger.clone();
            let blackboard_worker = self.blackboard.clone();
            let audit_log_worker = self.audit_log.clone();
            let dispatch_bus_worker = self.dispatch_bus.clone();
            let outcome_recipient = self.system_escalation_target.clone();
            let task_labels = task.labels.clone();
            let task_subject = task.subject.clone();
            let agent_name_for_records = agent_name.clone();
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
                                task_id: task_id_clone.clone(),
                                worker: "worker".to_string(),
                                cost_usd: 0.0, // Claude Code Max subscription = no cost
                                turns,
                                timestamp: chrono::Utc::now(),
                                source: "claude_code".to_string(),
                                tokens: 0, // TODO: extract from Claude Code result
                            });
                        }

                        // Record metrics.
                        if let Some(ref m) = metrics {
                            m.worker_duration_seconds.observe(duration_secs);
                            m.task_cost_usd.observe(cost_usd);
                            match &outcome {
                                TaskOutcome::Done(_) => m.tasks_completed.inc(),
                                TaskOutcome::Blocked { .. } | TaskOutcome::Handoff { .. } => {
                                    m.tasks_blocked.inc()
                                }
                                TaskOutcome::Failed(_) => m.tasks_failed.inc(),
                            }
                        }

                        // Record to expertise ledger.
                        if let Some(ref ledger) = expertise_ledger {
                            let domain =
                                ExpertiseLedger::extract_domain(&task_labels, &task_subject);
                            let outcome_kind = match &outcome {
                                TaskOutcome::Done(_) => TaskOutcomeKind::Done,
                                TaskOutcome::Failed(_) => TaskOutcomeKind::Failed,
                                TaskOutcome::Handoff { .. } => TaskOutcomeKind::Handoff,
                                TaskOutcome::Blocked { .. } => TaskOutcomeKind::Blocked,
                            };
                            let _ = ledger.record(&ExpertiseRecord {
                                agent_name: agent_name_for_records.clone(),
                                task_domain: domain,
                                outcome: outcome_kind,
                                cost_usd,
                                duration_secs,
                                turns,
                                timestamp: chrono::Utc::now(),
                            });
                        }

                        // Record audit: task outcome.
                        if let Some(ref audit) = audit_log_worker {
                            let dt = match &outcome {
                                TaskOutcome::Done(_) => DecisionType::TaskCompleted,
                                TaskOutcome::Failed(_) => DecisionType::TaskFailed,
                                TaskOutcome::Handoff { .. } => DecisionType::TaskRetried,
                                TaskOutcome::Blocked { .. } => DecisionType::TaskBlocked,
                            };
                            let _ = audit.record(
                                &AuditEvent::new(
                                    &project_name_task,
                                    dt,
                                    format!(
                                        "Outcome: {:?}, cost=${cost_usd:.3}, turns={turns}",
                                        std::mem::discriminant(&outcome)
                                    ),
                                )
                                .with_task(&task_id_clone)
                                .with_agent(&agent_name_for_records),
                            );
                        }

                        match &outcome {
                            TaskOutcome::Done(summary) => {
                                dispatch_bus_worker
                                    .send(Dispatch::new_typed(
                                        &format!("supervisor-{project_name_task}"),
                                        &outcome_recipient,
                                        DispatchKind::TaskDone {
                                            task_id: task_id_clone.clone(),
                                            summary: summary.clone(),
                                        },
                                    ))
                                    .await;

                                // Post completion summary to blackboard for sibling workers.
                                if let Some(ref bb) = blackboard_worker
                                    && !summary.is_empty()
                                {
                                    let key = format!("completed:{}", task_id_clone);
                                    let content = format!(
                                        "Task '{}' completed: {}",
                                        task_subject,
                                        summary.chars().take(500).collect::<String>()
                                    );
                                    if bb
                                        .post(
                                            &key,
                                            &content,
                                            &agent_name_for_records,
                                            &project_name_task,
                                            &task_labels,
                                            crate::blackboard::EntryDurability::Transient,
                                        )
                                        .is_ok()
                                        && let Some(ref audit) = audit_log_worker
                                    {
                                        let _ = audit.record(
                                            &AuditEvent::new(
                                                &project_name_task,
                                                DecisionType::BlackboardPost,
                                                format!(
                                                    "Posted completion summary for task {}",
                                                    task_id_clone
                                                ),
                                            )
                                            .with_task(&task_id_clone)
                                            .with_agent(&agent_name_for_records),
                                        );
                                    }
                                }
                            }
                            TaskOutcome::Blocked {
                                question,
                                full_text,
                            } => {
                                dispatch_bus_worker
                                    .send(Dispatch::new_typed(
                                        &format!("supervisor-{project_name_task}"),
                                        &outcome_recipient,
                                        DispatchKind::TaskBlocked {
                                            task_id: task_id_clone.clone(),
                                            question: question.clone(),
                                            context: full_text.clone(),
                                        },
                                    ))
                                    .await;
                            }
                            TaskOutcome::Failed(error) => {
                                dispatch_bus_worker
                                    .send(Dispatch::new_typed(
                                        &format!("supervisor-{project_name_task}"),
                                        &outcome_recipient,
                                        DispatchKind::TaskFailed {
                                            task_id: task_id_clone.clone(),
                                            error: error.clone(),
                                        },
                                    ))
                                    .await;
                            }
                            TaskOutcome::Handoff { .. } => {}
                        }

                        // Update emotional state based on outcome.
                        if let Some(ref emo) = emo_state {
                            let mut state = emo.lock().await;
                            match &outcome {
                                TaskOutcome::Done(_) => state.record_positive(),
                                TaskOutcome::Blocked { .. } | TaskOutcome::Handoff { .. } => {
                                    state.record_interaction()
                                }
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
                            task = %task_id_clone,
                            error = %e,
                            "worker execution error — resetting task to Pending"
                        );
                        // Reset task to Pending so patrol picks it up again.
                        {
                            let mut store = tasks_for_err.lock().await;
                            let _ = store.update(&task_id_clone, |b| {
                                b.status = TaskStatus::Pending;
                                b.assignee = None;
                            });
                        }
                        if let Some(ref m) = metrics {
                            m.tasks_failed.inc();
                        }
                        // Record failure in emotional state.
                        if let Some(ref emo) = emo_state {
                            let mut state = emo.lock().await;
                            state.record_negative();
                            if let Some(ref path) = emo_path {
                                let _ = state.save(path);
                            }
                        }
                    }
                }
            });
            self.running_tasks.push(TrackedWorker {
                handle,
                task_id,
                started_at: std::time::Instant::now(),
                child_pid: child_pid_tracker,
                timeout_secs: self.worker_timeout_secs.max(1800),
                progress_rx: worker_progress_rx,
            });
        }

        // 3.5. Detect stalled missions (all tasks blocked/cancelled) and redecompose.
        if self.auto_redecompose && !self.decomposition_model.is_empty() {
            let store = self.tasks.lock().await;
            let active_missions = store.active_missions(None);
            for mission in &active_missions {
                let tasks = store.mission_tasks(&mission.id);
                if tasks.is_empty() {
                    continue;
                }
                let all_stalled = tasks
                    .iter()
                    .all(|t| t.status == TaskStatus::Blocked || t.status == TaskStatus::Cancelled);
                if all_stalled {
                    info!(
                        project = %self.project_name,
                        mission = %mission.id,
                        "stalled mission detected — all tasks blocked/cancelled"
                    );
                    // Drop store lock before async LLM call.
                    let mission_id = mission.id.clone();
                    let mission_name = mission.name.clone();
                    let mission_desc = mission.description.clone();
                    drop(store);

                    let prompt =
                        DecompositionResult::decomposition_prompt(&mission_name, &mission_desc);
                    let request = ChatRequest {
                        model: self.decomposition_model.clone(),
                        messages: vec![Message {
                            role: Role::User,
                            content: MessageContent::text(&prompt),
                        }],
                        tools: vec![],
                        max_tokens: 2048,
                        temperature: 0.0,
                    };
                    let provider = self.reflect_provider.as_ref().unwrap_or(&self.provider);
                    if let Ok(response) = provider.chat(&request).await
                        && let Some(ref text) = response.content
                    {
                        let mut result = DecompositionResult::parse(text);
                        let mut store = self.tasks.lock().await;
                        let prefix = mission_id.split('-').next().unwrap_or("xx");
                        match result.materialize(&mut store, prefix, &mission_id) {
                            Ok(task_ids) => {
                                info!(
                                    project = %self.project_name,
                                    mission = %mission_id,
                                    new_tasks = task_ids.len(),
                                    "redecomposed stalled mission"
                                );
                                // Infer dependencies between newly created tasks.
                                if self.infer_deps_threshold > 0.0 {
                                    match store
                                        .apply_inferred_dependencies(self.infer_deps_threshold)
                                    {
                                        Ok(n) if n > 0 => {
                                            info!(
                                                project = %self.project_name,
                                                mission = %mission_id,
                                                inferred = n,
                                                "inferred task dependencies"
                                            );
                                            if let Some(ref audit) = self.audit_log {
                                                let _ = audit.record(
                                                    &AuditEvent::new(
                                                        &self.project_name,
                                                        DecisionType::DependencyInferred,
                                                        format!(
                                                            "Inferred {n} dependencies in mission {mission_id}"
                                                        ),
                                                    )
                                                    .with_task(&mission_id),
                                                );
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                if let Some(ref audit) = self.audit_log {
                                    let _ = audit.record(
                                        &AuditEvent::new(
                                            &self.project_name,
                                            DecisionType::MissionDecomposed,
                                            format!(
                                                "Redecomposed stalled mission {} into {} tasks",
                                                mission_id,
                                                task_ids.len()
                                            ),
                                        )
                                        .with_task(&mission_id),
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(
                                    project = %self.project_name,
                                    mission = %mission_id,
                                    error = %e,
                                    "redecomposition materialization failed"
                                );
                            }
                        }
                    }
                    // Only redecompose one mission per patrol to avoid thrashing.
                    break;
                }
            }
        }

        // 4. Report to Leader Agent (only on state change).
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
                    &format!("supervisor-{}", self.project_name),
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
            m.patrol_cycle_seconds
                .observe(patrol_start.elapsed().as_secs_f64());
            m.workers_active.set(active as f64);
            m.tasks_pending.set(pending as f64);
        }

        Ok(())
    }

    /// Handle blocked tasks: attempt project-level resolution or escalate to Leader Agent.
    ///
    /// Escalation chain:
    ///   1. Worker BLOCKED → Supervisor spawns resolver worker (same project, has full codebase access)
    ///   2. Resolver answers → Supervisor appends answer to task, resets to Pending for re-attempt
    ///   3. Resolver also blocked → Supervisor escalates to Leader Agent via dispatch
    ///   4. Leader Agent tries (has KNOWLEDGE.md + cross-project context)
    ///   5. Leader Agent resolves → sends RESOLVED dispatch back → Supervisor re-opens task
    ///   6. Leader Agent stuck → routes to human via Telegram
    async fn handle_blocked_tasks(&mut self) {
        let blocked_tasks = {
            let store = self.tasks.lock().await;
            store
                .all()
                .into_iter()
                .filter(|b| b.status == TaskStatus::Blocked)
                .cloned()
                .collect::<Vec<_>>()
        };

        for task in blocked_tasks {
            let escalation_depth = Self::get_escalation_depth(&task.labels);

            if escalation_depth >= self.max_resolution_attempts {
                // Already tried project-level resolution. Escalate to team leader.
                self.escalate_to_leader(&task).await;
            } else {
                // Attempt project-level resolution: re-open as Pending with resolution context.
                self.attempt_project_resolution(&task, escalation_depth)
                    .await;
            }
        }
    }

    /// Attempt to resolve a blocker at the project level.
    ///
    /// Increments escalation depth, appends the blocker question to the task
    /// description as resolution context, and resets to Pending so a new worker
    /// picks it up with the full context.
    async fn attempt_project_resolution(&self, task: &sigil_tasks::Task, current_depth: u32) {
        let new_depth = current_depth + 1;
        let new_label = format!("{ESCALATION_LABEL_PREFIX}{new_depth}");

        // Extract the blocker question from closed_reason or description.
        let blocker_context = task
            .closed_reason
            .as_deref()
            .unwrap_or("(no blocker details captured)");

        info!(
            project = %self.project_name,
            task = %task.id,
            depth = new_depth,
            "attempting project-level resolution"
        );

        // Record audit: TaskRetried.
        if let Some(ref audit) = self.audit_log {
            let _ = audit.record(
                &AuditEvent::new(
                    &self.project_name,
                    DecisionType::TaskRetried,
                    format!("Project-level resolution attempt {new_depth}"),
                )
                .with_task(&task.id.0),
            );
        }

        // Update task: append resolution context, increment depth, reset to Pending.
        let max_desc = self.max_description_chars;
        let mut store = self.tasks.lock().await;
        let _ = store.update(&task.id.0, |b| {
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
            Self::cap_description_with_limit(&mut b.description, max_desc);

            // Track escalation depth.
            b.labels.retain(|l| !l.starts_with(ESCALATION_LABEL_PREFIX));
            b.labels.push(new_label);

            // Reset to Pending so patrol picks it up for a new worker.
            b.status = TaskStatus::Pending;
            b.assignee = None;
        });
    }

    /// Escalate a blocked task through the escalation chain:
    ///   1. Project leader — first escalation
    ///   2. System leader (orchestrator) — if project leader can't resolve
    ///   3. Human (Telegram) — last resort
    async fn escalate_to_leader(&self, task: &sigil_tasks::Task) {
        // Determine escalation target based on current state.
        let already_escalated_project = task.labels.iter().any(|l| l == "escalated");
        let already_escalated_system = task.labels.iter().any(|l| l == "escalated-system");

        if already_escalated_system {
            // Already at system level. Check if we've already notified the human.
            let already_notified_human = task.labels.iter().any(|l| l == "escalated-human");
            if already_notified_human {
                return; // Already notified, waiting for human.
            }
            // Fire human escalation through gate channels.
            let summary = task.closed_reason.as_deref().unwrap_or("(no details)");
            let msg = format!(
                "BLOCKED: {}/{} — {}\n\n{}\n\nThis task has exhausted all automated resolution attempts.",
                self.project_name, task.id, task.subject, summary
            );
            for gate in &self.gate_channels {
                let outgoing = OutgoingMessage {
                    channel: gate.name().to_string(),
                    recipient: String::new(), // broadcast
                    text: msg.clone(),
                    metadata: serde_json::Value::Null,
                };
                if let Err(e) = gate.send(outgoing).await {
                    warn!(channel = %gate.name(), error = %e, "failed to send human escalation");
                }
            }
            // Send HumanEscalation dispatch for internal tracking.
            self.dispatch_bus
                .send(Dispatch::new_typed(
                    &format!("supervisor-{}", self.project_name),
                    "human",
                    DispatchKind::HumanEscalation {
                        project: self.project_name.clone(),
                        task_id: task.id.to_string(),
                        subject: task.subject.clone(),
                        summary: summary.to_string(),
                    },
                ))
                .await;
            // Mark as notified to prevent re-notification.
            {
                let mut store = self.tasks.lock().await;
                let _ = store.update(&task.id.0, |b| {
                    b.labels.push("escalated-human".to_string());
                });
            }
            info!(
                project = %self.project_name,
                task = %task.id,
                channels = self.gate_channels.len(),
                "human escalation sent"
            );
            return;
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
            task = %task.id,
            target = %target,
            "escalating blocked task"
        );

        // Record audit: TaskEscalated.
        if let Some(ref audit) = self.audit_log {
            let _ = audit.record(
                &AuditEvent::new(
                    &self.project_name,
                    DecisionType::TaskEscalated,
                    format!("Escalated to {target}"),
                )
                .with_task(&task.id.0),
            );
        }

        if let Some(ref m) = self.metrics {
            m.escalations_total.inc();
        }
        {
            let mut store = self.tasks.lock().await;
            let _ = store.update(&task.id.0, |b| {
                b.labels.push(label.to_string());
            });
        }

        self.dispatch_bus
            .send(Dispatch::new_typed(
                &format!("supervisor-{}", self.project_name),
                target,
                DispatchKind::Escalation {
                    project: self.project_name.clone(),
                    task_id: task.id.to_string(),
                    subject: task.subject.clone(),
                    description: format!(
                        "Priority: {}\n\nFull description:\n{}\n\n\
                         This task has been blocked after {} resolution attempt(s). \
                         Escalated to: {target}. \
                         Please try to resolve using your cross-project knowledge (KNOWLEDGE.md). \
                         If you can answer the blocker question, send a RESOLVED dispatch back to \
                         supervisor-{} with the answer. If you cannot resolve it, escalate to the \
                         human operator via Telegram.",
                        task.priority,
                        task.description,
                        Self::get_escalation_depth(&task.labels),
                        self.project_name,
                    ),
                    attempts: Self::get_escalation_depth(&task.labels),
                },
            ))
            .await;
    }

    /// Process a RESOLVED dispatch from the Leader Agent: re-open the blocked task
    /// with the answer appended to the description.
    pub async fn handle_resolution(&self, task_id: &str, answer: &str) {
        info!(
            project = %self.project_name,
            task = %task_id,
            "received resolution from leader"
        );

        let max_desc = self.max_description_chars;
        let mut store = self.tasks.lock().await;
        let _ = store.update(task_id, |b| {
            b.description.push_str(&format!(
                "\n\n---\n## Resolution (from Leader Agent)\n\n{answer}\n\n\
                 **Now proceed with the original task using this answer.**\n"
            ));
            Self::cap_description_with_limit(&mut b.description, max_desc);
            b.status = TaskStatus::Pending;
            b.assignee = None;
            // Remove escalation labels — fresh start with the answer.
            b.labels.retain(|l| {
                !l.starts_with(ESCALATION_LABEL_PREFIX)
                    && l != "escalated"
                    && l != "escalated-system"
            });
        });
    }

    /// Cap a task description to the configured limit, keeping the most recent content.
    fn cap_description_with_limit(desc: &mut String, max_chars: usize) {
        let total_chars = desc.chars().count();
        if total_chars <= max_chars {
            return;
        }

        // Keep the tail (most recent context is appended at the end).
        let excess_chars = total_chars.saturating_sub(max_chars).saturating_add(60);
        let base_cut = Self::byte_index_for_char(desc, excess_chars);
        let cut = desc[base_cut..]
            .find('\n')
            .map(|i| base_cut + i + 1)
            .unwrap_or(base_cut);
        let truncated = format!(
            "[... {} chars of earlier context truncated]\n{}",
            desc[..cut].chars().count(),
            &desc[cut..]
        );
        *desc = truncated;
    }

    fn byte_index_for_char(text: &str, char_idx: usize) -> usize {
        text.char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    /// Get escalation depth from task labels.
    fn get_escalation_depth(labels: &[String]) -> u32 {
        labels
            .iter()
            .filter_map(|l| l.strip_prefix(ESCALATION_LABEL_PREFIX))
            .filter_map(|n| n.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
    }

    /// Get worker count by state: (idle, running, 0).
    /// Workers launch immediately as background tasks.
    pub fn worker_counts(&self) -> (usize, usize, usize) {
        let running = self
            .running_tasks
            .iter()
            .filter(|t| !t.handle.is_finished())
            .count();
        let capacity = self.max_workers as usize;
        let idle = capacity.saturating_sub(running);
        (idle, running, 0)
    }

    /// Get real-time progress from all active workers.
    pub fn worker_progress(&self) -> Vec<serde_json::Value> {
        self.running_tasks
            .iter()
            .filter(|t| !t.handle.is_finished())
            .map(|t| {
                let (turns, cost, last_tool, status_msg) = match &t.progress_rx {
                    Some(rx) => {
                        let p = rx.borrow();
                        (
                            p.turns_so_far,
                            p.cost_so_far,
                            p.last_tool.clone(),
                            p.status_message.clone(),
                        )
                    }
                    None => (0, 0.0, None, None),
                };
                serde_json::json!({
                    "task_id": t.task_id,
                    "turns": turns,
                    "cost_usd": cost,
                    "last_tool": last_tool,
                    "status": status_msg,
                    "elapsed_secs": t.started_at.elapsed().as_secs(),
                    "timeout_secs": t.timeout_secs,
                })
            })
            .collect()
    }

    /// Resolve relevant domain skill file paths based on task labels and subject.
    /// Returns a markdown snippet listing relevant files the worker should read.
    fn resolve_domain_hints(labels: &[String], subject: &str, skills_dirs: &[std::path::PathBuf]) -> String {
        let text = format!("{} {}", subject, labels.join(" ")).to_lowercase();

        // Domain keyword → skill subdirectory paths to check
        let mappings: &[(&[&str], &[&str])] = &[
            (&["trading", "pms", "oms", "ems", "risk", "rms", "mms", "market making", "quote"],
             &["pipelines/trading.md", "services/pms.md", "services/oms.md", "services/ems.md"]),
            (&["data", "ingestion", "aggregation", "persistence", "orderbook"],
             &["pipelines/data.md", "services/ingestion.md", "services/aggregation.md"]),
            (&["strategy", "feature", "prediction", "signal", "optimizer", "fno", "ltc", "pfe"],
             &["pipelines/strategy.md", "services/feature.md", "services/prediction.md", "services/signal.md"]),
            (&["gateway", "api", "stream", "websocket", "configuration"],
             &["pipelines/gateway.md", "services/api.md", "services/stream.md"]),
            (&["types", "flatbuffer", "shared crate"],
             &["crates/types.md", "crates/keys.md"]),
            (&["zmq", "transport", "pubsub"],
             &["crates/zmq_transport.md", "zmq.md"]),
            (&["deploy", "systemd", "infrastructure"],
             &["systemd.md", "infrastructure-overview.md"]),
        ];

        let mut hints = Vec::new();
        for (keywords, paths) in mappings {
            if keywords.iter().any(|kw| text.contains(kw)) {
                for rel_path in *paths {
                    for dir in skills_dirs {
                        let full = dir.join(rel_path);
                        if full.exists() {
                            hints.push(format!("- `skills/{rel_path}`"));
                            break;
                        }
                    }
                }
            }
        }

        if hints.is_empty() {
            return String::new();
        }
        hints.dedup();
        format!("## Relevant Skill Files\nRead these for domain context:\n{}", hints.join("\n"))
    }

    /// Cancel a task by ID. Marks it as Cancelled and aborts any running worker.
    pub async fn cancel_task(&mut self, task_id: &str) -> Result<bool> {
        let mut store = self.tasks.lock().await;
        let task = store.get(task_id);
        if task.is_none() {
            return Ok(false);
        }

        let _ = store.update(task_id, |q| {
            q.status = sigil_tasks::TaskStatus::Cancelled;
            q.assignee = None;
            q.closed_reason = Some("Cancelled by user".to_string());
        });

        // Kill process group + abort the running worker if one exists for this task.
        self.running_tasks.retain(|t| {
            if t.task_id == task_id {
                let pid = t.child_pid.load(std::sync::atomic::Ordering::Relaxed);
                ClaudeCodeExecutor::kill_process_group(pid);
                t.handle.abort();
                info!(task_id, "cancelled running worker task");
                false
            } else {
                true
            }
        });

        info!(task_id, "task cancelled");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::Supervisor;

    #[test]
    fn cap_description_handles_unicode_without_panicking() {
        let mut description = format!("{}\n{}", "alpha ".repeat(120), "🙂".repeat(200),);

        Supervisor::cap_description_with_limit(&mut description, 160);

        assert!(description.starts_with("[... "));
        assert!(description.is_char_boundary(description.len()));
        assert!(description.contains('🙂'));
    }
}
