use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::agent_registry::AgentRegistry;
use crate::chat_engine::{ChatEngine, ChatMessage, ChatSource};
use crate::conversation_store::{
    agency_chat_id, department_chat_id, named_channel_chat_id, project_chat_id,
};
use crate::execution_events::{EventBroadcaster, ExecutionEvent};
use crate::message::{Dispatch, DispatchBus, DispatchHealth};
use crate::registry::ProjectRegistry;
use crate::session_tracker::SessionTracker;
use crate::trigger::TriggerStore;

const ACK_RETRY_AGE_SECS: u64 = 60;
const MAX_EVENT_BUFFER_LEN: usize = 512;

#[derive(Debug, Clone, Default)]
struct ReadinessContext {
    configured_projects: usize,
    configured_advisors: usize,
    skipped_projects: Vec<String>,
    skipped_advisors: Vec<String>,
}

#[derive(Debug, Clone)]
struct BufferedExecutionEvent {
    cursor: u64,
    event: ExecutionEvent,
}

#[derive(Debug, Clone)]
struct EventReadResult {
    events: Vec<ExecutionEvent>,
    next_cursor: u64,
    oldest_cursor: u64,
    reset: bool,
}

#[derive(Debug, Default)]
struct EventBuffer {
    next_cursor: u64,
    events: Vec<BufferedExecutionEvent>,
}

impl EventBuffer {
    fn push(&mut self, event: ExecutionEvent) {
        let cursor = self.next_cursor;
        self.next_cursor = self.next_cursor.saturating_add(1);
        self.events.push(BufferedExecutionEvent { cursor, event });

        let overflow = self.events.len().saturating_sub(MAX_EVENT_BUFFER_LEN);
        if overflow > 0 {
            self.events.drain(..overflow);
        }
    }

    fn read_since(&self, cursor: Option<u64>) -> EventReadResult {
        let oldest_cursor = self
            .events
            .first()
            .map(|event| event.cursor)
            .unwrap_or(self.next_cursor);
        let requested_cursor = cursor.unwrap_or(oldest_cursor);
        let reset = requested_cursor < oldest_cursor;
        let effective_cursor = if reset {
            oldest_cursor
        } else {
            requested_cursor.min(self.next_cursor)
        };

        let events = self
            .events
            .iter()
            .filter(|event| event.cursor >= effective_cursor)
            .map(|event| event.event.clone())
            .collect();

        EventReadResult {
            events,
            next_cursor: self.next_cursor,
            oldest_cursor,
            reset,
        }
    }
}

fn request_field<'a>(request: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    request
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
}

fn resolve_web_chat_id(
    explicit_chat_id: Option<i64>,
    project_hint: Option<&str>,
    department_hint: Option<&str>,
    channel_name: Option<&str>,
) -> i64 {
    if let Some(chat_id) = explicit_chat_id {
        return chat_id;
    }

    if let Some(project) = project_hint {
        if let Some(department) = department_hint {
            return department_chat_id(project, department);
        }
        return project_chat_id(project);
    }

    if department_hint.is_some() {
        warn!("web chat scope included a department without a project; dropping department scope");
    }

    if let Some(name) = channel_name {
        if name.eq_ignore_ascii_case("sigil") {
            return agency_chat_id();
        }
        return named_channel_chat_id(name);
    }

    agency_chat_id()
}

fn task_snapshot(task: &sigil_tasks::Task) -> serde_json::Value {
    serde_json::json!({
        "id": task.id.0,
        "subject": task.subject,
        "status": task.status.to_string(),
        "closed_reason": task.closed_reason,
        "runtime": task.runtime(),
        "outcome": task.task_outcome(),
    })
}

fn merge_timeline_metadata(
    metadata: Option<&serde_json::Value>,
    task: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    match (metadata.cloned(), task) {
        (None, None) => None,
        (Some(mut metadata), Some(task)) => {
            if let Some(object) = metadata.as_object_mut() {
                object.insert("task".to_string(), task);
                Some(metadata)
            } else {
                Some(serde_json::json!({
                    "raw": metadata,
                    "task": task,
                }))
            }
        }
        (Some(metadata), None) => Some(metadata),
        (None, Some(task)) => Some(serde_json::json!({ "task": task })),
    }
}

async fn find_task_snapshot(
    registry: &Arc<ProjectRegistry>,
    project_hint: Option<&str>,
    task_id: &str,
) -> Option<serde_json::Value> {
    if let Some(project_name) = project_hint
        && let Some(board) = registry.get_task_board(project_name).await
    {
        let board = board.lock().await;
        if let Some(task) = board.get(task_id) {
            return Some(task_snapshot(task));
        }
    }

    for project_name in registry.project_names().await {
        if let Some(board) = registry.get_task_board(&project_name).await {
            let board = board.lock().await;
            if let Some(task) = board.get(task_id) {
                return Some(task_snapshot(task));
            }
        }
    }

    None
}

fn attach_chat_id(mut payload: serde_json::Value, chat_id: i64) -> serde_json::Value {
    payload["chat_id"] = serde_json::json!(chat_id);
    payload
}

/// The Daemon: background process that runs the ProjectRegistry patrol loop
/// and trigger system.
pub struct Daemon {
    pub registry: Arc<ProjectRegistry>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub patrol_interval_secs: u64,
    pub background_automation_enabled: bool,
    pub trigger_store: Option<Arc<TriggerStore>>,
    pub agent_registry: Option<Arc<AgentRegistry>>,
    pub chat_engine: Option<Arc<ChatEngine>>,
    pub write_queue: Arc<std::sync::Mutex<sigil_memory::debounce::WriteQueue>>,
    pub event_broadcaster: Arc<EventBroadcaster>,
    event_buffer: Arc<Mutex<EventBuffer>>,
    pub pid_file: Option<PathBuf>,
    pub socket_path: Option<PathBuf>,
    session_tracker_shutdown: Option<Arc<tokio::sync::Notify>>,
    running: Arc<std::sync::atomic::AtomicBool>,
    config_reloaded: Arc<std::sync::atomic::AtomicBool>,
    shutdown_notify: Arc<tokio::sync::Notify>,
    readiness: ReadinessContext,
}

impl Daemon {
    pub fn new(registry: Arc<ProjectRegistry>, dispatch_bus: Arc<DispatchBus>) -> Self {
        Self {
            registry,
            dispatch_bus,
            patrol_interval_secs: 30,
            background_automation_enabled: true,
            trigger_store: None,
            agent_registry: None,
            chat_engine: None,
            write_queue: Arc::new(std::sync::Mutex::new(
                sigil_memory::debounce::WriteQueue::default(),
            )),
            event_broadcaster: Arc::new(EventBroadcaster::new()),
            event_buffer: Arc::new(Mutex::new(EventBuffer::default())),
            pid_file: None,
            socket_path: None,
            session_tracker_shutdown: None,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            config_reloaded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
            readiness: ReadinessContext::default(),
        }
    }

    pub fn set_background_automation_enabled(&mut self, enabled: bool) {
        self.background_automation_enabled = enabled;
    }

    /// Fire a trigger: look up the owning agent, create a task with the trigger's skill.
    async fn fire_trigger(&self, trigger: &crate::trigger::Trigger) {
        // Look up agent to determine project.
        let project = if let Some(ref registry) = self.agent_registry {
            match registry.get(&trigger.agent_id).await {
                Ok(Some(agent)) => match agent.project {
                    Some(p) => p,
                    None => {
                        warn!(
                            agent = %trigger.agent_id,
                            "trigger agent has no project scope, skipping"
                        );
                        return;
                    }
                },
                Ok(None) => {
                    warn!(agent_id = %trigger.agent_id, "trigger agent not found");
                    return;
                }
                Err(e) => {
                    warn!(agent_id = %trigger.agent_id, error = %e, "failed to look up trigger agent");
                    return;
                }
            }
        } else {
            warn!("no agent registry available for trigger firing");
            return;
        };

        // Advance-before-execute: update last_fired BEFORE creating the task.
        // If the agent crashes mid-execution, the trigger won't re-fire on restart.
        if let Some(ref ts) = self.trigger_store
            && let Err(e) = ts.advance_before_execute(&trigger.id).await
        {
            warn!(trigger = %trigger.name, error = %e, "failed to advance trigger");
        }

        let subject = format!("[trigger:{}] {}", trigger.name, trigger.skill);
        let description = format!(
            "Trigger '{}' fired. Run skill '{}' for agent {}.",
            trigger.name, trigger.skill, trigger.agent_id
        );

        match self
            .registry
            .assign_with_skill_and_agent(
                &project,
                &subject,
                &description,
                &trigger.skill,
                Some(&trigger.agent_id),
            )
            .await
        {
            Ok(task) => {
                info!(
                    task = %task.id,
                    trigger = %trigger.name,
                    project = %project,
                    "trigger created task"
                );
            }
            Err(e) => {
                warn!(
                    trigger = %trigger.name,
                    project = %project,
                    error = %e,
                    "trigger failed to create task"
                );
            }
        }

        // Record the fire.
        if let Some(ref trigger_store) = self.trigger_store {
            let _ = trigger_store.record_fire(&trigger.id, 0.0).await;

            // Auto-disable one-shot triggers.
            if matches!(
                trigger.trigger_type,
                crate::trigger::TriggerType::Once { .. }
            ) {
                let _ = trigger_store.update_enabled(&trigger.id, false).await;
                info!(trigger = %trigger.name, "one-shot trigger auto-disabled");
            }
        }
    }

    /// Start the session tracker in a dedicated tokio::spawn.
    /// Returns the shutdown Notify so it can be stopped later.
    pub fn start_session_tracker(&mut self, tracker: SessionTracker) {
        let shutdown = Arc::new(tokio::sync::Notify::new());
        let shutdown_clone = shutdown.clone();
        tokio::spawn(async move {
            tracker.run(shutdown_clone).await;
        });
        self.session_tracker_shutdown = Some(shutdown);
        info!("session tracker launched");
    }

    /// Stop the session tracker if running.
    pub fn stop_session_tracker(&mut self) {
        if let Some(notify) = self.session_tracker_shutdown.take() {
            notify.notify_waiters();
            info!("session tracker stopped");
        }
    }

    /// Set the trigger store for agent-owned triggers.
    pub fn set_trigger_store(&mut self, store: Arc<TriggerStore>) {
        self.trigger_store = Some(store);
    }

    /// Set the agent registry for trigger agent lookups.
    pub fn set_agent_registry(&mut self, registry: Arc<AgentRegistry>) {
        self.agent_registry = Some(registry);
    }

    /// Set a PID file path (written on start, removed on stop).
    pub fn set_pid_file(&mut self, path: PathBuf) {
        self.pid_file = Some(path);
    }

    /// Set a Unix socket path for IPC.
    pub fn set_socket_path(&mut self, path: PathBuf) {
        self.socket_path = Some(path);
    }

