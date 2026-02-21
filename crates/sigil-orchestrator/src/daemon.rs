use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::cron::CronStore;
use crate::familiar::Familiar;
use crate::heartbeat::Heartbeat;
use crate::mail::MailBus;

/// The Daemon: background process that runs the Familiar + all Witnesses + Heartbeats + Cron.
pub struct Daemon {
    pub familiar: Arc<Mutex<Familiar>>,
    pub mail_bus: Arc<MailBus>,
    pub patrol_interval_secs: u64,
    pub heartbeats: Vec<Heartbeat>,
    pub cron_store: Option<Arc<Mutex<CronStore>>>,
    pub pid_file: Option<PathBuf>,
    running: Arc<std::sync::atomic::AtomicBool>,
    config_reloaded: Arc<std::sync::atomic::AtomicBool>,
}

impl Daemon {
    pub fn new(familiar: Familiar, mail_bus: Arc<MailBus>) -> Self {
        Self {
            familiar: Arc::new(Mutex::new(familiar)),
            mail_bus,
            patrol_interval_secs: 60,
            heartbeats: Vec::new(),
            cron_store: None,
            pid_file: None,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            config_reloaded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Add a heartbeat to the daemon.
    pub fn add_heartbeat(&mut self, heartbeat: Heartbeat) {
        self.heartbeats.push(heartbeat);
    }

    /// Set the cron store for scheduled jobs.
    pub fn set_cron_store(&mut self, store: CronStore) {
        self.cron_store = Some(Arc::new(Mutex::new(store)));
    }

    /// Set a PID file path (written on start, removed on stop).
    pub fn set_pid_file(&mut self, path: PathBuf) {
        self.pid_file = Some(path);
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
        if let Ok(content) = std::fs::read_to_string(pid_path) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                // Check if process exists.
                return Path::new(&format!("/proc/{pid}")).exists();
            }
        }
        false
    }

    /// Start the daemon loop with graceful shutdown on Ctrl+C.
    pub async fn run(&mut self) -> Result<()> {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        self.write_pid_file()?;

        // Set up Ctrl+C handler.
        let running = self.running.clone();
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("received Ctrl+C, shutting down...");
                running.store(false, std::sync::atomic::Ordering::SeqCst);
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

        info!(
            heartbeats = self.heartbeats.len(),
            cron = self.cron_store.is_some(),
            "daemon started"
        );

        while self.running.load(std::sync::atomic::Ordering::SeqCst) {
            // 1. Patrol cycle (familiar + witnesses).
            {
                let mut familiar = self.familiar.lock().await;
                if let Err(e) = familiar.patrol().await {
                    warn!(error = %e, "patrol cycle failed");
                }

                // Execute any ready workers.
                let executed = familiar.execute_all().await;
                if executed > 0 {
                    info!(workers = executed, "executed workers");
                }
            }

            // 2. Run due heartbeats.
            for heartbeat in self.heartbeats.iter_mut() {
                if heartbeat.is_due() {
                    match heartbeat.run().await {
                        Ok(result) => {
                            info!(rig = %heartbeat.rig_name, "heartbeat completed");
                            let _ = result; // Result already logged/mailed by heartbeat.
                        }
                        Err(e) => {
                            warn!(rig = %heartbeat.rig_name, error = %e, "heartbeat failed");
                        }
                    }
                }
            }

            // 3. Run due cron jobs.
            if let Some(ref cron_store) = self.cron_store {
                let due_jobs = {
                    let store = cron_store.lock().await;
                    store.due_jobs()
                        .into_iter()
                        .map(|j| (j.name.clone(), j.rig.clone(), j.prompt.clone(), j.isolated))
                        .collect::<Vec<_>>()
                };

                for (name, rig, prompt, _isolated) in due_jobs {
                    info!(name = %name, rig = %rig, "cron job triggered");

                    // Assign the cron job's prompt as a bead.
                    {
                        let mut familiar = self.familiar.lock().await;
                        match familiar.assign(&rig, &format!("[cron] {name}"), &prompt).await {
                            Ok(bead) => {
                                info!(bead = %bead.id, "cron job created bead");
                            }
                            Err(e) => {
                                warn!(name = %name, error = %e, "cron job failed to create bead");
                            }
                        }
                    }

                    // Mark the job as run.
                    let mut store = cron_store.lock().await;
                    let _ = store.mark_run(&name);
                }

                // Cleanup completed one-shots.
                let mut store = cron_store.lock().await;
                let _ = store.cleanup_oneshots();
            }

            // 4. Check for config reload signal (SIGHUP).
            if self.config_reloaded.swap(false, std::sync::atomic::Ordering::SeqCst) {
                info!("config reload requested (SIGHUP received)");
                // The caller should re-read config and update state as needed.
                // For now we log it; full hot-reload requires rebuilding rigs/witnesses.
            }

            // Sleep until next patrol (interruptible).
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(self.patrol_interval_secs)) => {},
                _ = async {
                    while self.running.load(std::sync::atomic::Ordering::SeqCst) {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                } => break,
            }
        }

        self.remove_pid_file();
        info!("daemon stopped");
        Ok(())
    }

    /// Stop the daemon.
    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if daemon is running.
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }
}
