use anyhow::Result;
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
use crate::escalation::{EscalationPolicy, EscalationTracker};
use crate::execution_events::EventBroadcaster;
use crate::executor::TaskOutcome;
use crate::expertise::{ExpertiseLedger, ExpertiseRecord, TaskOutcomeKind};
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::metrics::SigilMetrics;
use crate::middleware::{
    ClarificationMiddleware, ContextBudgetMiddleware, ContextCompressionMiddleware,
    CostTrackingMiddleware, GraphGuardrailsMiddleware, GuardrailsMiddleware,
    LoopDetectionMiddleware, MemoryRefreshMiddleware, MiddlewareChain, Outcome, OutcomeStatus,
    SafetyNetMiddleware,
};
use crate::preflight::{PreflightAssessment, PreflightVerdict};
use crate::project::Project;
use crate::verification::{TaskContext, VerificationPipeline};

/// Label prefix for tracking escalation depth on tasks.
const ESCALATION_LABEL_PREFIX: &str = "escalation:";

/// A running worker with age tracking for timeout detection.
struct TrackedWorker {
    handle: tokio::task::JoinHandle<()>,
    task_id: String,
    started_at: std::time::Instant,
    /// Effective timeout for the running worker.
    timeout_secs: u64,
}

/// Supervisor: per-rig supervisor. Runs patrol cycles, manages workers,
/// detects stuck/orphaned tasks, handles escalation, reports to Leader Agent.
pub struct Supervisor {
    pub project_name: String,
    pub max_workers: u32,
    pub patrol_interval_secs: u64,
    pub dispatch_bus: Arc<DispatchBus>,
    pub tasks: Arc<Mutex<TaskBoard>>,
    pub provider: Arc<dyn Provider>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub model: String,
    pub identity: sigil_core::Identity,
    /// Repo path used for checkpoint capture and verification context.
    pub repo: Option<std::path::PathBuf>,
    /// Optional per-worker budget passed into middleware cost tracking.
    pub worker_max_budget_usd: Option<f64>,
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
    /// Event broadcaster for real-time execution events (Priority 2).
    pub event_broadcaster: Option<Arc<EventBroadcaster>>,
    /// Whether to run the verification pipeline on Done outcomes.
    pub verification_enabled: bool,
    /// Escalation tracker for task failure recovery (three-strikes policy).
    pub escalation_tracker: Arc<Mutex<EscalationTracker>>,
    /// Execution mode for workers (native agent loop vs Claude Code).
    pub execution_mode: sigil_core::ExecutionMode,
    /// Persistent agent registry — when set, workers look up agent identity
    /// from registry by name, loading system_prompt + entity memory scope.
    pub agent_registry: Option<Arc<crate::agent_registry::AgentRegistry>>,
    /// Trigger store — when set, agents with manage_triggers capability get the tool.
    pub trigger_store: Option<Arc<crate::trigger::TriggerStore>>,
    /// Conversation store — for channel_post tool and org context.
    pub conversation_store: Option<Arc<crate::ConversationStore>>,
}

