use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

use crate::mail::MailBus;
use crate::rig::Rig;
use crate::witness::Witness;

/// The Familiar: global coordinator (Mayor role).
/// Routes work to rigs, handles cross-rig coordination, talks to Emperor.
pub struct Familiar {
    pub rigs: HashMap<String, Arc<Rig>>,
    pub witnesses: HashMap<String, Witness>,
    pub mail_bus: Arc<MailBus>,
}

impl Familiar {
    pub fn new(mail_bus: Arc<MailBus>) -> Self {
        Self {
            rigs: HashMap::new(),
            witnesses: HashMap::new(),
            mail_bus,
        }
    }

    /// Register a rig with its witness.
    pub fn register_rig(&mut self, rig: Arc<Rig>, witness: Witness) {
        self.witnesses.insert(rig.name.clone(), witness);
        self.rigs.insert(rig.name.clone(), rig);
    }

    /// Assign a task to a specific rig.
    pub async fn assign(&mut self, rig_name: &str, subject: &str, description: &str) -> Result<sigil_beads::Bead> {
        let rig = self
            .rigs
            .get(rig_name)
            .ok_or_else(|| anyhow::anyhow!("rig not found: {rig_name}"))?;

        let mut bead = rig.create_bead(subject).await?;

        // Update with description if provided.
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

    /// Run one patrol cycle across all rigs.
    pub async fn patrol(&mut self) -> Result<()> {
        // Read our mail first.
        let mail = self.mail_bus.read("familiar").await;
        for m in &mail {
            match m.subject.as_str() {
                "PATROL" => {
                    // Witness status reports — just log.
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
        for (name, witness) in &mut self.witnesses {
            if let Err(e) = witness.patrol().await {
                warn!(rig = %name, error = %e, "witness patrol failed");
            }
        }

        Ok(())
    }

    /// Execute all ready workers across all rigs.
    pub async fn execute_all(&mut self) -> usize {
        let mut total = 0;
        for witness in self.witnesses.values_mut() {
            total += witness.execute_workers().await;
        }
        total
    }

    /// Get status summary.
    pub async fn status(&self) -> FamiliarStatus {
        let mut rig_statuses = Vec::new();

        for (name, rig) in &self.rigs {
            let open = rig.open_beads().await.len();
            let ready = rig.ready_beads().await.len();
            let witness = self.witnesses.get(name);
            let (idle, working, hooked) = witness
                .map(|w| w.worker_counts())
                .unwrap_or((0, 0, 0));

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

        FamiliarStatus {
            rigs: rig_statuses,
            unread_mail: unread,
        }
    }

    /// Get all ready beads across all rigs.
    pub async fn all_ready(&self) -> Vec<(String, sigil_beads::Bead)> {
        let mut all = Vec::new();
        for (name, rig) in &self.rigs {
            for bead in rig.ready_beads().await {
                all.push((name.clone(), bead));
            }
        }
        all
    }
}

#[derive(Debug)]
pub struct FamiliarStatus {
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
