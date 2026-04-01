use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use sigil_core::traits::{ChatRequest, Message, MessageContent, Provider, Role};

use crate::audit::{AuditEvent, AuditLog, DecisionType};
use crate::blackboard::Blackboard;
use crate::conversation_store::ConversationStore;
use crate::cost_ledger::CostLedger;
use crate::decomposition::DecompositionResult;
use crate::expertise::ExpertiseLedger;
use crate::message::DispatchBus;
use crate::metrics::SigilMetrics;
use crate::operation::OperationStore;
use crate::project::Project;
use crate::supervisor::Supervisor;

pub struct ProjectRegistry {
    projects: RwLock<HashMap<String, Arc<Project>>>,
    supervisors: RwLock<HashMap<String, Arc<Mutex<Supervisor>>>>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub wake: Arc<tokio::sync::Notify>,
    pub cost_ledger: Arc<CostLedger>,
    pub metrics: Arc<SigilMetrics>,
    /// Name of the leader agent for dispatch routing.
    pub leader_agent_name: String,
    /// Optional operation store for cross-project task tracking.
    pub operation_store: Option<Arc<Mutex<OperationStore>>>,
    /// Decision audit log (Phase 1).
    pub audit_log: Option<Arc<AuditLog>>,
    /// Agent expertise ledger for smart routing (Phase 2).
    pub expertise_ledger: Option<Arc<ExpertiseLedger>>,
    /// Inter-agent blackboard for shared knowledge (Phase 3).
    pub blackboard: Option<Arc<Blackboard>>,
    /// Unified conversation store for all chat channels.
    pub conversation_store: Option<Arc<ConversationStore>>,
    /// Names from [[projects]] config (to distinguish from agent entries).
    pub config_project_names: Vec<String>,
}

impl ProjectRegistry {
    pub fn new(dispatch_bus: Arc<DispatchBus>, leader_agent_name: String) -> Self {
        Self {
            projects: RwLock::new(HashMap::new()),
            supervisors: RwLock::new(HashMap::new()),
            dispatch_bus,
            wake: Arc::new(tokio::sync::Notify::new()),
            cost_ledger: Arc::new(CostLedger::new(50.0)),
            metrics: Arc::new(SigilMetrics::new()),
            leader_agent_name,
            operation_store: None,
            audit_log: None,
            expertise_ledger: None,
            blackboard: None,
            conversation_store: None,
            config_project_names: Vec::new(),
        }
    }

    /// Set a custom cost ledger (e.g., with persistence).
    pub fn set_cost_ledger(&mut self, ledger: Arc<CostLedger>) {
        self.cost_ledger = ledger;
    }

    /// Set the operation store for cross-project task tracking.
    pub fn set_operation_store(&mut self, store: Arc<Mutex<OperationStore>>) {
        self.operation_store = Some(store);
    }

    /// Register a project without creating a Supervisor.
    /// Used for dynamically registered projects
    /// but don't run daemon-driven execution.
    pub async fn register_project_only(&self, project: Arc<Project>) {
        let name = project.name.clone();
        self.metrics.ensure_project(&name);
        self.projects.write().await.insert(name, project);
    }

    /// Remove a project from the registry (in-memory only).
    pub async fn remove_project(&self, name: &str) -> bool {
        self.projects.write().await.remove(name).is_some()
    }

    pub async fn register_project(&self, project: Arc<Project>, mut supervisor: Supervisor) {
        let name = project.name.clone();
        // Inject cost ledger + metrics + v3 components into the supervisor.
        supervisor.cost_ledger = Some(self.cost_ledger.clone());
        supervisor.metrics = Some(self.metrics.clone());
        supervisor.audit_log = self.audit_log.clone();
        supervisor.expertise_ledger = self.expertise_ledger.clone();
        supervisor.blackboard = self.blackboard.clone();
        self.metrics.ensure_project(&name);
        self.projects.write().await.insert(name.clone(), project);
        self.supervisors
            .write()
            .await
            .insert(name, Arc::new(Mutex::new(supervisor)));
    }

