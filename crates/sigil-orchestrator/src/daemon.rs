use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::familiar::Familiar;
use crate::mail::MailBus;

/// The Daemon: background process that runs the Familiar + all Witnesses.
pub struct Daemon {
    pub familiar: Arc<Mutex<Familiar>>,
    pub mail_bus: Arc<MailBus>,
    pub patrol_interval_secs: u64,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl Daemon {
    pub fn new(familiar: Familiar, mail_bus: Arc<MailBus>) -> Self {
        Self {
            familiar: Arc::new(Mutex::new(familiar)),
            mail_bus,
            patrol_interval_secs: 60,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start the daemon loop.
    pub async fn run(&self) -> Result<()> {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        info!("daemon started");

        while self.running.load(std::sync::atomic::Ordering::SeqCst) {
            // Run patrol cycle.
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

            // Sleep until next patrol.
            tokio::time::sleep(std::time::Duration::from_secs(self.patrol_interval_secs)).await;
        }

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
