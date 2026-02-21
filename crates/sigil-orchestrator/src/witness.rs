use anyhow::Result;
use sigil_beads::BeadStore;
use sigil_core::traits::{Provider, Tool};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::mail::{Mail, MailBus};
use crate::rig::Rig;
use crate::worker::{Worker, WorkerState};

/// Witness: per-rig supervisor. Runs patrol cycles, manages workers,
/// detects stuck/orphaned beads, reports to Familiar.
pub struct Witness {
    pub rig_name: String,
    pub workers: Vec<Worker>,
    pub max_workers: u32,
    pub patrol_interval_secs: u64,
    pub mail_bus: Arc<MailBus>,
    pub beads: Arc<Mutex<BeadStore>>,
    pub provider: Arc<dyn Provider>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub model: String,
    pub identity: sigil_core::Identity,
}

impl Witness {
    pub fn new(rig: &Rig, provider: Arc<dyn Provider>, tools: Vec<Arc<dyn Tool>>, mail_bus: Arc<MailBus>) -> Self {
        Self {
            rig_name: rig.name.clone(),
            workers: Vec::new(),
            max_workers: rig.max_workers,
            patrol_interval_secs: 60,
            mail_bus,
            beads: rig.beads.clone(),
            provider,
            tools,
            model: rig.model.clone(),
            identity: rig.identity.clone(),
        }
    }

    /// Run one patrol cycle: check workers, assign ready work, report status.
    pub async fn patrol(&mut self) -> Result<()> {
        debug!(rig = %self.rig_name, "patrol cycle");

        // 1. Clean up done/failed workers.
        self.workers.retain(|w| {
            !matches!(w.state, WorkerState::Done | WorkerState::Failed(_))
        });

        // 2. Check for ready beads and assign to idle workers.
        let ready_beads = {
            let store = self.beads.lock().await;
            store.ready().into_iter().cloned().collect::<Vec<_>>()
        };

        for bead in ready_beads {
            if self.workers.len() as u32 >= self.max_workers {
                break;
            }

            // Check if already assigned.
            if bead.assignee.is_some() {
                continue;
            }

            let worker_name = format!("{}-worker-{}", self.rig_name, self.workers.len() + 1);
            info!(
                rig = %self.rig_name,
                worker = %worker_name,
                bead = %bead.id,
                subject = %bead.subject,
                "assigning work"
            );

            let mut worker = Worker::new(
                worker_name,
                self.rig_name.clone(),
                self.provider.clone(),
                self.tools.clone(),
                self.identity.clone(),
                self.model.clone(),
                self.mail_bus.clone(),
                self.beads.clone(),
            );

            worker.assign(&bead);
            self.workers.push(worker);
        }

        // 3. Detect stuck workers (no state change for too long).
        // For now, just log worker states.
        for worker in &self.workers {
            debug!(
                rig = %self.rig_name,
                worker = %worker.name,
                state = ?worker.state,
                "worker status"
            );
        }

        // 4. Report to Familiar.
        let active = self.workers.iter().filter(|w| w.state == WorkerState::Working).count();
        let pending = {
            let store = self.beads.lock().await;
            store.ready().len()
        };

        if active > 0 || pending > 0 {
            self.mail_bus
                .send(Mail::new(
                    &format!("witness-{}", self.rig_name),
                    "familiar",
                    "PATROL",
                    &format!(
                        "Rig {}: {} active workers, {} pending tasks",
                        self.rig_name, active, pending
                    ),
                ))
                .await;
        }

        Ok(())
    }

    /// Execute all hooked workers. Returns the number of workers that ran.
    pub async fn execute_workers(&mut self) -> usize {
        let mut executed = 0;
        for worker in &mut self.workers {
            if worker.state == WorkerState::Hooked {
                if let Err(e) = worker.execute().await {
                    warn!(
                        rig = %self.rig_name,
                        worker = %worker.name,
                        error = %e,
                        "worker execution failed"
                    );
                }
                executed += 1;
            }
        }
        executed
    }

    /// Get worker count by state.
    pub fn worker_counts(&self) -> (usize, usize, usize) {
        let idle = self.workers.iter().filter(|w| w.state == WorkerState::Idle).count();
        let working = self.workers.iter().filter(|w| w.state == WorkerState::Working).count();
        let hooked = self.workers.iter().filter(|w| w.state == WorkerState::Hooked).count();
        (idle, working, hooked)
    }
}
