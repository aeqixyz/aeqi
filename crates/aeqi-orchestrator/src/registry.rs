use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::audit::AuditLog;
use crate::blackboard::Blackboard;
use crate::conversation_store::ConversationStore;
use crate::cost_ledger::CostLedger;
use crate::expertise::ExpertiseLedger;
use crate::message::DispatchBus;
use crate::metrics::AEQIMetrics;
use crate::operation::OperationStore;
use crate::project::Project;
use crate::worker_pool::WorkerPool;

pub struct ProjectRegistry {
    projects: RwLock<HashMap<String, Arc<Project>>>,
    worker_pools: RwLock<HashMap<String, Arc<Mutex<WorkerPool>>>>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub wake: Arc<tokio::sync::Notify>,
    pub cost_ledger: Arc<CostLedger>,
    pub metrics: Arc<AEQIMetrics>,
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
    /// Agent registry for global per-agent concurrency enforcement.
    agent_registry: RwLock<Option<Arc<crate::agent_registry::AgentRegistry>>>,
}

impl ProjectRegistry {
    pub fn new(dispatch_bus: Arc<DispatchBus>, leader_agent_name: String) -> Self {
        Self {
            projects: RwLock::new(HashMap::new()),
            worker_pools: RwLock::new(HashMap::new()),
            dispatch_bus,
            wake: Arc::new(tokio::sync::Notify::new()),
            cost_ledger: Arc::new(CostLedger::new(50.0)),
            metrics: Arc::new(AEQIMetrics::new()),
            leader_agent_name,
            operation_store: None,
            audit_log: None,
            expertise_ledger: None,
            blackboard: None,
            conversation_store: None,
            config_project_names: Vec::new(),
            agent_registry: RwLock::new(None),
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

    /// Register a project without creating a WorkerPool.
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

    pub async fn register_project(&self, project: Arc<Project>, mut pool: WorkerPool) {
        let name = project.name.clone();
        // Inject cost ledger + metrics + v3 components into the worker pool.
        pool.cost_ledger = Some(self.cost_ledger.clone());
        pool.metrics = Some(self.metrics.clone());
        pool.audit_log = self.audit_log.clone();
        pool.expertise_ledger = self.expertise_ledger.clone();
        pool.blackboard = self.blackboard.clone();
        self.metrics.ensure_project(&name);
        self.projects.write().await.insert(name.clone(), project);
        self.worker_pools
            .write()
            .await
            .insert(name, Arc::new(Mutex::new(pool)));
    }

    /// Wire agent registry, trigger store, and conversation store into all worker pools.
    pub async fn wire_agent_system(
        &self,
        agent_registry: Arc<crate::agent_registry::AgentRegistry>,
        trigger_store: Arc<crate::trigger::TriggerStore>,
        conversation_store: Option<Arc<crate::ConversationStore>>,
    ) {
        // Store registry reference for global concurrency enforcement.
        *self.agent_registry.write().await = Some(agent_registry.clone());

        let sups = self.worker_pools.write().await;
        for (_name, sup) in sups.iter() {
            let mut s = sup.lock().await;
            s.agent_registry = Some(agent_registry.clone());
            s.trigger_store = Some(trigger_store.clone());
            if let Some(ref cs) = conversation_store {
                s.conversation_store = Some(cs.clone());
            }
        }
    }

    /// Create an unbound task (no agent_id). Callers should prefer
    /// `assign_with_agent` with an explicit agent ID when available.
    pub async fn assign(
        &self,
        project_name: &str,
        subject: &str,
        description: &str,
    ) -> Result<aeqi_tasks::Task> {
        self.assign_with_agent(project_name, subject, description, None)
            .await
    }

    pub async fn assign_with_agent(
        &self,
        project_name: &str,
        subject: &str,
        description: &str,
        agent_id: Option<&str>,
    ) -> Result<aeqi_tasks::Task> {
        let projects = self.projects.read().await;
        let project = projects
            .get(project_name)
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;

        let mut task = project.create_task(subject, agent_id).await?;

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
    ) -> Result<aeqi_tasks::Task> {
        self.assign_with_skill_and_agent(project_name, subject, description, skill, None)
            .await
    }

    /// Assign a task with a specific skill and optional agent binding.
    pub async fn assign_with_skill_and_agent(
        &self,
        project_name: &str,
        subject: &str,
        description: &str,
        skill: &str,
        agent_id: Option<&str>,
    ) -> Result<aeqi_tasks::Task> {
        self.assign_with_skill_agent_labels(
            project_name,
            subject,
            description,
            skill,
            agent_id,
            &[],
        )
        .await
    }

    pub async fn assign_with_skill_agent_labels(
        &self,
        project_name: &str,
        subject: &str,
        description: &str,
        skill: &str,
        agent_id: Option<&str>,
        labels: &[String],
    ) -> Result<aeqi_tasks::Task> {
        let projects = self.projects.read().await;
        let project = projects
            .get(project_name)
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;

        let mut task = project.create_task(subject, agent_id).await?;

        {
            let mut store = project.tasks.lock().await;
            task = store.update(&task.id.0, |q| {
                if !description.is_empty() {
                    q.description = description.to_string();
                }
                q.skill = Some(skill.to_string());
                for label in labels {
                    q.labels.push(label.clone());
                }
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

    pub async fn patrol_all(&self) -> Result<()> {
        let whispers = self.dispatch_bus.read(&self.leader_agent_name).await;
        for w in &whispers {
            info!(from = %w.from, kind = %w.kind.subject_tag(), "dispatch received");
            if w.requires_ack {
                self.dispatch_bus.acknowledge(&w.id).await;
            }
        }

        let pool_entries: Vec<(String, Arc<Mutex<WorkerPool>>)> = {
            let pools = self.worker_pools.read().await;
            pools.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        // Phase 1: Reap completed workers + reload tasks (parallel per pool).
        let reap_futures: Vec<_> = pool_entries
            .iter()
            .map(|(name, pool)| {
                let name = name.clone();
                let pool = pool.clone();
                async move {
                    let mut p = pool.lock().await;
                    if let Err(e) = p.reap_and_reload().await {
                        warn!(project = %name, error = %e, "reap_and_reload failed");
                    }
                }
            })
            .collect();
        futures::future::join_all(reap_futures).await;

        // Phase 2: Collect all ready tasks + running agent counts across all projects.
        let mut all_ready: Vec<(String, aeqi_tasks::Task, String)> = Vec::new();
        let mut global_agent_counts: HashMap<String, u32> = HashMap::new();

        for (name, pool) in &pool_entries {
            let p = pool.lock().await;
            // Accumulate running counts globally.
            for (agent, count) in p.running_agent_counts() {
                *global_agent_counts.entry(agent).or_default() += count;
            }
            // Collect ready tasks.
            let ready = p.ready_for_spawn().await;
            for (task, agent) in ready {
                all_ready.push((name.clone(), task, agent));
            }
        }

        // Phase 3: Spawn with global per-agent limits.
        let agent_reg = self.agent_registry.read().await;
        for (project, task, agent_name) in &all_ready {
            // Per-agent concurrency check (global across projects).
            let max_concurrent = if let Some(ref ar) = *agent_reg {
                ar.get_max_concurrent_by_name(agent_name).await.unwrap_or(1)
            } else {
                1
            };
            let current = global_agent_counts.get(agent_name).copied().unwrap_or(0);
            if current >= max_concurrent {
                debug!(
                    agent = %agent_name,
                    running = current,
                    max = max_concurrent,
                    "agent at global max concurrency, deferring"
                );
                continue;
            }

            // Per-project worker limit.
            let pool = match pool_entries.iter().find(|(n, _)| n == project) {
                Some((_, p)) => p,
                None => continue,
            };
            {
                let p = pool.lock().await;
                if p.active_worker_count() >= p.max_workers as usize {
                    continue;
                }
            }

            // Spawn via the project's pool.
            {
                let mut p = pool.lock().await;
                p.spawn_worker(task, agent_name).await;
            }
            *global_agent_counts.entry(agent_name.clone()).or_default() += 1;
        }
        drop(agent_reg);

        // Phase 4: Per-pool reporting + metrics.
        for (name, pool) in &pool_entries {
            let mut p = pool.lock().await;
            if let Err(e) = p.patrol_report().await {
                warn!(project = %name, error = %e, "patrol report failed");
            }
        }

        Ok(())
    }

    pub async fn status(&self) -> RegistryStatus {
        let mut project_statuses = Vec::new();
        let projects = self.projects.read().await;
        let pools = self.worker_pools.read().await;

        for (name, project) in projects.iter() {
            let open = project.open_tasks().await.len();
            let ready = project.ready_tasks().await.len();
            let (idle, working, bonded) = if let Some(s) = pools.get(name) {
                s.lock().await.worker_counts()
            } else {
                (0, 0, 0)
            };

            // Get escalation target from the worker pool.
            let team_leader = if let Some(s) = pools.get(name) {
                let guard = s.lock().await;
                Some(guard.escalation_target.clone())
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

    pub async fn all_ready(&self) -> Vec<(String, aeqi_tasks::Task)> {
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
        let pools = self.worker_pools.read().await;
        let mut all = Vec::new();
        for (name, sup) in pools.iter() {
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

    /// Get a worker pool by project name (for config reload).
    pub async fn get_worker_pool(&self, project: &str) -> Option<Arc<Mutex<WorkerPool>>> {
        self.worker_pools.read().await.get(project).cloned()
    }

    /// Get a project's TaskBoard for direct task access.
    pub async fn get_task_board(
        &self,
        project_name: &str,
    ) -> Option<std::sync::Arc<tokio::sync::Mutex<aeqi_tasks::TaskBoard>>> {
        self.projects
            .read()
            .await
            .get(project_name)
            .map(|p| p.tasks.clone())
    }

    /// List all projects with summary stats (task counts, team info).
    /// Designed to minimize lock hold times — snapshot project list first, then read each
    /// project's task board independently without holding the registry-level RwLocks.
    pub async fn list_project_summaries(&self) -> Vec<ProjectSummary> {
        // Step 1: Snapshot project list + worker pool refs, then release RwLocks immediately.
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
            ) = if let Ok(board) = project.tasks.try_lock() {
                let all_tasks = board.all();
                let open = all_tasks.iter().filter(|t| !t.is_closed()).count() as u32;
                let total = all_tasks.len() as u32;
                let pending = all_tasks
                    .iter()
                    .filter(|t| t.status == aeqi_tasks::task::TaskStatus::Pending)
                    .count() as u32;
                let in_progress = all_tasks
                    .iter()
                    .filter(|t| t.status == aeqi_tasks::task::TaskStatus::InProgress)
                    .count() as u32;
                let done = all_tasks
                    .iter()
                    .filter(|t| t.status == aeqi_tasks::task::TaskStatus::Done)
                    .count() as u32;
                let cancelled = all_tasks
                    .iter()
                    .filter(|t| t.status == aeqi_tasks::task::TaskStatus::Cancelled)
                    .count() as u32;
                (
                    open,
                    total,
                    pending,
                    in_progress,
                    done,
                    cancelled,
                )
            } else {
                // Lock held by patrol — return stale/zero data rather than blocking.
                (0, 0, 0, 0, 0, 0)
            };

            summaries.push(ProjectSummary {
                name: name.clone(),
                prefix: project.prefix.clone(),
                open_tasks,
                total_tasks,
                pending_tasks,
                in_progress_tasks,
                done_tasks,
                cancelled_tasks,
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
    pub open_tasks: u32,
    pub total_tasks: u32,
    pub pending_tasks: u32,
    pub in_progress_tasks: u32,
    pub done_tasks: u32,
    pub cancelled_tasks: u32,
    pub departments: Vec<DepartmentSummary>,
}

#[derive(Debug, Clone)]
pub struct DepartmentSummary {
    pub name: String,
    pub lead: Option<String>,
    pub agents: Vec<String>,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Dispatch, DispatchKind};

    #[tokio::test]
    async fn patrol_all_consumes_leader_dispatches() {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let registry = ProjectRegistry::new(dispatch_bus.clone(), "leader".to_string());

        dispatch_bus
            .send(Dispatch::new_typed(
                "pool-demo",
                "leader",
                DispatchKind::DelegateResponse {
                    reply_to: "t1".to_string(),
                    response_mode: "origin".to_string(),
                    content: "done".to_string(),
                },
            ))
            .await;

        registry.patrol_all().await.unwrap();

        // Dispatches are consumed by patrol_all.
        let remaining = dispatch_bus.read("leader").await;
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn patrol_all_acknowledges_processed_dispatches() {
        let dispatch_bus = Arc::new(DispatchBus::new());
        let registry = ProjectRegistry::new(dispatch_bus.clone(), "leader".to_string());

        dispatch_bus
            .send(
                Dispatch::new_typed(
                    "pool-demo",
                    "leader",
                    DispatchKind::DelegateRequest {
                        prompt: "do something".to_string(),
                        response_mode: "origin".to_string(),
                        create_task: false,
                        skill: None,
                        reply_to: None,
                    },
                )
                .with_ack_required(),
            )
            .await;

        registry.patrol_all().await.unwrap();

        let retries = dispatch_bus.retry_unacked(0).await;
        assert!(
            retries.is_empty(),
            "processed dispatch should be acknowledged"
        );
    }
}
