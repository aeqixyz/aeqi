use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::agent_registry::AgentRegistry;
use crate::event_store::EventStore;
use crate::event_store::{Dispatch, DispatchHealth, DispatchKind};
use crate::execution_events::{EventBroadcaster, ExecutionEvent};
use crate::message_router::{IncomingMessage, MessageRouter, MessageSource};
use crate::metrics::AEQIMetrics;
use crate::progress_tracker::ProgressTracker;
use crate::scheduler::Scheduler;
use crate::session_manager::SessionManager;
use crate::session_store::{
    SessionStore, agency_chat_id, department_chat_id, named_channel_chat_id, project_chat_id,
};
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
        if name.eq_ignore_ascii_case("aeqi") {
            return agency_chat_id();
        }
        return named_channel_chat_id(name);
    }

    agency_chat_id()
}

fn task_snapshot(task: &aeqi_quests::Quest) -> serde_json::Value {
    serde_json::json!({
        "id": task.id.0,
        "subject": task.name,
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
    agent_registry: &Arc<AgentRegistry>,
    task_id: &str,
) -> Option<serde_json::Value> {
    agent_registry
        .get_task(task_id)
        .await
        .ok()
        .flatten()
        .map(|t| task_snapshot(&t))
}

fn attach_chat_id(mut payload: serde_json::Value, chat_id: i64) -> serde_json::Value {
    payload["chat_id"] = serde_json::json!(chat_id);
    payload
}

/// Context struct bundling shared service references for IPC handlers.
/// Avoids passing many individual parameters to socket_accept_loop / handle_socket_connection.
struct IpcContext {
    metrics: Arc<AEQIMetrics>,
    event_store: Arc<EventStore>,
    session_store: Option<Arc<SessionStore>>,
    leader_agent_name: String,
    daily_budget_usd: f64,
    project_budgets: std::collections::HashMap<String, f64>,
}

/// The Daemon: background process that runs the scheduler patrol loop
/// and trigger system.
pub struct Daemon {
    pub metrics: Arc<AEQIMetrics>,
    pub event_store: Arc<EventStore>,
    pub session_store: Option<Arc<SessionStore>>,
    pub leader_agent_name: String,
    pub shared_primer: Option<String>,
    pub project_primer: Option<String>,
    pub patrol_interval_secs: u64,
    pub background_automation_enabled: bool,
    pub trigger_store: Option<Arc<TriggerStore>>,
    pub agent_registry: Arc<AgentRegistry>,
    pub message_router: Option<Arc<MessageRouter>>,
    pub write_queue: Arc<std::sync::Mutex<aeqi_insights::debounce::WriteQueue>>,
    pub event_broadcaster: Arc<EventBroadcaster>,
    pub default_provider: Option<Arc<dyn aeqi_core::traits::Provider>>,
    pub default_model: String,
    event_buffer: Arc<Mutex<EventBuffer>>,
    pub session_manager: Arc<SessionManager>,
    pub pid_file: Option<PathBuf>,
    pub socket_path: Option<PathBuf>,
    session_tracker_shutdown: Option<Arc<tokio::sync::Notify>>,
    running: Arc<std::sync::atomic::AtomicBool>,
    config_reloaded: Arc<std::sync::atomic::AtomicBool>,
    shutdown_notify: Arc<tokio::sync::Notify>,
    readiness: ReadinessContext,
    /// Global daily budget cap.
    pub daily_budget_usd: f64,
    /// Per-project budget caps.
    pub project_budgets: std::collections::HashMap<String, f64>,
    /// Global scheduler for the unified schedule() loop.
    pub scheduler: Arc<Scheduler>,
}

impl Daemon {
    pub fn new(
        metrics: Arc<AEQIMetrics>,
        scheduler: Arc<Scheduler>,
        agent_registry: Arc<AgentRegistry>,
        event_store: Arc<EventStore>,
    ) -> Self {
        Self {
            metrics,
            event_store,
            session_store: None,
            leader_agent_name: String::new(),
            shared_primer: None,
            project_primer: None,
            patrol_interval_secs: 30,
            background_automation_enabled: true,
            trigger_store: None,
            agent_registry,
            message_router: None,
            write_queue: Arc::new(std::sync::Mutex::new(
                aeqi_insights::debounce::WriteQueue::default(),
            )),
            event_broadcaster: Arc::new(EventBroadcaster::new()),
            default_provider: None,
            default_model: String::new(),
            event_buffer: Arc::new(Mutex::new(EventBuffer::default())),
            session_manager: Arc::new(SessionManager::new()),
            pid_file: None,
            socket_path: None,
            session_tracker_shutdown: None,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            config_reloaded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
            readiness: ReadinessContext::default(),
            daily_budget_usd: 50.0,
            project_budgets: std::collections::HashMap::new(),
            scheduler,
        }
    }

    pub fn set_background_automation_enabled(&mut self, enabled: bool) {
        self.background_automation_enabled = enabled;
    }

    /// Fire a trigger: look up the owning agent, create a task with the trigger's skill.
    async fn fire_trigger(&self, trigger: &crate::trigger::Trigger) {
        // Look up agent to determine parent (project context).
        let _project = match self.agent_registry.get(&trigger.agent_id).await {
            Ok(Some(agent)) => match agent.parent_id {
                Some(p) => p,
                None => {
                    // Root agent — use agent name as project key.
                    agent.name.clone()
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
            .agent_registry
            .create_task(
                &trigger.agent_id,
                &subject,
                &description,
                Some(&trigger.skill),
                &[],
            )
            .await
        {
            Ok(task) => {
                self.scheduler.wake.notify_one();
                info!(
                    task = %task.id,
                    trigger = %trigger.name,
                    agent_id = %trigger.agent_id,
                    "trigger created task"
                );
            }
            Err(e) => {
                warn!(
                    trigger = %trigger.name,
                    agent_id = %trigger.agent_id,
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

    /// Consume pending DelegateRequest dispatches for all active agents.
    /// For each dispatch, creates an agent-bound task with the delegation prompt.
    /// This is the primary consumption path — agents don't need DispatchReceived
    /// event triggers to receive delegated work.
    async fn consume_agent_dispatches(&self) {
        let agents = match self.agent_registry.list_active().await {
            Ok(a) => a,
            Err(_) => return,
        };

        for agent in &agents {
            // Leader dispatches are already consumed elsewhere. Skip to avoid double-processing.
            if agent.name == self.leader_agent_name {
                continue;
            }

            let dispatches = self.event_store.read(&agent.name).await;
            if dispatches.is_empty() {
                // Also check by agent UUID (dispatches may be addressed by ID).
                let id_dispatches = self.event_store.read(&agent.id).await;
                if id_dispatches.is_empty() {
                    continue;
                }
                self.process_agent_dispatches(
                    &agent.id,
                    &agent.name,
                    &agent.parent_id,
                    &id_dispatches,
                )
                .await;
            } else {
                self.process_agent_dispatches(
                    &agent.id,
                    &agent.name,
                    &agent.parent_id,
                    &dispatches,
                )
                .await;
            }
        }
    }

    /// Process a batch of dispatches for a specific agent.
    async fn process_agent_dispatches(
        &self,
        agent_id: &str,
        agent_name: &str,
        project: &Option<String>,
        dispatches: &[crate::event_store::Dispatch],
    ) {
        let _project = match project {
            Some(p) => p.clone(),
            None => {
                warn!(agent = %agent_name, "agent has no project scope, cannot create task for dispatch");
                return;
            }
        };

        for dispatch in dispatches {
            if dispatch.requires_ack {
                self.event_store.acknowledge(&dispatch.id).await;
            }

            match &dispatch.kind {
                DispatchKind::DelegateRequest {
                    prompt,
                    response_mode,
                    create_task,
                    skill,
                    parent_session_id,
                    ..
                } => {
                    if !create_task {
                        // Fire-and-forget: just log and skip task creation.
                        info!(
                            agent = %agent_name,
                            from = %dispatch.from,
                            "dispatch consumed (no task requested)"
                        );
                        continue;
                    }

                    let subject = format!("Delegation from {}", dispatch.from);
                    let description = format!(
                        "## Delegated Work\n\n{}\n\n---\n*From: {} | Response mode: {}*",
                        prompt, dispatch.from, response_mode
                    );

                    let mut labels = vec![
                        format!("delegate_from:{}", dispatch.from),
                        format!("delegate_dispatch:{}", dispatch.id),
                        format!("delegate_response_mode:{}", response_mode),
                    ];
                    if let Some(psid) = &parent_session_id {
                        labels.push(format!("parent_session_id:{psid}"));
                    }

                    let skill_name = skill.as_deref().unwrap_or("process-dispatch");

                    match self
                        .agent_registry
                        .create_task(agent_id, &subject, &description, Some(skill_name), &labels)
                        .await
                    {
                        Ok(task) => {
                            self.scheduler.wake.notify_one();
                            info!(
                                task = %task.id,
                                agent = %agent_name,
                                from = %dispatch.from,
                                response_mode = %response_mode,
                                "dispatch consumed → task created"
                            );
                        }
                        Err(e) => {
                            warn!(
                                agent = %agent_name,
                                from = %dispatch.from,
                                error = %e,
                                "failed to create task from dispatch"
                            );
                        }
                    }
                }
                DispatchKind::DelegateResponse {
                    content, reply_to, ..
                } => {
                    // DelegateResponses for non-leader agents: log for now.
                    // Future: inject into agent's perpetual session.
                    info!(
                        agent = %agent_name,
                        reply_to = %reply_to,
                        content_len = content.len(),
                        "delegate response received (logged, not yet injected into session)"
                    );
                }
                DispatchKind::HumanEscalation { subject, .. } => {
                    info!(
                        agent = %agent_name,
                        subject = %subject,
                        "human escalation received by non-leader agent, re-routing to leader"
                    );
                    // Re-route to leader.
                    let mut rerouted = dispatch.clone();
                    rerouted.to = self.leader_agent_name.clone();
                    rerouted.read = false;
                    self.event_store.send(rerouted).await;
                }
            }
        }
    }

    /// Start the session tracker in a dedicated tokio::spawn.
    /// Returns the shutdown Notify so it can be stopped later.
    pub fn start_session_tracker(&mut self, tracker: ProgressTracker) {
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
    /// Main daemon entry point. Spawns background services, then runs the patrol loop.
    pub async fn run(&mut self) -> Result<()> {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        self.write_pid_file()?;

        self.spawn_signal_handlers();
        self.spawn_event_listeners();
        self.spawn_ipc_listener();
        self.load_persisted_state().await;

        info!(triggers = self.trigger_store.is_some(), "daemon started");

        self.run_patrol_loop().await;

        self.stop_session_tracker();
        self.remove_pid_file();
        self.remove_socket_file();
        info!("daemon stopped");
        Ok(())
    }

    /// Spawn OS signal handlers: Ctrl+C, SIGHUP (config reload), SIGTERM (graceful shutdown).
    fn spawn_signal_handlers(&self) {
        let running = self.running.clone();
        let shutdown_notify = self.shutdown_notify.clone();
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("received Ctrl+C, shutting down...");
                running.store(false, std::sync::atomic::Ordering::SeqCst);
                shutdown_notify.notify_waiters();
            }
        });

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
    }

    /// Spawn background listeners for event triggers and execution event buffering.
    fn spawn_event_listeners(&self) {
        // Event trigger listener — matches events against trigger patterns, fires tasks.
        if let Some(ref trigger_store) = self.trigger_store {
            let ts = trigger_store.clone();
            let agent_reg = self.agent_registry.clone();
            let scheduler = self.scheduler.clone();
            let dispatch_es = self.event_store.clone();
            let mut rx = self.event_broadcaster.subscribe();
            tokio::spawn(async move {
                let mut cooldowns: std::collections::HashMap<String, chrono::DateTime<Utc>> =
                    std::collections::HashMap::new();
                loop {
                    let event = match rx.recv().await {
                        Ok(event) => event,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(
                                skipped = n,
                                "event trigger listener lagged — events dropped"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    };
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
                        if let Some(last) = cooldowns.get(&trigger.id)
                            && (Utc::now() - *last).num_seconds() < cooldown_secs as i64
                        {
                            continue;
                        }
                        cooldowns.insert(trigger.id.clone(), Utc::now());

                        let project = match agent_reg.get(&trigger.agent_id).await {
                            Ok(Some(a)) => a.parent_id.or_else(|| Some(a.name.clone())),
                            _ => None,
                        };
                        if let Some(project) = project {
                            let subject = format!("[trigger:{}] {}", trigger.name, trigger.skill);
                            let mut delegation_labels: Vec<String> = Vec::new();
                            let dispatch_context =
                                if let crate::execution_events::ExecutionEvent::DispatchReceived {
                                    ref to_agent,
                                    ..
                                } = event
                                {
                                    let dispatches = dispatch_es.read(to_agent).await;
                                    let prompts: Vec<String> = dispatches
                                        .iter()
                                        .filter_map(|d| {
                                            if let DispatchKind::DelegateRequest {
                                                ref prompt,
                                                ref response_mode,
                                                ref parent_session_id,
                                                ..
                                            } = d.kind
                                            {
                                                delegation_labels
                                                    .push(format!("delegate_from:{}", d.from));
                                                delegation_labels
                                                    .push(format!("delegate_dispatch:{}", d.id));
                                                delegation_labels.push(format!(
                                                    "delegate_response_mode:{response_mode}"
                                                ));
                                                if let Some(psid) = &parent_session_id {
                                                    delegation_labels
                                                        .push(format!("parent_session_id:{psid}"));
                                                }
                                                Some(format!("From {}: {}", d.from, prompt))
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();
                                    if prompts.is_empty() {
                                        String::new()
                                    } else {
                                        format!(
                                            "\n\n## Pending Delegations\n{}",
                                            prompts.join("\n\n")
                                        )
                                    }
                                } else {
                                    String::new()
                                };
                            let desc = format!(
                                "Event trigger '{}' fired. Run skill '{}'.{}",
                                trigger.name, trigger.skill, dispatch_context
                            );
                            let trigger_agent_id = trigger.agent_id.clone();
                            match agent_reg
                                .create_task(
                                    &trigger_agent_id,
                                    &subject,
                                    &desc,
                                    Some(&trigger.skill),
                                    &delegation_labels,
                                )
                                .await
                            {
                                Ok(_task) => {
                                    scheduler.wake.notify_one();
                                    info!(
                                        trigger = %trigger.name,
                                        project = %project,
                                        "event trigger fired"
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        trigger = %trigger.name,
                                        error = %e,
                                        "event trigger failed to create task"
                                    );
                                }
                            }
                            let _ = ts.record_fire(&trigger.id, 0.0).await;
                        }
                    }
                }
            });
        }

        // Execution event buffer — collects events for the event buffer API.
        {
            let event_buffer = self.event_buffer.clone();
            let mut rx = self.event_broadcaster.subscribe();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let mut buffer = event_buffer.lock().await;
                            buffer.push(event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(
                                skipped = n,
                                "event buffer subscriber lagged — events dropped"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }
    }

    /// Bind the Unix socket for IPC queries (if configured).
    #[cfg(unix)]
    fn spawn_ipc_listener(&self) {
        let Some(ref sock_path) = self.socket_path else {
            return;
        };
        let _ = std::fs::remove_file(sock_path);
        if let Some(parent) = sock_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match tokio::net::UnixListener::bind(sock_path) {
            Ok(listener) => {
                let ipc_ctx = Arc::new(IpcContext {
                    metrics: self.metrics.clone(),
                    event_store: self.event_store.clone(),
                    session_store: self.session_store.clone(),
                    leader_agent_name: self.leader_agent_name.clone(),
                    daily_budget_usd: self.daily_budget_usd,
                    project_budgets: self.project_budgets.clone(),
                });
                let dispatch_es = self.event_store.clone();
                let trigger_store = self.trigger_store.clone();
                let agent_registry = self.agent_registry.clone();
                let message_router = self.message_router.clone();
                let event_buffer = self.event_buffer.clone();
                let running = self.running.clone();
                let readiness = self.readiness.clone();
                let default_provider = self.default_provider.clone();
                let default_model = self.default_model.clone();
                let session_manager = self.session_manager.clone();
                let event_broadcaster = self.event_broadcaster.clone();
                let scheduler = self.scheduler.clone();
                info!(path = %sock_path.display(), "IPC socket listening");
                tokio::spawn(async move {
                    Self::socket_accept_loop(
                        listener,
                        ipc_ctx,
                        dispatch_es,
                        trigger_store,
                        agent_registry,
                        message_router,
                        event_buffer,
                        running,
                        readiness,
                        default_provider,
                        default_model,
                        session_manager,
                        event_broadcaster,
                        scheduler,
                    )
                    .await;
                });
            }
            Err(e) => {
                warn!(error = %e, path = %sock_path.display(), "failed to bind IPC socket");
            }
        }
    }

    #[cfg(not(unix))]
    fn spawn_ipc_listener(&self) {
        // IPC over Unix sockets is not supported on non-unix platforms.
    }

    /// Load persisted state (dispatch bus, cost ledger) from disk.
    async fn load_persisted_state(&self) {
        match self.event_store.load_dispatches().await {
            Ok(n) if n > 0 => info!(count = n, "loaded persisted dispatches"),
            Ok(_) => {}
            Err(e) => warn!(error = %e, "failed to load dispatch bus"),
        }
        // Cost entries are now stored in EventStore (SQLite) — no JSONL load needed.
    }

    /// Run one patrol iteration: triggers, config reload, persistence, metrics, pruning.
    async fn run_patrol_iteration(&mut self) {
        // 1. Patrol cycle: unified scheduler handles reap -> query -> spawn.
        if let Err(e) = self.scheduler.schedule().await {
            warn!(error = %e, "scheduler cycle failed");
        }

        // 1b. Consume dispatches for all active agents.
        self.consume_agent_dispatches().await;

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
            self.apply_config_reload().await;
        }

        // 4. Periodic persistence: save dispatch bus every patrol.
        //    Cost entries are persisted automatically via EventStore (SQLite).
        if let Err(e) = self.event_store.save_dispatches().await {
            warn!(error = %e, "failed to save dispatch bus");
        }

        // 5. Surface dispatch retries / dead letters for critical dispatches.
        let retried = self.event_store.retry_unacked(ACK_RETRY_AGE_SECS).await;
        for dispatch in &retried {
            warn!(
                to = %dispatch.to,
                subject = %dispatch.kind.subject_tag(),
                retry = dispatch.retry_count,
                "retrying unacknowledged dispatch"
            );
        }
        self.metrics.dispatch_retries.inc_by(retried.len() as u64);
        let dead_letters = self.event_store.dead_letters().await;
        for dispatch in &dead_letters {
            warn!(
                to = %dispatch.to,
                subject = %dispatch.kind.subject_tag(),
                retries = dispatch.retry_count,
                "dispatch moved to dead-letter state"
            );
        }

        // 6. Update daily cost gauge and dispatch health metrics.
        let spent = self.event_store.daily_cost().await.unwrap_or(0.0);
        self.metrics.daily_cost_usd.set(spent);
        let dispatch_health = self.event_store.dispatch_health(ACK_RETRY_AGE_SECS).await;
        self.metrics
            .dispatch_queue_depth
            .set(dispatch_health.unread as f64);
        self.metrics
            .dispatches_awaiting_ack
            .set(dispatch_health.awaiting_ack as f64);
        self.metrics
            .dispatches_overdue_ack
            .set(dispatch_health.overdue_ack as f64);
        self.metrics
            .dispatch_dead_letters
            .set(dispatch_health.dead_letters as f64);

        // 7. Prune old cost events (older than 7 days).
        let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
        if let Err(e) = self.event_store.prune("cost", &cutoff).await {
            warn!(error = %e, "failed to prune old cost events");
        }

        // 8. Flush debounced memory writes to project memory stores.
        self.flush_debounced_writes().await;

        // 10. Reap dead sessions (agent loops that exited on their own).
        self.session_manager.reap_dead().await;
    }

    /// Handle SIGHUP config reload: apply budgets, patrol interval.
    async fn apply_config_reload(&mut self) {
        info!("config reload requested (SIGHUP received)");
        match aeqi_core::config::AEQIConfig::discover() {
            Ok((new_config, path)) => {
                self.daily_budget_usd = new_config.security.max_cost_per_day_usd;

                for pcfg in &new_config.agent_spawns {
                    if let Some(budget) = pcfg.max_cost_per_day_usd {
                        self.project_budgets.insert(pcfg.name.clone(), budget);
                    }
                }

                if let Some(interval) = new_config.aeqi.patrol_interval_secs {
                    self.patrol_interval_secs = interval;
                }

                info!(path = %path.display(), "config reloaded and applied via SIGHUP");
            }
            Err(e) => {
                warn!(error = %e, "failed to reload config, keeping current");
            }
        }
    }

    /// Drain the debounced write queue and persist entries to project memory stores.
    async fn flush_debounced_writes(&self) {
        let ready = match self.write_queue.lock() {
            Ok(mut wq) => wq.drain_ready(chrono::Utc::now()),
            Err(_) => Vec::new(),
        };
        if ready.is_empty() {
            return;
        }

        info!(count = ready.len(), "flushing debounced memory writes");
        let Some(ref engine) = self.message_router else {
            return;
        };
        for w in &ready {
            if let Some(mem) = engine.insight_store.as_ref() {
                let category = match w.category.as_str() {
                    "fact" => aeqi_core::traits::InsightCategory::Fact,
                    "procedure" => aeqi_core::traits::InsightCategory::Procedure,
                    "preference" => aeqi_core::traits::InsightCategory::Preference,
                    "context" => aeqi_core::traits::InsightCategory::Context,
                    _ => aeqi_core::traits::InsightCategory::Fact,
                };
                match mem.store(&w.key, &w.content, category, None).await {
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
                    "no insight store available — write dropped"
                );
            }
        }
    }

    /// The main patrol loop: runs until shutdown signal received.
    async fn run_patrol_loop(&mut self) {
        while self.running.load(std::sync::atomic::Ordering::SeqCst) {
            self.run_patrol_iteration().await;

            let wake = self.scheduler.wake.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(self.patrol_interval_secs)) => {},
                _ = wake.notified() => {
                    debug!("woken by scheduler");
                },
                _ = self.shutdown_notify.notified() => break,
            }
        }
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
        ipc_ctx: Arc<IpcContext>,
        dispatch_es: Arc<EventStore>,
        trigger_store: Option<Arc<TriggerStore>>,
        agent_registry: Arc<AgentRegistry>,
        message_router: Option<Arc<MessageRouter>>,
        event_buffer: Arc<Mutex<EventBuffer>>,
        running: Arc<std::sync::atomic::AtomicBool>,
        readiness: ReadinessContext,
        default_provider: Option<Arc<dyn aeqi_core::traits::Provider>>,
        default_model: String,
        session_manager: Arc<SessionManager>,
        event_broadcaster: Arc<EventBroadcaster>,
        scheduler: Arc<Scheduler>,
    ) {
        loop {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            match listener.accept().await {
                Ok((stream, _)) => {
                    let ipc_ctx = ipc_ctx.clone();
                    let dispatch_es = dispatch_es.clone();
                    let trigger_store = trigger_store.clone();
                    let agent_registry = agent_registry.clone();
                    let message_router = message_router.clone();
                    let event_buffer = event_buffer.clone();
                    let readiness = readiness.clone();
                    let default_provider = default_provider.clone();
                    let default_model = default_model.clone();
                    let session_manager = session_manager.clone();
                    let event_broadcaster = event_broadcaster.clone();
                    let scheduler = scheduler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_socket_connection(
                            stream,
                            ipc_ctx,
                            dispatch_es,
                            trigger_store,
                            agent_registry,
                            message_router,
                            event_buffer,
                            readiness,
                            default_provider,
                            default_model,
                            session_manager,
                            event_broadcaster,
                            scheduler,
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
        ipc_ctx: Arc<IpcContext>,
        dispatch_es: Arc<EventStore>,
        trigger_store: Option<Arc<TriggerStore>>,
        agent_registry: Arc<AgentRegistry>,
        message_router: Option<Arc<MessageRouter>>,
        event_buffer: Arc<Mutex<EventBuffer>>,
        readiness: ReadinessContext,
        default_provider: Option<Arc<dyn aeqi_core::traits::Provider>>,
        default_model: String,
        session_manager: Arc<SessionManager>,
        _event_broadcaster: Arc<EventBroadcaster>,
        scheduler: Arc<Scheduler>,
    ) -> Result<()> {
        const MAX_IPC_LINE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            let n = buf_reader.read_line(&mut line).await?;
            if n == 0 {
                break; // EOF
            }
            if n > MAX_IPC_LINE_BYTES {
                let resp = serde_json::json!({"ok": false, "error": "request too large"});
                writer.write_all(resp.to_string().as_bytes()).await?;
                writer.write_all(b"\n").await?;
                continue;
            }
            let line = line.trim_end();
            let request: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|_| serde_json::json!({"cmd": "unknown"}));

            let cmd = request
                .get("cmd")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let response = match cmd {
                "ping" => serde_json::json!({"ok": true, "pong": true}),

                "status" => {
                    let project_names: Vec<String> = agent_registry
                        .list_active()
                        .await
                        .map(|agents| agents.iter().map(|a| a.name.clone()).collect())
                        .unwrap_or_default();
                    let worker_count = scheduler.config.max_workers;
                    let dispatch_health = dispatch_es.dispatch_health(ACK_RETRY_AGE_SECS).await;
                    let mail_count = dispatch_health.unread;
                    let trigger_count = if let Some(ref ts) = trigger_store {
                        ts.count_enabled().await.unwrap_or(0)
                    } else {
                        0
                    };

                    let spent = ipc_ctx.event_store.daily_cost().await.unwrap_or(0.0);
                    let budget = ipc_ctx.daily_budget_usd;
                    let remaining = (budget - spent).max(0.0);
                    let project_costs = ipc_ctx
                        .event_store
                        .daily_costs_by_project()
                        .await
                        .unwrap_or_default();
                    let project_budget_info: serde_json::Map<String, serde_json::Value> = {
                        let mut all_projects: std::collections::HashSet<String> =
                            ipc_ctx.project_budgets.keys().cloned().collect();
                        all_projects.extend(project_costs.keys().cloned());
                        all_projects
                            .into_iter()
                            .map(|name| {
                                let p_spent = project_costs.get(&name).copied().unwrap_or(0.0);
                                let p_budget = ipc_ctx
                                    .project_budgets
                                    .get(&name)
                                    .copied()
                                    .unwrap_or(budget);
                                let p_remaining = (p_budget - p_spent).max(0.0);
                                (
                                    name,
                                    serde_json::json!({
                                        "spent_usd": p_spent,
                                        "budget_usd": p_budget,
                                        "remaining_usd": p_remaining,
                                    }),
                                )
                            })
                            .collect()
                    };

                    let active = scheduler.active_count().await;
                    let agent_counts = scheduler.agent_counts().await;
                    let workers = scheduler.worker_status().await;

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
                        "scheduler_active": true,
                        "scheduler_active_workers": active,
                        "scheduler_agent_counts": agent_counts,
                        "scheduler_workers": workers,
                    })
                }

                "readiness" => {
                    // Build worker limits from agent_registry: each active agent gets max_workers from scheduler config.
                    let worker_limits: Vec<(String, u32)> = agent_registry
                        .list_active()
                        .await
                        .map(|agents| {
                            agents
                                .iter()
                                .map(|a| (a.name.clone(), scheduler.config.max_workers))
                                .collect()
                        })
                        .unwrap_or_default();
                    let dispatch_health = dispatch_es.dispatch_health(ACK_RETRY_AGE_SECS).await;
                    let spent = ipc_ctx.event_store.daily_cost().await.unwrap_or(0.0);
                    let budget = ipc_ctx.daily_budget_usd;
                    let remaining = (budget - spent).max(0.0);
                    readiness_response(
                        &ipc_ctx.leader_agent_name,
                        worker_limits,
                        dispatch_health,
                        (spent, budget, remaining),
                        &readiness,
                    )
                }

                "worker_progress" => {
                    let workers = scheduler.worker_status().await;
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

                "companies" => {
                    // List agents and their task counts from agent_registry.
                    let agents = agent_registry.list_active().await.unwrap_or_default();
                    let mut projects: Vec<serde_json::Value> = Vec::new();
                    for agent in &agents {
                        let task_counts = agent_registry
                            .list_tasks(None, Some(&agent.id))
                            .await
                            .map(|tasks| {
                                let total = tasks.len();
                                let open = tasks.iter().filter(|t| !t.is_closed()).count();
                                let pending = tasks
                                    .iter()
                                    .filter(|t| t.status == aeqi_quests::QuestStatus::Pending)
                                    .count();
                                let in_progress = tasks
                                    .iter()
                                    .filter(|t| t.status == aeqi_quests::QuestStatus::InProgress)
                                    .count();
                                let done = tasks
                                    .iter()
                                    .filter(|t| t.status == aeqi_quests::QuestStatus::Done)
                                    .count();
                                let cancelled = tasks
                                    .iter()
                                    .filter(|t| t.status == aeqi_quests::QuestStatus::Cancelled)
                                    .count();
                                (total, open, pending, in_progress, done, cancelled)
                            })
                            .unwrap_or_default();
                        projects.push(serde_json::json!({
                            "name": agent.name,
                            "prefix": "",
                            "open_tasks": task_counts.1,
                            "total_tasks": task_counts.0,
                            "pending_tasks": task_counts.2,
                            "in_progress_tasks": task_counts.3,
                            "done_tasks": task_counts.4,
                            "cancelled_tasks": task_counts.5,
                            "departments": [],
                        }));
                    }
                    serde_json::json!({"ok": true, "projects": projects})
                }

                "mail" => {
                    let messages = dispatch_es.drain();
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
                    let mut dispatches = dispatch_es.all().await;
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
                    let health = dispatch_es.dispatch_health(ACK_RETRY_AGE_SECS).await;
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
                    let text = ipc_ctx.metrics.render();
                    serde_json::json!({"ok": true, "metrics": text})
                }

                "cost" => {
                    let spent = ipc_ctx.event_store.daily_cost().await.unwrap_or(0.0);
                    let budget = ipc_ctx.daily_budget_usd;
                    let remaining = (budget - spent).max(0.0);
                    let report = ipc_ctx
                        .event_store
                        .daily_costs_by_project()
                        .await
                        .unwrap_or_default();
                    let project_budget_info: serde_json::Map<String, serde_json::Value> = {
                        let mut all_projects: std::collections::HashSet<String> =
                            ipc_ctx.project_budgets.keys().cloned().collect();
                        all_projects.extend(report.keys().cloned());
                        all_projects
                            .into_iter()
                            .map(|name| {
                                let p_spent = report.get(&name).copied().unwrap_or(0.0);
                                let p_budget = ipc_ctx
                                    .project_budgets
                                    .get(&name)
                                    .copied()
                                    .unwrap_or(budget);
                                let p_remaining = (p_budget - p_spent).max(0.0);
                                (
                                    name,
                                    serde_json::json!({
                                        "spent_usd": p_spent,
                                        "budget_usd": p_budget,
                                        "remaining_usd": p_remaining,
                                    }),
                                )
                            })
                            .collect()
                    };
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
                    let task_filter = request
                        .get("task_id")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let last = request.get("last").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
                    let filter = crate::event_store::EventFilter {
                        event_type: Some("decision".to_string()),
                        quest_id: task_filter,
                        ..Default::default()
                    };
                    match ipc_ctx.event_store.query(&filter, last, 0).await {
                        Ok(events) => {
                            let items: Vec<serde_json::Value> = events
                                .iter()
                                .map(|e| {
                                    serde_json::json!({
                                        "timestamp": e.created_at.to_rfc3339(),
                                        "decision_type": e.content.get("decision_type").and_then(|v| v.as_str()).unwrap_or(""),
                                        "quest_id": e.quest_id,
                                        "agent": e.content.get("agent").and_then(|v| v.as_str()).unwrap_or(""),
                                        "reasoning": e.content.get("reasoning").and_then(|v| v.as_str()).unwrap_or(""),
                                    })
                                })
                                .collect();
                            serde_json::json!({"ok": true, "events": items})
                        }
                        Err(e) => {
                            serde_json::json!({"ok": false, "error": e.to_string()})
                        }
                    }
                }

                // Notes commands — backed by insight store (post/query/get/delete)
                // and agent_registry quests (claim/release/check_claim).
                "notes" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*");
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

                    if let Some(ref engine) = message_router {
                        if let Some(mem) = engine.insight_store.as_ref() {
                            let query_text = request
                                .get("tags")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .unwrap_or_default();
                            let search_text = if query_text.is_empty() {
                                "*".to_string()
                            } else {
                                query_text
                            };
                            let q = aeqi_core::traits::InsightQuery::new(&search_text, limit);
                            match mem.search(&q).await {
                                Ok(entries) => {
                                    let items: Vec<serde_json::Value> = entries
                                        .iter()
                                        .map(|e| {
                                            serde_json::json!({
                                                "key": e.key,
                                                "content": e.content,
                                                "agent": e.agent_id.as_deref().unwrap_or("system"),
                                                "project": project,
                                                "tags": [],
                                                "created_at": e.created_at.to_rfc3339(),
                                            })
                                        })
                                        .collect();
                                    serde_json::json!({"ok": true, "entries": items})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": true, "entries": []})
                        }
                    } else {
                        serde_json::json!({"ok": true, "entries": []})
                    }
                }

                "get_notes" => {
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let key = request.get("key").and_then(|v| v.as_str()).unwrap_or("");

                    if let Some(ref engine) = message_router {
                        if let Some(mem) = engine.insight_store.as_ref() {
                            let q = aeqi_core::traits::InsightQuery::new(key, 1);
                            match mem.search(&q).await {
                                Ok(entries) => {
                                    if let Some(e) = entries.into_iter().find(|e| e.key == key) {
                                        serde_json::json!({
                                            "ok": true,
                                            "entry": {
                                                "key": e.key,
                                                "content": e.content,
                                                "agent": e.agent_id.as_deref().unwrap_or("system"),
                                                "project": project,
                                                "tags": [],
                                                "created_at": e.created_at.to_rfc3339(),
                                            }
                                        })
                                    } else {
                                        serde_json::json!({"ok": true, "entry": null})
                                    }
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": true, "entry": null})
                        }
                    } else {
                        serde_json::json!({"ok": true, "entry": null})
                    }
                }

                "claim_notes" => {
                    // Claims are quests. Create a quest with label "claim:{resource}".
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
                        let claim_label = format!("claim:{resource}");
                        // Check for existing in-progress claim quest.
                        let existing = agent_registry
                            .list_tasks(Some("in_progress"), None)
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .find(|t| t.labels.contains(&claim_label));
                        match existing {
                            Some(task) => {
                                let holder = task.assignee.as_deref().unwrap_or("unknown");
                                if holder == agent {
                                    // Same agent — renew (no-op, quest already active).
                                    serde_json::json!({"ok": true, "result": "renewed", "resource": resource})
                                } else {
                                    serde_json::json!({"ok": true, "result": "held", "holder": holder, "content": task.description})
                                }
                            }
                            None => {
                                // Resolve agent_id from name. Try to find the agent.
                                let agent_id = agent_registry
                                    .resolve_by_hint(agent)
                                    .await
                                    .ok()
                                    .flatten()
                                    .map(|a| a.name.clone())
                                    .unwrap_or_else(|| agent.to_string());
                                match agent_registry
                                    .create_task(
                                        &agent_id,
                                        &format!("claim: {resource}"),
                                        content,
                                        None,
                                        &[claim_label],
                                    )
                                    .await
                                {
                                    Ok(task) => {
                                        // Immediately mark in_progress.
                                        let _ = agent_registry
                                            .update_task_status(
                                                &task.id.0,
                                                aeqi_quests::QuestStatus::InProgress,
                                            )
                                            .await;
                                        serde_json::json!({"ok": true, "result": "acquired", "resource": resource})
                                    }
                                    Err(e) => {
                                        serde_json::json!({"ok": false, "error": e.to_string()})
                                    }
                                }
                            }
                        }
                    }
                }

                "release_notes" => {
                    // Release = close the claim quest.
                    let resource = request
                        .get("resource")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let _agent = request
                        .get("agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("worker");
                    let _force = request
                        .get("force")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let claim_label = format!("claim:{resource}");
                    let existing = agent_registry
                        .list_tasks(Some("in_progress"), None)
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .find(|t| t.labels.contains(&claim_label));
                    match existing {
                        Some(task) => {
                            match agent_registry
                                .update_task_status(&task.id.0, aeqi_quests::QuestStatus::Done)
                                .await
                            {
                                Ok(()) => serde_json::json!({"ok": true, "released": true}),
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        }
                        None => {
                            serde_json::json!({"ok": true, "released": false, "reason": "not found or not owned"})
                        }
                    }
                }

                "delete_notes" => {
                    let _project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let key = request.get("key").and_then(|v| v.as_str()).unwrap_or("");

                    if let Some(ref engine) = message_router {
                        if let Some(mem) = engine.insight_store.as_ref() {
                            // Search for the insight by key, then delete by id.
                            let q = aeqi_core::traits::InsightQuery::new(key, 5);
                            match mem.search(&q).await {
                                Ok(entries) => {
                                    let mut deleted = false;
                                    for e in &entries {
                                        if e.key == key {
                                            let _ = mem.delete(&e.id).await;
                                            deleted = true;
                                        }
                                    }
                                    serde_json::json!({"ok": true, "deleted": deleted})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": true, "deleted": false})
                        }
                    } else {
                        serde_json::json!({"ok": true, "deleted": false})
                    }
                }

                "check_claim" => {
                    let resource = request
                        .get("resource")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let claim_label = format!("claim:{resource}");
                    let existing = agent_registry
                        .list_tasks(Some("in_progress"), None)
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .find(|t| t.labels.contains(&claim_label));
                    match existing {
                        Some(task) => {
                            let agent = task.assignee.as_deref().unwrap_or("unknown");
                            serde_json::json!({"ok": true, "claimed": true, "agent": agent, "content": task.description})
                        }
                        None => serde_json::json!({"ok": true, "claimed": false}),
                    }
                }

                "expertise" => match ipc_ctx.event_store.query_expertise().await {
                    Ok(scores) => {
                        serde_json::json!({"ok": true, "scores": scores})
                    }
                    Err(e) => {
                        serde_json::json!({"ok": false, "error": e.to_string()})
                    }
                },

                "tasks" => {
                    let project_filter = request.get("project").and_then(|v| v.as_str());
                    let status_filter = request.get("status").and_then(|v| v.as_str());

                    // AgentRegistry path: unified task store.
                    let agent_filter = request.get("agent_id").and_then(|v| v.as_str());
                    // If project filter provided, try to resolve to an agent_id.
                    let resolved_agent = if agent_filter.is_some() {
                        agent_filter.map(|s| s.to_string())
                    } else if let Some(proj) = project_filter {
                        agent_registry
                            .resolve_by_hint(proj)
                            .await
                            .ok()
                            .flatten()
                            .map(|a| a.id)
                    } else {
                        None
                    };
                    match agent_registry
                        .list_tasks(status_filter, resolved_agent.as_deref())
                        .await
                    {
                        Ok(tasks) => {
                            let all_tasks: Vec<serde_json::Value> = tasks
                                .iter()
                                .map(|task| {
                                    serde_json::json!({
                                        "id": task.id.0,
                                        "subject": task.name,
                                        "description": task.description,
                                        "status": task.status.to_string(),
                                        "priority": task.priority.to_string(),
                                        "assignee": task.assignee,
                                        "agent_id": task.agent_id,
                                        "skill": task.skill,
                                        "labels": task.labels,
                                        "retry_count": task.retry_count,
                                        "project": task.agent_id.as_deref().unwrap_or(""),
                                        "created_at": task.created_at.to_rfc3339(),
                                        "updated_at": task.updated_at.map(|t| t.to_rfc3339()),
                                        "closed_at": task.closed_at.map(|t| t.to_rfc3339()),
                                        "closed_reason": task.closed_reason,
                                        "runtime": task.runtime(),
                                        "task_outcome": task.task_outcome(),
                                    })
                                })
                                .collect();
                            serde_json::json!({"ok": true, "tasks": all_tasks, "partial": false})
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                    }
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
                    let explicit_agent_id = request.get("agent_id").and_then(|v| v.as_str());

                    if project.is_empty() || subject.is_empty() {
                        serde_json::json!({"ok": false, "error": "project and subject are required"})
                    } else {
                        // AgentRegistry path: resolve agent, then create task in unified store.
                        let agent = if let Some(aid) = explicit_agent_id {
                            agent_registry.resolve_by_hint(aid).await.ok().flatten()
                        } else {
                            agent_registry
                                .default_agent(Some(project))
                                .await
                                .ok()
                                .flatten()
                        };
                        match agent {
                            Some(agent) => {
                                let skill = request.get("skill").and_then(|v| v.as_str());
                                let labels: Vec<String> = request
                                    .get("labels")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                match agent_registry
                                    .create_task(&agent.id, subject, description, skill, &labels)
                                    .await
                                {
                                    Ok(task) => {
                                        scheduler.wake.notify_one();
                                        serde_json::json!({
                                            "ok": true,
                                            "task": {
                                                "id": task.id.0,
                                                "subject": task.name,
                                                "status": task.status.to_string(),
                                                "agent_id": task.agent_id,
                                                "project": project,
                                            }
                                        })
                                    }
                                    Err(e) => {
                                        serde_json::json!({"ok": false, "error": e.to_string()})
                                    }
                                }
                            }
                            None => {
                                serde_json::json!({"ok": false, "error": "no agent found for project"})
                            }
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

                    if task_id.is_empty() {
                        serde_json::json!({"ok": false, "error": "task_id is required"})
                    } else {
                        // AgentRegistry path: update status to Done via unified store.
                        match agent_registry
                            .update_task(task_id, |task| {
                                task.status = aeqi_quests::QuestStatus::Done;
                                task.closed_at = Some(chrono::Utc::now());
                                task.closed_reason = Some(reason.to_string());
                            })
                            .await
                        {
                            Ok(task) => serde_json::json!({
                                "ok": true,
                                "task": {
                                    "id": task.id.0,
                                    "status": task.status.to_string(),
                                    "closed_reason": task.closed_reason,
                                    "runtime": task.runtime(),
                                    "task_outcome": task.task_outcome(),
                                }
                            }),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                "post_notes" => {
                    let key = request.get("key").and_then(|v| v.as_str()).unwrap_or("");
                    let content = request
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if key.is_empty() || content.is_empty() {
                        serde_json::json!({"ok": false, "error": "key and content are required"})
                    } else if let Some(ref engine) = message_router {
                        if let Some(mem) = engine.insight_store.as_ref() {
                            match mem
                                .store(key, content, aeqi_core::traits::InsightCategory::Fact, None)
                                .await
                            {
                                Ok(id) => {
                                    serde_json::json!({"ok": true, "entry": {"id": id, "key": key}})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": false, "error": format!("no insight store for project: {project}")})
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "insight stores not initialized"})
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

                    match &message_router {
                        Some(engine) => {
                            let chat_id = resolve_web_chat_id(
                                request.get("chat_id").and_then(|v| v.as_i64()),
                                project_hint,
                                department_hint,
                                channel_name,
                            );

                            let msg = IncomingMessage {
                                message: message.to_string(),
                                chat_id,
                                sender: sender.to_string(),
                                source: MessageSource::Web,
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

                    match &message_router {
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

                                let msg = IncomingMessage {
                                    message: message.to_string(),
                                    chat_id,
                                    sender: sender.to_string(),
                                    source: MessageSource::Web,
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
                    match &message_router {
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
                    let agent_id_param = request_field(&request, "agent_id").map(|s| s.to_string());

                    // If agent_id is provided, look up timeline (messages + tool events) by agent UUID.
                    if let Some(ref aid) = agent_id_param {
                        if let Some(ref ss) = ipc_ctx.session_store {
                            match ss.get_timeline_by_agent_id(aid, limit).await {
                                Ok(events) => {
                                    let msgs: Vec<serde_json::Value> = events
                                        .iter()
                                        .map(|e| {
                                            let mut obj = serde_json::json!({
                                                "role": e.role,
                                                "content": e.content,
                                                "timestamp": e.timestamp.to_rfc3339(),
                                                "source": e.source,
                                                "event_type": e.event_type,
                                            });
                                            if let Some(ref meta) = e.metadata {
                                                obj["metadata"] = meta.clone();
                                            }
                                            obj
                                        })
                                        .collect();
                                    serde_json::json!({"ok": true, "messages": msgs})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": false, "error": "session store not initialized"})
                        }
                    } else {
                        match &message_router {
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
                                    Err(e) => {
                                        serde_json::json!({"ok": false, "error": e.to_string()})
                                    }
                                }
                            }
                            None => {
                                serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                            }
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

                    match &message_router {
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
                                                find_task_snapshot(&agent_registry, task_id).await
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

                "chat_channels" => match &message_router {
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
                                let mut item = serde_json::json!({
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
                                });
                                if let crate::trigger::TriggerType::Webhook {
                                    public_id,
                                    signing_secret,
                                } = &t.trigger_type
                                {
                                    item["public_id"] = serde_json::json!(public_id);
                                    item["has_signing_secret"] =
                                        serde_json::json!(signing_secret.is_some());
                                }
                                item
                            })
                            .collect();
                        serde_json::json!({"ok": true, "triggers": items})
                    }
                    None => serde_json::json!({"ok": true, "triggers": []}),
                },

                "webhook_fire" => {
                    let public_id = request
                        .get("public_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let signature = request
                        .get("signature")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let body_b64 = request
                        .get("body_b64")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if public_id.is_empty() {
                        serde_json::json!({"ok": false, "error": "public_id is required"})
                    } else {
                        match &trigger_store {
                            Some(store) => {
                                match store.find_by_public_id(public_id).await {
                                    Ok(Some(trigger)) => {
                                        // Verify HMAC signature if signing_secret is set.
                                        let sig_error =
                                            if let crate::trigger::TriggerType::Webhook {
                                                signing_secret: Some(secret),
                                                ..
                                            } = &trigger.trigger_type
                                            {
                                                let raw_body = base64::Engine::decode(
                                                    &base64::engine::general_purpose::STANDARD,
                                                    body_b64,
                                                )
                                                .unwrap_or_default();
                                                match &signature {
                                                    Some(sig) => {
                                                        if !crate::trigger::verify_webhook_signature(
                                                            secret, &raw_body, sig,
                                                        ) {
                                                            Some(
                                                                serde_json::json!({"ok": false, "error": "invalid signature"}),
                                                            )
                                                        } else {
                                                            None
                                                        }
                                                    }
                                                    None => Some(
                                                        serde_json::json!({"ok": false, "error": "signature required but not provided"}),
                                                    ),
                                                }
                                            } else {
                                                None
                                            };

                                        if let Some(err_resp) = sig_error {
                                            err_resp
                                        } else {
                                            // Look up agent to get parent (project context).
                                            let project =
                                                match agent_registry.get(&trigger.agent_id).await {
                                                    Ok(Some(agent)) => agent
                                                        .parent_id
                                                        .clone()
                                                        .or_else(|| Some(agent.name.clone())),
                                                    _ => None,
                                                };

                                            match project {
                                                Some(_project) => {
                                                    // Advance before execute.
                                                    let _ = store
                                                        .advance_before_execute(&trigger.id)
                                                        .await;

                                                    let subject = format!(
                                                        "[webhook:{}] {}",
                                                        trigger.name, trigger.skill
                                                    );
                                                    let description = format!(
                                                        "Webhook '{}' fired. Run skill '{}' for agent {}.",
                                                        trigger.name,
                                                        trigger.skill,
                                                        trigger.agent_id
                                                    );

                                                    match agent_registry
                                                        .create_task(
                                                            &trigger.agent_id,
                                                            &subject,
                                                            &description,
                                                            Some(&trigger.skill),
                                                            &[],
                                                        )
                                                        .await
                                                    {
                                                        Ok(task) => {
                                                            scheduler.wake.notify_one();
                                                            let _ = store
                                                                .record_fire(&trigger.id, 0.0)
                                                                .await;
                                                            serde_json::json!({
                                                                "ok": true,
                                                                "task_id": task.id
                                                            })
                                                        }
                                                        Err(e) => {
                                                            serde_json::json!({"ok": false, "error": format!("failed to create task: {e}")})
                                                        }
                                                    }
                                                }
                                                None => {
                                                    serde_json::json!({"ok": false, "error": "trigger agent has no project scope"})
                                                }
                                            }
                                        } // else (no signature error)
                                    }
                                    Ok(None) => {
                                        serde_json::json!({"ok": false, "error": "webhook not found"})
                                    }
                                    Err(e) => {
                                        serde_json::json!({"ok": false, "error": format!("lookup failed: {e}")})
                                    }
                                }
                            }
                            None => {
                                serde_json::json!({"ok": false, "error": "trigger store not initialized"})
                            }
                        }
                    }
                }

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
                        .join(".aeqi")
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
                    let _project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let query = request.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

                    if let Some(ref engine) = message_router {
                        if let Some(mem) = engine.insight_store.as_ref() {
                            let agent_id_param = request.get("agent_id").and_then(|v| v.as_str());

                            let mut mq = aeqi_core::traits::InsightQuery::new(query, limit);
                            if let Some(aid) = agent_id_param {
                                mq = mq.with_agent(aid);
                            }
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
                                                "agent_id": e.agent_id,
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
                    let _project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    {
                        let aeqi_data_dir = std::env::var("HOME")
                            .map(|h| PathBuf::from(h).join(".aeqi"))
                            .unwrap_or_else(|_| PathBuf::from("/tmp"));
                        let db_path = aeqi_data_dir.join("insights.db");
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
                                     FROM insights \
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
                    let _project = request
                        .get("project")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

                    {
                        let aeqi_data_dir = std::env::var("HOME")
                            .map(|h| PathBuf::from(h).join(".aeqi"))
                            .unwrap_or_else(|_| PathBuf::from("/tmp"));
                        let db_path = aeqi_data_dir.join("insights.db");
                        if !db_path.exists() {
                            serde_json::json!({"ok": true, "nodes": [], "edges": []})
                        } else if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                            let sql = format!(
                                "SELECT id, key, content, category, created_at \
                                 FROM insights \
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

                "company_knowledge" => {
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
                        if let Some(ref engine) = message_router
                            && let Some(mem) = engine.insight_store.as_ref()
                        {
                            let q = if query.is_empty() { project } else { query };
                            let mq = aeqi_core::traits::InsightQuery::new(q, limit);
                            if let Ok(results) = mem.search(&mq).await {
                                for entry in results {
                                    items.push(serde_json::json!({
                                        "id": entry.id,
                                        "key": entry.key,
                                        "content": entry.content,
                                        "category": format!("{:?}", entry.category).to_lowercase(),
                                        "agent_id": entry.agent_id,
                                        "source": "memory",
                                        "created_at": entry.created_at.to_rfc3339(),
                                        "project": project,
                                    }));
                                }
                            }
                        }

                        // 2. Notes are now insights — already included above.

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

                    if project.is_empty() || key.is_empty() || content.is_empty() {
                        serde_json::json!({"ok": false, "error": "project, key, and content required"})
                    } else if let Some(ref engine) = message_router {
                        if let Some(mem) = engine.insight_store.as_ref() {
                            let cat = match category {
                                "procedure" => aeqi_core::traits::InsightCategory::Procedure,
                                "preference" => aeqi_core::traits::InsightCategory::Preference,
                                "context" => aeqi_core::traits::InsightCategory::Context,
                                "evergreen" => aeqi_core::traits::InsightCategory::Evergreen,
                                _ => aeqi_core::traits::InsightCategory::Fact,
                            };
                            let agent_id = request.get("agent_id").and_then(|v| v.as_str());
                            match mem.store(key, content, cat, agent_id).await {
                                Ok(id) => serde_json::json!({"ok": true, "id": id}),
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": false, "error": format!("no insight store available: {project}")})
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
                    } else if let Some(ref engine) = message_router {
                        if let Some(mem) = engine.insight_store.as_ref() {
                            match mem.delete(id).await {
                                Ok(_) => serde_json::json!({"ok": true}),
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"ok": false, "error": "no insight store available"})
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "chat engine not initialized"})
                    }
                }

                // ── Persistent Agent Registry ──
                "agents_registry" => {
                    let parent_id = request.get("parent_id").and_then(|v| v.as_str());
                    let parent_filter: Option<Option<&str>> = if request.get("parent_id").is_some()
                    {
                        Some(parent_id)
                    } else {
                        None
                    };
                    let status_filter = request.get("status").and_then(|v| v.as_str());
                    let status = status_filter.and_then(|s| match s {
                        "active" => Some(crate::agent_registry::AgentStatus::Active),
                        "paused" => Some(crate::agent_registry::AgentStatus::Paused),
                        "retired" => Some(crate::agent_registry::AgentStatus::Retired),
                        _ => None,
                    });
                    match agent_registry.list(parent_filter, status).await {
                        Ok(agents) => {
                            let items: Vec<serde_json::Value> = agents
                                .iter()
                                .map(|a| {
                                    serde_json::json!({
                                        "id": a.id,
                                        "name": a.name,
                                        "display_name": a.display_name,
                                        "template": a.template,
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
                                        "session_id": a.session_id,
                                    })
                                })
                                .collect();
                            serde_json::json!({"ok": true, "agents": items})
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                    }
                }

                "agent_children" => {
                    let agent_id = request
                        .get("agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if agent_id.is_empty() {
                        serde_json::json!({"ok": false, "error": "agent_id is required"})
                    } else {
                        match agent_registry.get_children(agent_id).await {
                            Ok(children) => {
                                let items: Vec<serde_json::Value> = children
                                    .iter()
                                    .map(|a| {
                                        serde_json::json!({
                                            "id": a.id,
                                            "name": a.name,
                                            "display_name": a.display_name,
                                            "template": a.template,
                                            "parent_id": a.parent_id,
                                            "model": a.model,
                                            "status": a.status,
                                            "created_at": a.created_at.to_rfc3339(),
                                        })
                                    })
                                    .collect();
                                serde_json::json!({"ok": true, "children": items})
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                "agent_spawn" => {
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
                                match agent_registry.spawn_from_template(&content, project).await {
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

                "agent_set_status" => {
                    let name = request.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let status_str = request.get("status").and_then(|v| v.as_str()).unwrap_or("");
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
                            Some(s) => match agent_registry.set_status(name, s).await {
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

                "agent_info" => {
                    let name = request.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() {
                        serde_json::json!({"ok": false, "error": "name is required"})
                    } else {
                        match agent_registry.get_active_by_name(name).await {
                            Ok(Some(agent)) => serde_json::json!({
                                "ok": true,
                                "id": agent.id,
                                "name": agent.name,
                                "display_name": agent.display_name,
                                "template": agent.template,
                                "system_prompt": agent.system_prompt,
                                "parent_id": agent.parent_id,
                                "model": agent.model,
                                "capabilities": agent.capabilities,
                                "status": agent.status,
                            }),
                            Ok(None) => {
                                serde_json::json!({"ok": false, "error": format!("agent '{}' not found", name)})
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                // ── Budget Policies ──
                "budget_policies" => match agent_registry.list_budget_policies().await {
                    Ok(policies) => serde_json::json!({"ok": true, "policies": policies}),
                    Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                },

                "create_budget_policy" => {
                    let agent_id = request_field(&request, "agent_id").unwrap_or("");
                    let window = request_field(&request, "window").unwrap_or("");
                    let amount_usd = request
                        .get("amount_usd")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    if agent_id.is_empty() || window.is_empty() || amount_usd <= 0.0 {
                        serde_json::json!({"ok": false, "error": "agent_id, window, and positive amount_usd are required"})
                    } else {
                        match agent_registry
                            .create_budget_policy(agent_id, window, amount_usd)
                            .await
                        {
                            Ok(id) => serde_json::json!({"ok": true, "id": id}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                // ── Approval Queue ──
                "approvals" => {
                    let status = request_field(&request, "status");
                    match agent_registry.list_approvals(status).await {
                        Ok(approvals) => {
                            serde_json::json!({"ok": true, "approvals": approvals})
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                    }
                }

                "resolve_approval" => {
                    let approval_id = request_field(&request, "approval_id").unwrap_or("");
                    let status = request_field(&request, "status").unwrap_or("");
                    let decided_by = request_field(&request, "decided_by").unwrap_or("");
                    let note = request_field(&request, "note");

                    if approval_id.is_empty() || status.is_empty() || decided_by.is_empty() {
                        serde_json::json!({"ok": false, "error": "approval_id, status, and decided_by are required"})
                    } else {
                        match agent_registry
                            .resolve_approval(approval_id, status, decided_by, note)
                            .await
                        {
                            Ok(()) => serde_json::json!({"ok": true}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                // ── Sessions ──
                "list_sessions" => {
                    if let Some(ref ss) = ipc_ctx.session_store {
                        let hint = request_field(&request, "agent_id").unwrap_or("");
                        if hint.is_empty() {
                            serde_json::json!({"ok": false, "error": "agent_id is required"})
                        } else {
                            // Resolve hint to agent UUID if needed.
                            let resolved_id = if hint.len() == 36 && hint.contains('-') {
                                hint.to_string()
                            } else {
                                match agent_registry.resolve_by_hint(hint).await {
                                    Ok(Some(agent)) => agent.id,
                                    _ => hint.to_string(),
                                }
                            };
                            match ss.list_sessions(Some(&resolved_id), 100).await {
                                Ok(sessions) => {
                                    serde_json::json!({"ok": true, "sessions": sessions})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "session store not available"})
                    }
                }

                // ── Sessions ──
                "sessions" => {
                    let agent_id = request_field(&request, "agent_id").map(|s| s.to_string());
                    let limit =
                        request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

                    if let Some(ref ss) = ipc_ctx.session_store {
                        match ss.list_sessions(agent_id.as_deref(), limit).await {
                            Ok(sessions) => {
                                serde_json::json!({"ok": true, "sessions": sessions})
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "session store not available"})
                    }
                }

                "create_session" => {
                    if let Some(ref ss) = ipc_ctx.session_store {
                        let agent_id = request_field(&request, "agent_id").unwrap_or("");
                        if agent_id.is_empty() {
                            serde_json::json!({"ok": false, "error": "agent_id is required"})
                        } else {
                            match ss
                                .create_session(
                                    agent_id,
                                    "perpetual",
                                    "Permanent Session",
                                    None,
                                    None,
                                )
                                .await
                            {
                                Ok(session_id) => {
                                    serde_json::json!({"ok": true, "session_id": session_id})
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                            }
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "session store not available"})
                    }
                }

                "close_session" => {
                    let session_id = request_field(&request, "session_id").unwrap_or("");
                    if session_id.is_empty() {
                        serde_json::json!({"ok": false, "error": "session_id is required"})
                    } else {
                        // Stop the running session (drops input channel → agent exits).
                        let was_running = session_manager.close(session_id).await;

                        // Close in DB via session_store.
                        let db_closed = if let Some(ref ss) = ipc_ctx.session_store {
                            ss.close_session(session_id).await.is_ok()
                        } else {
                            false
                        };

                        serde_json::json!({
                            "ok": true,
                            "was_running": was_running,
                            "db_closed": db_closed,
                        })
                    }
                }

                "session_messages" => {
                    if let Some(ref ss) = ipc_ctx.session_store {
                        let session_id = request_field(&request, "session_id").unwrap_or("");
                        let limit =
                            request.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
                        // Use timeline_by_session to include tool events alongside messages.
                        match ss.timeline_by_session(session_id, limit).await {
                            Ok(events) => {
                                let msgs: Vec<serde_json::Value> = events
                                    .iter()
                                    .map(|e| {
                                        let mut obj = serde_json::json!({
                                            "role": e.role,
                                            "content": e.content,
                                            "created_at": e.timestamp.to_rfc3339(),
                                            "source": e.source,
                                            "event_type": e.event_type,
                                        });
                                        if let Some(ref meta) = e.metadata {
                                            obj["metadata"] = meta.clone();
                                        }
                                        obj
                                    })
                                    .collect();
                                serde_json::json!({"ok": true, "messages": msgs})
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "session store not available"})
                    }
                }

                "session_children" => {
                    if let Some(ref ss) = ipc_ctx.session_store {
                        let session_id = request_field(&request, "session_id").unwrap_or("");
                        match ss.list_children(session_id).await {
                            Ok(children) => serde_json::json!({"ok": true, "sessions": children}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "session store not available"})
                    }
                }

                "session_send" => {
                    let message = request_field(&request, "message").unwrap_or("");
                    let agent_hint = request_field(&request, "agent")
                        .map(|s| s.to_lowercase())
                        .unwrap_or_else(|| "assistant".to_string());
                    let agent_id_direct =
                        request_field(&request, "agent_id").map(|s| s.to_string());
                    let session_id_hint =
                        request_field(&request, "session_id").map(|s| s.to_string());
                    let stream_mode = request
                        .get("stream")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if message.is_empty() {
                        serde_json::json!({"ok": false, "error": "message is required"})
                    } else {
                        let chat_id = request
                            .get("chat_id")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_else(|| {
                                named_channel_chat_id(
                                    agent_id_direct.as_deref().unwrap_or(&agent_hint),
                                )
                            });

                        let session_store = ipc_ctx.session_store.clone();

                        // Ensure session and record user message.
                        let store_session_id = if let Some(ref cs) = session_store {
                            let _ = cs
                                .ensure_channel_with_agent(
                                    chat_id,
                                    "web",
                                    &agent_hint,
                                    agent_id_direct.as_deref(),
                                )
                                .await;
                            // Get or create a session UUID for this chat_id.
                            let usid = cs
                                .ensure_session(
                                    chat_id,
                                    "web",
                                    &agent_hint,
                                    agent_id_direct.as_deref(),
                                )
                                .await
                                .ok();
                            // Record via session if available, else fall back to legacy.
                            if let Some(ref sid) = usid {
                                let _ = cs
                                    .record_by_session(sid, "user", message, Some("web"))
                                    .await;
                            } else {
                                let _ = cs
                                    .record_with_source(chat_id, "user", message, Some("web"))
                                    .await;
                            }
                            usid
                        } else {
                            None
                        };

                        // Resolve session_id: explicit > agent's permanent session > create new.
                        let resolved_session_id = if let Some(ref sid) = session_id_hint {
                            sid.clone()
                        } else {
                            // Find agent's permanent (first active) session via session_store.
                            // If we have a direct agent UUID, skip resolve_by_hint (saves 2 queries).
                            let agent_uuid = if let Some(ref aid) = agent_id_direct {
                                Some(aid.clone())
                            } else {
                                match agent_registry.resolve_by_hint(&agent_hint).await {
                                    Ok(Some(agent)) => Some(agent.id),
                                    _ => None,
                                }
                            };
                            if let Some(ref uuid) = agent_uuid {
                                if let Some(ref ss) = session_store {
                                    match ss.list_sessions(Some(uuid), 1).await {
                                        Ok(sessions) => sessions
                                            .first()
                                            .filter(|s| s.status == "active")
                                            .map(|s| s.id.clone())
                                            .unwrap_or_default(),
                                        Err(_) => String::new(),
                                    }
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            }
                        };

                        // Check if session is already running in memory.
                        if !resolved_session_id.is_empty()
                            && session_manager.is_running(&resolved_session_id).await
                        {
                            if stream_mode {
                                // Inject message and get a broadcast receiver for streaming.
                                match session_manager
                                    .send_streaming(&resolved_session_id, message)
                                    .await
                                {
                                    Ok(mut rx) => {
                                        // Stream events to the IPC writer.
                                        let mut text = String::new();
                                        let mut iterations = 0u32;
                                        let mut prompt_tokens = 0u32;
                                        let mut completion_tokens = 0u32;

                                        loop {
                                            match tokio::time::timeout(
                                                std::time::Duration::from_secs(300),
                                                rx.recv(),
                                            )
                                            .await
                                            {
                                                Ok(Ok(event)) => {
                                                    // Forward ALL events to IPC (they serialize with #[serde(tag = "type")])
                                                    if let Ok(ev_bytes) = serde_json::to_vec(&event) {
                                                        let mut bytes = ev_bytes;
                                                        bytes.push(b'\n');
                                                        let _ = writer.write_all(&bytes).await;
                                                    }

                                                    // Track text accumulation and completion for recording
                                                    match &event {
                                                        aeqi_core::ChatStreamEvent::TextDelta { text: delta } => {
                                                            text.push_str(delta);
                                                        }
                                                        aeqi_core::ChatStreamEvent::ToolComplete {
                                                            tool_use_id: _,
                                                            tool_name,
                                                            success,
                                                            input_preview,
                                                            output_preview,
                                                            duration_ms,
                                                        } => {
                                                            // Persist tool completion for reload
                                                            if let (Some(cs), Some(usid)) = (&session_store, &store_session_id) {
                                                                let meta = serde_json::json!({
                                                                    "tool_name": tool_name,
                                                                    "success": success,
                                                                    "input_preview": input_preview,
                                                                    "output_preview": output_preview,
                                                                    "duration_ms": duration_ms,
                                                                });
                                                                let _ = cs.record_event_by_session(
                                                                    usid, "tool_complete", "system",
                                                                    tool_name, Some("session"), Some(&meta),
                                                                ).await;
                                                            }
                                                        }
                                                        aeqi_core::ChatStreamEvent::Complete {
                                                            total_prompt_tokens: pt,
                                                            total_completion_tokens: ct,
                                                            iterations: it,
                                                            ..
                                                        } => {
                                                            prompt_tokens = *pt;
                                                            completion_tokens = *ct;
                                                            iterations = *it;
                                                            break;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                                                    warn!(session_id = %resolved_session_id, lagged = n, "stream subscriber lagged");
                                                }
                                                Ok(Err(_)) => break,
                                                Err(_) => {
                                                    text = "Session response timed out".to_string();
                                                    break;
                                                }
                                            }
                                        }

                                        // Record assistant response via session or legacy.
                                        if let Some(ref cs) = session_store {
                                            if let Some(ref usid) = store_session_id {
                                                let _ = cs
                                                    .record_by_session(
                                                        usid,
                                                        "assistant",
                                                        &text,
                                                        Some("web"),
                                                    )
                                                    .await;
                                            } else {
                                                let _ = cs
                                                    .record_with_source(
                                                        chat_id,
                                                        "assistant",
                                                        &text,
                                                        Some("web"),
                                                    )
                                                    .await;
                                            }
                                        }

                                        // Final streaming event.
                                        let cost_usd = aeqi_providers::estimate_cost(
                                            &default_model,
                                            prompt_tokens,
                                            completion_tokens,
                                        );
                                        let done = serde_json::json!({
                                            "done": true,
                                            "type": "Complete",
                                            "session_id": resolved_session_id,
                                            "store_session_id": store_session_id,
                                            "iterations": iterations,
                                            "prompt_tokens": prompt_tokens,
                                            "completion_tokens": completion_tokens,
                                            "cost_usd": cost_usd,
                                        });
                                        let mut bytes =
                                            serde_json::to_vec(&done).unwrap_or_default();
                                        bytes.push(b'\n');
                                        let _ = writer.write_all(&bytes).await;
                                        serde_json::Value::Null
                                    }
                                    Err(e) => {
                                        serde_json::json!({"ok": false, "error": e.to_string()})
                                    }
                                }
                            } else {
                                // Non-streaming: inject message and wait.
                                match session_manager.send(&resolved_session_id, message).await {
                                    Ok(resp) => {
                                        // Record assistant response via session or legacy.
                                        if let Some(ref cs) = session_store {
                                            if let Some(ref usid) = store_session_id {
                                                let _ = cs
                                                    .record_by_session(
                                                        usid,
                                                        "assistant",
                                                        &resp.text,
                                                        Some("web"),
                                                    )
                                                    .await;
                                            } else {
                                                let _ = cs
                                                    .record_with_source(
                                                        chat_id,
                                                        "assistant",
                                                        &resp.text,
                                                        Some("web"),
                                                    )
                                                    .await;
                                            }
                                        }
                                        // Track cost.
                                        let cost_usd = aeqi_providers::estimate_cost(
                                            &default_model,
                                            resp.prompt_tokens,
                                            resp.completion_tokens,
                                        );
                                        let _ = ipc_ctx
                                            .event_store
                                            .record_cost(
                                                &agent_hint,
                                                &resolved_session_id,
                                                &agent_hint,
                                                cost_usd,
                                                resp.iterations,
                                            )
                                            .await;
                                        serde_json::json!({
                                            "ok": true,
                                            "text": resp.text,
                                            "chat_id": chat_id,
                                            "session_id": resolved_session_id,
                                            "store_session_id": store_session_id,
                                            "iterations": resp.iterations,
                                            "prompt_tokens": resp.prompt_tokens,
                                            "completion_tokens": resp.completion_tokens,
                                            "cost_usd": cost_usd,
                                        })
                                    }
                                    Err(e) => {
                                        serde_json::json!({"ok": false, "error": e.to_string()})
                                    }
                                }
                            }
                        } else if let Some(ref provider) = default_provider {
                            // No running session — boot a new one via SessionManager.spawn_session().
                            let agent_id_or_hint =
                                agent_id_direct.as_deref().unwrap_or(&agent_hint);

                            match session_manager
                                .spawn_session(
                                    agent_id_or_hint,
                                    message,
                                    provider.clone(),
                                    crate::session_manager::SpawnOptions::interactive(),
                                )
                                .await
                            {
                                Ok(spawned) => {
                                    let session_id = spawned.session_id.clone();

                                    // Subscribe and stream events (same as existing-session path).
                                    let mut rx = spawned.stream_sender.subscribe();
                                    let mut text = String::new();
                                    let mut iterations = 0u32;
                                    let mut prompt_tokens = 0u32;
                                    let mut completion_tokens = 0u32;

                                    loop {
                                        match tokio::time::timeout(
                                            std::time::Duration::from_secs(300),
                                            rx.recv(),
                                        )
                                        .await
                                        {
                                            Ok(Ok(event)) => {
                                                if stream_mode
                                                    && let Ok(ev_bytes) = serde_json::to_vec(&event)
                                                {
                                                    let mut bytes = ev_bytes;
                                                    bytes.push(b'\n');
                                                    let _ = writer.write_all(&bytes).await;
                                                }

                                                match &event {
                                                    aeqi_core::ChatStreamEvent::TextDelta {
                                                        text: delta,
                                                    } => {
                                                        text.push_str(delta);
                                                    }
                                                    aeqi_core::ChatStreamEvent::ToolComplete {
                                                        tool_use_id: _,
                                                        tool_name,
                                                        success,
                                                        input_preview,
                                                        output_preview,
                                                        duration_ms,
                                                    } => {
                                                        if let (Some(cs), Some(usid)) =
                                                            (&session_store, &store_session_id)
                                                        {
                                                            let meta = serde_json::json!({
                                                                "tool_name": tool_name,
                                                                "success": success,
                                                                "input_preview": input_preview,
                                                                "output_preview": output_preview,
                                                                "duration_ms": duration_ms,
                                                            });
                                                            let _ = cs
                                                                .record_event_by_session(
                                                                    usid,
                                                                    "tool_complete",
                                                                    "system",
                                                                    tool_name,
                                                                    Some("session"),
                                                                    Some(&meta),
                                                                )
                                                                .await;
                                                        }
                                                    }
                                                    aeqi_core::ChatStreamEvent::Complete {
                                                        total_prompt_tokens: pt,
                                                        total_completion_tokens: ct,
                                                        iterations: it,
                                                        ..
                                                    } => {
                                                        prompt_tokens = *pt;
                                                        completion_tokens = *ct;
                                                        iterations = *it;
                                                        break;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                            Ok(Err(_)) => break,
                                            Err(_) => {
                                                text = "Session response timed out".to_string();
                                                break;
                                            }
                                        }
                                    }

                                    // Record assistant response via session or legacy.
                                    if let Some(ref cs) = session_store {
                                        if let Some(ref usid) = store_session_id {
                                            let _ = cs
                                                .record_by_session(
                                                    usid,
                                                    "assistant",
                                                    &text,
                                                    Some("web"),
                                                )
                                                .await;
                                        } else {
                                            let _ = cs
                                                .record_with_source(
                                                    chat_id,
                                                    "assistant",
                                                    &text,
                                                    Some("web"),
                                                )
                                                .await;
                                        }
                                    }

                                    // Track cost in EventStore.
                                    let cost_usd = aeqi_providers::estimate_cost(
                                        &default_model,
                                        prompt_tokens,
                                        completion_tokens,
                                    );
                                    let _ = ipc_ctx
                                        .event_store
                                        .record_cost(
                                            &agent_hint,
                                            &session_id,
                                            &agent_hint,
                                            cost_usd,
                                            iterations,
                                        )
                                        .await;

                                    if stream_mode {
                                        let done = serde_json::json!({
                                            "done": true,
                                            "type": "Complete",
                                            "session_id": session_id,
                                            "store_session_id": store_session_id,
                                            "iterations": iterations,
                                            "prompt_tokens": prompt_tokens,
                                            "completion_tokens": completion_tokens,
                                            "cost_usd": cost_usd,
                                        });
                                        let mut bytes =
                                            serde_json::to_vec(&done).unwrap_or_default();
                                        bytes.push(b'\n');
                                        let _ = writer.write_all(&bytes).await;
                                        serde_json::Value::Null
                                    } else {
                                        serde_json::json!({
                                            "ok": true,
                                            "text": text,
                                            "chat_id": chat_id,
                                            "session_id": session_id,
                                            "store_session_id": store_session_id,
                                            "iterations": iterations,
                                            "prompt_tokens": prompt_tokens,
                                            "completion_tokens": completion_tokens,
                                            "model": default_model,
                                            "cost_usd": cost_usd,
                                        })
                                    }
                                }
                                Err(e) => {
                                    serde_json::json!({"ok": false, "error": e.to_string()})
                                }
                            }
                        } else {
                            serde_json::json!({"ok": false, "error": "no provider available"})
                        }
                    }
                }

                // --- VFS commands ---
                "vfs_list" => {
                    let path = request.get("path").and_then(|v| v.as_str()).unwrap_or("/");
                    let vfs = crate::vfs::VfsTree::with_direct_deps(
                        agent_registry.clone(),
                        ipc_ctx.session_store.clone(),
                    );
                    match vfs.list(path).await {
                        Ok(resp) => serde_json::to_value(resp)
                            .unwrap_or_else(|_| serde_json::json!({"ok": false})),
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                    }
                }

                "vfs_read" => {
                    let path = request.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    if path.is_empty() {
                        serde_json::json!({"ok": false, "error": "path required"})
                    } else {
                        let vfs = crate::vfs::VfsTree::with_direct_deps(
                            agent_registry.clone(),
                            ipc_ctx.session_store.clone(),
                        );
                        match vfs.read(path).await {
                            Ok(resp) => serde_json::to_value(resp)
                                .unwrap_or_else(|_| serde_json::json!({"ok": false})),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                "vfs_search" => {
                    let query = request.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    if query.is_empty() {
                        serde_json::json!({"ok": false, "error": "query required"})
                    } else {
                        let vfs = crate::vfs::VfsTree::with_direct_deps(
                            agent_registry.clone(),
                            ipc_ctx.session_store.clone(),
                        );
                        match vfs.search(query).await {
                            Ok(resp) => serde_json::to_value(resp)
                                .unwrap_or_else(|_| serde_json::json!({"ok": false})),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }

                _ => serde_json::json!({"ok": false, "error": format!("unknown command: {cmd}")}),
            };

            // Skip writing if response is null (already streamed inline).
            if !response.is_null() {
                let mut resp_bytes = serde_json::to_vec(&response)?;
                resp_bytes.push(b'\n');
                writer.write_all(&resp_bytes).await?;
            }
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
    use crate::session_store::{
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
        buffer.push(ExecutionEvent::QuestStarted {
            task_id: "t-1".into(),
            agent: "engineer".into(),
            project: "aeqi".into(),
            runtime_session: None,
        });
        buffer.push(ExecutionEvent::QuestCompleted {
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
            resolve_web_chat_id(None, None, None, Some("aeqi")),
            agency_chat_id()
        );
        assert_eq!(
            resolve_web_chat_id(None, None, None, None),
            agency_chat_id()
        );
    }
}