    /// Wire agent registry and trigger store into all registered supervisors.
    pub async fn wire_agent_system(
        &self,
        agent_registry: Arc<crate::agent_registry::AgentRegistry>,
        trigger_store: Arc<crate::trigger::TriggerStore>,
    ) {
        let sups = self.supervisors.write().await;
        for (_name, sup) in sups.iter() {
            let mut s = sup.lock().await;
            s.agent_registry = Some(agent_registry.clone());
            s.trigger_store = Some(trigger_store.clone());
        }
    }

    pub async fn assign(
        &self,
        project_name: &str,
        subject: &str,
        description: &str,
    ) -> Result<sigil_tasks::Task> {
        let projects = self.projects.read().await;
        let project = projects
            .get(project_name)
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;

        let mut task = project.create_task(subject).await?;

        if !description.is_empty() {
            let mut store = project.tasks.lock().await;
            task = store.update(&task.id.0, |q| {
                q.description = description.to_string();
            })?;
        }

        info!(
            project = %project_name,
            task = %task.id,
            subject = %subject,
            "task assigned"
        );

        self.wake.notify_one();
        Ok(task)
    }

    /// Assign a task with a specific skill to load on the worker.
    /// Used by the trigger system to ensure the agent runs the trigger's skill.
    pub async fn assign_with_skill(
        &self,
        project_name: &str,
        subject: &str,
        description: &str,
        skill: &str,
    ) -> Result<sigil_tasks::Task> {
        let projects = self.projects.read().await;
        let project = projects
            .get(project_name)
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;

        let mut task = project.create_task(subject).await?;

        {
            let mut store = project.tasks.lock().await;
            task = store.update(&task.id.0, |q| {
                if !description.is_empty() {
                    q.description = description.to_string();
                }
                q.skill = Some(skill.to_string());
            })?;
        }

        info!(
            project = %project_name,
            task = %task.id,
            subject = %subject,
            skill = %skill,
            "task assigned with skill"
        );

        self.wake.notify_one();
        Ok(task)
    }

    /// Create a mission and optionally decompose it into a task DAG using an LLM.
    pub async fn create_mission_with_decomposition(
        &self,
        project_name: &str,
        mission_name: &str,
        description: &str,
        decomposition_model: &str,
        provider: &Arc<dyn Provider>,
        infer_deps_threshold: f64,
    ) -> Result<sigil_tasks::Mission> {
        let projects = self.projects.read().await;
        let project = projects
            .get(project_name)
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;

        let mut store = project.tasks.lock().await;
        let mut mission = store.create_mission(&project.prefix, mission_name)?;

        if !description.is_empty() {
            mission = store.update_mission(&mission.id, |m| {
                m.description = description.to_string();
            })?;
        }

        // Decompose if model is provided and description is non-empty.
        if !decomposition_model.is_empty() && !description.is_empty() {
            let prompt = DecompositionResult::decomposition_prompt(mission_name, description);
            let request = ChatRequest {
                model: decomposition_model.to_string(),
                messages: vec![Message {
                    role: Role::User,
                    content: MessageContent::text(&prompt),
                }],
                tools: vec![],
                max_tokens: 2048,
                temperature: 0.0,
            };
            match provider.chat(&request).await {
                Ok(response) if response.content.is_some() => {
                    let mut result =
                        DecompositionResult::parse(response.content.as_deref().unwrap());
                    let task_ids = result.materialize(&mut store, &project.prefix, &mission.id)?;
                    info!(
                        project = %project_name,
                        mission = %mission.id,
                        tasks = task_ids.len(),
                        critical_path = result.critical_path.len(),
                        "mission decomposed into task DAG"
                    );

                    // Infer dependencies between newly created tasks.
                    if infer_deps_threshold > 0.0
                        && let Ok(n) = store.apply_inferred_dependencies(infer_deps_threshold)
                        && n > 0
                    {
                        info!(
                            project = %project_name,
                            mission = %mission.id,
                            inferred = n,
                            "inferred task dependencies"
                        );
                        if let Some(ref audit) = self.audit_log {
                            let _ = audit.record(
                                &AuditEvent::new(
                                    project_name,
                                    DecisionType::DependencyInferred,
                                    format!("Inferred {n} dependencies in mission {}", mission.id),
                                )
                                .with_task(&mission.id),
                            );
                        }
                    }

                    if let Some(ref audit) = self.audit_log {
                        let _ = audit.record(
                            &AuditEvent::new(
                                project_name,
                                DecisionType::MissionDecomposed,
                                format!(
                                    "Mission {} decomposed into {} tasks",
                                    mission.id,
                                    task_ids.len()
                                ),
                            )
                            .with_task(&mission.id),
                        );
                    }
                }
                Ok(_) => {
                    warn!(
                        project = %project_name,
                        mission = %mission.id,
                        "decomposition returned empty response"
                    );
                }
                Err(e) => {
                    warn!(
                        project = %project_name,
                        mission = %mission.id,
                        error = %e,
                        "decomposition failed"
                    );
                }
            }
        }

        self.wake.notify_one();
        Ok(mission)
    }

