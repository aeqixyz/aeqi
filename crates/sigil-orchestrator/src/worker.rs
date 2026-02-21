use anyhow::Result;
use sigil_beads::{Bead, BeadStatus};
use sigil_core::traits::{LogObserver, Observer, Tool};
use sigil_core::{Agent, AgentConfig, Identity};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::hook::Hook;
use crate::mail::{Mail, MailBus};

/// Worker states.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerState {
    Idle,
    Hooked,
    Working,
    Done,
    Failed(String),
}

/// A Worker is an ephemeral task executor. Each worker runs as a tokio task
/// with its own identity, hook, and tool allowlist.
pub struct Worker {
    pub name: String,
    pub rig_name: String,
    pub state: WorkerState,
    pub hook: Option<Hook>,
    pub provider: Arc<dyn sigil_core::traits::Provider>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub identity: Identity,
    pub model: String,
    pub mail_bus: Arc<MailBus>,
    pub beads: Arc<Mutex<sigil_beads::BeadStore>>,
}

impl Worker {
    pub fn new(
        name: String,
        rig_name: String,
        provider: Arc<dyn sigil_core::traits::Provider>,
        tools: Vec<Arc<dyn Tool>>,
        identity: Identity,
        model: String,
        mail_bus: Arc<MailBus>,
        beads: Arc<Mutex<sigil_beads::BeadStore>>,
    ) -> Self {
        Self {
            name,
            rig_name,
            state: WorkerState::Idle,
            hook: None,
            provider,
            tools,
            identity,
            model,
            mail_bus,
            beads,
        }
    }

    /// Assign a bead to this worker (set hook).
    pub fn assign(&mut self, bead: &Bead) {
        self.hook = Some(Hook::new(bead.id.clone(), bead.subject.clone()));
        self.state = WorkerState::Hooked;
    }

    /// Execute the hooked work. This is the main worker loop.
    pub async fn execute(&mut self) -> Result<()> {
        let hook = match &self.hook {
            Some(h) => h.clone(),
            None => {
                warn!(worker = %self.name, "no hook assigned, nothing to do");
                return Ok(());
            }
        };

        info!(
            worker = %self.name,
            bead = %hook.bead_id,
            subject = %hook.subject,
            "starting work"
        );

        self.state = WorkerState::Working;

        // Mark bead as in_progress.
        {
            let mut store = self.beads.lock().await;
            let _ = store.update(&hook.bead_id.0, |b| {
                b.status = BeadStatus::InProgress;
                b.assignee = Some(self.name.clone());
            });
        }

        // Build the prompt from the bead.
        let bead_context = {
            let store = self.beads.lock().await;
            match store.get(&hook.bead_id.0) {
                Some(b) => {
                    let mut ctx = format!("## Task: {}\n\n", b.subject);
                    if !b.description.is_empty() {
                        ctx.push_str(&format!("{}\n\n", b.description));
                    }
                    ctx.push_str(&format!("Bead ID: {}\nPriority: {}\n", b.id, b.priority));
                    ctx
                }
                None => format!("Task: {}", hook.subject),
            }
        };

        // Build agent and run.
        let observer: Arc<dyn Observer> = Arc::new(LogObserver);
        let agent_config = AgentConfig {
            model: self.model.clone(),
            max_iterations: 20,
            name: self.name.clone(),
            ..Default::default()
        };

        let agent = Agent::new(
            agent_config,
            self.provider.clone(),
            self.tools.clone(),
            observer,
            self.identity.clone(),
        );

        match agent.run(&bead_context).await {
            Ok(result) => {
                info!(worker = %self.name, bead = %hook.bead_id, "work completed");

                // Close the bead.
                {
                    let mut store = self.beads.lock().await;
                    let _ = store.close(&hook.bead_id.0, &result);
                }

                // Notify witness.
                self.mail_bus
                    .send(Mail::new(
                        &self.name,
                        &format!("witness-{}", self.rig_name),
                        "DONE",
                        &format!("Completed bead {}: {}", hook.bead_id, hook.subject),
                    ))
                    .await;

                self.state = WorkerState::Done;
            }
            Err(e) => {
                warn!(worker = %self.name, bead = %hook.bead_id, error = %e, "work failed");

                // Mark bead back to pending.
                {
                    let mut store = self.beads.lock().await;
                    let _ = store.update(&hook.bead_id.0, |b| {
                        b.status = BeadStatus::Pending;
                        b.assignee = None;
                    });
                }

                // Notify witness of failure.
                self.mail_bus
                    .send(Mail::new(
                        &self.name,
                        &format!("witness-{}", self.rig_name),
                        "FAILED",
                        &format!("Failed bead {}: {}", hook.bead_id, e),
                    ))
                    .await;

                self.state = WorkerState::Failed(e.to_string());
            }
        }

        self.hook = None;
        Ok(())
    }
}
