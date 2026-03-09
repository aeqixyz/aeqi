use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use crate::audit::AuditLog;
use crate::blackboard::Blackboard;
use crate::cost_ledger::CostLedger;
use crate::expertise::ExpertiseLedger;
use crate::metrics::SigilMetrics;
use crate::operation::OperationStore;
use crate::message::DispatchBus;
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
        self.supervisors.write().await.insert(name, Arc::new(Mutex::new(supervisor)));
    }

    pub async fn assign(&self, project_name: &str, subject: &str, description: &str) -> Result<sigil_tasks::Task> {
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

    pub async fn patrol_all(&self) -> Result<()> {
        let whispers = self.dispatch_bus.read(&self.leader_agent_name).await;
        for w in &whispers {
            match &w.kind {
                crate::message::DispatchKind::PatrolReport { project, active, pending } => {
                    info!(from = %w.from, project = %project, active = active, pending = pending, "supervisor report");
                }
                crate::message::DispatchKind::WorkerCrashed { project, worker, error } => {
                    warn!(from = %w.from, project = %project, worker = %worker, error = %error, "worker crashed");
                }
                _ => {
                    info!(from = %w.from, kind = %w.kind.subject_tag(), "dispatch received");
                }
            }
        }

        // Parallel patrol: collect Arc clones, drop read lock, then join_all.
        let supervisor_entries: Vec<(String, Arc<Mutex<Supervisor>>)> = {
            let supervisors = self.supervisors.read().await;
            supervisors.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
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
                }
            }
        }

        // Track completed tasks in operation store.
        if let Some(ref operation_store) = self.operation_store {
            for w in &whispers {
                if let crate::message::DispatchKind::TaskDone { task_id, .. } = &w.kind {
                    let qid = sigil_tasks::TaskId(task_id.clone());
                    let mut store = operation_store.lock().await;
                    match store.mark_task_closed(&qid) {
                        Ok(completed_ops) => {
                            for op_id in completed_ops {
                                info!(operation = %op_id, "operation completed");
                            }
                        }
                        Err(e) => {
                            warn!(task = %task_id, error = %e, "failed to update operation store");
                        }
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

        let unread = self.dispatch_bus.unread_count(&self.leader_agent_name).await;

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
        self.projects.read().await.values().map(|d| d.max_workers).sum()
    }

    pub async fn projects_info(&self) -> Vec<serde_json::Value> {
        self.projects.read().await.values().map(|d| {
            serde_json::json!({
                "name": d.name,
                "prefix": d.prefix,
                "model": d.model,
                "max_workers": d.max_workers,
            })
        }).collect()
    }

    /// Get a supervisor by project name (for config reload).
    pub async fn get_supervisor(&self, project: &str) -> Option<Arc<Mutex<Supervisor>>> {
        self.supervisors.read().await.get(project).cloned()
    }

    /// Get a project's TaskBoard for direct task/mission access.
    pub async fn get_task_board(&self, project_name: &str) -> Option<std::sync::Arc<tokio::sync::Mutex<sigil_tasks::TaskBoard>>> {
        self.projects.read().await.get(project_name).map(|p| p.tasks.clone())
    }

    /// List all projects with summary stats (task counts, mission counts, team info).
    pub async fn list_project_summaries(&self) -> Vec<ProjectSummary> {
        let projects = self.projects.read().await;
        let supervisors = self.supervisors.read().await;
        let mut summaries = Vec::new();

        for (name, project) in projects.iter() {
            let board = project.tasks.lock().await;
            let all_tasks = board.all();
            let open_tasks = all_tasks.iter().filter(|t| !t.is_closed()).count() as u32;
            let total_tasks = all_tasks.len() as u32;

            let all_missions = board.missions(Some(&project.prefix));
            let active_missions = all_missions.iter().filter(|m| !m.is_closed()).count() as u32;
            let total_missions = all_missions.len() as u32;

            let team_info = if let Some(s) = supervisors.get(name) {
                let guard = s.lock().await;
                guard.team.as_ref().map(|t| TeamSummary {
                    leader: t.leader.clone(),
                    agents: t.effective_agents(),
                })
            } else {
                None
            };

            summaries.push(ProjectSummary {
                name: name.clone(),
                prefix: project.prefix.clone(),
                team: team_info,
                open_tasks,
                total_tasks,
                active_missions,
                total_missions,
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
    pub active_missions: u32,
    pub total_missions: u32,
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
    use sigil_core::config::ProjectConfig;
    use sigil_core::traits::{ChatRequest, ChatResponse, Provider, StopReason, Usage};
    use sigil_core::ExecutionMode;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

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
            max_workers: 1,
            worktree_root: None,
            execution_mode: ExecutionMode::Agent,
            max_turns: Some(1),
            max_budget_usd: None,
            worker_timeout_secs: 60,
            max_cost_per_day_usd: None,
            team: None,
            orchestrator: None,
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
        let supervisor = Supervisor::new(&project, provider, Vec::new(), dispatch_bus.clone());
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

        assert!(completed, "operation should close after TaskDone dispatch is processed");
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
}
