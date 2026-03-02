use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::schedule::ScheduleStore;
use crate::heartbeat::Heartbeat;
use crate::reflection::Reflection;
use crate::session_tracker::SessionTracker;
use crate::message::DispatchBus;
use crate::registry::ProjectRegistry;

/// The Daemon: background process that runs the ProjectRegistry patrol loop,
/// pulses, and cron jobs.
pub struct Daemon {
    pub registry: Arc<ProjectRegistry>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub patrol_interval_secs: u64,
    pub pulses: Vec<Heartbeat>,
    pub reflections: Vec<Reflection>,
    pub fate_store: Option<Arc<Mutex<ScheduleStore>>>,
    pub pid_file: Option<PathBuf>,
    pub socket_path: Option<PathBuf>,
    session_tracker_shutdown: Option<Arc<tokio::sync::Notify>>,
    running: Arc<std::sync::atomic::AtomicBool>,
    config_reloaded: Arc<std::sync::atomic::AtomicBool>,
    shutdown_notify: Arc<tokio::sync::Notify>,
}

impl Daemon {
    pub fn new(registry: Arc<ProjectRegistry>, dispatch_bus: Arc<DispatchBus>) -> Self {
        Self {
            registry,
            dispatch_bus,
            patrol_interval_secs: 30,
            pulses: Vec::new(),
            reflections: Vec::new(),
            fate_store: None,
            pid_file: None,
            socket_path: None,
            session_tracker_shutdown: None,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            config_reloaded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Add a pulse to the daemon.
    pub fn add_pulse(&mut self, pulse: Heartbeat) {
        self.pulses.push(pulse);
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

    /// Set the cron store for scheduled jobs.
    pub fn set_fate_store(&mut self, store: ScheduleStore) {
        self.fate_store = Some(Arc::new(Mutex::new(store)));
    }

    /// Set a PID file path (written on start, removed on stop).
    pub fn set_pid_file(&mut self, path: PathBuf) {
        self.pid_file = Some(path);
    }

    /// Set a Unix socket path for IPC.
    pub fn set_socket_path(&mut self, path: PathBuf) {
        self.socket_path = Some(path);
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
            && let Ok(pid) = content.trim().parse::<u32>() {
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
                let mut signal = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::hangup(),
                ).expect("failed to register SIGHUP handler");
                loop {
                    signal.recv().await;
                    info!("received SIGHUP, flagging config reload");
                    config_reloaded.store(true, std::sync::atomic::Ordering::SeqCst);
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
                    let pulse_count = self.pulses.len();
                    let fate_store = self.fate_store.clone();
                    let running = self.running.clone();
                    info!(path = %sock_path.display(), "IPC socket listening");
                    tokio::spawn(async move {
                        Self::socket_accept_loop(
                            listener, registry, dispatch_bus,
                            pulse_count, fate_store, running,
                        ).await;
                    });
                }
                Err(e) => {
                    warn!(error = %e, path = %sock_path.display(), "failed to bind IPC socket");
                }
            }
        }

        // Load persisted state from disk.
        match self.dispatch_bus.load().await {
            Ok(n) if n > 0 => info!(count = n, "loaded persisted whispers"),
            Ok(_) => {}
            Err(e) => warn!(error = %e, "failed to load whisper bus"),
        }
        match self.registry.cost_ledger.load() {
            Ok(n) if n > 0 => info!(count = n, "loaded persisted cost entries"),
            Ok(_) => {}
            Err(e) => warn!(error = %e, "failed to load cost ledger"),
        }

        info!(
            pulses = self.pulses.len(),
            cron = self.fate_store.is_some(),
            "daemon started"
        );

        while self.running.load(std::sync::atomic::Ordering::SeqCst) {
            // 1. Patrol cycle: reap finished workers, assign + launch new ones (non-blocking).
            if let Err(e) = self.registry.patrol_all().await {
                warn!(error = %e, "patrol cycle failed");
            }

            // 2. Run due pulses.
            for pulse in self.pulses.iter_mut() {
                if pulse.is_due() {
                    match pulse.run().await {
                        Ok(result) => {
                            info!(project = %pulse.project_name, "pulse completed");
                            let _ = result;
                        }
                        Err(e) => {
                            warn!(project = %pulse.project_name, error = %e, "pulse failed");
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

            // 4. Run due cron jobs.
            if let Some(ref fate_store) = self.fate_store {
                let due_jobs = {
                    let store = fate_store.lock().await;
                    store.due_jobs()
                        .into_iter()
                        .map(|j| (j.name.clone(), j.project.clone(), j.prompt.clone(), j.isolated))
                        .collect::<Vec<_>>()
                };

                for (name, project, prompt, _isolated) in due_jobs {
                    info!(name = %name, project = %project, "cron job triggered");

                    match self.registry.assign(&project, &format!("[cron] {name}"), &prompt).await {
                        Ok(bead) => {
                            info!(bead = %bead.id, "cron job created bead");
                        }
                        Err(e) => {
                            warn!(name = %name, error = %e, "cron job failed to create bead");
                        }
                    }

                    let mut store = fate_store.lock().await;
                    let _ = store.mark_run(&name);
                }

                // Cleanup completed one-shots.
                let mut store = fate_store.lock().await;
                let _ = store.cleanup_oneshots();
            }

            // 5. Check for config reload signal (SIGHUP).
            if self.config_reloaded.swap(false, std::sync::atomic::Ordering::SeqCst) {
                info!("config reload requested (SIGHUP received)");
                match system_core::config::SystemConfig::discover() {
                    Ok((_new_config, _path)) => {
                        info!("config reloaded successfully via SIGHUP");
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to reload config, keeping current");
                    }
                }
            }

            // 6. Periodic persistence: save whisper bus + cost ledger every patrol.
            if let Err(e) = self.dispatch_bus.save().await {
                warn!(error = %e, "failed to save whisper bus");
            }
            if let Err(e) = self.registry.cost_ledger.save() {
                warn!(error = %e, "failed to save cost ledger");
            }

            // 7. Update daily cost gauge.
            let (spent, _, _) = self.registry.cost_ledger.budget_status();
            self.registry.metrics.daily_cost_usd.set(spent);
            let pending_whispers = self.dispatch_bus.pending_count();
            self.registry.metrics.whisper_queue_depth.set(pending_whispers as f64);

            // 8. Prune old cost entries (older than 7 days) every cycle.
            self.registry.cost_ledger.prune_old();

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(self.patrol_interval_secs)) => {},
                _ = self.registry.wake.notified() => {
                    debug!("woken by new bead");
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
    async fn socket_accept_loop(
        listener: tokio::net::UnixListener,
        registry: Arc<ProjectRegistry>,
        dispatch_bus: Arc<DispatchBus>,
        pulse_count: usize,
        fate_store: Option<Arc<Mutex<ScheduleStore>>>,
        running: Arc<std::sync::atomic::AtomicBool>,
    ) {
        loop {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            match listener.accept().await {
                Ok((stream, _)) => {
                    let registry = registry.clone();
                    let dispatch_bus = dispatch_bus.clone();
                    let fate_store = fate_store.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_socket_connection(
                            stream, registry, dispatch_bus,
                            pulse_count, fate_store,
                        ).await {
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
        fate_store: Option<Arc<Mutex<ScheduleStore>>>,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            let request: serde_json::Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| serde_json::json!({"cmd": "unknown"}));

            let cmd = request.get("cmd").and_then(|v| v.as_str()).unwrap_or("unknown");

            let response = match cmd {
                "ping" => serde_json::json!({"ok": true, "pong": true}),

                "status" => {
                    let project_names: Vec<String> = registry.project_names().await;
                    let worker_count = registry.total_max_spirits().await;
                    let mail_count = dispatch_bus.pending_count();
                    let cron_count = if let Some(ref cs) = fate_store {
                        cs.lock().await.jobs.len()
                    } else {
                        0
                    };

                    let (spent, budget, remaining) = registry.cost_ledger.budget_status();
                    let project_budgets = registry.cost_ledger.all_project_budget_statuses();
                    let project_budget_info: serde_json::Map<String, serde_json::Value> = project_budgets
                        .into_iter()
                        .map(|(name, (spent, budget, remaining))| {
                            (name, serde_json::json!({
                                "spent_usd": spent,
                                "budget_usd": budget,
                                "remaining_usd": remaining,
                            }))
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
                        "cost_today_usd": spent,
                        "daily_budget_usd": budget,
                        "budget_remaining_usd": remaining,
                        "project_budgets": project_budget_info,
                    })
                }

                "rigs" | "domains" | "projects" => {
                    let projects = registry.projects_info().await;
                    serde_json::json!({"ok": true, "projects": projects})
                }

                "mail" => {
                    let messages = dispatch_bus.drain();
                    let msgs: Vec<serde_json::Value> = messages.iter().map(|m| {
                        serde_json::json!({
                            "from": m.from,
                            "to": m.to,
                            "subject": m.kind.subject_tag(),
                            "body": m.kind.body_text(),
                        })
                    }).collect();
                    serde_json::json!({"ok": true, "messages": msgs})
                }

                "metrics" => {
                    let text = registry.metrics.render();
                    serde_json::json!({"ok": true, "metrics": text})
                }

                "cost" => {
                    let (spent, budget, remaining) = registry.cost_ledger.budget_status();
                    let report = registry.cost_ledger.daily_report();
                    let project_budgets = registry.cost_ledger.all_project_budget_statuses();
                    let project_budget_info: serde_json::Map<String, serde_json::Value> = project_budgets
                        .into_iter()
                        .map(|(name, (spent, budget, remaining))| {
                            (name, serde_json::json!({
                                "spent_usd": spent,
                                "budget_usd": budget,
                                "remaining_usd": remaining,
                            }))
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