    pub fn set_readiness_context(
        &mut self,
        configured_projects: usize,
        configured_advisors: usize,
        skipped_projects: Vec<String>,
        skipped_advisors: Vec<String>,
    ) {
        self.readiness = ReadinessContext {
            configured_projects,
            configured_advisors,
            skipped_projects,
            skipped_advisors,
        };
    }

    /// Write PID file.
    fn write_pid_file(&self) -> Result<()> {
        if let Some(ref path) = self.pid_file {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, std::process::id().to_string())?;
        }
        Ok(())
    }

    /// Remove PID file.
    fn remove_pid_file(&self) {
        if let Some(ref path) = self.pid_file {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Check if a daemon is already running by reading the PID file.
    pub fn is_running_from_pid(pid_path: &Path) -> bool {
        if let Ok(content) = std::fs::read_to_string(pid_path)
            && let Ok(pid) = content.trim().parse::<u32>()
        {
            // Check if process exists.
            return Path::new(&format!("/proc/{pid}")).exists();
        }
        false
    }

    /// Start the daemon loop with graceful shutdown on Ctrl+C.
    pub async fn run(&mut self) -> Result<()> {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        self.write_pid_file()?;

        let running = self.running.clone();
        let shutdown_notify = self.shutdown_notify.clone();
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("received Ctrl+C, shutting down...");
                running.store(false, std::sync::atomic::Ordering::SeqCst);
                shutdown_notify.notify_waiters();
            }
        });

        // Set up SIGHUP handler for config reload.
        #[cfg(unix)]
        {
            let config_reloaded = self.config_reloaded.clone();
            tokio::spawn(async move {
                let mut signal =
                    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("failed to register SIGHUP handler: {e}");
                            return;
                        }
                    };
                loop {
                    signal.recv().await;
                    info!("received SIGHUP, flagging config reload");
                    config_reloaded.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            });
        }

        // Set up SIGTERM handler for graceful shutdown (e.g. `rm daemon stop`, Docker, systemd).
        #[cfg(unix)]
        {
            let running = self.running.clone();
            let shutdown_notify = self.shutdown_notify.clone();
            tokio::spawn(async move {
                let mut signal =
                    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("failed to register SIGTERM handler: {e}");
                            return;
                        }
                    };
                signal.recv().await;
                info!("received SIGTERM, shutting down...");
                running.store(false, std::sync::atomic::Ordering::SeqCst);
                shutdown_notify.notify_waiters();
            });
        }

        // Spawn event trigger listener.
        if let Some(ref trigger_store) = self.trigger_store {
            let ts = trigger_store.clone();
            let registry = self.registry.clone();
            let agent_reg = self.agent_registry.clone();
            let mut rx = self.event_broadcaster.subscribe();
            tokio::spawn(async move {
                let mut cooldowns: std::collections::HashMap<String, chrono::DateTime<Utc>> =
                    std::collections::HashMap::new();
                while let Ok(event) = rx.recv().await {
                    let event_triggers = match ts.list_event_triggers().await {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    for trigger in event_triggers {
                        let (pattern, cooldown_secs) = match &trigger.trigger_type {
                            crate::trigger::TriggerType::Event {
                                pattern,
                                cooldown_secs,
                            } => (pattern, *cooldown_secs),
                            _ => continue,
                        };
                        if !pattern.matches_event(&event) {
                            continue;
                        }
                        // Check cooldown.
                        if let Some(last) = cooldowns.get(&trigger.id)
                            && (Utc::now() - *last).num_seconds() < cooldown_secs as i64
                        {
                            continue;
                        }
                        cooldowns.insert(trigger.id.clone(), Utc::now());

                        // Look up agent project.
                        let project = if let Some(ref ar) = agent_reg {
                            match ar.get(&trigger.agent_id).await {
                                Ok(Some(a)) => a.project,
                                _ => None,
                            }
                        } else {
                            None
                        };
                        if let Some(project) = project {
                            let subject = format!("[trigger:{}] {}", trigger.name, trigger.skill);
                            let desc = format!(
                                "Event trigger '{}' fired. Run skill '{}'.",
                                trigger.name, trigger.skill
                            );
                            let trigger_agent_id = trigger.agent_id.clone();
                            if let Err(e) = registry
                                .assign_with_skill_and_agent(&project, &subject, &desc, &trigger.skill, Some(&trigger_agent_id))
                                .await
                            {
                                warn!(
                                    trigger = %trigger.name,
                                    error = %e,
                                    "event trigger failed to create task"
                                );
                            } else {
                                info!(
                                    trigger = %trigger.name,
                                    project = %project,
                                    "event trigger fired"
                                );
                            }
                            let _ = ts.record_fire(&trigger.id, 0.0).await;
                        }
                    }
                }
            });
        }

        // Spawn background task to collect execution events from the broadcaster.
        {
            let event_buffer = self.event_buffer.clone();
            let mut rx = self.event_broadcaster.subscribe();
            tokio::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    let mut buffer = event_buffer.lock().await;
                    buffer.push(event);
                }
            });
        }

        // Start Unix socket listener for IPC queries.
        #[cfg(unix)]
        if let Some(ref sock_path) = self.socket_path {
            // Remove stale socket file.
            let _ = std::fs::remove_file(sock_path);
            if let Some(parent) = sock_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match tokio::net::UnixListener::bind(sock_path) {
                Ok(listener) => {
                    let registry = self.registry.clone();
                    let dispatch_bus = self.dispatch_bus.clone();
                    let trigger_store = self.trigger_store.clone();
                    let agent_registry = self.agent_registry.clone();
                    let chat_engine = self.chat_engine.clone();
                    let event_buffer = self.event_buffer.clone();
                    let running = self.running.clone();
                    let readiness = self.readiness.clone();
                    info!(path = %sock_path.display(), "IPC socket listening");
                    tokio::spawn(async move {
                        Self::socket_accept_loop(
                            listener,
                            registry,
                            dispatch_bus,
                            trigger_store,
                            agent_registry,
                            chat_engine,
                            event_buffer,
                            running,
                            readiness,
                        )
                        .await;
                    });
                }
                Err(e) => {
                    warn!(error = %e, path = %sock_path.display(), "failed to bind IPC socket");
                }
            }
        }

        // Load persisted state from disk.
        match self.dispatch_bus.load().await {
            Ok(n) if n > 0 => info!(count = n, "loaded persisted dispatches"),
            Ok(_) => {}
            Err(e) => warn!(error = %e, "failed to load dispatch bus"),
        }
        match self.registry.cost_ledger.load() {
            Ok(n) if n > 0 => info!(count = n, "loaded persisted cost entries"),
            Ok(_) => {}
            Err(e) => warn!(error = %e, "failed to load cost ledger"),
        }

        info!(triggers = self.trigger_store.is_some(), "daemon started");

        while self.running.load(std::sync::atomic::Ordering::SeqCst) {
            // 1. Patrol cycle: reap finished workers, assign + launch new ones (non-blocking).
            if let Err(e) = self.registry.patrol_all().await {
                warn!(error = %e, "patrol cycle failed");
            }

            // 2. Run due triggers (schedule + once types).
            if let Some(ref trigger_store) = self.trigger_store {
                match trigger_store.due_schedule_triggers().await {
                    Ok(due) => {
                        for trigger in due {
                            info!(
                                trigger_id = %trigger.id,
                                agent_id = %trigger.agent_id,
                                name = %trigger.name,
                                skill = %trigger.skill,
                                "trigger fired"
                            );
                            self.fire_trigger(&trigger).await;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to check due triggers");
                    }
                }
            }

            // 3. Check for config reload signal (SIGHUP).
            if self
                .config_reloaded
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                info!("config reload requested (SIGHUP received)");
                match sigil_core::config::SigilConfig::discover() {
                    Ok((new_config, path)) => {
                        // Apply runtime-safe fields from the reloaded config.

                        // (a) Global daily budget.
                        self.registry
                            .cost_ledger
                            .set_daily_budget(new_config.security.max_cost_per_day_usd);

                        // (b) Per-project budgets + worker counts + orchestrator params.
                        let orch = &new_config.orchestrator;
                        for pcfg in &new_config.projects {
                            if let Some(budget) = pcfg.max_cost_per_day_usd {
                                self.registry
                                    .cost_ledger
                                    .set_project_budget(&pcfg.name, budget);
                            }

                            // Update supervisor parameters.
                            if let Some(sup) = self.registry.get_supervisor(&pcfg.name).await {
                                let mut s = sup.lock().await;
                                s.max_workers = pcfg.max_workers;

                                // Apply orchestrator config (per-project override or global).
                                let proj_orch = pcfg.orchestrator.as_ref().unwrap_or(orch);
                                s.max_resolution_attempts = proj_orch.max_resolution_attempts;
                                s.max_description_chars = proj_orch.max_description_chars;
                                s.max_task_retries = proj_orch.max_task_retries;

                                // V3 feature flags.
                                s.expertise_routing = orch.expertise_routing;
                                s.preflight_enabled = orch.preflight_enabled;
                                s.preflight_model = orch.preflight_model.clone();
                                s.preflight_max_cost_usd = orch.preflight_max_cost_usd;
                                s.adaptive_retry = orch.adaptive_retry;
                                s.failure_analysis_model = orch.failure_analysis_model.clone();
                                s.auto_redecompose = orch.auto_redecompose;
                                s.decomposition_model = orch.decomposition_model.clone();
                                s.infer_deps_threshold = orch.infer_deps_threshold;

                                debug!(
                                    project = %pcfg.name,
                                    max_workers = s.max_workers,
                                    max_retries = s.max_task_retries,
                                    expertise_routing = s.expertise_routing,
                                    preflight = s.preflight_enabled,
                                    adaptive_retry = s.adaptive_retry,
                                    "supervisor config updated via SIGHUP"
                                );
                            }
                        }

                        // (c) Patrol interval.
                        if let Some(interval) = new_config.sigil.patrol_interval_secs {
                            self.patrol_interval_secs = interval;
                        }

                        info!(path = %path.display(), "config reloaded and applied via SIGHUP");
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to reload config, keeping current");
                    }
                }
            }

            // 7. Periodic persistence: save dispatch bus + cost ledger every patrol.
            if let Err(e) = self.dispatch_bus.save().await {
                warn!(error = %e, "failed to save dispatch bus");
            }
            if let Err(e) = self.registry.cost_ledger.save() {
                warn!(error = %e, "failed to save cost ledger");
            }

            // 8. Surface dispatch retries / dead letters for critical mail.
            let retried = self.dispatch_bus.retry_unacked(ACK_RETRY_AGE_SECS).await;
            for dispatch in &retried {
                warn!(
                    to = %dispatch.to,
                    subject = %dispatch.kind.subject_tag(),
                    retry = dispatch.retry_count,
                    "retrying unacknowledged dispatch"
                );
            }
            self.registry
                .metrics
                .dispatch_retries
                .inc_by(retried.len() as u64);
            let dead_letters = self.dispatch_bus.dead_letters().await;
            for dispatch in &dead_letters {
                warn!(
                    to = %dispatch.to,
                    subject = %dispatch.kind.subject_tag(),
                    retries = dispatch.retry_count,
                    "dispatch moved to dead-letter state"
                );
            }

            // 9. Update daily cost gauge.
            let (spent, _, _) = self.registry.cost_ledger.budget_status();
            self.registry.metrics.daily_cost_usd.set(spent);
            let dispatch_health = self.dispatch_bus.health(ACK_RETRY_AGE_SECS).await;
            self.registry
                .metrics
                .dispatch_queue_depth
                .set(dispatch_health.unread as f64);
            self.registry
                .metrics
                .dispatches_awaiting_ack
                .set(dispatch_health.awaiting_ack as f64);
            self.registry
                .metrics
                .dispatches_overdue_ack
                .set(dispatch_health.overdue_ack as f64);
            self.registry
                .metrics
                .dispatch_dead_letters
                .set(dispatch_health.dead_letters as f64);

            // 10. Prune old cost entries (older than 7 days) every cycle.
            self.registry.cost_ledger.prune_old();

            // 11. Prune expired blackboard entries.
            if let Some(ref bb) = self.registry.blackboard
                && let Err(e) = bb.prune_expired()
            {
                warn!(error = %e, "failed to prune blackboard");
            }

            // 9. Flush debounced memory writes to project memory stores.
            let ready = {
                match self.write_queue.lock() {
                    Ok(mut wq) => wq.drain_ready(chrono::Utc::now()),
                    Err(_) => Vec::new(),
                }
            };
            {
                if !ready.is_empty() {
                    info!(count = ready.len(), "flushing debounced memory writes");
                    if let Some(ref engine) = self.chat_engine {
                        for w in &ready {
                            if let Some(mem) = engine.memory_stores.get(&w.project) {
                                let category = match w.category.as_str() {
                                    "fact" => sigil_core::traits::MemoryCategory::Fact,
                                    "procedure" => sigil_core::traits::MemoryCategory::Procedure,
                                    "preference" => sigil_core::traits::MemoryCategory::Preference,
                                    "context" => sigil_core::traits::MemoryCategory::Context,
                                    _ => sigil_core::traits::MemoryCategory::Fact,
                                };
                                let scope = match w.scope.as_str() {
                                    "entity" => sigil_core::traits::MemoryScope::Entity,
                                    "system" => sigil_core::traits::MemoryScope::System,
                                    _ => sigil_core::traits::MemoryScope::Domain,
                                };
                                match mem.store(&w.key, &w.content, category, scope, None).await {
                                    Ok(id) => debug!(
                                        project = %w.project,
                                        id = %id,
                                        key = %w.key,
                                        "debounced write persisted"
                                    ),
                                    Err(e) => warn!(
                                        project = %w.project,
                                        key = %w.key,
                                        "debounced write failed: {e}"
                                    ),
                                }
                            } else {
                                debug!(
                                    project = %w.project,
                                    key = %w.key,
                                    "no memory store for project — write dropped"
                                );
                            }
                        }
                    }
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(self.patrol_interval_secs)) => {},
                _ = self.registry.wake.notified() => {
                    debug!("woken by new task");
                },
                _ = self.shutdown_notify.notified() => break,
            }
        }

        self.stop_session_tracker();
        self.remove_pid_file();
        self.remove_socket_file();
        info!("daemon stopped");
        Ok(())
    }

    /// Remove Unix socket file.
    fn remove_socket_file(&self) {
        if let Some(ref path) = self.socket_path {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Accept loop for Unix socket IPC connections.
    #[cfg(unix)]
    #[allow(clippy::too_many_arguments)]
    async fn socket_accept_loop(
        listener: tokio::net::UnixListener,
        registry: Arc<ProjectRegistry>,
        dispatch_bus: Arc<DispatchBus>,
        trigger_store: Option<Arc<TriggerStore>>,
        agent_registry: Option<Arc<AgentRegistry>>,
        chat_engine: Option<Arc<ChatEngine>>,
        event_buffer: Arc<Mutex<EventBuffer>>,
        running: Arc<std::sync::atomic::AtomicBool>,
        readiness: ReadinessContext,
    ) {
        loop {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            match listener.accept().await {
                Ok((stream, _)) => {
                    let registry = registry.clone();
                    let dispatch_bus = dispatch_bus.clone();
                    let trigger_store = trigger_store.clone();
                    let agent_registry = agent_registry.clone();
                    let chat_engine = chat_engine.clone();
                    let event_buffer = event_buffer.clone();
                    let readiness = readiness.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_socket_connection(
                            stream,
                            registry,
                            dispatch_bus,
                            trigger_store,
                            agent_registry,
                            chat_engine,
                            event_buffer,
                            readiness,
                        )
                        .await
                        {
                            debug!(error = %e, "IPC connection error");
                        }
                    });
                }
                Err(e) => {
                    warn!(error = %e, "IPC accept error");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Handle a single IPC connection. Protocol: one JSON line in, one JSON line out.
    #[cfg(unix)]
    #[allow(clippy::too_many_arguments)]
    async fn handle_socket_connection(
        stream: tokio::net::UnixStream,
        registry: Arc<ProjectRegistry>,
        dispatch_bus: Arc<DispatchBus>,
        trigger_store: Option<Arc<TriggerStore>>,
        agent_registry: Option<Arc<AgentRegistry>>,
        chat_engine: Option<Arc<ChatEngine>>,
        event_buffer: Arc<Mutex<EventBuffer>>,
        readiness: ReadinessContext,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            let request: serde_json::Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| serde_json::json!({"cmd": "unknown"}));

            let cmd = request
                .get("cmd")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let response = match cmd {
                "ping" => serde_json::json!({"ok": true, "pong": true}),

                "status" => {
                    let project_names: Vec<String> = registry.project_names().await;
                    let worker_count = registry.total_max_workers().await;
                    let dispatch_health = dispatch_bus.health(ACK_RETRY_AGE_SECS).await;
                    let mail_count = dispatch_health.unread;
                    let trigger_count = if let Some(ref ts) = trigger_store {
                        ts.count_enabled().await.unwrap_or(0)
                    } else {
                        0
                    };

                    let (spent, budget, remaining) = registry.cost_ledger.budget_status();
                    let project_budgets = registry.cost_ledger.all_project_budget_statuses();
                    let project_budget_info: serde_json::Map<String, serde_json::Value> =
                        project_budgets
                            .into_iter()
                            .map(|(name, (spent, budget, remaining))| {
                                (
                                    name,
                                    serde_json::json!({
                                        "spent_usd": spent,
                                        "budget_usd": budget,
                                        "remaining_usd": remaining,
                                    }),
                                )
                            })
                            .collect();

                    serde_json::json!({
                        "ok": true,
                        "projects": project_names,
                        "project_count": project_names.len(),
                        "max_workers": worker_count,
                        "triggers": trigger_count,
                        "pending_mail": mail_count,
                        "dispatch_health": {
                            "unread": dispatch_health.unread,
                            "awaiting_ack": dispatch_health.awaiting_ack,
                            "retrying_delivery": dispatch_health.retrying_delivery,
                            "overdue_ack": dispatch_health.overdue_ack,
                            "dead_letters": dispatch_health.dead_letters,
                        },
                        "cost_today_usd": spent,
                        "daily_budget_usd": budget,
                        "budget_remaining_usd": remaining,
                        "project_budgets": project_budget_info,
                    })
                }

                "readiness" => {
                    let worker_limits = registry.project_worker_limits().await;
                    let dispatch_health = dispatch_bus.health(ACK_RETRY_AGE_SECS).await;
                    let (spent, budget, remaining) = registry.cost_ledger.budget_status();
                    readiness_response(
                        &registry.leader_agent_name,
                        worker_limits,
                        dispatch_health,
                        (spent, budget, remaining),
                        &readiness,
                    )
                }

                "worker_progress" => {
                    let workers = registry.worker_progress().await;
                    serde_json::json!({"ok": true, "workers": workers})
                }

                "worker_events" => {
                    let cursor = request.get("cursor").and_then(|v| v.as_u64());
                    let snapshot = {
                        let buffer = event_buffer.lock().await;
                        buffer.read_since(cursor)
                    };
                    serde_json::json!({
                        "ok": true,
                        "events": snapshot.events,
                        "next_cursor": snapshot.next_cursor,
                        "oldest_cursor": snapshot.oldest_cursor,
                        "reset": snapshot.reset,
                    })
                }

                "projects" => {
                    let summaries = registry.list_project_summaries().await;
                    let projects: Vec<serde_json::Value> = summaries
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "name": s.name,
                                "prefix": s.prefix,
                                "team": s.team.as_ref().map(|t| serde_json::json!({
                                    "leader": t.leader,
                                    "agents": t.agents,
                                })),
                                "open_tasks": s.open_tasks,
                                "total_tasks": s.total_tasks,
                                "pending_tasks": s.pending_tasks,
                                "in_progress_tasks": s.in_progress_tasks,
                                "done_tasks": s.done_tasks,
                                "cancelled_tasks": s.cancelled_tasks,
                                "active_missions": s.active_missions,
                                "total_missions": s.total_missions,
                                "departments": s.departments.iter().map(|d| serde_json::json!({
                                    "name": d.name,
                                    "lead": d.lead,
                                    "agents": d.agents,
                                    "description": d.description,
                                })).collect::<Vec<_>>(),
                            })
                        })
                        .collect();
                    serde_json::json!({"ok": true, "projects": projects})
                }

                "mail" => {
                    let messages = dispatch_bus.drain();
                    let msgs: Vec<serde_json::Value> = messages
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "from": m.from,
                                "to": m.to,
                                "subject": m.kind.subject_tag(),
                                "body": m.kind.body_text(),
                            })
                        })
                        .collect();
                    serde_json::json!({"ok": true, "messages": msgs})
                }

                "dispatches" => {
                    let recipient = request.get("recipient").and_then(|v| v.as_str());
                    let state = request.get("state").and_then(|v| v.as_str());
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
                    let overdue_cutoff =
                        Utc::now() - chrono::Duration::seconds(ACK_RETRY_AGE_SECS as i64);
                    let mut dispatches = dispatch_bus.all().await;
                    if let Some(recipient) = recipient {
                        dispatches.retain(|d| d.to == recipient);
                    }
                    if let Some(state) = state {
                        dispatches.retain(|d| dispatch_state(d, overdue_cutoff) == state);
                    }
                    dispatches.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                    dispatches.truncate(limit);
                    let items: Vec<serde_json::Value> = dispatches
                        .iter()
                        .map(|d| dispatch_summary_json(d, overdue_cutoff))
                        .collect();
                    let health = dispatch_bus.health(ACK_RETRY_AGE_SECS).await;
                    serde_json::json!({
                        "ok": true,
                        "count": items.len(),
                        "dispatch_health": {
                            "unread": health.unread,
                            "awaiting_ack": health.awaiting_ack,
                            "retrying_delivery": health.retrying_delivery,
                            "overdue_ack": health.overdue_ack,
                            "dead_letters": health.dead_letters,
                        },
                        "dispatches": items,
                    })
                }

                "metrics" => {
                    let text = registry.metrics.render();
                    serde_json::json!({"ok": true, "metrics": text})
                }

                "cost" => {
                    let (spent, budget, remaining) = registry.cost_ledger.budget_status();
                    let report = registry.cost_ledger.daily_report();
                    let project_budgets = registry.cost_ledger.all_project_budget_statuses();
                    let project_budget_info: serde_json::Map<String, serde_json::Value> =
                        project_budgets
                            .into_iter()
                            .map(|(name, (spent, budget, remaining))| {
                                (
                                    name,
                                    serde_json::json!({
                                        "spent_usd": spent,
                                        "budget_usd": budget,
                                        "remaining_usd": remaining,
                                    }),
                                )
                            })
                            .collect();
                    serde_json::json!({
                        "ok": true,
                        "spent_today_usd": spent,
                        "daily_budget_usd": budget,
                        "remaining_usd": remaining,
                        "per_project": report,
                        "project_budgets": project_budget_info,
                    })
                }

                "audit" => {
                    let project_filter = request.get("project").and_then(|v| v.as_str());
                    let last = request.get("last").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
                    match &registry.audit_log {
                        Some(audit) => {
                            let events = if let Some(proj) = project_filter {
                                audit.query_by_project(proj).unwrap_or_default()
                            } else {
                                audit.query_recent(last).unwrap_or_default()
                            };
                            let items: Vec<serde_json::Value> = events
                                .iter()
                                .map(|e| {
                                    serde_json::json!({
                                        "timestamp": e.timestamp.to_rfc3339(),
                                        "project": e.project,
                                        "decision_type": e.decision_type.to_string(),
                                        "task_id": e.task_id,
                                        "agent": e.agent,
                                        "reasoning": e.reasoning,
                                    })
                                })
                                .collect();
                            serde_json::json!({"ok": true, "events": items})
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "audit log not initialized"})
                        }
                    }
                }

                "blackboard" => {
                    let project_filter = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*");
                    let limit = request.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
                    let since = request
                        .get("since")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc));
                    let tags: Vec<String> = request
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let cross_project = request
                        .get("cross_project")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    match &registry.blackboard {
                        Some(bb) => {
                            let entries = if cross_project {
                                bb.query_cross_project(&tags, since, limit)
                                    .unwrap_or_default()
                            } else if !tags.is_empty() {
                                if let Some(since_dt) = since {
                                    bb.query_since(project_filter, &tags, since_dt, limit)
                                        .unwrap_or_default()
                                } else {
                                    bb.query(project_filter, &tags, limit).unwrap_or_default()
                                }
                            } else if let Some(since_dt) = since {
                                bb.query_since(project_filter, &[], since_dt, limit)
                                    .unwrap_or_default()
                            } else {
                                bb.list_project(project_filter, limit).unwrap_or_default()
                            };
                            let items: Vec<serde_json::Value> = entries
                                .iter()
                                .map(|e| {
                                    serde_json::json!({
                                        "key": e.key,
                                        "content": e.content,
                                        "agent": e.agent,
                                        "project": e.project,
                                        "tags": e.tags,
                                        "created_at": e.created_at.to_rfc3339(),
                                        "expires_at": e.expires_at.to_rfc3339(),
                                    })
                                })
                                .collect();
                            serde_json::json!({"ok": true, "entries": items})
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "blackboard not initialized"})
                        }
                    }
                }

                "get_blackboard" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let key = request.get("key").and_then(|v| v.as_str()).unwrap_or("");
                    match &registry.blackboard {
                        Some(bb) => match bb.get_by_key(project, key) {
                            Ok(Some(entry)) => serde_json::json!({
                                "ok": true,
                                "entry": {
                                    "key": entry.key,
                                    "content": entry.content,
                                    "agent": entry.agent,
                                    "project": entry.project,
                                    "tags": entry.tags,
                                    "created_at": entry.created_at.to_rfc3339(),
                                    "expires_at": entry.expires_at.to_rfc3339(),
                                }
                            }),
                            Ok(None) => serde_json::json!({"ok": true, "entry": null}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        },
                        None => {
                            serde_json::json!({"ok": false, "error": "blackboard not initialized"})
                        }
                    }
                }

                "claim_blackboard" => {
                    let resource = request
                        .get("resource")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let agent = request
                        .get("agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("worker");
                    let content = request
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if resource.is_empty() || project.is_empty() {
                        serde_json::json!({"ok": false, "error": "resource and project are required"})
                    } else {
                        match &registry.blackboard {
                            Some(bb) => match bb.claim(resource, agent, project, content) {
                                Ok(crate::blackboard::ClaimResult::Acquired) => {
                                    serde_json::json!({"ok": true, "result": "acquired", "resource": resource})
                                }
                                Ok(crate::blackboard::ClaimResult::Renewed) => {
                                    serde_json::json!({"ok": true, "result": "renewed", "resource": resource})
                                }
                                Ok(crate::blackboard::ClaimResult::Held { holder, content }) => {
                                    serde_json::json!({"ok": true, "result": "held", "holder": holder, "content": content})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            },
                            None => {
                                serde_json::json!({"ok": false, "error": "blackboard not initialized"})
                            }
                        }
                    }
                }

                "release_blackboard" => {
                    let resource = request
                        .get("resource")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let agent = request
                        .get("agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("worker");
                    let force = request
                        .get("force")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    match &registry.blackboard {
                        Some(bb) => match bb.release(resource, agent, project, force) {
                            Ok(true) => serde_json::json!({"ok": true, "released": true}),
                            Ok(false) => {
                                serde_json::json!({"ok": true, "released": false, "reason": "not found or not owned"})
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        },
                        None => {
                            serde_json::json!({"ok": false, "error": "blackboard not initialized"})
                        }
                    }
                }

                "delete_blackboard" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let key = request.get("key").and_then(|v| v.as_str()).unwrap_or("");

                    match &registry.blackboard {
                        Some(bb) => match bb.delete_by_key(project, key) {
                            Ok(true) => serde_json::json!({"ok": true, "deleted": true}),
                            Ok(false) => serde_json::json!({"ok": true, "deleted": false}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        },
                        None => {
                            serde_json::json!({"ok": false, "error": "blackboard not initialized"})
                        }
                    }
                }

                "check_claim" => {
                    let resource = request
                        .get("resource")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    match &registry.blackboard {
                        Some(bb) => match bb.check_claim(resource, project) {
                            Ok(Some((agent, content))) => serde_json::json!({
                                "ok": true, "claimed": true, "agent": agent, "content": content
                            }),
                            Ok(None) => serde_json::json!({"ok": true, "claimed": false}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        },
                        None => {
                            serde_json::json!({"ok": false, "error": "blackboard not initialized"})
                        }
                    }
                }

                "expertise" => {
                    let domain = request
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .unwrap_or("general");
                    match &registry.expertise_ledger {
                        Some(ledger) => {
                            let scores = ledger.rank_for_domain(domain).unwrap_or_default();
                            let items: Vec<serde_json::Value> = scores
                                .iter()
                                .map(|s| {
                                    serde_json::json!({
                                        "agent": s.agent_name,
                                        "success_rate": s.success_rate,
                                        "avg_cost": s.avg_cost,
                                        "total_tasks": s.total_tasks,
                                        "confidence": s.confidence,
                                    })
                                })
                                .collect();
                            serde_json::json!({"ok": true, "scores": items})
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "expertise ledger not initialized"})
                        }
                    }
                }

                "tasks" => {
                    let project_filter = request.get("project").and_then(|v| v.as_str());
                    let status_filter = request.get("status").and_then(|v| v.as_str());

                    let project_names: Vec<String> = if let Some(name) = project_filter {
                        vec![name.to_string()]
                    } else {
                        registry.project_names().await
                    };

                    let mut all_tasks = Vec::new();
                    for name in &project_names {
                        if let Some(board) = registry.get_task_board(name).await {
                            let Ok(board) = board.try_lock() else {
                                continue;
                            };
                            for task in board.all() {
                                if let Some(status) = status_filter
                                    && task.status.to_string() != status
                                {
                                    continue;
                                }
                                all_tasks.push(serde_json::json!({
                                    "id": task.id.0,
                                    "subject": task.subject,
                                    "description": task.description,
                                    "status": task.status.to_string(),
                                    "priority": task.priority.to_string(),
                                    "assignee": task.assignee,
                                    "mission_id": task.mission_id,
                                    "skill": task.skill,
                                    "labels": task.labels,
                                    "retry_count": task.retry_count,
                                    "project": name,
                                    "created_at": task.created_at.to_rfc3339(),
                                    "updated_at": task.updated_at.map(|t| t.to_rfc3339()),
                                    "closed_at": task.closed_at.map(|t| t.to_rfc3339()),
                                    "closed_reason": task.closed_reason,
                                    "runtime": task.runtime(),
                                    "task_outcome": task.task_outcome(),
                                }));
                            }
                        }
                    }
                    serde_json::json!({"ok": true, "tasks": all_tasks})
                }

                "missions" => {
                    let project_filter = request.get("project").and_then(|v| v.as_str());

                    let project_names: Vec<String> = if let Some(name) = project_filter {
                        vec![name.to_string()]
                    } else {
                        registry.project_names().await
                    };

                    let mut all_missions = Vec::new();
                    for name in &project_names {
                        if let Some(board) = registry.get_task_board(name).await {
                            let Ok(board) = board.try_lock() else {
                                continue;
                            };
                            let prefix = name.clone(); // prefix lookup from project
                            for mission in board.missions(None) {
                                let (done, total) =
                                    sigil_tasks::Mission::check_progress(&mission.id, &board.all());
                                all_missions.push(serde_json::json!({
                                    "id": mission.id,
                                    "name": mission.name,
                                    "description": mission.description,
                                    "status": mission.status.to_string(),
                                    "project": prefix,
                                    "labels": mission.labels,
                                    "task_count": total,
                                    "done_count": done,
                                    "created_at": mission.created_at.to_rfc3339(),
                                    "updated_at": mission.updated_at.map(|t| t.to_rfc3339()),
                                    "closed_at": mission.closed_at.map(|t| t.to_rfc3339()),
                                }));
                            }
                        }
                    }
                    serde_json::json!({"ok": true, "missions": all_missions})
                }

                "create_task" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let subject = request
                        .get("subject")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let description = request
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let agent_id = request
                        .get("agent_id")
                        .and_then(|v| v.as_str());

                    if project.is_empty() || subject.is_empty() {
                        serde_json::json!({"ok": false, "error": "project and subject are required"})
                    } else {
                        match registry.assign_with_agent(project, subject, description, agent_id).await {
                            Ok(task) => serde_json::json!({
                                "ok": true,
                                "task": {
                                    "id": task.id.0,
                                    "subject": task.subject,
                                    "status": task.status.to_string(),
                                    "project": project,
                                }
                            }),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                "close_task" => {
                    let task_id = request
                        .get("task_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let reason = request
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("closed via web");
                    let project = request.get("project").and_then(|v| v.as_str());

                    if task_id.is_empty() {
                        serde_json::json!({"ok": false, "error": "task_id is required"})
                    } else {
                        // Find project by explicit param or by task ID prefix.
                        let project_name = if let Some(p) = project {
                            Some(p.to_string())
                        } else {
                            let _prefix = task_id.split('-').next().unwrap_or("");
                            let mut found = None;
                            for name in registry.project_names().await {
                                if let Some(board) = registry.get_task_board(&name).await {
                                    let board = board.lock().await;
                                    if board.get(task_id).is_some() {
                                        found = Some(name);
                                        break;
                                    }
                                }
                            }
                            found
                        };

                        match project_name {
                            Some(name) => {
                                if let Some(board) = registry.get_task_board(&name).await {
                                    let mut board = board.lock().await;
                                    match board.close(task_id, reason) {
                                        Ok(task) => {
                                            // Clean up task:* blackboard entries on close.
                                            if let Some(ref bb) = registry.blackboard {
                                                let prefix = format!("task:{}:", task_id);
                                                if let Ok(entries) = bb.list_project(&name, 200) {
                                                    let mut cleaned = 0u32;
                                                    for entry in &entries {
                                                        if entry.key.starts_with(&prefix) {
                                                            let _ =
                                                                bb.delete_by_key(&name, &entry.key);
                                                            cleaned += 1;
                                                        }
                                                    }
                                                    if cleaned > 0 {
                                                        tracing::debug!(
                                                            task_id,
                                                            cleaned,
                                                            "cleaned blackboard entries on task close"
                                                        );
                                                    }
                                                }
                                            }
                                            serde_json::json!({
                                                "ok": true,
                                                "task": {
                                                    "id": task.id.0,
                                                    "status": task.status.to_string(),
                                                    "closed_reason": task.closed_reason,
                                                    "runtime": task.runtime(),
                                                    "task_outcome": task.task_outcome(),
                                                }
                                            })
                                        }
                                        Err(e) => {
                                            serde_json::json!({"ok": false, "error": e.to_string()})
                                        }
                                    }
                                } else {
                                    serde_json::json!({"ok": false, "error": "project not found"})
                                }
                            }
                            None => {
                                serde_json::json!({"ok": false, "error": "could not find project for task"})
                            }
                        }
                    }
                }

                "post_blackboard" => {
                    let key = request.get("key").and_then(|v| v.as_str()).unwrap_or("");
                    let content = request
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let agent = request
                        .get("agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("web");
                    let tags: Vec<String> = request
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let durability = match request.get("durability").and_then(|v| v.as_str()) {
                        Some("durable") => crate::blackboard::EntryDurability::Durable,
                        _ => crate::blackboard::EntryDurability::Transient,
                    };

                    if key.is_empty() || content.is_empty() {
                        serde_json::json!({"ok": false, "error": "key and content are required"})
                    } else {
                        match &registry.blackboard {
                            Some(bb) => match bb
                                .post(key, content, agent, project, &tags, durability)
                            {
                                Ok(entry) => serde_json::json!({
                                    "ok": true,
                                    "entry": {
                                        "id": entry.id,
                                        "key": entry.key,
                                    }
                                }),
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            },
                            None => {
                                serde_json::json!({"ok": false, "error": "blackboard not initialized"})
                            }
                        }
                    }
                }

                "chat" => {
                    let message = request
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let project_hint = request_field(&request, "project");
                    let department_hint = request_field(&request, "department");
                    let channel_name = request_field(&request, "channel_name");
                    let sender = request
                        .get("sender")
                        .and_then(|v| v.as_str())
                        .unwrap_or("user");

                    match &chat_engine {
                        Some(engine) => {
                            let chat_id = resolve_web_chat_id(
                                request.get("chat_id").and_then(|v| v.as_i64()),
                                project_hint,
                                department_hint,
                                channel_name,
                            );

                            let msg = ChatMessage {
                                message: message.to_string(),
                                chat_id,
                                sender: sender.to_string(),
                                source: ChatSource::Web,
                                project_hint: project_hint.map(|s| s.to_string()),
                                department_hint: department_hint.map(|s| s.to_string()),
                                channel_name: channel_name.map(|s| s.to_string()),
                                agent_id: None,
                            };

                            // Try command shortcuts first (create task, close task).
                            if let Some(response) = engine.handle_message(&msg).await {
                                attach_chat_id(response.to_json(), chat_id)
                            } else {
                                // Fall back to status response enriched with memory.
                                let response =
                                    engine.status_response(project_hint, Some(message)).await;
                                engine.record_exchange(&msg, &response.context).await;
                                attach_chat_id(response.to_json(), chat_id)
                            }
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                        }
                    }
                }

                "chat_full" => {
                    let message = request
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let project_hint = request_field(&request, "project");
                    let department_hint = request_field(&request, "department");
                    let channel_name = request_field(&request, "channel_name");
                    let sender = request
                        .get("sender")
                        .and_then(|v| v.as_str())
                        .unwrap_or("user");
                    let agent_id = request_field(&request, "agent_id");

                    match &chat_engine {
                        Some(engine) => {
                            if message.is_empty() {
                                serde_json::json!({"ok": false, "error": "message is required"})
                            } else {
                                let chat_id = resolve_web_chat_id(
                                    request.get("chat_id").and_then(|v| v.as_i64()),
                                    project_hint,
                                    department_hint,
                                    channel_name,
                                );

                                let msg = ChatMessage {
                                    message: message.to_string(),
                                    chat_id,
                                    sender: sender.to_string(),
                                    source: ChatSource::Web,
                                    project_hint: project_hint.map(|s| s.to_string()),
                                    department_hint: department_hint.map(|s| s.to_string()),
                                    channel_name: channel_name.map(|s| s.to_string()),
                                    agent_id: agent_id.map(|s| s.to_string()),
                                };

                                // Try quick path first.
                                if let Some(response) = engine.handle_message(&msg).await {
                                    attach_chat_id(response.to_json(), chat_id)
                                } else {
                                    // Full LLM pipeline.
                                    match engine.handle_message_full(&msg, None).await {
                                        Ok(handle) => serde_json::json!({
                                            "ok": true,
                                            "action": "task_created",
                                            "task_handle": handle.task_id,
                                            "chat_id": handle.chat_id,
                                            "context": "Processing your message...",
                                        }),
                                        Err(e) => serde_json::json!({
                                            "ok": false,
                                            "error": e.to_string(),
                                        }),
                                    }
                                }
                            }
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                        }
                    }
                }

                "chat_poll" => {
                    let task_id = request
                        .get("task_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    match &chat_engine {
                        Some(engine) => {
                            if task_id.is_empty() {
                                serde_json::json!({"ok": false, "error": "task_id is required"})
                            } else {
                                match engine.poll_completion(task_id).await {
                                    Some(completion) => serde_json::json!({
                                        "ok": true,
                                        "completed": true,
                                        "status": format!("{:?}", completion.status),
                                        "text": completion.text,
                                        "chat_id": completion.chat_id,
                                    }),
                                    None => serde_json::json!({
                                        "ok": true,
                                        "completed": false,
                                    }),
                                }
                            }
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                        }
                    }
                }

                "chat_history" => {
                    let chat_id = request.get("chat_id").and_then(|v| v.as_i64()).unwrap_or(0);
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
                    let offset =
                        request.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let project_hint = request_field(&request, "project");
                    let department_hint = request_field(&request, "department");
                    let channel_name = request_field(&request, "channel_name");

                    match &chat_engine {
                        Some(engine) => {
                            let resolved_chat_id = resolve_web_chat_id(
                                if chat_id != 0 { Some(chat_id) } else { None },
                                project_hint,
                                department_hint,
                                channel_name,
                            );
                            match engine.get_history(resolved_chat_id, limit, offset).await {
                                Ok(messages) => {
                                    let msgs: Vec<serde_json::Value> = messages
                                        .iter()
                                        .map(|m| {
                                            serde_json::json!({
                                                "role": m.role,
                                                "content": m.content,
                                                "timestamp": m.timestamp.to_rfc3339(),
                                                "source": m.source,
                                            })
                                        })
                                        .collect();
                                    serde_json::json!({"ok": true, "messages": msgs, "chat_id": resolved_chat_id})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                        }
                    }
                }

                "chat_timeline" => {
                    let chat_id = request.get("chat_id").and_then(|v| v.as_i64()).unwrap_or(0);
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
                    let offset =
                        request.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let project_hint = request_field(&request, "project");
                    let department_hint = request_field(&request, "department");
                    let channel_name = request_field(&request, "channel_name");

                    match &chat_engine {
                        Some(engine) => {
                            let resolved_chat_id = resolve_web_chat_id(
                                if chat_id != 0 { Some(chat_id) } else { None },
                                project_hint,
                                department_hint,
                                channel_name,
                            );
                            match engine.get_timeline(resolved_chat_id, limit, offset).await {
                                Ok(events) => {
                                    let mut items = Vec::with_capacity(events.len());
                                    for event in &events {
                                        let task_snapshot = if let Some(metadata) =
                                            event.metadata.as_ref()
                                        {
                                            if let Some(task_id) = metadata
                                                .get("task_id")
                                                .and_then(|value| value.as_str())
                                            {
                                                let project_hint = metadata
                                                    .get("project")
                                                    .and_then(|value| value.as_str());
                                                find_task_snapshot(&registry, project_hint, task_id)
                                                    .await
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        items.push(serde_json::json!({
                                            "id": event.id,
                                            "chat_id": event.chat_id,
                                            "event_type": event.event_type,
                                            "role": event.role,
                                            "content": event.content,
                                            "timestamp": event.timestamp.to_rfc3339(),
                                            "source": event.source,
                                            "metadata": merge_timeline_metadata(event.metadata.as_ref(), task_snapshot),
                                        }));
                                    }
                                    serde_json::json!({"ok": true, "events": items, "chat_id": resolved_chat_id})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        }
                        None => {
                            serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                        }
                    }
                }

                "chat_channels" => match &chat_engine {
                    Some(engine) => match engine.list_channels().await {
                        Ok(channels) => {
                            let chs: Vec<serde_json::Value> = channels
                                .iter()
                                .map(|c| {
                                    serde_json::json!({
                                        "chat_id": c.chat_id,
                                        "channel_type": c.channel_type,
                                        "name": c.name,
                                        "created_at": c.created_at,
                                        "last_message": c.last_message,
                                        "last_message_at": c.last_message_at,
                                    })
                                })
                                .collect();
                            serde_json::json!({"ok": true, "channels": chs})
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                    },
                    None => {
                        serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                    }
                },

                "triggers" => match &trigger_store {
                    Some(store) => {
                        let triggers = store.list_all().await.unwrap_or_default();
                        let items: Vec<serde_json::Value> = triggers
                            .iter()
                            .map(|t| {
                                serde_json::json!({
                                    "id": t.id,
                                    "agent_id": t.agent_id,
                                    "name": t.name,
                                    "type": t.trigger_type.type_str(),
                                    "skill": t.skill,
                                    "enabled": t.enabled,
                                    "max_budget_usd": t.max_budget_usd,
                                    "last_fired": t.last_fired.map(|dt| dt.to_rfc3339()),
                                    "fire_count": t.fire_count,
                                    "total_cost_usd": t.total_cost_usd,
                                    "created_at": t.created_at.to_rfc3339(),
                                })
                            })
                            .collect();
                        serde_json::json!({"ok": true, "triggers": items})
                    }
                    None => serde_json::json!({"ok": true, "triggers": []}),
                },

                "agent_identity" => {
                    let agent_name = request.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if agent_name.is_empty() {
                        serde_json::json!({"ok": false, "error": "name is required"})
                    } else {
                        // Find agent directory by walking up from cwd to find agents/{name}/
                        let agent_dir = std::env::current_dir()
                            .unwrap_or_default()
                            .join("agents")
                            .join(agent_name);

                        if !agent_dir.exists() {
                            serde_json::json!({"ok": false, "error": format!("agent directory not found: {}", agent_dir.display())})
                        } else {
                            let mut files = serde_json::Map::new();
                            let identity_files = [
                                "PERSONA.md",
                                "IDENTITY.md",
                                "KNOWLEDGE.md",
                                "MEMORY.md",
                                "PREFERENCES.md",
                                "AGENTS.md",
                                "agent.toml",
                            ];
                            for filename in &identity_files {
                                let path = agent_dir.join(filename);
                                if path.exists()
                                    && let Ok(content) = std::fs::read_to_string(&path)
                                {
                                    files.insert(
                                        filename.to_string(),
                                        serde_json::Value::String(content),
                                    );
                                }
                            }
                            serde_json::json!({
                                "ok": true,
                                "agent": agent_name,
                                "files": files,
                            })
                        }
                    }
                }

                "save_agent_file" => {
                    let agent_name = request.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let filename = request
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let content = request
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    // Only allow editing known identity files.
                    let allowed = [
                        "PERSONA.md",
                        "IDENTITY.md",
                        "KNOWLEDGE.md",
                        "MEMORY.md",
                        "PREFERENCES.md",
                        "AGENTS.md",
                        "agent.toml",
                    ];
                    if agent_name.is_empty() || filename.is_empty() {
                        serde_json::json!({"ok": false, "error": "name and filename required"})
                    } else if !allowed.contains(&filename) {
                        serde_json::json!({"ok": false, "error": format!("cannot edit {filename}")})
                    } else {
                        let agent_dir = std::env::current_dir()
                            .unwrap_or_default()
                            .join("agents")
                            .join(agent_name);
                        let path = agent_dir.join(filename);
                        match std::fs::write(&path, content) {
                            Ok(_) => {
                                info!(
                                    agent = agent_name,
                                    file = filename,
                                    "agent file updated via web"
                                );
                                serde_json::json!({"ok": true, "saved": filename})
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                "rate_limit" => {
                    let rl_path = dirs::home_dir()
                        .unwrap_or_default()
                        .join(".sigil")
                        .join("rate_limit.json");
                    match std::fs::read_to_string(&rl_path) {
                        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                            Ok(rl) => serde_json::json!({"ok": true, "rate_limit": rl}),
                            Err(_) => serde_json::json!({"ok": true, "rate_limit": null}),
                        },
                        Err(_) => serde_json::json!({"ok": true, "rate_limit": null}),
                    }
                }

                "memories" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let query = request.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

                    if project.is_empty() {
                        // List all projects with memory counts.
                        let cwd = std::env::current_dir().unwrap_or_default();
                        let mut project_memories = Vec::new();
                        for entry in std::fs::read_dir(cwd.join("projects"))
                            .into_iter()
                            .flatten()
                            .flatten()
                        {
                            let name = entry.file_name().to_string_lossy().to_string();
                            let db_path = entry.path().join(".sigil").join("memory.db");
                            if db_path.exists() {
                                let count = rusqlite::Connection::open(&db_path)
                                    .and_then(|conn| {
                                        conn.query_row("SELECT COUNT(*) FROM memories", [], |r| {
                                            r.get::<_, i64>(0)
                                        })
                                    })
                                    .unwrap_or(0);
                                if count > 0 {
                                    project_memories
                                        .push(serde_json::json!({"project": name, "count": count}));
                                }
                            }
                        }
                        serde_json::json!({"ok": true, "projects": project_memories})
                    } else if let Some(ref engine) = chat_engine {
                        if let Some(mem) = engine.memory_stores.get(project) {
                            let mq = sigil_core::traits::MemoryQuery::new(query, limit);
                            match mem.search(&mq).await {
                                Ok(entries) => {
                                    let rows: Vec<serde_json::Value> = entries
                                        .iter()
                                        .map(|e| {
                                            serde_json::json!({
                                                "id": e.id,
                                                "key": e.key,
                                                "content": e.content,
                                                "category": format!("{:?}", e.category),
                                                "scope": format!("{:?}", e.scope),
                                                "entity_id": e.entity_id,
                                                "created_at": e.created_at.to_rfc3339(),
                                            })
                                        })
                                        .collect();
                                    serde_json::json!({"ok": true, "memories": rows, "count": rows.len()})
                                }
                                Err(e) => {
                                    serde_json::json!({"ok": false, "error": format!("search failed: {e}")})
                                }
                            }
                        } else {
                            serde_json::json!({"ok": true, "memories": [], "count": 0})
                        }
                    } else {
                        serde_json::json!({"ok": true, "memories": []})
                    }
                }

                "memory_profile" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if project.is_empty() {
                        serde_json::json!({"ok": false, "error": "project parameter required"})
                    } else {
                        let cwd = std::env::current_dir().unwrap_or_default();
                        let db_path = cwd
                            .join("projects")
                            .join(project)
                            .join(".sigil")
                            .join("memory.db");
                        if !db_path.exists() {
                            serde_json::json!({"ok": true, "profile": {"static": [], "dynamic": []}})
                        } else if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                            // Static categories: Fact, Preference, Evergreen (stable facts).
                            // Dynamic categories: Decision, Context, Insight, Procedure (changing context).
                            // Sort by created_at DESC as hotness proxy (no access_count column in schema).
                            let fetch = |categories: &[&str]| -> Vec<serde_json::Value> {
                                let placeholders: Vec<String> =
                                    categories.iter().map(|c| format!("LOWER('{c}')")).collect();
                                let sql = format!(
                                    "SELECT id, key, content, category, scope, created_at \
                                     FROM memories \
                                     WHERE LOWER(category) IN ({}) \
                                     ORDER BY created_at DESC \
                                     LIMIT 20",
                                    placeholders.join(", ")
                                );
                                conn.prepare(&sql)
                                    .ok()
                                    .map(|mut stmt| {
                                        stmt.query_map([], |row| {
                                            Ok(serde_json::json!({
                                                "id": row.get::<_, String>(0)?,
                                                "key": row.get::<_, String>(1)?,
                                                "content": row.get::<_, String>(2)?,
                                                "category": row.get::<_, String>(3)?,
                                                "scope": row.get::<_, String>(4)?,
                                                "created_at": row.get::<_, String>(5)?,
                                            }))
                                        })
                                        .ok()
                                        .map(|iter| iter.filter_map(|r| r.ok()).collect())
                                        .unwrap_or_default()
                                    })
                                    .unwrap_or_default()
                            };

                            let static_memories = fetch(&["fact", "preference", "evergreen"]);
                            let dynamic_memories =
                                fetch(&["decision", "context", "insight", "procedure"]);

                            serde_json::json!({
                                "ok": true,
                                "profile": {
                                    "static": static_memories,
                                    "dynamic": dynamic_memories,
                                }
                            })
                        } else {
                            serde_json::json!({"ok": true, "profile": {"static": [], "dynamic": []}})
                        }
                    }
                }

                "memory_graph" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

                    if project.is_empty() {
                        serde_json::json!({"ok": false, "error": "project parameter required"})
                    } else {
                        let cwd = std::env::current_dir().unwrap_or_default();
                        let db_path = cwd
                            .join("projects")
                            .join(project)
                            .join(".sigil")
                            .join("memory.db");
                        if !db_path.exists() {
                            serde_json::json!({"ok": true, "nodes": [], "edges": []})
                        } else if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                            let sql = format!(
                                "SELECT id, key, content, category, created_at \
                                 FROM memories \
                                 ORDER BY created_at DESC \
                                 LIMIT {limit}"
                            );
                            let nodes: Vec<serde_json::Value> = conn
                                .prepare(&sql)
                                .ok()
                                .map(|mut stmt| {
                                    stmt.query_map([], |row| {
                                        let id: String = row.get(0)?;
                                        let key: String = row.get(1)?;
                                        let content: String = row.get(2)?;
                                        let category: String = row.get(3)?;
                                        let created_at: String = row.get(4)?;

                                        // Simple position: hash of key/content mod 1000.
                                        use std::hash::{Hash, Hasher};
                                        let mut h =
                                            std::collections::hash_map::DefaultHasher::new();
                                        key.hash(&mut h);
                                        let x = (h.finish() % 1000) as u32;

                                        let mut h2 =
                                            std::collections::hash_map::DefaultHasher::new();
                                        content.hash(&mut h2);
                                        let y = (h2.finish() % 1000) as u32;

                                        // Hotness proxy: parse created_at and use recency.
                                        let hotness = chrono::NaiveDateTime::parse_from_str(
                                            &created_at,
                                            "%Y-%m-%dT%H:%M:%S%.f",
                                        )
                                        .or_else(|_| {
                                            chrono::NaiveDateTime::parse_from_str(
                                                &created_at,
                                                "%Y-%m-%d %H:%M:%S",
                                            )
                                        })
                                        .map(|dt| {
                                            let age_secs = (chrono::Utc::now()
                                                .naive_utc()
                                                .signed_duration_since(dt))
                                            .num_seconds()
                                            .max(0)
                                                as f64;
                                            let days = age_secs / 86400.0;
                                            // Exponential decay with 7-day half-life.
                                            let lambda = (2.0_f64).ln() / 7.0;
                                            (-lambda * days).exp() as f32
                                        })
                                        .unwrap_or(0.5);

                                        Ok(serde_json::json!({
                                            "id": id,
                                            "key": key,
                                            "content": content,
                                            "category": category,
                                            "x": x,
                                            "y": y,
                                            "hotness": hotness,
                                        }))
                                    })
                                    .ok()
                                    .map(|iter| iter.filter_map(|r| r.ok()).collect())
                                    .unwrap_or_default()
                                })
                                .unwrap_or_default();

                            // Placeholder edges array — real edges need memory_edges table
                            // which isn't in SQLite yet.
                            let edges: Vec<serde_json::Value> = Vec::new();

                            serde_json::json!({
                                "ok": true,
                                "nodes": nodes,
                                "edges": edges,
                            })
                        } else {
                            serde_json::json!({"ok": true, "nodes": [], "edges": []})
                        }
                    }
                }

                "skills" => {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    let mut skills = Vec::new();

                    // Helper: scan a directory for .toml and .md files.
                    let scan_skills =
                        |dir: &std::path::Path, source: &str, out: &mut Vec<serde_json::Value>| {
                            if !dir.exists() {
                                return;
                            }
                            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                                let path = entry.path();
                                if path.is_dir() {
                                    continue;
                                }
                                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                                if ext == "toml" || ext == "md" {
                                    let name = path
                                        .file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                    let content =
                                        std::fs::read_to_string(&path).unwrap_or_default();
                                    let kind = if ext == "toml" { "skill" } else { "doc" };
                                    out.push(serde_json::json!({
                                        "name": name,
                                        "source": source,
                                        "kind": kind,
                                        "path": path.display().to_string(),
                                        "content": content,
                                    }));
                                }
                            }
                        };

                    // Shared skills.
                    scan_skills(
                        &cwd.join("projects").join("shared").join("skills"),
                        "shared",
                        &mut skills,
                    );

                    // Shared agents.
                    scan_skills(
                        &cwd.join("projects").join("shared").join("agents"),
                        "shared/agents",
                        &mut skills,
                    );

                    // Per-project skills + agents.
                    for entry in std::fs::read_dir(cwd.join("projects"))
                        .into_iter()
                        .flatten()
                        .flatten()
                    {
                        let project = entry.file_name().to_string_lossy().to_string();
                        if project == "shared" {
                            continue;
                        }
                        scan_skills(&entry.path().join("skills"), &project, &mut skills);
                        scan_skills(
                            &entry.path().join("agents"),
                            &format!("{project}/agents"),
                            &mut skills,
                        );
                    }

                    serde_json::json!({"ok": true, "skills": skills})
                }

                "pipelines" => {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    let mut pipelines = Vec::new();
                    let shared_dir = cwd.join("projects").join("shared").join("pipelines");
                    if shared_dir.exists() {
                        for entry in std::fs::read_dir(&shared_dir)
                            .into_iter()
                            .flatten()
                            .flatten()
                        {
                            let path = entry.path();
                            if path.extension().is_some_and(|e| e == "toml") {
                                let name = path
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                let content = std::fs::read_to_string(&path).unwrap_or_default();
                                pipelines.push(serde_json::json!({
                                    "name": name,
                                    "content": content,
                                }));
                            }
                        }
                    }
                    serde_json::json!({"ok": true, "pipelines": pipelines})
                }

                "project_knowledge" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if project.is_empty() {
                        serde_json::json!({"ok": false, "error": "project required"})
                    } else {
                        let cwd = std::env::current_dir().unwrap_or_default();
                        let project_dir = cwd.join("projects").join(project);
                        let mut files = serde_json::Map::new();
                        let knowledge_files =
                            ["KNOWLEDGE.md", "AGENTS.md", "HEARTBEAT.md", "project.toml"];
                        for filename in &knowledge_files {
                            let path = project_dir.join(filename);
                            if path.exists()
                                && let Ok(content) = std::fs::read_to_string(&path)
                            {
                                files.insert(
                                    filename.to_string(),
                                    serde_json::Value::String(content),
                                );
                            }
                        }
                        serde_json::json!({"ok": true, "project": project, "files": files})
                    }
                }

                "channel_knowledge" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let query = request.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;

                    if project.is_empty() {
                        serde_json::json!({"ok": false, "error": "project required"})
                    } else {
                        let mut items: Vec<serde_json::Value> = Vec::new();

                        // 1. Search project memories.
                        if let Some(ref engine) = chat_engine
                            && let Some(mem) = engine.memory_stores.get(project)
                        {
                            let q = if query.is_empty() { project } else { query };
                            let mq = sigil_core::traits::MemoryQuery::new(q, limit)
                                .with_scope(sigil_core::traits::MemoryScope::Domain);
                            if let Ok(results) = mem.search(&mq).await {
                                for entry in results {
                                    items.push(serde_json::json!({
                                        "id": entry.id,
                                        "key": entry.key,
                                        "content": entry.content,
                                        "category": format!("{:?}", entry.category).to_lowercase(),
                                        "scope": format!("{:?}", entry.scope).to_lowercase(),
                                        "source": "memory",
                                        "created_at": entry.created_at.to_rfc3339(),
                                        "project": project,
                                    }));
                                }
                            }
                        }

                        // 2. Fetch blackboard entries for this project.
                        if let Some(ref bb) = registry.blackboard
                            && let Ok(entries) = bb.list_project(project, limit as u32)
                        {
                            for entry in entries {
                                items.push(serde_json::json!({
                                    "id": entry.id,
                                    "key": entry.key,
                                    "content": entry.content,
                                    "source": "blackboard",
                                    "agent": entry.agent,
                                    "tags": entry.tags,
                                    "created_at": entry.created_at.to_rfc3339(),
                                    "expires_at": entry.expires_at.to_rfc3339(),
                                    "project": project,
                                }));
                            }
                        }

                        serde_json::json!({"ok": true, "items": items, "count": items.len()})
                    }
                }

                "knowledge_store" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let key = request.get("key").and_then(|v| v.as_str()).unwrap_or("");
                    let content = request
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let category = request
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("fact");
                    let scope = request
                        .get("scope")
                        .and_then(|v| v.as_str())
                        .unwrap_or("domain");

                    if project.is_empty() || key.is_empty() || content.is_empty() {
                        serde_json::json!({"ok": false, "error": "project, key, and content required"})
                    } else if let Some(ref engine) = chat_engine {
                        if let Some(mem) = engine.memory_stores.get(project) {
                            let cat = match category {
                                "procedure" => sigil_core::traits::MemoryCategory::Procedure,
                                "preference" => sigil_core::traits::MemoryCategory::Preference,
                                "context" => sigil_core::traits::MemoryCategory::Context,
                                "evergreen" => sigil_core::traits::MemoryCategory::Evergreen,
                                _ => sigil_core::traits::MemoryCategory::Fact,
                            };
                            let sc = match scope {
                                "system" => sigil_core::traits::MemoryScope::System,
                                "entity" => sigil_core::traits::MemoryScope::Entity,
                                _ => sigil_core::traits::MemoryScope::Domain,
                            };
                            match mem.store(key, content, cat, sc, None).await {
                                Ok(id) => serde_json::json!({"ok": true, "id": id}),
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": false, "error": format!("no memory store for project: {project}")})
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                    }
                }

                "knowledge_delete" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let id = request.get("id").and_then(|v| v.as_str()).unwrap_or("");

                    if project.is_empty() || id.is_empty() {
                        serde_json::json!({"ok": false, "error": "project and id required"})
                    } else if let Some(ref engine) = chat_engine {
                        if let Some(mem) = engine.memory_stores.get(project) {
                            match mem.delete(id).await {
                                Ok(_) => serde_json::json!({"ok": true}),
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": false, "error": "no memory store for project"})
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                    }
                }

                // ── Persistent Agent Registry ──
                "agents_registry" => {
                    match &agent_registry {
                        Some(reg) => {
                            let project = request.get("project").and_then(|v| v.as_str());
                            let status_filter = request.get("status").and_then(|v| v.as_str());
                            let status = status_filter.and_then(|s| match s {
                                "active" => Some(crate::agent_registry::AgentStatus::Active),
                                "paused" => Some(crate::agent_registry::AgentStatus::Paused),
                                "retired" => Some(crate::agent_registry::AgentStatus::Retired),
                                _ => None,
                            });
                            match reg.list(project, status).await {
                                Ok(agents) => {
                                    let items: Vec<serde_json::Value> = agents.iter().map(|a| {
                                    serde_json::json!({
                                        "id": a.id,
                                        "name": a.name,
                                        "display_name": a.display_name,
                                        "template": a.template,
                                        "project": a.project,
                                        "department": a.department,
                                        "parent_id": a.parent_id,
                                        "model": a.model,
                                        "capabilities": a.capabilities,
                                        "status": a.status,
                                        "created_at": a.created_at.to_rfc3339(),
                                        "last_active": a.last_active.map(|dt| dt.to_rfc3339()),
                                        "session_count": a.session_count,
                                        "total_tokens": a.total_tokens,
                                        "color": a.color,
                                        "avatar": a.avatar,
                                        "faces": a.faces,
                                    })
                                }).collect();
                                    serde_json::json!({"ok": true, "agents": items})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        }
                        None => serde_json::json!({"ok": true, "agents": []}),
                    }
                }

                "agent_spawn" => match &agent_registry {
                    Some(reg) => {
                        let template = request
                            .get("template")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if template.is_empty() {
                            serde_json::json!({"ok": false, "error": "template is required"})
                        } else {
                            // Read template file from agents/ directory
                            let cwd = std::env::current_dir().unwrap_or_default();
                            let md_path = cwd.join("agents").join(template).join("agent.md");
                            let toml_path = cwd.join("agents").join(template).join("agent.toml");
                            let template_content = if md_path.exists() {
                                std::fs::read_to_string(&md_path).ok()
                            } else if toml_path.exists() {
                                std::fs::read_to_string(&toml_path).ok()
                            } else {
                                None
                            };
                            match template_content {
                                Some(content) => {
                                    let project = request.get("project").and_then(|v| v.as_str());
                                    let department =
                                        request.get("department").and_then(|v| v.as_str());
                                    match reg
                                        .spawn_from_template(&content, project, department)
                                        .await
                                    {
                                        Ok(agent) => serde_json::json!({
                                            "ok": true,
                                            "agent": {
                                                "id": agent.id,
                                                "name": agent.name,
                                                "display_name": agent.display_name,
                                                "status": agent.status,
                                            }
                                        }),
                                        Err(e) => {
                                            serde_json::json!({"ok": false, "error": e.to_string()})
                                        }
                                    }
                                }
                                None => {
                                    serde_json::json!({"ok": false, "error": format!("template not found: {template}")})
                                }
                            }
                        }
                    }
                    None => {
                        serde_json::json!({"ok": false, "error": "agent registry not available"})
                    }
                },

                "agent_set_status" => match &agent_registry {
                    Some(reg) => {
                        let name = request.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let status_str =
                            request.get("status").and_then(|v| v.as_str()).unwrap_or("");
                        if name.is_empty() || status_str.is_empty() {
                            serde_json::json!({"ok": false, "error": "name and status required"})
                        } else {
                            let status = match status_str {
                                "active" => Some(crate::agent_registry::AgentStatus::Active),
                                "paused" => Some(crate::agent_registry::AgentStatus::Paused),
                                "retired" => Some(crate::agent_registry::AgentStatus::Retired),
                                _ => None,
                            };
                            match status {
                                Some(s) => match reg.set_status(name, s).await {
                                    Ok(_) => serde_json::json!({"ok": true}),
                                    Err(e) => {
                                        serde_json::json!({"ok": false, "error": e.to_string()})
                                    }
                                },
                                None => {
                                    serde_json::json!({"ok": false, "error": format!("invalid status: {status_str}")})
                                }
                            }
                        }
                    }
                    None => {
                        serde_json::json!({"ok": false, "error": "agent registry not available"})
                    }
                },

                _ => serde_json::json!({"ok": false, "error": format!("unknown command: {cmd}")}),
            };

            let mut resp_bytes = serde_json::to_vec(&response)?;
            resp_bytes.push(b'\n');
            writer.write_all(&resp_bytes).await?;
        }

        Ok(())
    }

    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.shutdown_notify.notify_waiters();
    }

    /// Check if daemon is running.
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }
}

fn dispatch_state(dispatch: &Dispatch, overdue_cutoff: chrono::DateTime<Utc>) -> &'static str {
    if dispatch.requires_ack && dispatch.retry_count >= dispatch.max_retries {
        "dead_letter"
    } else if dispatch.requires_ack && dispatch.read && dispatch.timestamp < overdue_cutoff {
        "overdue_ack"
    } else if dispatch.requires_ack && dispatch.read {
        "awaiting_ack"
    } else if dispatch.requires_ack && !dispatch.read && dispatch.retry_count > 0 {
        "retrying_delivery"
    } else if !dispatch.read {
        "unread"
    } else {
        "handled"
    }
}

fn dispatch_summary_json(
    dispatch: &Dispatch,
    overdue_cutoff: chrono::DateTime<Utc>,
) -> serde_json::Value {
    serde_json::json!({
        "id": dispatch.id,
        "from": dispatch.from,
        "to": dispatch.to,
        "subject": dispatch.kind.subject_tag(),
        "body": dispatch.kind.body_text(),
        "timestamp": dispatch.timestamp.to_rfc3339(),
        "first_sent_at": dispatch.first_sent_at.to_rfc3339(),
        "read": dispatch.read,
        "requires_ack": dispatch.requires_ack,
        "retry_count": dispatch.retry_count,
        "max_retries": dispatch.max_retries,
        "state": dispatch_state(dispatch, overdue_cutoff),
        "age_seconds": (Utc::now() - dispatch.timestamp).num_seconds().max(0),
        "delivery_seconds": (Utc::now() - dispatch.first_sent_at).num_seconds().max(0),
    })
}

fn readiness_response(
    leader_agent_name: &str,
    mut worker_limits: Vec<(String, u32)>,
    dispatch_health: DispatchHealth,
    budget_status: (f64, f64, f64),
    readiness: &ReadinessContext,
) -> serde_json::Value {
    let (spent, budget, remaining) = budget_status;
    worker_limits.sort_by(|a, b| a.0.cmp(&b.0));

    let managed_owners: Vec<(String, u32)> = worker_limits
        .into_iter()
        .filter(|(name, _)| name != leader_agent_name)
        .collect();
    let registered_owners: Vec<String> = managed_owners
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    let max_workers: u32 = managed_owners.iter().map(|(_, workers)| *workers).sum();

    let mut blocking_reasons = Vec::new();
    if readiness.configured_projects + readiness.configured_advisors == 0 {
        blocking_reasons.push("no projects or advisor agents are configured".to_string());
    }
    if registered_owners.is_empty() {
        blocking_reasons.push("no projects or advisor agents were registered".to_string());
    }
    if !readiness.skipped_projects.is_empty() {
        blocking_reasons.push(format!(
            "{} configured project(s) were skipped because their directories were missing",
            readiness.skipped_projects.len()
        ));
    }
    if !readiness.skipped_advisors.is_empty() {
        blocking_reasons.push(format!(
            "{} advisor agent(s) were skipped because their directories were missing",
            readiness.skipped_advisors.len()
        ));
    }
    if max_workers == 0 {
        blocking_reasons
            .push("registered projects and advisors expose zero worker capacity".to_string());
    }
    if remaining <= 0.0 {
        blocking_reasons.push(format!(
            "daily budget exhausted (${spent:.2} spent of ${budget:.2})"
        ));
    }

    let mut warnings = Vec::new();
    if dispatch_health.overdue_ack > 0 {
        warnings.push(format!(
            "{} dispatch(es) are overdue for acknowledgment",
            dispatch_health.overdue_ack
        ));
    }
    if dispatch_health.dead_letters > 0 {
        warnings.push(format!(
            "{} dispatch(es) are in dead-letter state",
            dispatch_health.dead_letters
        ));
    }
    if dispatch_health.retrying_delivery > 0 {
        warnings.push(format!(
            "{} dispatch(es) are retrying delivery",
            dispatch_health.retrying_delivery
        ));
    }

    serde_json::json!({
        "ok": true,
        "ready": blocking_reasons.is_empty(),
        "leader_agent": leader_agent_name,
        "configured_projects": readiness.configured_projects,
        "configured_advisors": readiness.configured_advisors,
        "registered_owners": registered_owners,
        "registered_owner_count": managed_owners.len(),
        "max_workers": max_workers,
        "dispatch_health": {
            "unread": dispatch_health.unread,
            "awaiting_ack": dispatch_health.awaiting_ack,
            "retrying_delivery": dispatch_health.retrying_delivery,
            "overdue_ack": dispatch_health.overdue_ack,
            "dead_letters": dispatch_health.dead_letters,
        },
        "cost_today_usd": spent,
        "daily_budget_usd": budget,
        "budget_remaining_usd": remaining,
        "skipped_projects": readiness.skipped_projects.clone(),
        "skipped_advisors": readiness.skipped_advisors.clone(),
        "blocking_reasons": blocking_reasons,
        "warnings": warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DispatchHealth, EventBuffer, ExecutionEvent, ReadinessContext, readiness_response,
        resolve_web_chat_id,
    };
    use crate::conversation_store::{
        agency_chat_id, department_chat_id, named_channel_chat_id, project_chat_id,
    };

    #[test]
    fn readiness_blocks_when_owner_registration_is_incomplete() {
        let response = readiness_response(
            "leader",
            vec![("leader".to_string(), 1), ("alpha".to_string(), 2)],
            DispatchHealth::default(),
            (2.5, 50.0, 47.5),
            &ReadinessContext {
                configured_projects: 2,
                configured_advisors: 0,
                skipped_projects: vec!["beta".to_string()],
                skipped_advisors: Vec::new(),
            },
        );

        assert_eq!(response["ready"], serde_json::json!(false));
        assert_eq!(response["registered_owner_count"], serde_json::json!(1));
        assert_eq!(response["max_workers"], serde_json::json!(2));
        assert_eq!(response["skipped_projects"], serde_json::json!(["beta"]));
        assert!(
            response["blocking_reasons"]
                .as_array()
                .expect("blocking_reasons array")
                .iter()
                .any(|reason| reason.as_str().is_some_and(|text| text.contains("skipped")))
        );
    }

    #[test]
    fn readiness_surfaces_dispatch_warnings_without_blocking() {
        let response = readiness_response(
            "leader",
            vec![("leader".to_string(), 1), ("alpha".to_string(), 2)],
            DispatchHealth {
                unread: 0,
                awaiting_ack: 1,
                retrying_delivery: 1,
                overdue_ack: 1,
                dead_letters: 1,
            },
            (3.0, 50.0, 47.0),
            &ReadinessContext {
                configured_projects: 1,
                configured_advisors: 0,
                skipped_projects: Vec::new(),
                skipped_advisors: Vec::new(),
            },
        );

        assert_eq!(response["ready"], serde_json::json!(true));
        assert_eq!(
            response["warnings"].as_array().map(|items| items.len()),
            Some(3)
        );
    }

    #[test]
    fn readiness_blocks_when_budget_is_exhausted() {
        let response = readiness_response(
            "leader",
            vec![("leader".to_string(), 1), ("alpha".to_string(), 2)],
            DispatchHealth::default(),
            (50.0, 50.0, 0.0),
            &ReadinessContext {
                configured_projects: 1,
                configured_advisors: 0,
                skipped_projects: Vec::new(),
                skipped_advisors: Vec::new(),
            },
        );

        assert_eq!(response["ready"], serde_json::json!(false));
        assert!(
            response["blocking_reasons"]
                .as_array()
                .expect("blocking_reasons array")
                .iter()
                .any(|reason| reason
                    .as_str()
                    .is_some_and(|text| text.contains("budget exhausted")))
        );
    }

    #[test]
    fn event_buffer_supports_independent_cursors() {
        let mut buffer = EventBuffer::default();
        buffer.push(ExecutionEvent::TaskStarted {
            task_id: "t-1".into(),
            agent: "engineer".into(),
            project: "sigil".into(),
            runtime_session: None,
        });
        buffer.push(ExecutionEvent::TaskCompleted {
            task_id: "t-1".into(),
            outcome: "done".into(),
            confidence: 1.0,
            cost_usd: 0.1,
            turns: 2,
            duration_ms: 100,
            runtime: None,
        });

        let client_a = buffer.read_since(Some(0));
        let client_b = buffer.read_since(Some(0));
        assert_eq!(client_a.events.len(), 2);
        assert_eq!(client_b.events.len(), 2);
        assert_eq!(client_a.next_cursor, 2);
        assert_eq!(client_b.next_cursor, 2);

        buffer.push(ExecutionEvent::Progress {
            task_id: "t-2".into(),
            turns: 1,
            cost_usd: 0.05,
            last_tool: Some("shell".into()),
        });

        let client_a_next = buffer.read_since(Some(client_a.next_cursor));
        let client_b_still_old = buffer.read_since(Some(0));
        assert_eq!(client_a_next.events.len(), 1);
        assert_eq!(client_b_still_old.events.len(), 3);
    }

    #[test]
    fn event_buffer_flags_cursor_resets_after_truncation() {
        let mut buffer = EventBuffer::default();
        for i in 0..(super::MAX_EVENT_BUFFER_LEN + 5) {
            buffer.push(ExecutionEvent::Progress {
                task_id: format!("t-{i}"),
                turns: i as u32,
                cost_usd: i as f64,
                last_tool: None,
            });
        }

        let snapshot = buffer.read_since(Some(0));
        assert!(snapshot.reset);
        assert_eq!(snapshot.events.len(), super::MAX_EVENT_BUFFER_LEN);
        assert!(snapshot.oldest_cursor > 0);
    }

    #[test]
    fn web_chat_resolution_prefers_scoped_channels() {
        assert_eq!(
            resolve_web_chat_id(None, Some("alpha"), Some("backend"), Some("alpha/backend"),),
            department_chat_id("alpha", "backend")
        );
        assert_eq!(
            resolve_web_chat_id(None, Some("alpha"), None, Some("alpha"),),
            project_chat_id("alpha")
        );
        assert_eq!(
            resolve_web_chat_id(None, None, None, Some("ops")),
            named_channel_chat_id("ops")
        );
    }

    #[test]
    fn web_chat_resolution_uses_global_fallback() {
        assert_eq!(
            resolve_web_chat_id(None, None, None, Some("sigil")),
            agency_chat_id()
        );
        assert_eq!(
            resolve_web_chat_id(None, None, None, None),
            agency_chat_id()
        );
    }
}
