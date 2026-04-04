//! Global Scheduler — the single event-driven worker pool.
//!
//! One loop: **wake → reap → query → spawn**
//!
//! No patrol timers. No per-project pools. The daemon owns one Scheduler.
//! The Scheduler owns the running workers. Agent properties (workdir, model,
//! budget, concurrency) live on the agent tree in AgentRegistry.
//!
//! ```text
//! Scheduler
//! ├── wake signal ← task created / worker finished / config changed
//! ├── reap()     → clean finished workers, handle timeouts
//! ├── ready()    → query tasks WHERE status=pending AND deps met AND agent not maxed
//! └── spawn()    → tokio::spawn worker for each ready task
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use crate::agent_registry::AgentRegistry;
use crate::agent_worker::AgentWorker;
use crate::escalation::{EscalationPolicy, EscalationTracker};
use crate::event_store::EventStore;
use crate::execution_events::{EventBroadcaster, ExecutionEvent};
use crate::metrics::AEQIMetrics;
use crate::middleware::{
    ClarificationMiddleware, ContextBudgetMiddleware, ContextCompressionMiddleware,
    CostTrackingMiddleware, GraphGuardrailsMiddleware, GuardrailsMiddleware,
    LoopDetectionMiddleware, MemoryRefreshMiddleware, MiddlewareChain, SafetyNetMiddleware,
};
use crate::session_store::SessionStore;
use crate::trigger::TriggerStore;
use aeqi_core::traits::{Channel, Insight, Provider, Tool};

/// A running worker with age tracking for timeout detection.
struct TrackedWorker {
    handle: tokio::task::JoinHandle<()>,
    task_id: String,
    agent_id: String,
    agent_name: String,
    started_at: Instant,
    timeout_secs: u64,
}

/// Configuration for the scheduler.
pub struct SchedulerConfig {
    /// Global max concurrent workers.
    pub max_workers: u32,
    /// Default worker timeout (overridden by agent-level setting).
    pub default_timeout_secs: u64,
    /// Default per-worker budget.
    pub worker_max_budget_usd: f64,
    /// Global daily budget cap (replaces CostLedger daily budget).
    pub daily_budget_usd: f64,
    /// Directories to search for skill TOML files.
    pub skills_dirs: Vec<PathBuf>,
    /// Shared primer injected into ALL agents.
    pub shared_primer: Option<String>,
    /// Model for post-execution reflection.
    pub reflect_model: String,
    /// Enable adaptive retry with failure analysis.
    pub adaptive_retry: bool,
    /// Model for failure analysis.
    pub failure_analysis_model: String,
    /// Max task retries before auto-cancel.
    pub max_task_retries: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_workers: 4,
            default_timeout_secs: 3600,
            worker_max_budget_usd: 5.0,
            daily_budget_usd: 50.0,
            skills_dirs: Vec::new(),
            shared_primer: None,
            reflect_model: String::new(),
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            max_task_retries: 3,
        }
    }
}

/// The global scheduler — one pool, event-driven, no project scoping.
pub struct Scheduler {
    pub config: SchedulerConfig,

    // Core services
    pub agent_registry: Arc<AgentRegistry>,
    pub provider: Arc<dyn Provider>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub metrics: Arc<AEQIMetrics>,
    pub event_broadcaster: Arc<EventBroadcaster>,

    // Optional services
    pub insight_store: Option<Arc<dyn Insight>>,
    pub reflect_provider: Option<Arc<dyn Provider>>,
    pub event_store: Arc<EventStore>,
    pub session_store: Option<Arc<SessionStore>>,
    pub trigger_store: Option<Arc<TriggerStore>>,
    pub gate_channels: Vec<Arc<dyn Channel>>,

    // Runtime state
    running: Mutex<Vec<TrackedWorker>>,
    #[allow(dead_code)]
    escalation_tracker: Mutex<EscalationTracker>,

    /// Wake signal — coalesces multiple notifications into one schedule() call.
    pub wake: Arc<Notify>,
}

impl Scheduler {
    pub fn new(
        config: SchedulerConfig,
        agent_registry: Arc<AgentRegistry>,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        metrics: Arc<AEQIMetrics>,
        event_broadcaster: Arc<EventBroadcaster>,
        event_store: Arc<EventStore>,
    ) -> Self {
        Self {
            config,
            agent_registry,
            provider,
            tools,
            metrics,
            event_broadcaster,
            insight_store: None,
            reflect_provider: None,
            event_store,
            session_store: None,
            trigger_store: None,
            gate_channels: Vec::new(),
            running: Mutex::new(Vec::new()),
            escalation_tracker: Mutex::new(EscalationTracker::new(EscalationPolicy {
                max_retries: 4,
                cooldown_secs: 300,
                escalate_model: None,
            })),
            wake: Arc::new(Notify::new()),
        }
    }

