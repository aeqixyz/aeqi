use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use crate::mail::MailBus;
use crate::rig::Rig;
use crate::witness::Witness;

/// RigRegistry: shared data structure holding all rigs, witnesses, and the mail bus.
/// Replaces the old Familiar coordinator struct. The registry is a pure data structure —
/// the daemon loop and CLI commands use it directly.
pub struct RigRegistry {
    rigs: RwLock<HashMap<String, Arc<Rig>>>,
    witnesses: RwLock<HashMap<String, Mutex<Witness>>>,
    pub mail_bus: Arc<MailBus>,
}

impl RigRegistry {
    pub fn new(mail_bus: Arc<MailBus>) -> Self {
        Self {
            rigs: RwLock::new(HashMap::new()),
            witnesses: RwLock::new(HashMap::new()),
            mail_bus,
        }
    }

    /// Register a rig with its witness.
    pub async fn register_rig(&self, rig: Arc<Rig>, witness: Witness) {
        let name = rig.name.clone();
        self.rigs.write().await.insert(name.clone(), rig);
        self.witnesses.write().await.insert(name, Mutex::new(witness));
    }

    /// Assign a task to a specific rig by creating a bead.
    pub async fn assign(&self, rig_name: &str, subject: &str, description: &str) -> Result<sigil_beads::Bead> {
        let rigs = self.rigs.read().await;
        let rig = rigs
            .get(rig_name)
            .ok_or_else(|| anyhow::anyhow!("rig not found: {rig_name}"))?;

        let mut bead = rig.create_bead(subject).await?;

        if !description.is_empty() {
            let mut store = rig.beads.lock().await;
            bead = store.update(&bead.id.0, |b| {
                b.description = description.to_string();
            })?;
        }

        info!(
            rig = %rig_name,
            bead = %bead.id,
            subject = %subject,
            "task assigned"
        );

        Ok(bead)
    }

    /// Run one patrol cycle: read familiar mail, then patrol all witnesses.
    pub async fn patrol_all(&self) -> Result<()> {
        // Read familiar mail first.
        let mail = self.mail_bus.read("familiar").await;
        for m in &mail {
            match m.subject.as_str() {
                "PATROL" => {
                    info!(from = %m.from, body = %m.body, "witness report");
                }
                "WORKER_CRASHED" => {
                    warn!(from = %m.from, body = %m.body, "worker crashed");
                }
                _ => {
                    info!(from = %m.from, subject = %m.subject, "mail received");
                }
            }
        }

        // Patrol each witness.
        let witnesses = self.witnesses.read().await;
        for (name, witness) in witnesses.iter() {
            let mut w = witness.lock().await;
            if let Err(e) = w.patrol().await {
                warn!(rig = %name, error = %e, "witness patrol failed");
            }
        }

        Ok(())
    }

    /// Execute all ready workers across all rigs.
    pub async fn execute_all(&self) -> usize {
        let mut total = 0;
        let witnesses = self.witnesses.read().await;
        for witness in witnesses.values() {
            let mut w = witness.lock().await;
            total += w.execute_workers().await;
        }
        total
    }

    /// Get status summary across all rigs.
    pub async fn status(&self) -> RegistryStatus {
        let mut rig_statuses = Vec::new();
        let rigs = self.rigs.read().await;
        let witnesses = self.witnesses.read().await;

        for (name, rig) in rigs.iter() {
            let open = rig.open_beads().await.len();
            let ready = rig.ready_beads().await.len();
            let (idle, working, hooked) = if let Some(w) = witnesses.get(name) {
                w.lock().await.worker_counts()
            } else {
                (0, 0, 0)
            };

            rig_statuses.push(RigStatus {
                name: name.clone(),
                open_beads: open,
                ready_beads: ready,
                workers_idle: idle,
                workers_working: working,
                workers_hooked: hooked,
            });
        }

        let unread = self.mail_bus.unread_count("familiar").await;

        RegistryStatus {
            rigs: rig_statuses,
            unread_mail: unread,
        }
    }

    /// Get all ready beads across all rigs.
    pub async fn all_ready(&self) -> Vec<(String, sigil_beads::Bead)> {
        let mut all = Vec::new();
        let rigs = self.rigs.read().await;
        for (name, rig) in rigs.iter() {
            for bead in rig.ready_beads().await {
                all.push((name.clone(), bead));
            }
        }
        all
    }

    /// List all registered rig names.
    pub async fn rig_names(&self) -> Vec<String> {
        self.rigs.read().await.keys().cloned().collect()
    }

    /// Get a rig by name.
    pub async fn get_rig(&self, name: &str) -> Option<Arc<Rig>> {
        self.rigs.read().await.get(name).cloned()
    }

    /// Get rig count.
    pub async fn rig_count(&self) -> usize {
        self.rigs.read().await.len()
    }

    /// Get total max workers across all rigs.
    pub async fn total_max_workers(&self) -> u32 {
        self.rigs.read().await.values().map(|r| r.max_workers).sum()
    }

    /// Get all rigs as JSON-serializable values (for IPC).
    pub async fn rigs_info(&self) -> Vec<serde_json::Value> {
        self.rigs.read().await.values().map(|r| {
            serde_json::json!({
                "name": r.name,
                "prefix": r.prefix,
                "model": r.model,
                "max_workers": r.max_workers,
            })
        }).collect()
    }
}

#[derive(Debug)]
pub struct RegistryStatus {
    pub rigs: Vec<RigStatus>,
    pub unread_mail: usize,
}

#[derive(Debug)]
pub struct RigStatus {
    pub name: String,
    pub open_beads: usize,
    pub ready_beads: usize,
    pub workers_idle: usize,
    pub workers_working: usize,
    pub workers_hooked: usize,
}
