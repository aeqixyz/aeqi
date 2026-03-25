use anyhow::Result;
use chrono::{Timelike, Utc};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::chat_engine::{ChatEngine, ChatMessage, ChatSource};
use crate::conversation_store::web_chat_id;
use crate::heartbeat::Heartbeat;
use crate::lifecycle::LifecycleEngine;
use crate::message::{Dispatch, DispatchBus, DispatchHealth};
use crate::reflection::Reflection;
use crate::registry::ProjectRegistry;
use crate::schedule::ScheduleStore;
use crate::session_tracker::SessionTracker;
use crate::watchdog::WatchdogEngine;

const ACK_RETRY_AGE_SECS: u64 = 60;

#[derive(Debug, Clone, Default)]
struct ReadinessContext {
    configured_projects: usize,
    configured_advisors: usize,
    skipped_projects: Vec<String>,
    skipped_advisors: Vec<String>,
}

/// The Daemon: background process that runs the ProjectRegistry patrol loop,
/// pulses, and cron jobs.
pub struct Daemon {
    pub registry: Arc<ProjectRegistry>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub patrol_interval_secs: u64,
    pub pulses: Vec<Heartbeat>,
    pub reflections: Vec<Reflection>,
    pub lifecycle: Option<LifecycleEngine>,
    pub cron_store: Option<Arc<Mutex<ScheduleStore>>>,
    pub watchdog: Option<WatchdogEngine>,
    pub chat_engine: Option<Arc<ChatEngine>>,
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
            pulses: Vec::new(),
            reflections: Vec::new(),
            lifecycle: None,
            cron_store: None,
            watchdog: None,
            chat_engine: None,
            pid_file: None,
            socket_path: None,
            session_tracker_shutdown: None,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            config_reloaded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
            readiness: ReadinessContext::default(),
        }
    }

    /// Add a heartbeat to the daemon.
    pub fn add_heartbeat(&mut self, heartbeat: Heartbeat) {
        self.pulses.push(heartbeat);
    }

    /// Add a reflection cycle to the daemon.
    pub fn add_reflection(&mut self, reflection: Reflection) {
        self.reflections.push(reflection);
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

    /// Set the lifecycle engine for autonomous agent processes.
    pub fn set_lifecycle(&mut self, engine: LifecycleEngine) {
        self.lifecycle = Some(engine);
    }

    /// Set the cron store for scheduled jobs.
    pub fn set_cron_store(&mut self, store: ScheduleStore) {
        self.cron_store = Some(Arc::new(Mutex::new(store)));
    }

    /// Set the watchdog engine for event-driven automation.
    pub fn set_watchdog(&mut self, engine: WatchdogEngine) {
        self.watchdog = Some(engine);
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
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                        .expect("failed to register SIGHUP handler");
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
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("failed to register SIGTERM handler");
                signal.recv().await;
                info!("received SIGTERM, shutting down...");
                running.store(false, std::sync::atomic::Ordering::SeqCst);
                shutdown_notify.notify_waiters();
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
                    let pulse_count = self.pulses.len();
                    let cron_store = self.cron_store.clone();
                    let chat_engine = self.chat_engine.clone();
                    let running = self.running.clone();
                    let readiness = self.readiness.clone();
                    info!(path = %sock_path.display(), "IPC socket listening");
                    tokio::spawn(async move {
                        Self::socket_accept_loop(
                            listener,
                            registry,
                            dispatch_bus,
                            pulse_count,
                            cron_store,
                            chat_engine,
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

        info!(
            pulses = self.pulses.len(),
            cron = self.cron_store.is_some(),
            "daemon started"
        );

        while self.running.load(std::sync::atomic::Ordering::SeqCst) {
            // 1. Patrol cycle: reap finished workers, assign + launch new ones (non-blocking).
            if let Err(e) = self.registry.patrol_all().await {
                warn!(error = %e, "patrol cycle failed");
            }

            // 2. Run due heartbeats.
            for heartbeat in self.pulses.iter_mut() {
                if heartbeat.is_due() {
                    match heartbeat.run().await {
                        Ok(result) => {
                            info!(project = %heartbeat.project_name, "heartbeat completed");
                            let _ = result;
                        }
                        Err(e) => {
                            warn!(project = %heartbeat.project_name, error = %e, "heartbeat failed");
                        }
                    }
                }
            }

            // 3. Run due reflections (self-examination of identity files).
            for reflection in self.reflections.iter_mut() {
                if reflection.is_due() {
                    match reflection.run().await {
                        Ok(result) => {
                            info!(project = %reflection.project_name, result = %result, "reflection completed");
                        }
                        Err(e) => {
                            warn!(project = %reflection.project_name, error = %e, "reflection failed");
                        }
                    }
                }
            }

            // 4. Run due lifecycle processes (autonomous agent evolution).
            if let Some(ref mut lifecycle) = self.lifecycle {
                for result in lifecycle.tick().await {
                    if let Some(ref err) = result.error {
                        warn!(agent=%result.agent, process=%result.process, error=%err, "lifecycle failed");
                    } else {
                        info!(agent=%result.agent, process=%result.process, summary=%result.summary,
                            cost_usd=%result.cost_usd, "lifecycle completed");
                    }
                }
            }

            // 5. Run due cron jobs.
            if let Some(ref cron_store) = self.cron_store {
                let due_jobs = {
                    let store = cron_store.lock().await;
                    store
                        .due_jobs()
                        .into_iter()
                        .map(|j| {
                            (
                                j.name.clone(),
                                j.project.clone(),
                                j.prompt.clone(),
                                j.isolated,
                            )
                        })
                        .collect::<Vec<_>>()
                };

                for (name, project, prompt, _isolated) in due_jobs {
                    info!(name = %name, project = %project, "cron job triggered");

                    match self
                        .registry
                        .assign(&project, &format!("[cron] {name}"), &prompt)
                        .await
                    {
                        Ok(task) => {
                            info!(task = %task.id, "cron job created task");
                        }
                        Err(e) => {
                            warn!(name = %name, error = %e, "cron job failed to create task");
                        }
                    }

                    let mut store = cron_store.lock().await;
                    let _ = store.mark_run(&name);
                }

                // Cleanup completed one-shots.
                let mut store = cron_store.lock().await;
                let _ = store.cleanup_oneshots();
            }

            // 6. Check for config reload signal (SIGHUP).
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

            // 12. Evaluate watchdog rules and execute fired actions.
            if let Some(ref mut watchdog) = self.watchdog
                && let Some(ref audit) = self.registry.audit_log
            {
                let (spent, budget, _) = self.registry.cost_ledger.budget_status();
                let budget_pct = if budget > 0.0 {
                    Some(spent / budget)
                } else {
                    None
                };
                let fired = watchdog.evaluate(audit, budget_pct);
                for (name, action) in &fired {
                    info!(rule = %name, "watchdog rule fired");

                    // Record audit event.
                    let _ = audit.record(
                        &crate::audit::AuditEvent::new(
                            "*",
                            crate::audit::DecisionType::WatchdogFired,
                            format!("Rule '{}' fired", name),
                        )
                        .with_metadata(serde_json::json!({"action": format!("{action:?}")})),
                    );

                    // Execute the action.
                    match action {
                        crate::watchdog::WatchdogAction::CreateTask {
                            project,
                            subject,
                            description,
                        } => match self.registry.assign(project, subject, description).await {
                            Ok(task) => {
                                info!(
                                    rule = %name,
                                    task = %task.id,
                                    project = %project,
                                    "watchdog created task"
                                );
                            }
                            Err(e) => {
                                warn!(
                                    rule = %name,
                                    project = %project,
                                    error = %e,
                                    "watchdog failed to create task"
                                );
                            }
                        },
                        crate::watchdog::WatchdogAction::SendDispatch { to, message } => {
                            self.dispatch_bus
                                .send(crate::message::Dispatch::new_typed(
                                    "watchdog",
                                    to,
                                    crate::message::DispatchKind::Escalation {
                                        project: "*".to_string(),
                                        task_id: String::new(),
                                        subject: format!("[watchdog] {name}"),
                                        description: message.clone(),
                                        attempts: 0,
                                    },
                                ))
                                .await;
                            info!(rule = %name, to = %to, "watchdog sent dispatch");
                        }
                        crate::watchdog::WatchdogAction::Escalate { message } => {
                            self.dispatch_bus
                                .send(crate::message::Dispatch::new_typed(
                                    "watchdog",
                                    &self.registry.leader_agent_name,
                                    crate::message::DispatchKind::Escalation {
                                        project: "*".to_string(),
                                        task_id: String::new(),
                                        subject: format!("[watchdog] {name}"),
                                        description: message.clone(),
                                        attempts: 0,
                                    },
                                ))
                                .await;
                            info!(rule = %name, "watchdog escalated to leader");
                        }
                        crate::watchdog::WatchdogAction::PauseProject { project } => {
                            if let Some(sup) = self.registry.get_supervisor(project).await {
                                let mut s = sup.lock().await;
                                s.paused = true;
                                info!(
                                    rule = %name,
                                    project = %project,
                                    "watchdog paused project"
                                );
                            }
                        }
                        crate::watchdog::WatchdogAction::RunCommand { command } => {
                            info!(rule = %name, command = %command, "watchdog executing command");
                            match tokio::process::Command::new("sh")
                                .arg("-c")
                                .arg(command)
                                .status()
                                .await
                            {
                                Ok(status) => {
                                    info!(
                                        rule = %name,
                                        status = %status,
                                        "watchdog command completed"
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        rule = %name,
                                        error = %e,
                                        "watchdog command failed"
                                    );
                                }
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
        pulse_count: usize,
        cron_store: Option<Arc<Mutex<ScheduleStore>>>,
        chat_engine: Option<Arc<ChatEngine>>,
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
                    let cron_store = cron_store.clone();
                    let chat_engine = chat_engine.clone();
                    let readiness = readiness.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_socket_connection(
                            stream,
                            registry,
                            dispatch_bus,
                            pulse_count,
                            cron_store,
                            chat_engine,
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
    async fn handle_socket_connection(
        stream: tokio::net::UnixStream,
        registry: Arc<ProjectRegistry>,
        dispatch_bus: Arc<DispatchBus>,
        pulse_count: usize,
        cron_store: Option<Arc<Mutex<ScheduleStore>>>,
        chat_engine: Option<Arc<ChatEngine>>,
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
                    let cron_count = if let Some(ref cs) = cron_store {
                        cs.lock().await.jobs.len()
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
                        "pulses": pulse_count,
                        "cron_jobs": cron_count,
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
                        pulse_count,
                        dispatch_health,
                        (spent, budget, remaining),
                        &readiness,
                    )
                }

                "worker_progress" => {
                    let workers = registry.worker_progress().await;
                    serde_json::json!({"ok": true, "workers": workers})
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
                    match &registry.blackboard {
                        Some(bb) => {
                            let entries =
                                bb.list_project(project_filter, limit).unwrap_or_default();
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

                    if project.is_empty() || subject.is_empty() {
                        serde_json::json!({"ok": false, "error": "project and subject are required"})
                    } else {
                        match registry.assign(project, subject, description).await {
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
                                        Ok(task) => serde_json::json!({
                                            "ok": true,
                                            "task": {
                                                "id": task.id.0,
                                                "status": task.status.to_string(),
                                                "closed_reason": task.closed_reason,
                                            }
                                        }),
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
                    let project_hint = request.get("project").and_then(|v| v.as_str());
                    let session_id = request
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("ipc");
                    let sender = request
                        .get("sender")
                        .and_then(|v| v.as_str())
                        .unwrap_or("user");

                    match &chat_engine {
                        Some(engine) => {
                            let chat_id = request
                                .get("chat_id")
                                .and_then(|v| v.as_i64())
                                .unwrap_or_else(|| web_chat_id(session_id));

                            let msg = ChatMessage {
                                message: message.to_string(),
                                chat_id,
                                sender: sender.to_string(),
                                source: ChatSource::Web {
                                    session_id: session_id.to_string(),
                                },
                                project_hint: project_hint.map(|s| s.to_string()),
                            };

                            // Try quick path first (intent detection).
                            if let Some(response) = engine.handle_message(&msg).await {
                                response.to_json()
                            } else {
                                // Fall back to status response enriched with memory.
                                engine
                                    .status_response(project_hint, Some(message))
                                    .await
                                    .to_json()
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
                    let project_hint = request.get("project").and_then(|v| v.as_str());
                    let session_id = request
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("ipc");
                    let sender = request
                        .get("sender")
                        .and_then(|v| v.as_str())
                        .unwrap_or("user");

                    match &chat_engine {
                        Some(engine) => {
                            if message.is_empty() {
                                serde_json::json!({"ok": false, "error": "message is required"})
                            } else {
                                let chat_id = request
                                    .get("chat_id")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or_else(|| web_chat_id(session_id));

                                let msg = ChatMessage {
                                    message: message.to_string(),
                                    chat_id,
                                    sender: sender.to_string(),
                                    source: ChatSource::Web {
                                        session_id: session_id.to_string(),
                                    },
                                    project_hint: project_hint.map(|s| s.to_string()),
                                };

                                // Try quick path first.
                                if let Some(response) = engine.handle_message(&msg).await {
                                    response.to_json()
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
                    let session_id = request.get("session_id").and_then(|v| v.as_str());

                    match &chat_engine {
                        Some(engine) => {
                            let resolved_chat_id = if chat_id != 0 {
                                chat_id
                            } else if let Some(sid) = session_id {
                                web_chat_id(sid)
                            } else {
                                0
                            };
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

                "crons" => match &cron_store {
                    Some(store) => {
                        let store = store.lock().await;
                        let jobs: Vec<serde_json::Value> = store
                            .jobs
                            .iter()
                            .map(|j| {
                                let schedule_str = match &j.schedule {
                                    crate::schedule::CronSchedule::Cron { expr } => expr.clone(),
                                    crate::schedule::CronSchedule::Once { at } => {
                                        format!("once@{}", at.to_rfc3339())
                                    }
                                };
                                serde_json::json!({
                                    "name": j.name,
                                    "project": j.project,
                                    "schedule": schedule_str,
                                    "prompt": j.prompt,
                                    "last_run": j.last_run.map(|t| t.to_rfc3339()),
                                    "created_at": j.created_at.to_rfc3339(),
                                })
                            })
                            .collect();
                        serde_json::json!({"ok": true, "jobs": jobs})
                    }
                    None => serde_json::json!({"ok": true, "jobs": []}),
                },

                "watchdogs" => {
                    serde_json::json!({"ok": true, "rules": registry.watchdog_rules_config})
                }

                "brief" => {
                    let summaries = registry.list_project_summaries().await;
                    let (spent, budget, _remaining) = registry.cost_ledger.budget_status();
                    let worker_count = registry.total_max_workers().await;
                    let dispatch_health = dispatch_bus.health(ACK_RETRY_AGE_SECS).await;

                    // Get recent audit events (last 50 for analysis).
                    let recent = match &registry.audit_log {
                        Some(audit) => audit.query_recent(50).unwrap_or_default(),
                        None => Vec::new(),
                    };

                    // Compute summary stats from audit.
                    let tasks_completed = recent
                        .iter()
                        .filter(|e| e.decision_type.to_string().contains("completed"))
                        .count();
                    let tasks_failed = recent
                        .iter()
                        .filter(|e| e.decision_type.to_string().contains("failed"))
                        .count();
                    let tasks_assigned = recent
                        .iter()
                        .filter(|e| e.decision_type.to_string().contains("assigned"))
                        .count();

                    // Cron status.
                    let cron_count = if let Some(ref cs) = cron_store {
                        cs.lock().await.jobs.len()
                    } else {
                        0
                    };

                    let mut brief = String::new();
                    brief.push_str(&format!(
                        "Good {}. Here's your brief.\n\n",
                        if chrono::Utc::now().hour() < 12 {
                            "morning"
                        } else if chrono::Utc::now().hour() < 18 {
                            "afternoon"
                        } else {
                            "evening"
                        }
                    ));

                    // Projects overview.
                    brief.push_str("Projects:\n");
                    for s in &summaries {
                        brief.push_str(&format!(
                            "  {} — {} open tasks, {} done, {} missions\n",
                            s.name, s.open_tasks, s.done_tasks, s.active_missions
                        ));
                    }

                    // Recent activity summary.
                    brief.push_str(&format!(
                        "\nRecent activity: {} tasks completed, {} failed, {} assigned\n",
                        tasks_completed, tasks_failed, tasks_assigned
                    ));

                    // Budget.
                    brief.push_str(&format!(
                        "Budget: ${:.3} spent of ${:.2} ({:.1}% used)\n",
                        spent,
                        budget,
                        (spent / budget) * 100.0
                    ));

                    // System health.
                    brief.push_str(&format!(
                        "System: {} workers, {} cron jobs, {} pending messages\n",
                        worker_count, cron_count, dispatch_health.unread
                    ));

                    if dispatch_health.dead_letters > 0 {
                        brief.push_str(&format!(
                            "⚠ {} dead letters in dispatch queue\n",
                            dispatch_health.dead_letters
                        ));
                    }

                    serde_json::json!({
                        "ok": true,
                        "brief": brief.trim(),
                        "stats": {
                            "tasks_completed": tasks_completed,
                            "tasks_failed": tasks_failed,
                            "tasks_assigned": tasks_assigned,
                            "budget_used_pct": (spent / budget) * 100.0,
                            "workers": worker_count,
                            "cron_jobs": cron_count,
                            "dead_letters": dispatch_health.dead_letters,
                        }
                    })
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
                    } else {
                        // Query memories for a specific project.
                        let cwd = std::env::current_dir().unwrap_or_default();
                        let db_path = cwd
                            .join("projects")
                            .join(project)
                            .join(".sigil")
                            .join("memory.db");
                        if !db_path.exists() {
                            serde_json::json!({"ok": true, "memories": []})
                        } else if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                            let sql = if query.is_empty() {
                                format!(
                                    "SELECT id, key, content, category, scope, entity_id, created_at FROM memories ORDER BY created_at DESC LIMIT {limit}"
                                )
                            } else {
                                format!(
                                    "SELECT id, key, content, category, scope, entity_id, created_at FROM memories WHERE content LIKE '%{}%' OR key LIKE '%{}%' ORDER BY created_at DESC LIMIT {limit}",
                                    query.replace('\'', ""),
                                    query.replace('\'', "")
                                )
                            };
                            let rows: Vec<serde_json::Value> = conn
                                .prepare(&sql)
                                .ok()
                                .map(|mut stmt| {
                                    stmt.query_map([], |row| {
                                        Ok(serde_json::json!({
                                            "id": row.get::<_, String>(0)?,
                                            "key": row.get::<_, String>(1)?,
                                            "content": row.get::<_, String>(2)?,
                                            "category": row.get::<_, String>(3)?,
                                            "scope": row.get::<_, String>(4)?,
                                            "entity_id": row.get::<_, Option<String>>(5)?,
                                            "created_at": row.get::<_, String>(6)?,
                                        }))
                                    })
                                    .ok()
                                    .map(|iter| iter.filter_map(|r| r.ok()).collect())
                                    .unwrap_or_default()
                                })
                                .unwrap_or_default();
                            serde_json::json!({"ok": true, "memories": rows, "count": rows.len()})
                        } else {
                            serde_json::json!({"ok": true, "memories": []})
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

                    // Shared subagents.
                    scan_skills(
                        &cwd.join("projects").join("shared").join("subagents"),
                        "shared/subagents",
                        &mut skills,
                    );

                    // Per-project skills + subagents.
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
                            &entry.path().join("subagents"),
                            &format!("{project}/subagents"),
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
    pulse_count: usize,
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
        "pulses": pulse_count,
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
    use super::{DispatchHealth, ReadinessContext, readiness_response};

    #[test]
    fn readiness_blocks_when_owner_registration_is_incomplete() {
        let response = readiness_response(
            "leader",
            vec![("leader".to_string(), 1), ("alpha".to_string(), 2)],
            1,
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
            2,
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
            0,
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
}