    // -----------------------------------------------------------------------
    // Main loop
    // -----------------------------------------------------------------------

    /// Run the scheduler loop. Blocks until shutdown.
    /// Call `wake.notify_one()` to trigger immediate scheduling.
    pub async fn run(&self, shutdown: Arc<tokio::sync::Notify>) {
        info!(max_workers = self.config.max_workers, "scheduler started");
        loop {
            tokio::select! {
                _ = self.wake.notified() => {
                    if let Err(e) = self.schedule().await {
                        warn!(error = %e, "schedule cycle failed");
                    }
                }
                _ = shutdown.notified() => {
                    info!("scheduler shutting down");
                    self.shutdown().await;
                    return;
                }
            }
        }
    }

    /// One scheduling cycle: reap → query → spawn.
    pub async fn schedule(&self) -> Result<()> {
        let cycle_start = Instant::now();

        // Phase 1: Reap finished workers + handle timeouts.
        self.reap().await;

        // Phase 2: Get ready tasks.
        let ready = self.agent_registry.ready_tasks().await?;
        if ready.is_empty() {
            return Ok(());
        }

        // Phase 3: Build concurrency map (agent_id -> running count).
        let running = self.running.lock().await;
        let total_running = running.len();
        let mut agent_counts: HashMap<String, u32> = HashMap::new();
        for w in running.iter() {
            *agent_counts.entry(w.agent_id.clone()).or_default() += 1;
        }
        drop(running);

        // Phase 4: Spawn workers for tasks we can run.
        let mut spawned = 0u32;
        for task in &ready {
            // Global worker limit.
            if total_running as u32 + spawned >= self.config.max_workers {
                debug!(
                    running = total_running,
                    max = self.config.max_workers,
                    "global worker limit reached"
                );
                break;
            }

            let agent_id = match &task.agent_id {
                Some(id) => id.clone(),
                None => {
                    warn!(task = %task.id, "task has no agent_id, skipping");
                    continue;
                }
            };

            // Per-agent concurrency limit.
            let max_concurrent = self
                .agent_registry
                .get_max_concurrent(&agent_id)
                .await
                .unwrap_or(1);
            let current = agent_counts.get(&agent_id).copied().unwrap_or(0);
            if current >= max_concurrent {
                debug!(
                    agent = %agent_id,
                    running = current,
                    max = max_concurrent,
                    "agent at max concurrency"
                );
                continue;
            }

            // Budget check via EventStore.
            let daily_cost = self.event_store.daily_cost().await.unwrap_or(0.0);
            if daily_cost >= self.config.daily_budget_usd {
                debug!(
                    agent = %agent_id,
                    daily_cost,
                    budget = self.config.daily_budget_usd,
                    "global budget exhausted"
                );
                continue;
            }

            // Phase 5: Expertise routing — check if a sibling agent has a better track record.
            // Only reroute if the assigned agent has siblings and expertise data exists.
            if let Ok(expertise) = self.event_store.query_expertise().await
                && let Ok(Some(assigned)) = self.agent_registry.get(&agent_id).await
            {
                // Find sibling agents (same parent) that could handle this task.
                if let Some(ref parent_id) = assigned.parent_id
                    && let Ok(siblings) = self.agent_registry.get_children(parent_id).await
                {
                    let mut best_agent: Option<(String, f64)> = None;
                    for sibling in &siblings {
                        if sibling.id == agent_id {
                            continue;
                        }
                        // Check if sibling is under concurrency limit.
                        let sib_max = self
                            .agent_registry
                            .get_max_concurrent(&sibling.id)
                            .await
                            .unwrap_or(1);
                        let sib_current = agent_counts.get(&sibling.id).copied().unwrap_or(0);
                        if sib_current >= sib_max {
                            continue;
                        }
                        // Check expertise score.
                        if let Some(score) = expertise.iter().find(|s| {
                            s.get("agent").and_then(|a| a.as_str()) == Some(&sibling.name)
                        }) {
                            let rate = score
                                .get("success_rate")
                                .and_then(|r| r.as_f64())
                                .unwrap_or(0.0);
                            if best_agent
                                .as_ref()
                                .is_none_or(|(_, best_rate)| rate > *best_rate)
                            {
                                best_agent = Some((sibling.id.clone(), rate));
                            }
                        }
                    }
                    // Reassign if a sibling has a significantly better track record.
                    if let Some((better_id, better_rate)) = best_agent {
                        let own_rate = expertise
                            .iter()
                            .find(|s| {
                                s.get("agent").and_then(|a| a.as_str()) == Some(&assigned.name)
                            })
                            .and_then(|s| s.get("success_rate").and_then(|r| r.as_f64()))
                            .unwrap_or(0.0);
                        if better_rate > own_rate + 0.2 {
                            debug!(
                                task = %task.id,
                                from = %agent_id,
                                to = %better_id,
                                own_rate,
                                better_rate,
                                "expertise routing: reassigning to better agent"
                            );
                            // TODO: Update task.agent_id in AgentRegistry to the better agent.
                            // For now just log — full reassignment needs agent_registry.update_task().
                        }
                    }
                }
            }

            // Spawn.
            self.spawn_worker(task).await;
            *agent_counts.entry(agent_id).or_default() += 1;
            spawned += 1;
        }

        if spawned > 0 {
            info!(
                spawned,
                ready = ready.len(),
                elapsed_ms = cycle_start.elapsed().as_millis(),
                "schedule cycle"
            );
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Reap
    // -----------------------------------------------------------------------

    async fn reap(&self) {
        let mut running = self.running.lock().await;
        let mut timed_out = Vec::new();

        running.retain(|w| {
            if w.handle.is_finished() {
                return false;
            }
            if w.started_at.elapsed() > std::time::Duration::from_secs(w.timeout_secs) {
                w.handle.abort();
                timed_out.push((w.task_id.clone(), w.agent_name.clone(), w.timeout_secs));
                return false;
            }
            true
        });
        drop(running);

        // Handle timed-out workers.
        for (task_id, agent_name, timeout) in timed_out {
            warn!(task = %task_id, agent = %agent_name, timeout, "worker timed out");

            let _ = self
                .event_store
                .emit(
                    "decision",
                    None,
                    None,
                    Some(&task_id),
                    &serde_json::json!({
                        "decision_type": "WorkerTimedOut",
                        "reasoning": format!("Timed out after {timeout}s"),
                    }),
                )
                .await;

            // Reset task to pending.
            if let Err(e) = self
                .agent_registry
                .update_task_status(&task_id, aeqi_quests::QuestStatus::Pending)
                .await
            {
                warn!(task = %task_id, error = %e, "failed to reset timed-out task");
            }

            // Wake to re-schedule.
            self.wake.notify_one();
        }
    }

    // -----------------------------------------------------------------------
    // Spawn
    // -----------------------------------------------------------------------

    async fn spawn_worker(&self, task: &aeqi_quests::Quest) {
        let agent_id = match &task.agent_id {
            Some(id) => id.clone(),
            None => return,
        };

        let agent = match self.agent_registry.get(&agent_id).await {
            Ok(Some(a)) => a,
            Ok(None) => {
                warn!(agent_id = %agent_id, "agent not found for task");
                return;
            }
            Err(e) => {
                warn!(agent_id = %agent_id, error = %e, "failed to load agent");
                return;
            }
        };

        // Resolve inherited properties.
        let workdir = self
            .agent_registry
            .resolve_workdir(&agent_id)
            .await
            .ok()
            .flatten();
        let execution_mode = self
            .agent_registry
            .resolve_execution_mode(&agent_id)
            .await
            .unwrap_or_else(|_| "agent".to_string());
        let timeout = self
            .agent_registry
            .resolve_worker_timeout(&agent_id)
            .await
            .unwrap_or(self.config.default_timeout_secs);

        // Mark task as in-progress.
        if let Err(e) = self
            .agent_registry
            .update_task_status(&task.id.0, aeqi_quests::QuestStatus::InProgress)
            .await
        {
            warn!(task = %task.id, error = %e, "failed to mark task in-progress");
            return;
        }

        let worker_name = format!(
            "{}:{}:{}",
            agent.name,
            task.id,
            chrono::Utc::now().timestamp()
        );

        // Assemble prompts from ancestor chain + task.
        let task_prompts: Vec<aeqi_core::PromptEntry> = task
            .skill
            .as_ref()
            .and_then(|skill_name| {
                load_skill_prompt(skill_name, &self.config.skills_dirs)
                    .map(|prompt| vec![aeqi_core::PromptEntry::task_prepend(prompt)])
            })
            .unwrap_or_default();
        let assembled = crate::prompt_assembly::assemble_prompts(
            &self.agent_registry,
            &agent_id,
            &task_prompts,
        )
        .await;

        // Pass assembled prompt string directly to AgentWorker.
        let system_prompt = assembled.full_system_prompt();

        // Build the AgentWorker.
        let mut worker = match execution_mode.as_str() {
            "claude_code" => {
                let cwd = workdir
                    .clone()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                let budget = agent
                    .budget_usd
                    .unwrap_or(self.config.worker_max_budget_usd);
                AgentWorker::new_claude_code(
                    agent.name.clone(),
                    worker_name.clone(),
                    "global".to_string(),
                    cwd,
                    budget,
                    system_prompt.clone(),
                    self.event_store.clone(),
                )
            }
            _ => {
                let model = self
                    .agent_registry
                    .resolve_model(&agent_id, "anthropic/claude-sonnet-4-6")
                    .await;
                AgentWorker::new(
                    agent.name.clone(),
                    worker_name.clone(),
                    "global".to_string(),
                    self.provider.clone(),
                    self.tools.clone(),
                    system_prompt,
                    model,
                    self.event_store.clone(),
                )
            }
        };

        // Inject persistent agent identity.
        worker = worker.with_persistent_agent(agent_id.clone());

        // Inject insight store.
        if let Some(ref mem) = self.insight_store {
            worker = worker.with_insight_store(mem.clone());
        }

        // Inject reflection provider.
        if let Some(ref provider) = self.reflect_provider {
            worker = worker.with_reflect(provider.clone(), self.config.reflect_model.clone());
        }

        // Inject working directory.
        if let Some(ref wd) = workdir {
            worker = worker.with_project_dir(PathBuf::from(wd));
        }

        // Skill prompt is now assembled via assemble_prompts() above.

        // Inject tools for persistent agents.
        if let crate::agent_worker::WorkerExecution::Agent { ref mut tools, .. } = worker.execution
        {
            // Trigger management tool.
            if agent.capabilities.iter().any(|c| c == "manage_triggers")
                && let Some(ref ts) = self.trigger_store
            {
                tools.push(Arc::new(crate::tools::TriggerManageTool::new(
                    ts.clone(),
                    agent_id.clone(),
                )));
            }

            // Transcript search tool.
            if let Some(ref ss) = self.session_store {
                tools.push(Arc::new(crate::tools::TranscriptSearchTool::new(
                    ss.clone(),
                )));
            }
        }

        // Build middleware chain.
        let budget = agent
            .budget_usd
            .unwrap_or(self.config.worker_max_budget_usd);
        let chain = MiddlewareChain::new(vec![
            Box::new(LoopDetectionMiddleware::new()),
            Box::new(CostTrackingMiddleware::new(budget)),
            Box::new(ContextBudgetMiddleware::new(200)),
            Box::new(GraphGuardrailsMiddleware::new(
                &dirs::home_dir().unwrap_or_default().join(".aeqi"),
            )),
            Box::new(GuardrailsMiddleware::with_defaults()),
            Box::new(ContextCompressionMiddleware::new()),
            Box::new(MemoryRefreshMiddleware::new()),
            Box::new(ClarificationMiddleware::new()),
            Box::new(SafetyNetMiddleware::new()),
        ]);
        worker.set_middleware(chain);

        // Inject event broadcaster.
        worker.set_broadcaster(self.event_broadcaster.clone());

        // Inject session store.
        if let Some(ref ss) = self.session_store {
            worker.session_store = Some(ss.clone());
        }

        // Inject blackboard.
        worker = worker.with_max_task_retries(self.config.max_task_retries);

        // Build completion callback — updates AgentRegistry task status.
        let cb_registry = self.agent_registry.clone();
        let cb_task_id = task.id.0.clone();
        worker.on_complete = Some(Box::new(move |status, outcome| {
            let registry = cb_registry;
            let task_id = cb_task_id;
            tokio::spawn(async move {
                match status {
                    aeqi_quests::QuestStatus::Done
                    | aeqi_quests::QuestStatus::Blocked
                    | aeqi_quests::QuestStatus::Cancelled => {
                        let _ = registry.update_task_status(&task_id, status).await;
                    }
                    aeqi_quests::QuestStatus::Pending => {
                        let _ = registry
                            .update_task(&task_id, |t| {
                                t.status = aeqi_quests::QuestStatus::Pending;
                                t.retry_count += 1;
                                t.assignee = None;
                            })
                            .await;
                    }
                    _ => {
                        let _ = registry.update_task_status(&task_id, status).await;
                    }
                }
                if let Some(record) = outcome {
                    let _ = registry
                        .update_task(&task_id, |t| {
                            t.set_task_outcome(&record);
                        })
                        .await;
                }
            });
        }));

        // Assign the task to the worker.
        worker.assign(task);

        // Spawn the worker as a background task.
        let task_id = task.id.0.clone();
        let agent_name = agent.name.clone();
        let agent_id_clone = agent_id.clone();
        let registry = self.agent_registry.clone();
        let spawn_event_store = self.event_store.clone();
        let event_broadcaster = self.event_broadcaster.clone();
        let wake = self.wake.clone();

        let handle = tokio::spawn(async move {
            let result = worker.execute().await;

            // The on_complete callback already updated task status in AgentRegistry.
            // Here we handle cost recording, expertise, and event broadcasting.
            let (outcome_status, cost_usd, turns) = match result {
                Ok((_task_outcome, runtime_exec, cost, turns)) => {
                    // Record cost as an event in the unified EventStore.
                    let _ = spawn_event_store
                        .emit(
                            "cost",
                            Some(&agent_id_clone),
                            None,
                            Some(&task_id),
                            &serde_json::json!({
                                "project": "global",
                                "agent_name": agent_name,
                                "cost_usd": cost,
                                "turns": turns,
                            }),
                        )
                        .await;
                    let status = match runtime_exec.outcome.status {
                        crate::runtime::RuntimeOutcomeStatus::Done => "done",
                        crate::runtime::RuntimeOutcomeStatus::Blocked => "blocked",
                        crate::runtime::RuntimeOutcomeStatus::Handoff
                        | crate::runtime::RuntimeOutcomeStatus::Failed => "retry",
                    };
                    (status, cost, turns)
                }
                Err(e) => {
                    warn!(task = %task_id, error = %e, "worker execution failed");
                    ("error", 0.0, 0)
                }
            };

            // Record task completion in unified event store.
            let _ = spawn_event_store
                .emit(
                    "quest_completed",
                    Some(&agent_id_clone),
                    None,
                    Some(&task_id),
                    &serde_json::json!({
                        "agent_name": agent_name,
                        "outcome": outcome_status,
                        "cost_usd": cost_usd,
                        "turns": turns,
                    }),
                )
                .await;

            let _ = registry.record_session(&agent_id_clone, 0).await;

            event_broadcaster.publish(ExecutionEvent::QuestCompleted {
                task_id: task_id.clone(),
                outcome: outcome_status.to_string(),
                confidence: 0.0,
                cost_usd,
                turns,
                duration_ms: 0,
                runtime: None,
            });

            info!(
                task = %task_id,
                agent = %agent_name,
                outcome = %outcome_status,
                cost_usd,
                "worker completed"
            );

            wake.notify_one();
        });

        // Track the running worker.
        self.running.lock().await.push(TrackedWorker {
            handle,
            task_id: task.id.0.clone(),
            agent_id,
            agent_name: agent.name.clone(),
            started_at: Instant::now(),
            timeout_secs: timeout,
        });

        info!(
            task = %task.id,
            agent = %agent.name,
            timeout,
            "worker spawned"
        );
    }

    // -----------------------------------------------------------------------
    // Status & queries
    // -----------------------------------------------------------------------

    /// Number of currently running workers.
    pub async fn active_count(&self) -> usize {
        self.running.lock().await.len()
    }

    /// Running worker counts per agent.
    pub async fn agent_counts(&self) -> HashMap<String, u32> {
        let running = self.running.lock().await;
        let mut counts = HashMap::new();
        for w in running.iter() {
            *counts.entry(w.agent_name.clone()).or_default() += 1;
        }
        counts
    }

    /// Get status of all running workers.
    pub async fn worker_status(&self) -> Vec<serde_json::Value> {
        let running = self.running.lock().await;
        running
            .iter()
            .map(|w| {
                serde_json::json!({
                    "task_id": w.task_id,
                    "agent_id": w.agent_id,
                    "agent_name": w.agent_name,
                    "running_secs": w.started_at.elapsed().as_secs(),
                    "timeout_secs": w.timeout_secs,
                })
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Shutdown
    // -----------------------------------------------------------------------

    async fn shutdown(&self) {
        let mut running = self.running.lock().await;
        info!(workers = running.len(), "aborting running workers");
        for w in running.drain(..) {
            w.handle.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a skill prompt from skills directories.
fn load_skill_prompt(skill_name: &str, skills_dirs: &[PathBuf]) -> Option<String> {
    for dir in skills_dirs {
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

            // Extract tool restrictions.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scheduler_config_defaults() {
        let config = SchedulerConfig::default();
        assert_eq!(config.max_workers, 4);
        assert_eq!(config.default_timeout_secs, 3600);
        assert_eq!(config.worker_max_budget_usd, 5.0);
        assert!((config.daily_budget_usd - 50.0).abs() < 0.01);
    }
}