impl Supervisor {
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
            provider,
            tools,
            model: project.model.clone(),
            identity: project.project_identity.clone(),
            repo: Some(project.repo.clone()),
            worker_max_budget_usd: None,
            running_tasks: Vec::new(),
            worker_timeout_secs: project.worker_timeout_secs,
            last_report: (0, 0),
            escalation_target: "leader".to_string(),
            system_escalation_target: "leader".to_string(),
            cost_ledger: None,
            metrics: None,
            memory: None,
            reflect_provider: None,
            reflect_model: String::new(),
            task_notify: project.task_notify.clone(),
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
            event_broadcaster: None,
            verification_enabled: true,
            escalation_tracker: Arc::new(Mutex::new(EscalationTracker::new(EscalationPolicy {
                max_retries: 4,
                cooldown_secs: 300,
                escalate_model: None,
            }))),
            execution_mode: sigil_core::ExecutionMode::default(),
            agent_registry: None,
            trigger_store: None,
            conversation_store: None,
        }
    }

    /// Set escalation targets.
    pub fn set_escalation_targets(&mut self, project_leader: &str, system_leader: &str) {
        self.escalation_target = project_leader.to_string();
        self.system_escalation_target = system_leader.to_string();
    }

    /// Look up a skill's system prompt by name from skills directories.
    /// Also extracts tool allow/deny lists and appends advisory restrictions to the prompt.
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
                let mut prompt = system.to_string();

                // Extract tool restrictions for advisory prompt injection.
                let allow: Vec<String> = value
                    .get("tools")
                    .and_then(|t| t.get("allow"))
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let deny: Vec<String> = value
                    .get("tools")
                    .and_then(|t| t.get("deny"))
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                if !allow.is_empty() || !deny.is_empty() {
                    prompt.push_str("\n\n## Tool Restrictions");
                    if !allow.is_empty() {
                        prompt.push_str(&format!(
                            "\nYou may ONLY use these tools: {}",
                            allow.join(", ")
                        ));
                    }
                    if !deny.is_empty() {
                        prompt.push_str(&format!("\nYou must NOT use: {}", deny.join(", ")));
                    }
                }

                return Some(prompt);
            }
        }
        None
    }

    /// Load a full Skill struct from the skills directories by name.
    fn load_skill(&self, skill_name: &str) -> Option<sigil_tools::Skill> {
        for dir in &self.skills_dirs {
            let path = dir.join(format!("{skill_name}.toml"));
            if path.exists() {
                match sigil_tools::Skill::load(&path) {
                    Ok(skill) => return Some(skill),
                    Err(e) => {
                        warn!(skill = %skill_name, error = %e, "failed to load skill TOML");
                    }
                }
            }
        }
        None
    }

    /// Create a native Sigil worker for a task.
    async fn create_worker(
        &self,
        agent_name: String,
        worker_name: String,
        task: &sigil_tasks::Task,
    ) -> AgentWorker {
        let mut identity = self.identity.clone();

        // Look up persistent agent from registry — override identity if found.
        let mut persistent_capabilities: Vec<String> = Vec::new();
        let mut agent_department: Option<String> = None;
        let persistent_agent_id = if let Some(ref registry) = self.agent_registry {
            if let Ok(Some(pa)) = registry.get_active_by_name(&agent_name).await {
                // Override system prompt with persistent agent's prompt.
                identity.persona = Some(pa.system_prompt.clone());
                persistent_capabilities = pa.capabilities.clone();
                agent_department = pa.department_id.clone();
                info!(
                    project = %self.project_name,
                    agent = %agent_name,
                    agent_id = %pa.id,
                    "loaded persistent agent identity from registry"
                );
                Some(pa.id.clone())
            } else {
                None
            }
        } else {
            None
        };

        let mut worker = match self.execution_mode {
            sigil_core::ExecutionMode::ClaudeCode => {
                let cwd = self
                    .repo
                    .clone()
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let budget = self.worker_max_budget_usd.unwrap_or(5.0);
                AgentWorker::new_claude_code(
                    agent_name.clone(),
                    worker_name,
                    self.project_name.clone(),
                    cwd,
                    budget,
                    identity.clone(),
                    self.dispatch_bus.clone(),
                    self.tasks.clone(),
                    self.task_notify.clone(),
                )
            }
            sigil_core::ExecutionMode::Agent => AgentWorker::new(
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
        };
        let persistent_agent_id_ref = persistent_agent_id.clone();
        if let Some(agent_id) = persistent_agent_id {
            worker = worker.with_persistent_agent(agent_id);
        }
        let persistent_agent_id = persistent_agent_id_ref;
        if let Some(ref mem) = self.memory {
            worker = worker.with_memory(mem.clone());
        }
        if let Some(ref provider) = self.reflect_provider {
            worker = worker.with_reflect(provider.clone(), self.reflect_model.clone());
        }
        if let Some(ref repo) = self.repo {
            worker = worker.with_project_dir(repo.clone());
        }

        // Pass adaptive retry config to the worker.
        if self.adaptive_retry {
            worker = worker.with_adaptive_retry(self.failure_analysis_model.clone());
        }

        // Pass blackboard + audit log for failure analysis mode-specific strategies.
        worker.blackboard = self.blackboard.clone();
        worker.audit_log = self.audit_log.clone();

        // Inject relevant blackboard entries into worker identity preamble.
        // Phase 7: Use query_scoped() with agent visibility to enforce
        // department-based blackboard access control.
        if let Some(ref bb) = self.blackboard {
            let tags: Vec<String> = task.labels.clone();
            let visibility = crate::blackboard::AgentVisibility {
                agent_id: persistent_agent_id.clone(),
                project: Some(self.project_name.clone()),
                department: agent_department.clone(),
            };
            let entries = bb
                .query_scoped(&self.project_name, &visibility, &tags, 5)
                .unwrap_or_default();
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

        // Inject TriggerManageTool if agent has manage_triggers capability.
        if persistent_capabilities
            .iter()
            .any(|c| c == "manage_triggers")
            && let (Some(ts), Some(agent_id)) = (&self.trigger_store, &persistent_agent_id)
            && let crate::agent_worker::WorkerExecution::Agent { ref mut tools, .. } =
                worker.execution
        {
            tools.push(Arc::new(crate::tools::TriggerManageTool::new(
                ts.clone(),
                agent_id.clone(),
            )));
            info!(
                project = %self.project_name,
                agent_id = %agent_id,
                "injected manage_triggers tool"
            );
        }

        // Inject communication tools for all persistent agents.
        if persistent_agent_id.is_some()
            && let crate::agent_worker::WorkerExecution::Agent { ref mut tools, .. } =
                worker.execution
        {
            // channel_post for department/project conversation channels.
            // transcript_search for cross-session recall.
            if let (Some(convs), Some(broadcaster)) =
                (&self.conversation_store, &self.event_broadcaster)
            {
                tools.push(Arc::new(crate::tools::ChannelPostTool::new(
                    convs.clone(),
                    broadcaster.clone(),
                    agent_name.clone(),
                )));
                tools.push(Arc::new(crate::tools::TranscriptSearchTool::new(
                    convs.clone(),
                )));
            }
        }

        // Inject org context into identity (department members, channels).
        if let (Some(registry), Some(agent_id)) = (&self.agent_registry, &persistent_agent_id) {
            let mut org_lines = Vec::new();

            // Show department peers if agent is in a department.
            if let Ok(Some(agent)) = registry.get(agent_id).await
                && let Some(ref dept_id) = agent.department_id
            {
                if let Ok(members) = registry.department_members(dept_id).await {
                    let names: Vec<&str> = members
                        .iter()
                        .filter(|m| {
                            m.id != *agent_id
                                && m.status == crate::agent_registry::AgentStatus::Active
                        })
                        .map(|m| m.name.as_str())
                        .collect();
                    if !names.is_empty() {
                        org_lines.push(format!("Department peers: {}", names.join(", ")));
                    }
                }
                if let Ok(Some(dept)) = registry.get_department(dept_id).await
                    && let Some(ref mgr_id) = dept.manager_id
                    && mgr_id != agent_id
                    && let Ok(Some(mgr)) = registry.get(mgr_id).await
                {
                    org_lines.push(format!(
                        "Department manager: {}{}",
                        mgr.name,
                        mgr.display_name
                            .as_ref()
                            .map(|d| format!(" ({d})"))
                            .unwrap_or_default()
                    ));
                }
            }

            if !org_lines.is_empty() {
                let org_context = format!("## Your Organization\n{}", org_lines.join("\n"));
                let existing = worker.identity.memory.clone().unwrap_or_default();
                worker.identity.memory = Some(format!("{existing}\n\n{org_context}"));
            }
        }

        // Inject skill system prompt and apply tool restrictions if task specifies a skill.
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

            if let Some(skill) = self.load_skill(skill_name)
                && (!skill.tools.allow.is_empty() || !skill.tools.deny.is_empty())
                && let crate::agent_worker::WorkerExecution::Agent { ref mut tools, .. } =
                    worker.execution
            {
                let before = tools.len();
                tools.retain(|t| skill.is_tool_allowed(t.name()));
                info!(
                    project = %self.project_name,
                    skill = %skill_name,
                    before = before,
                    after = tools.len(),
                    "filtered tools by skill policy"
                );
            }
        }

        // Inject domain knowledge hints based on task labels/subject.
        let hints = Self::resolve_domain_hints(&task.labels, &task.subject, &self.skills_dirs);
        if !hints.is_empty() {
            let existing = worker.identity.memory.clone().unwrap_or_default();
            worker.identity.memory = Some(format!("{existing}\n\n{hints}"));
        }

        // Build default middleware chain for this worker.
        let budget = self.worker_max_budget_usd.unwrap_or(10.0);
        let chain = MiddlewareChain::new(vec![
            Box::new(LoopDetectionMiddleware::new()),
            Box::new(CostTrackingMiddleware::new(budget)),
            Box::new(ContextBudgetMiddleware::new(200)),
            Box::new(GraphGuardrailsMiddleware::new(
                &dirs::home_dir().unwrap_or_default().join(".sigil"),
            )),
            Box::new(GuardrailsMiddleware::with_defaults()),
            Box::new(ContextCompressionMiddleware::new()),
            Box::new(MemoryRefreshMiddleware::new()),
            Box::new(ClarificationMiddleware::new()),
            Box::new(SafetyNetMiddleware::new()),
        ]);
        worker.set_middleware(chain);

        // Inject event broadcaster if available.
        if let Some(ref broadcaster) = self.event_broadcaster {
            worker.set_broadcaster(broadcaster.clone());
        }

        worker.with_max_task_retries(self.max_task_retries)
    }

    /// Run one patrol cycle: reap finished tasks, detect timeouts,
    /// assign + launch ready work, handle blocked tasks, report status.
    ///
    /// Worker execution is fully non-blocking — each worker runs as a background
    /// tokio task. The daemon loop never stalls waiting for workers.
    pub async fn patrol(&mut self) -> Result<()> {
        let patrol_start = std::time::Instant::now();
        debug!(project = %self.project_name, "patrol cycle");

        // 0. Reload tasks from disk to pick up externally-created tasks.
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
                if let Err(e) = store.update(&id, |t| {
                    t.status = TaskStatus::Pending;
                    t.assignee = None;
                }) {
                    warn!(project = %self.project_name, task = %id, error = %e, "failed to reset orphaned task");
                }
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
                if let Err(e) = store.update(&task_id, |q| {
                    q.status = TaskStatus::Pending;
                    q.assignee = None;
                }) {
                    warn!(project = %self.project_name, task = %task_id, error = %e, "failed to reset timed-out task");
                }
            }
            self.dispatch_bus
                .send(Dispatch::new_typed(
                    &format!("supervisor-{}", self.project_name),
                    &self.escalation_target,
                    DispatchKind::DelegateRequest {
                        prompt: format!(
                            "Worker timed out after {}s on task {} in project {}. Please investigate.",
                            self.worker_timeout_secs, task_id, self.project_name
                        ),
                        response_mode: "none".to_string(),
                        create_task: false,
                        skill: None,
                        reply_to: None,
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
            // Use task.agent_id if set (agent-bound), otherwise fall back to project name.
            let agent_name = task
                .agent_id
                .clone()
                .unwrap_or_else(|| self.project_name.clone());
            let worker_name = format!("{}:{}:{}", self.project_name, agent_name, worker_idx);

            if let Some(ref audit) = self.audit_log {
                let routing_summary = if task.agent_id.is_some() {
                    format!("Agent-bound → {agent_name}")
                } else {
                    format!("Project default → {agent_name}")
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
                mode = "agent",
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

            let mut worker = self
                .create_worker(agent_name.clone(), worker_name.clone(), &task)
                .await;

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
            let tasks_for_err = self.tasks.clone();
            let expertise_ledger = self.expertise_ledger.clone();
            let blackboard_worker = self.blackboard.clone();
            let audit_log_worker = self.audit_log.clone();
            let dispatch_bus_worker = self.dispatch_bus.clone();
            // Route response to the original delegating agent if present, else system leader.
            let outcome_recipient = task
                .labels
                .iter()
                .find_map(|l| l.strip_prefix("delegate_from:"))
                .unwrap_or(&self.system_escalation_target)
                .to_string();
            let delegate_dispatch_id = task
                .labels
                .iter()
                .find_map(|l| l.strip_prefix("delegate_dispatch:"))
                .map(String::from);
            let delegate_response_mode = task
                .labels
                .iter()
                .find_map(|l| l.strip_prefix("delegate_response_mode:"))
                .unwrap_or("origin")
                .to_string();
            let task_labels = task.labels.clone();
            let task_subject = task.subject.clone();
            let agent_name_for_records = agent_name.clone();
            let verification_enabled = self.verification_enabled;
            let verification_provider = self
                .reflect_provider
                .clone()
                .unwrap_or_else(|| self.provider.clone());
            let verification_model = self.preflight_model.clone();
            let verification_repo = self.repo.clone();
            let task_description = task.description.clone();
            let conversation_store = self.conversation_store.clone();
            let escalation_tracker = self.escalation_tracker.clone();
            let handle = tokio::spawn(async move {
                let start = std::time::Instant::now();
                match worker.execute().await {
                    Ok((outcome, mut runtime, cost_usd, turns)) => {
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
                                cost_usd,
                                turns,
                                timestamp: chrono::Utc::now(),
                                source: "agent".to_string(),
                                tokens: 0,
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

                        // Record to expertise ledger (skip cron tasks — automated, not skill-based).
                        if let Some(ref ledger) = expertise_ledger {
                            let domain =
                                ExpertiseLedger::extract_domain(&task_labels, &task_subject);
                            if domain == "cron" {
                                tracing::trace!(task_id = %task_id_clone, "skipping expertise record for cron task");
                            } else {
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
                                let response_content = format!(
                                    "Task {} completed: {}",
                                    task_id_clone, summary
                                );
                                let reply_to_id = delegate_dispatch_id
                                    .clone()
                                    .unwrap_or_else(|| task_id_clone.clone());

                                match delegate_response_mode.as_str() {
                                    "none" => {
                                        // Fire-and-forget: no response dispatch.
                                        debug!(
                                            task = %task_id_clone,
                                            "response_mode=none, skipping response dispatch"
                                        );
                                    }
                                    "department" => {
                                        // Post to department channel via ConversationStore.
                                        if let Some(ref cs) = conversation_store {
                                            let channel_name = format!("dept:{}", outcome_recipient);
                                            let chat_id = crate::conversation_store::named_channel_chat_id(&channel_name);
                                            let _ = cs.ensure_channel(chat_id, "department", &channel_name).await;
                                            let _ = cs.record_with_source(
                                                chat_id,
                                                "assistant",
                                                &response_content,
                                                Some("delegation"),
                                            ).await;
                                            debug!(
                                                task = %task_id_clone,
                                                channel = %channel_name,
                                                "response_mode=department, posted to channel"
                                            );
                                        }
                                        // Also send dispatch so delegate_from agent gets notified.
                                        dispatch_bus_worker
                                            .send(Dispatch::new_typed(
                                                &format!("supervisor-{project_name_task}"),
                                                &outcome_recipient,
                                                DispatchKind::DelegateResponse {
                                                    reply_to: reply_to_id,
                                                    response_mode: delegate_response_mode.clone(),
                                                    content: response_content.clone(),
                                                },
                                            ))
                                            .await;
                                    }
                                    // "origin", "perpetual", "async", or unknown: send to delegate_from.
                                    _ => {
                                        dispatch_bus_worker
                                            .send(Dispatch::new_typed(
                                                &format!("supervisor-{project_name_task}"),
                                                &outcome_recipient,
                                                DispatchKind::DelegateResponse {
                                                    reply_to: reply_to_id,
                                                    response_mode: delegate_response_mode.clone(),
                                                    content: response_content,
                                                },
                                            ))
                                            .await;
                                    }
                                }

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

                                // Verification pipeline: validate the outcome.
                                let verification_approved = if verification_enabled {
                                    // Extract done_condition from task description.
                                    // Convention: lines starting with "Done when:" or "Acceptance:" are conditions.
                                    let done_condition = task_description
                                        .lines()
                                        .find(|l| {
                                            let lower = l.to_lowercase();
                                            lower.starts_with("done when:")
                                                || lower.starts_with("acceptance:")
                                                || lower.starts_with("done_condition:")
                                        })
                                        .map(|l| l.to_string())
                                        .or_else(|| {
                                            // Fall back to using the full description as the spec.
                                            if !task_description.is_empty() {
                                                Some(task_description.clone())
                                            } else {
                                                None
                                            }
                                        });

                                    let runtime_artifacts = runtime.outcome.artifact_refs();
                                    let task_ctx = TaskContext {
                                        task_id: task_id_clone.clone(),
                                        subject: task_subject.clone(),
                                        done_condition,
                                        project: project_name_task.clone(),
                                        project_dir: verification_repo.clone(),
                                        artifacts: runtime_artifacts.clone(),
                                    };
                                    let mw_outcome = Outcome {
                                        status: OutcomeStatus::Done,
                                        confidence: 0.8,
                                        artifacts: runtime_artifacts,
                                        cost_usd,
                                        turns,
                                        duration_ms: (duration_secs * 1000.0) as u64,
                                        reason: Some(summary.clone()),
                                        runtime: Some(runtime.outcome.clone()),
                                    };

                                    let pipeline = if !verification_model.is_empty() {
                                        VerificationPipeline::with_defaults().with_provider(
                                            verification_provider.clone(),
                                            verification_model.clone(),
                                        )
                                    } else {
                                        VerificationPipeline::with_defaults()
                                    };

                                    let result = pipeline.verify(&mw_outcome, &task_ctx).await;

                                    info!(
                                        task = %task_id_clone,
                                        confidence = result.confidence,
                                        approved = result.approved,
                                        reason = %result.reason,
                                        suggestions = ?result.suggestions,
                                        "verification complete"
                                    );

                                    runtime.outcome.verification =
                                        Some(crate::runtime::VerificationReport::from(&result));

                                    if !result.approved {
                                        warn!(
                                            task = %task_id_clone,
                                            confidence = result.confidence,
                                            "verification rejected — task will be retried with feedback"
                                        );

                                        // Record verification failure in audit log.
                                        if let Some(ref audit) = audit_log_worker {
                                            let _ = audit.record(
                                                &AuditEvent::new(
                                                    &project_name_task,
                                                    DecisionType::TaskFailed,
                                                    format!(
                                                        "Verification rejected (confidence={:.2}): {}",
                                                        result.confidence, result.reason
                                                    ),
                                                )
                                                .with_task(&task_id_clone)
                                                .with_agent(&agent_name_for_records),
                                            );
                                        }

                                        // Dispatch verification feedback for retry context.
                                        let feedback = format!(
                                            "Verification rejected your work (confidence={:.2}).\n\
                                             Reason: {}\n\
                                             Suggestions:\n{}",
                                            result.confidence,
                                            result.reason,
                                            result
                                                .suggestions
                                                .iter()
                                                .map(|s| format!("- {s}"))
                                                .collect::<Vec<_>>()
                                                .join("\n")
                                        );
                                        dispatch_bus_worker
                                            .send(Dispatch::new_typed(
                                                &format!("verification-{project_name_task}"),
                                                &outcome_recipient,
                                                DispatchKind::DelegateRequest {
                                                    prompt: feedback,
                                                    response_mode: "origin".to_string(),
                                                    create_task: false,
                                                    skill: None,
                                                    reply_to: Some(task_id_clone.clone()),
                                                },
                                            ))
                                            .await;
                                    }

                                    result.approved
                                } else {
                                    true // No verification — implicitly approved.
                                };

                                // Clear escalation state on success (only if verification passed).
                                if verification_approved {
                                    let mut tracker = escalation_tracker.lock().await;
                                    tracker.record_success(&task_id_clone);
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
                                        DispatchKind::DelegateRequest {
                                            prompt: format!(
                                                "Task {} blocked: {}\n\nContext:\n{}",
                                                task_id_clone, question, full_text
                                            ),
                                            response_mode: "origin".to_string(),
                                            create_task: false,
                                            skill: None,
                                            reply_to: Some(task_id_clone.clone()),
                                        },
                                    ))
                                    .await;
                            }
                            TaskOutcome::Failed(error) => {
                                dispatch_bus_worker
                                    .send(Dispatch::new_typed(
                                        &format!("supervisor-{project_name_task}"),
                                        &outcome_recipient,
                                        DispatchKind::DelegateRequest {
                                            prompt: format!(
                                                "Task {} failed: {}",
                                                task_id_clone, error
                                            ),
                                            response_mode: "none".to_string(),
                                            create_task: false,
                                            skill: None,
                                            reply_to: Some(task_id_clone.clone()),
                                        },
                                    ))
                                    .await;

                                // Record failure and decide escalation action.
                                {
                                    let mut tracker = escalation_tracker.lock().await;
                                    tracker.record_failure(&task_id_clone, &agent_name_for_records);
                                    let action = tracker.decide(&task_id_clone);
                                    info!(
                                        task = %task_id_clone,
                                        agent = %agent_name_for_records,
                                        action = ?action,
                                        "escalation decision after failure"
                                    );
                                }
                            }
                            TaskOutcome::Handoff { .. } => {}
                        }

                        // Save full session transcript to ConversationStore.
                        if let (Some(cs), Some(repo)) = (&conversation_store, &verification_repo) {
                            let session_path = repo
                                .join(".sigil")
                                .join("sessions")
                                .join(format!("{}.json", task_id_clone));
                            if let Ok(content) = tokio::fs::read_to_string(&session_path).await
                                && let Ok(state) =
                                    serde_json::from_str::<sigil_core::SessionState>(&content)
                            {
                                let chat_id = crate::conversation_store::named_channel_chat_id(
                                    &format!("transcript:{}", agent_name_for_records),
                                );
                                let _ = cs
                                    .ensure_channel(chat_id, "transcript", &agent_name_for_records)
                                    .await;
                                for msg in &state.messages {
                                    let role = match msg.role {
                                        sigil_core::traits::Role::User => "user",
                                        sigil_core::traits::Role::Assistant => "assistant",
                                        sigil_core::traits::Role::System => "system",
                                        sigil_core::traits::Role::Tool => "tool",
                                    };
                                    let text = msg.content.to_transcript_text();
                                    if !text.is_empty() {
                                        let _ = cs
                                            .record_with_source(chat_id, role, &text, Some("agent"))
                                            .await;
                                    }
                                }
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
                    }
                }
            });
            self.running_tasks.push(TrackedWorker {
                handle,
                task_id,
                started_at: std::time::Instant::now(),
                timeout_secs: self.worker_timeout_secs.max(1800),
            });
        }

        // 3.5. Detect stalled missions and spawn redecomposition as a background task.
        // Runs outside the supervisor lock to avoid blocking IPC and next patrol.
        if self.auto_redecompose && !self.decomposition_model.is_empty() {
            let store = self.tasks.lock().await;
            let active_missions = store.active_missions(None);
            let mut stalled: Option<(String, String, String)> = None;
            for mission in &active_missions {
                let tasks = store.mission_tasks(&mission.id);
                if tasks.is_empty() {
                    continue;
                }
                let all_stalled = tasks
                    .iter()
                    .all(|t| t.status == TaskStatus::Blocked || t.status == TaskStatus::Cancelled);
                if all_stalled {
                    stalled = Some((
                        mission.id.clone(),
                        mission.name.clone(),
                        mission.description.clone(),
                    ));
                    break;
                }
            }
            drop(store);

            if let Some((mission_id, mission_name, mission_desc)) = stalled {
                info!(
                    project = %self.project_name,
                    mission = %mission_id,
                    "stalled mission detected — spawning background redecomposition"
                );
                let tasks = self.tasks.clone();
                let provider = self
                    .reflect_provider
                    .clone()
                    .unwrap_or_else(|| self.provider.clone());
                let model = self.decomposition_model.clone();
                let infer_threshold = self.infer_deps_threshold;
                let audit_log = self.audit_log.clone();
                let project_name = self.project_name.clone();

                tokio::spawn(async move {
                    let prompt =
                        DecompositionResult::decomposition_prompt(&mission_name, &mission_desc);
                    let request = ChatRequest {
                        model,
                        messages: vec![Message {
                            role: Role::User,
                            content: MessageContent::text(&prompt),
                        }],
                        tools: vec![],
                        max_tokens: 2048,
                        temperature: 0.0,
                    };
                    if let Ok(response) = provider.chat(&request).await
                        && let Some(ref text) = response.content
                    {
                        let mut result = DecompositionResult::parse(text);
                        let mut store = tasks.lock().await;
                        let prefix = mission_id.split('-').next().unwrap_or("xx");
                        match result.materialize(&mut store, prefix, &mission_id) {
                            Ok(task_ids) => {
                                info!(
                                    project = %project_name,
                                    mission = %mission_id,
                                    new_tasks = task_ids.len(),
                                    "redecomposed stalled mission"
                                );
                                if infer_threshold > 0.0
                                    && let Ok(n) =
                                        store.apply_inferred_dependencies(infer_threshold)
                                    && n > 0
                                {
                                    info!(
                                        project = %project_name,
                                        mission = %mission_id,
                                        inferred = n,
                                        "inferred task dependencies"
                                    );
                                    if let Some(ref audit) = audit_log {
                                        let _ = audit.record(
                                            &AuditEvent::new(
                                                &project_name,
                                                DecisionType::DependencyInferred,
                                                format!(
                                                    "Inferred {n} dependencies in mission {mission_id}"
                                                ),
                                            )
                                            .with_task(&mission_id),
                                        );
                                    }
                                }
                                if let Some(ref audit) = audit_log {
                                    let _ = audit.record(
                                        &AuditEvent::new(
                                            &project_name,
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
                                    project = %project_name,
                                    mission = %mission_id,
                                    error = %e,
                                    "redecomposition materialization failed"
                                );
                            }
                        }
                    }
                });
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
                    DispatchKind::DelegateResponse {
                        reply_to: format!("patrol-{}", self.project_name),
                        response_mode: "none".to_string(),
                        content: format!(
                            "Project {}: {} active workers, {} pending tasks",
                            self.project_name, active, pending
                        ),
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
    // Phase 8: Clarification routing now goes through the department hierarchy
    // via `escalate_to_leader()` which uses `escalation_chain_target()` (Phase 6).
    // Blocked tasks (including clarification-blocked) are handled here:
    //   - Low escalation depth → re-opened as Pending (project-level resolution)
    //   - High escalation depth → escalated via department chain → leader → human
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
                // Already tried project-level resolution. Escalate via department
                // chain (Phase 6) → project leader → system leader → human.
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

        // Extract the blocker context from the typed task outcome when available.
        let blocker_context = task
            .blocker_context()
            .unwrap_or_else(|| "(no blocker details captured)".to_string());

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

    /// Walk the department hierarchy for an agent and return the first
    /// department manager name that isn't the agent itself. Returns `None` if no
    /// department chain exists (falls back to legacy escalation_target).
    async fn escalation_chain_target(&self, agent_name: &str) -> Option<String> {
        let registry = self.agent_registry.as_ref()?;
        let agent = registry.get_active_by_name(agent_name).await.ok()??;

        // Walk department chain to find a manager.
        let mut current_dept_id = agent.department_id.clone();
        for _ in 0..10 {
            let dept_id = current_dept_id.as_ref()?;
            let dept = registry.get_department(dept_id).await.ok()??;
            if let Some(ref mgr_id) = dept.manager_id
                && let Ok(Some(mgr)) = registry.get(mgr_id).await
                && mgr.name != agent_name
            {
                return Some(mgr.name);
            }
            // Walk up to parent department.
            current_dept_id = dept.parent_id;
        }
        None
    }

    /// Escalate a blocked task through the escalation chain:
    ///   1. Department manager (via org tree) — preferred path
    ///   2. Project leader — first fallback
    ///   3. System leader (orchestrator) — if project leader can't resolve
    ///   4. Human (Telegram) — last resort
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
            let summary = task
                .outcome_summary()
                .unwrap_or_else(|| "(no details)".to_string());
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

        // Phase 6: Try department hierarchy first. If the task has an assignee,
        // walk the org tree to find the department manager as the preferred
        // escalation target. Falls back to the hardcoded escalation_target.
        let dept_target = if !already_escalated_project {
            if let Some(ref assignee) = task.assignee {
                self.escalation_chain_target(assignee).await
            } else {
                None
            }
        } else {
            None
        };

        let (target, label) = if let Some(ref dt) = dept_target {
            // Department chain escalation — preferred path.
            (dt.as_str(), "escalated")
        } else if !already_escalated_project && !project_leader_is_system {
            // First escalation → project leader (legacy path).
            (self.escalation_target.as_str(), "escalated")
        } else {
            // Second escalation → system leader.
            (self.system_escalation_target.as_str(), "escalated-system")
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
                DispatchKind::DelegateRequest {
                    prompt: format!(
                        "Escalation from project {}: task {} — {}\n\n\
                         Priority: {}\n\nFull description:\n{}\n\n\
                         This task has been blocked after {} resolution attempt(s). \
                         Escalated to: {target}. \
                         Please try to resolve using your cross-project knowledge (KNOWLEDGE.md). \
                         If you can answer the blocker question, send a delegation response back. \
                         If you cannot resolve it, escalate to the human operator via Telegram.",
                        self.project_name,
                        task.id,
                        task.subject,
                        task.priority,
                        task.description,
                        Self::get_escalation_depth(&task.labels),
                    ),
                    response_mode: "origin".to_string(),
                    create_task: false,
                    skill: None,
                    reply_to: Some(task.id.to_string()),
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
                serde_json::json!({
                    "task_id": t.task_id,
                    "turns": 0,
                    "cost_usd": 0.0,
                    "last_tool": serde_json::Value::Null,
                    "status": "running",
                    "elapsed_secs": t.started_at.elapsed().as_secs(),
                    "timeout_secs": t.timeout_secs,
                })
            })
            .collect()
    }

    /// Resolve relevant domain skill file paths based on task labels and subject.
    /// Returns a markdown snippet listing relevant files the worker should read.
    fn resolve_domain_hints(
        labels: &[String],
        subject: &str,
        skills_dirs: &[std::path::PathBuf],
    ) -> String {
        let text = format!("{} {}", subject, labels.join(" ")).to_lowercase();

        // Domain keyword → skill subdirectory paths to check
        let mappings: &[(&[&str], &[&str])] = &[
            (
                &[
                    "trading",
                    "pms",
                    "oms",
                    "ems",
                    "risk",
                    "rms",
                    "mms",
                    "market making",
                    "quote",
                ],
                &[
                    "pipelines/trading.md",
                    "services/pms.md",
                    "services/oms.md",
                    "services/ems.md",
                ],
            ),
            (
                &[
                    "data",
                    "ingestion",
                    "aggregation",
                    "persistence",
                    "orderbook",
                ],
                &[
                    "pipelines/data.md",
                    "services/ingestion.md",
                    "services/aggregation.md",
                ],
            ),
            (
                &[
                    "strategy",
                    "feature",
                    "prediction",
                    "signal",
                    "optimizer",
                    "fno",
                    "ltc",
                    "pfe",
                ],
                &[
                    "pipelines/strategy.md",
                    "services/feature.md",
                    "services/prediction.md",
                    "services/signal.md",
                ],
            ),
            (
                &["gateway", "api", "stream", "websocket", "configuration"],
                &[
                    "pipelines/gateway.md",
                    "services/api.md",
                    "services/stream.md",
                ],
            ),
            (
                &["types", "flatbuffer", "shared crate"],
                &["crates/types.md", "crates/keys.md"],
            ),
            (
                &["zmq", "transport", "pubsub"],
                &["crates/zmq_transport.md", "zmq.md"],
            ),
            (
                &["deploy", "systemd", "infrastructure"],
                &["systemd.md", "infrastructure-overview.md"],
            ),
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
        format!(
            "## Relevant Skill Files\nRead these for domain context:\n{}",
            hints.join("\n")
        )
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
