use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use crate::cost_ledger::CostLedger;
use crate::metrics::SystemMetrics;
use crate::operation::OperationStore;
use crate::message::DispatchBus;
use crate::project::Project;
use crate::supervisor::Supervisor;

pub struct ProjectRegistry {
    projects: RwLock<HashMap<String, Arc<Project>>>,
    scouts: RwLock<HashMap<String, Arc<Mutex<Supervisor>>>>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub wake: Arc<tokio::sync::Notify>,
    pub cost_ledger: Arc<CostLedger>,
    pub metrics: Arc<SystemMetrics>,
    /// Name of the leader agent for whisper routing.
    pub leader_agent_name: String,
    /// Optional raid store for cross-project quest tracking.
    pub raid_store: Option<Arc<Mutex<OperationStore>>>,
}

impl ProjectRegistry {
    pub fn new(dispatch_bus: Arc<DispatchBus>, leader_agent_name: String) -> Self {
        Self {
            projects: RwLock::new(HashMap::new()),
            scouts: RwLock::new(HashMap::new()),
            dispatch_bus,
            wake: Arc::new(tokio::sync::Notify::new()),
            cost_ledger: Arc::new(CostLedger::new(50.0)),
            metrics: Arc::new(SystemMetrics::new()),
            leader_agent_name,
            raid_store: None,
        }
    }

    /// Set a custom cost ledger (e.g., with persistence).
    pub fn set_cost_ledger(&mut self, ledger: Arc<CostLedger>) {
        self.cost_ledger = ledger;
    }

    /// Set the raid store for cross-project quest tracking.
    pub fn set_raid_store(&mut self, store: Arc<Mutex<OperationStore>>) {
        self.raid_store = Some(store);
    }

    pub async fn register_project(&self, project: Arc<Project>, mut scout: Supervisor) {
        let name = project.name.clone();
        // Inject cost ledger + metrics into the scout.
        scout.cost_ledger = Some(self.cost_ledger.clone());
        scout.metrics = Some(self.metrics.clone());
        self.metrics.ensure_project(&name);
        self.projects.write().await.insert(name.clone(), project);
        self.scouts.write().await.insert(name, Arc::new(Mutex::new(scout)));
    }

    pub async fn assign(&self, project_name: &str, subject: &str, description: &str) -> Result<system_tasks::Task> {
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
                    info!(from = %w.from, project = %project, active = active, pending = pending, "scout report");
                }
                crate::message::DispatchKind::WorkerCrashed { project, worker, error } => {
                    warn!(from = %w.from, project = %project, worker = %worker, error = %error, "worker crashed");
                }
                _ => {
                    info!(from = %w.from, kind = %w.kind.subject_tag(), "whisper received");
                }
            }
        }

        // Parallel patrol: collect Arc clones, drop read lock, then join_all.
        let scout_entries: Vec<(String, Arc<Mutex<Supervisor>>)> = {
            let scouts = self.scouts.read().await;
            scouts.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let futures: Vec<_> = scout_entries
            .iter()
            .map(|(name, scout)| {
                let name = name.clone();
                let scout = scout.clone();
                async move {
                    let mut s = scout.lock().await;
                    if let Err(e) = s.patrol().await {
                        warn!(project = %name, error = %e, "scout patrol failed");
                    }
                }
            })
            .collect();

        futures::future::join_all(futures).await;

        // Dispatch Resolution whispers to the appropriate scouts.
        // Focal agent sends Resolution whispers addressed to "scout-{project}".
        for (project_name, scout) in &scout_entries {
            let scout_recipient = format!("scout-{}", project_name);
            let whispers = self.dispatch_bus.read(&scout_recipient).await;
            for w in whispers {
                if let crate::message::DispatchKind::Resolution { task_id, answer } = &w.kind {
                    info!(project = %project_name, task = %task_id, "dispatching resolution to scout");
                    let s = scout.lock().await;
                    s.handle_resolution(task_id, answer).await;
                }
            }
        }

        // Track completed quests in raid store.
        if let Some(ref raid_store) = self.raid_store {
            for w in &whispers {
                if let crate::message::DispatchKind::QuestDone { task_id, .. } = &w.kind {
                    let qid = system_tasks::TaskId(task_id.clone());
                    let mut store = raid_store.lock().await;
                    match store.mark_bead_closed(&qid) {
                        Ok(completed_raids) => {
                            for raid_id in completed_raids {
                                info!(raid = %raid_id, "raid completed");
                            }
                        }
                        Err(e) => {
                            warn!(task = %task_id, error = %e, "failed to update raid store");
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
        let scouts = self.scouts.read().await;

        for (name, project) in projects.iter() {
            let open = project.open_tasks().await.len();
            let ready = project.ready_tasks().await.len();
            let (idle, working, bonded) = if let Some(s) = scouts.get(name) {
                s.lock().await.worker_counts()
            } else {
                (0, 0, 0)
            };

            // Get team leader from the supervisor.
            let team_leader = if let Some(s) = scouts.get(name) {
                let guard = s.lock().await;
                guard.team.as_ref().map(|t| t.leader.clone())
            } else {
                None
            };

            project_statuses.push(ProjectStatus {
                name: name.clone(),
                open_quests: open,
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
            unread_whispers: unread,
        }
    }

    pub async fn all_ready(&self) -> Vec<(String, system_tasks::Task)> {
        let mut all = Vec::new();
        let projects = self.projects.read().await;
        for (name, project) in projects.iter() {
            for quest in project.ready_tasks().await {
                all.push((name.clone(), quest));
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

    pub async fn total_max_spirits(&self) -> u32 {
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

    /// Get a project's TaskBoard for direct task/mission access.
    pub async fn get_task_board(&self, project_name: &str) -> Option<std::sync::Arc<tokio::sync::Mutex<system_tasks::TaskBoard>>> {
        self.projects.read().await.get(project_name).map(|p| p.tasks.clone())
    }

    /// List all projects with summary stats (task counts, mission counts, team info).
    pub async fn list_project_summaries(&self) -> Vec<ProjectSummary> {
        let projects = self.projects.read().await;
        let scouts = self.scouts.read().await;
        let mut summaries = Vec::new();

        for (name, project) in projects.iter() {
            let board = project.tasks.lock().await;
            let all_tasks = board.all();
            let open_tasks = all_tasks.iter().filter(|t| !t.is_closed()).count() as u32;
            let total_tasks = all_tasks.len() as u32;

            let all_missions = board.missions(Some(&project.prefix));
            let active_missions = all_missions.iter().filter(|m| !m.is_closed()).count() as u32;
            let total_missions = all_missions.len() as u32;

            let team_info = if let Some(s) = scouts.get(name) {
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
    pub unread_whispers: usize,
}

#[derive(Debug)]
pub struct ProjectStatus {
    pub name: String,
    pub open_quests: usize,
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