    pub async fn patrol_all(&self) -> Result<()> {
        let whispers = self.dispatch_bus.read(&self.leader_agent_name).await;
        for w in &whispers {
            let mut handled = true;
            match &w.kind {
                crate::message::DispatchKind::PatrolReport {
                    project,
                    active,
                    pending,
                } => {
                    info!(from = %w.from, project = %project, active = active, pending = pending, "supervisor report");
                }
                crate::message::DispatchKind::WorkerCrashed {
                    project,
                    worker,
                    error,
                } => {
                    warn!(from = %w.from, project = %project, worker = %worker, error = %error, "worker crashed");
                }
                crate::message::DispatchKind::TaskDone { task_id, .. } => {
                    if let Some(ref operation_store) = self.operation_store {
                        let qid = sigil_tasks::TaskId(task_id.clone());
                        let mut store = operation_store.lock().await;
                        match store.mark_task_closed(&qid) {
                            Ok(completed_ops) => {
                                for op_id in completed_ops {
                                    info!(operation = %op_id, "operation completed");
                                }
                            }
                            Err(e) => {
                                handled = false;
                                warn!(task = %task_id, error = %e, "failed to update operation store");
                            }
                        }
                    }
                }
                _ => {
                    info!(from = %w.from, kind = %w.kind.subject_tag(), "dispatch received");
                }
            }
            if handled && w.requires_ack {
                self.dispatch_bus.acknowledge(&w.id).await;
            }
        }

        // Parallel patrol: collect Arc clones, drop read lock, then join_all.
        let supervisor_entries: Vec<(String, Arc<Mutex<Supervisor>>)> = {
            let supervisors = self.supervisors.read().await;
            supervisors
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        let futures: Vec<_> = supervisor_entries
            .iter()
            .map(|(name, sup)| {
                let name = name.clone();
                let sup = sup.clone();
                async move {
                    let mut s = sup.lock().await;
                    if let Err(e) = s.patrol().await {
                        warn!(project = %name, error = %e, "supervisor patrol failed");
                    }
                }
            })
            .collect();

        futures::future::join_all(futures).await;

        // Dispatch Resolution messages to the appropriate supervisors.
        // Leader agent sends Resolution dispatches addressed to "supervisor-{project}".
        for (project_name, sup) in &supervisor_entries {
            let sup_recipient = format!("supervisor-{}", project_name);
            let dispatches = self.dispatch_bus.read(&sup_recipient).await;
            for w in dispatches {
                if let crate::message::DispatchKind::Resolution { task_id, answer } = &w.kind {
                    info!(project = %project_name, task = %task_id, "dispatching resolution to supervisor");
                    let s = sup.lock().await;
                    s.handle_resolution(task_id, answer).await;
                    if w.requires_ack {
                        self.dispatch_bus.acknowledge(&w.id).await;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn status(&self) -> RegistryStatus {
        let mut project_statuses = Vec::new();
        let projects = self.projects.read().await;
        let supervisors = self.supervisors.read().await;

        for (name, project) in projects.iter() {
            let open = project.open_tasks().await.len();
            let ready = project.ready_tasks().await.len();
            let (idle, working, bonded) = if let Some(s) = supervisors.get(name) {
                s.lock().await.worker_counts()
            } else {
                (0, 0, 0)
            };

            // Get team leader from the supervisor.
            let team_leader = if let Some(s) = supervisors.get(name) {
                let guard = s.lock().await;
                guard.team.as_ref().map(|t| t.leader.clone())
            } else {
                None
            };

            project_statuses.push(ProjectStatus {
                name: name.clone(),
                open_tasks: open,
                ready_tasks: ready,
                workers_idle: idle,
                workers_working: working,
                workers_bonded: bonded,
                team_leader,
            });
        }

        let unread = self
            .dispatch_bus
            .unread_count(&self.leader_agent_name)
            .await;

        RegistryStatus {
            projects: project_statuses,
            unread_dispatches: unread,
        }
    }

    pub async fn all_ready(&self) -> Vec<(String, sigil_tasks::Task)> {
        let mut all = Vec::new();
        let projects = self.projects.read().await;
        for (name, project) in projects.iter() {
            for task in project.ready_tasks().await {
                all.push((name.clone(), task));
            }
        }
        all
    }

    pub async fn project_names(&self) -> Vec<String> {
        self.projects.read().await.keys().cloned().collect()
    }

    pub async fn get_project(&self, name: &str) -> Option<Arc<Project>> {
        self.projects.read().await.get(name).cloned()
    }

    pub async fn project_count(&self) -> usize {
        self.projects.read().await.len()
    }

    pub async fn total_max_workers(&self) -> u32 {
        self.projects
            .read()
            .await
            .values()
            .map(|d| d.max_workers)
            .sum()
    }

    pub async fn project_worker_limits(&self) -> Vec<(String, u32)> {
        self.projects
            .read()
            .await
            .iter()
            .map(|(name, project)| (name.clone(), project.max_workers))
            .collect()
    }

    pub async fn projects_info(&self) -> Vec<serde_json::Value> {
        self.projects
            .read()
            .await
            .values()
            .map(|d| {
                serde_json::json!({
                    "name": d.name,
                    "prefix": d.prefix,
                    "model": d.model,
                    "max_workers": d.max_workers,
                })
            })
            .collect()
    }

    /// Get real-time progress from all active workers across all projects.
    pub async fn worker_progress(&self) -> Vec<serde_json::Value> {
        let supervisors = self.supervisors.read().await;
        let mut all = Vec::new();
        for (name, sup) in supervisors.iter() {
            if let Ok(sup) = sup.try_lock() {
                let mut entries = sup.worker_progress();
                for entry in &mut entries {
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert("project".to_string(), serde_json::json!(name));
                    }
                }
                all.extend(entries);
            }
        }
        all
    }

    /// Get a supervisor by project name (for config reload).
    pub async fn get_supervisor(&self, project: &str) -> Option<Arc<Mutex<Supervisor>>> {
        self.supervisors.read().await.get(project).cloned()
    }

    /// Get a project's TaskBoard for direct task/mission access.
    pub async fn get_task_board(
        &self,
        project_name: &str,
    ) -> Option<std::sync::Arc<tokio::sync::Mutex<sigil_tasks::TaskBoard>>> {
        self.projects
            .read()
            .await
            .get(project_name)
            .map(|p| p.tasks.clone())
    }

    /// List all projects with summary stats (task counts, mission counts, team info).
    /// Designed to minimize lock hold times — snapshot project list first, then read each
    /// project's task board independently without holding the registry-level RwLocks.
    pub async fn list_project_summaries(&self) -> Vec<ProjectSummary> {
        // Step 1: Snapshot project list + supervisor refs, then release RwLocks immediately.
        let project_list: Vec<(String, Arc<Project>)> = {
            let projects = self.projects.read().await;
            projects
                .iter()
                .filter(|(name, _)| {
                    self.config_project_names.is_empty()
                        || self.config_project_names.iter().any(|n| n == *name)
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        }; // projects RwLock RELEASED here.

        let supervisor_refs: Vec<(String, Arc<Mutex<Supervisor>>)> = {
            let supervisors = self.supervisors.read().await;
            supervisors
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        }; // supervisors RwLock RELEASED here.

        // Step 2: Read each project's data without holding registry locks.
        let mut summaries = Vec::new();

        for (name, project) in &project_list {
            // Try to acquire task board lock with a short timeout to avoid blocking.
            let (
                open_tasks,
                total_tasks,
                pending_tasks,
                in_progress_tasks,
                done_tasks,
                cancelled_tasks,
                active_missions,
                total_missions,
            ) = if let Ok(board) = project.tasks.try_lock() {
                let all_tasks = board.all();
                let open = all_tasks.iter().filter(|t| !t.is_closed()).count() as u32;
                let total = all_tasks.len() as u32;
                let pending = all_tasks
                    .iter()
                    .filter(|t| t.status == sigil_tasks::task::TaskStatus::Pending)
                    .count() as u32;
                let in_progress = all_tasks
                    .iter()
                    .filter(|t| t.status == sigil_tasks::task::TaskStatus::InProgress)
                    .count() as u32;
                let done = all_tasks
                    .iter()
                    .filter(|t| t.status == sigil_tasks::task::TaskStatus::Done)
                    .count() as u32;
                let cancelled = all_tasks
                    .iter()
                    .filter(|t| t.status == sigil_tasks::task::TaskStatus::Cancelled)
                    .count() as u32;
                let missions = board.missions(Some(&project.prefix));
                let active_m = missions.iter().filter(|m| !m.is_closed()).count() as u32;
                let total_m = missions.len() as u32;
                (
                    open,
                    total,
                    pending,
                    in_progress,
                    done,
                    cancelled,
                    active_m,
                    total_m,
                )
            } else {
                // Lock held by patrol — return stale/zero data rather than blocking.
                (0, 0, 0, 0, 0, 0, 0, 0)
            };

            let team_info = supervisor_refs
                .iter()
                .find(|(k, _)| k == name)
                .and_then(|(_, sup)| {
                    sup.try_lock().ok().and_then(|guard| {
                        guard.team.as_ref().map(|t| TeamSummary {
                            leader: t.leader.clone(),
                            agents: t.effective_agents(),
                        })
                    })
                });

            summaries.push(ProjectSummary {
                name: name.clone(),
                prefix: project.prefix.clone(),
                team: team_info,
                open_tasks,
                total_tasks,
                pending_tasks,
                in_progress_tasks,
                done_tasks,
                cancelled_tasks,
                active_missions,
                total_missions,
                departments: project
                    .departments
                    .iter()
                    .map(|d| DepartmentSummary {
                        name: d.name.clone(),
                        lead: d.lead.clone(),
                        agents: d.agents.clone(),
                        description: d.description.clone(),
                    })
                    .collect(),
            });
        }

        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        summaries
    }
}

#[derive(Debug)]
pub struct RegistryStatus {
    pub projects: Vec<ProjectStatus>,
    pub unread_dispatches: usize,
}

#[derive(Debug)]
pub struct ProjectStatus {
    pub name: String,
    pub open_tasks: usize,
    pub ready_tasks: usize,
    pub workers_idle: usize,
    pub workers_working: usize,
    pub workers_bonded: usize,
    /// Project team leader agent name (if per-project team is set).
    pub team_leader: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub name: String,
    pub prefix: String,
    pub team: Option<TeamSummary>,
    pub open_tasks: u32,
    pub total_tasks: u32,
    pub pending_tasks: u32,
    pub in_progress_tasks: u32,
    pub done_tasks: u32,
    pub cancelled_tasks: u32,
    pub active_missions: u32,
    pub total_missions: u32,
    pub departments: Vec<DepartmentSummary>,
}

#[derive(Debug, Clone)]
pub struct DepartmentSummary {
    pub name: String,
    pub lead: Option<String>,
    pub agents: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TeamSummary {
    pub leader: String,
    pub agents: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Dispatch, DispatchKind};
    use anyhow::Result;
    use async_trait::async_trait;
    use sigil_core::ExecutionMode;
    use sigil_core::config::ProjectConfig;
    use sigil_core::traits::{ChatRequest, ChatResponse, Provider, StopReason, Usage};
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::time::{Duration, sleep};

    struct DoneProvider;

    #[async_trait]
    impl Provider for DoneProvider {
        async fn chat(&self, _request: &ChatRequest) -> Result<ChatResponse> {
            Ok(ChatResponse {
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

    fn temp_project(name: &str, prefix: &str) -> Result<(Arc<Project>, TempDir)> {
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
            domain_hints: Vec::new(),
            compact_instructions: None,
        };
        let project = Project::from_config(&config, dir.path(), "test-model")?;
        Ok((Arc::new(project), dir))
    }

    #[tokio::test]
    async fn patrol_all_closes_operations_from_taskdone_dispatches() {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let mut registry = ProjectRegistry::new(dispatch_bus.clone(), "leader".to_string());

        let op_dir = TempDir::new().unwrap();
        let operation_store = Arc::new(Mutex::new(
            OperationStore::open(Path::new(&op_dir.path().join("operations.json"))).unwrap(),
        ));
        registry.set_operation_store(operation_store.clone());

        let (project, _project_dir) = temp_project("demo", "dm").unwrap();
        let provider: Arc<dyn Provider> = Arc::new(DoneProvider);
        let mut supervisor =
            Supervisor::new(&project, provider, Vec::new(), dispatch_bus.clone());
        supervisor.execution_mode = sigil_core::ExecutionMode::Agent;
        registry.register_project(project.clone(), supervisor).await;

        let task = registry.assign("demo", "close the loop", "").await.unwrap();
        let operation_id = {
            let mut store = operation_store.lock().await;
            store
                .create("demo-op", vec![(task.id.clone(), "demo".to_string())])
                .unwrap()
                .id
                .clone()
        };

        let mut completed = false;
        for _ in 0..20 {
            registry.patrol_all().await.unwrap();
            {
                let store = operation_store.lock().await;
                if let Some(op) = store.get(&operation_id)
                    && op.closed_at.is_some()
                {
                    completed = true;
                    break;
                }
            }
            sleep(Duration::from_millis(20)).await;
        }

        assert!(
            completed,
            "operation should close after TaskDone dispatch is processed"
        );
    }

    #[tokio::test]
    async fn patrol_all_updates_operations_from_leader_inbox_dispatches() {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let mut registry = ProjectRegistry::new(dispatch_bus.clone(), "leader".to_string());

        let op_dir = TempDir::new().unwrap();
        let operation_store = Arc::new(Mutex::new(
            OperationStore::open(Path::new(&op_dir.path().join("operations.json"))).unwrap(),
        ));
        registry.set_operation_store(operation_store.clone());

        let (project, _project_dir) = temp_project("demo", "dm").unwrap();
        registry.register_project_only(project.clone()).await;

        let task = registry.assign("demo", "manual close", "").await.unwrap();
        let operation_id = {
            let mut store = operation_store.lock().await;
            store
                .create("manual-op", vec![(task.id.clone(), "demo".to_string())])
                .unwrap()
                .id
                .clone()
        };

        dispatch_bus
            .send(Dispatch::new_typed(
                "supervisor-demo",
                "leader",
                DispatchKind::TaskDone {
                    task_id: task.id.to_string(),
                    summary: "done".to_string(),
                },
            ))
            .await;

        registry.patrol_all().await.unwrap();

        let store = operation_store.lock().await;
        let op = store.get(&operation_id).unwrap();
        assert!(op.closed_at.is_some());
    }

    #[tokio::test]
    async fn patrol_all_acknowledges_processed_taskdone_dispatches() {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let mut registry = ProjectRegistry::new(dispatch_bus.clone(), "leader".to_string());

        let op_dir = TempDir::new().unwrap();
        let operation_store = Arc::new(Mutex::new(
            OperationStore::open(Path::new(&op_dir.path().join("operations.json"))).unwrap(),
        ));
        registry.set_operation_store(operation_store.clone());

        let (project, _project_dir) = temp_project("demo", "dm").unwrap();
        registry.register_project_only(project.clone()).await;
        let task = registry.assign("demo", "acked close", "").await.unwrap();
        {
            let mut store = operation_store.lock().await;
            store
                .create("acked-op", vec![(task.id.clone(), "demo".to_string())])
                .unwrap();
        }

        dispatch_bus
            .send(Dispatch::new_typed(
                "supervisor-demo",
                "leader",
                DispatchKind::TaskDone {
                    task_id: task.id.to_string(),
                    summary: "done".to_string(),
                },
            ))
            .await;

        registry.patrol_all().await.unwrap();

        let retries = dispatch_bus.retry_unacked(0).await;
        assert!(
            retries.is_empty(),
            "processed TaskDone dispatch should be acknowledged"
        );
    }
}
