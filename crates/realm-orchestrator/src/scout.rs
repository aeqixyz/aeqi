use anyhow::Result;
use realm_quests::{QuestStatus, QuestBoard};
use realm_core::config::ExecutionMode;
use realm_core::traits::{Memory, Provider, Tool};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::checkpoint::SpiritCheckpoint;
use crate::cost_ledger::{CostLedger, CostEntry};
use crate::executor::{ClaudeCodeExecutor, QuestOutcome};
use crate::metrics::RealmMetrics;
use crate::whisper::{Whisper, WhisperBus, WhisperKind};
use crate::domain::Domain;
use crate::spirit::Spirit;

/// Max resolution attempts at the rig level before escalating to Familiar.
/// Each attempt spawns a new worker to try to answer the blocker question.
const MAX_RIG_RESOLUTION_ATTEMPTS: u32 = 1;

/// Label prefix for tracking escalation depth on beads.
const ESCALATION_LABEL_PREFIX: &str = "escalation:";

/// A running spirit with age tracking for timeout detection.
struct TrackedSpirit {
    handle: tokio::task::JoinHandle<()>,
    quest_id: String,
    started_at: std::time::Instant,
}

/// Scout: per-rig supervisor. Runs patrol cycles, manages workers,
/// detects stuck/orphaned beads, handles escalation, reports to Familiar.
pub struct Scout {
    pub domain_name: String,
    pub max_workers: u32,
    pub patrol_interval_secs: u64,
    pub whisper_bus: Arc<WhisperBus>,
    pub beads: Arc<Mutex<QuestBoard>>,
    /// Execution mode for this rig's workers.
    pub execution_mode: ExecutionMode,
    // Agent-mode fields (used when execution_mode == Agent).
    pub provider: Arc<dyn Provider>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub model: String,
    pub identity: realm_core::Identity,
    // ClaudeCode-mode fields (used when execution_mode == ClaudeCode).
    /// Rig's repo path for Claude Code working directory.
    pub repo: Option<std::path::PathBuf>,
    /// Max turns per Claude Code execution.
    pub cc_max_turns: u32,
    /// Max budget per Claude Code execution.
    pub cc_max_budget_usd: Option<f64>,
    /// Background worker tasks with age tracking.
    running_tasks: Vec<TrackedSpirit>,
    /// Timeout in seconds for spirit execution. Hung spirits are aborted after this.
    spirit_timeout_secs: u64,
    /// Last reported state — only send mail on change.
    last_report: (usize, usize),
    /// Shared cost ledger for budget enforcement.
    pub cost_ledger: Option<Arc<CostLedger>>,
    /// Shared metrics registry.
    pub metrics: Option<Arc<RealmMetrics>>,
    /// Memory instance for spirit reflection (domain-scoped).
    pub memory: Option<Arc<dyn Memory>>,
    /// Provider used for post-execution reflection (cheap model).
    pub reflect_provider: Option<Arc<dyn Provider>>,
    /// Model used for reflection extraction.
    pub reflect_model: String,
}

impl Scout {
    pub fn new(domain: &Domain, provider: Arc<dyn Provider>, tools: Vec<Arc<dyn Tool>>, whisper_bus: Arc<WhisperBus>) -> Self {
        Self {
            domain_name: domain.name.clone(),
            max_workers: domain.max_workers,
            patrol_interval_secs: 60,
            whisper_bus,
            beads: domain.quests.clone(),
            execution_mode: ExecutionMode::Agent,
            provider,
            tools,
            model: domain.model.clone(),
            identity: domain.identity.clone(),
            repo: None,
            cc_max_turns: 25,
            cc_max_budget_usd: None,
            running_tasks: Vec::new(),
            spirit_timeout_secs: domain.spirit_timeout_secs,
            last_report: (0, 0),
            cost_ledger: None,
            metrics: None,
            memory: None,
            reflect_provider: None,
            reflect_model: String::new(),
        }
    }

    /// Set execution mode to Claude Code with rig-specific settings.
    pub fn set_claude_code_mode(
        &mut self,
        repo: std::path::PathBuf,
        model: String,
        max_turns: u32,
        max_budget_usd: Option<f64>,
    ) {
        self.execution_mode = ExecutionMode::ClaudeCode;
        self.repo = Some(repo);
        self.model = model;
        self.cc_max_turns = max_turns;
        self.cc_max_budget_usd = max_budget_usd;
    }

    /// Create a worker based on the rig's execution mode.
    fn create_worker(&self, spirit_name: String) -> Spirit {
        let mut spirit = match self.execution_mode {
            ExecutionMode::Agent => Spirit::new(
                spirit_name,
                self.domain_name.clone(),
                self.provider.clone(),
                self.tools.clone(),
                self.identity.clone(),
                self.model.clone(),
                self.whisper_bus.clone(),
                self.beads.clone(),
            ),
            ExecutionMode::ClaudeCode => {
                let workdir = self.repo.clone().unwrap_or_default();
                let executor = ClaudeCodeExecutor::new(
                    workdir,
                    self.model.clone(),
                    self.cc_max_turns,
                    self.cc_max_budget_usd,
                );
                Spirit::new_claude_code(
                    spirit_name,
                    self.domain_name.clone(),
                    executor,
                    self.identity.clone(),
                    self.whisper_bus.clone(),
                    self.beads.clone(),
                )
            }
        };
        if let Some(ref mem) = self.memory {
            spirit = spirit.with_memory(mem.clone());
        }
        if let Some(ref provider) = self.reflect_provider {
            spirit = spirit.with_reflect(provider.clone(), self.reflect_model.clone());
        }
        spirit
    }

    /// Run one patrol cycle: reap finished tasks, detect timeouts,
    /// assign + launch ready work, handle blocked beads, report status.
    ///
    /// Spirit execution is fully non-blocking — each worker runs as a background
    /// tokio task. The daemon loop never stalls waiting for workers.
    pub async fn patrol(&mut self) -> Result<()> {
        let patrol_start = std::time::Instant::now();
        debug!(domain = %self.domain_name, "patrol cycle");

        // 0. Reload beads from disk to pick up externally-created beads
        //    (e.g., from `sg assign` CLI or Claude Code workers).
        {
            let mut store = self.beads.lock().await;
            if let Err(e) = store.reload() {
                warn!(domain = %self.domain_name, error = %e, "failed to reload beads from disk");
            }
        }

        // 1. Reap completed tasks + detect timed-out spirits.
        let timeout = std::time::Duration::from_secs(self.spirit_timeout_secs);
        let mut timed_out = Vec::new();
        self.running_tasks.retain(|t| {
            if t.handle.is_finished() {
                return false;
            }
            if t.started_at.elapsed() > timeout {
                t.handle.abort();
                timed_out.push(t.quest_id.clone());
                return false;
            }
            true
        });

        // Reset timed-out quests back to Pending and notify shadow.
        if !timed_out.is_empty()
            && let Some(ref m) = self.metrics {
                m.spirits_timed_out.inc_by(timed_out.len() as u64);
            }
        for quest_id in timed_out {
            warn!(
                domain = %self.domain_name,
                quest = %quest_id,
                timeout_secs = self.spirit_timeout_secs,
                "spirit timed out, aborting and resetting quest"
            );

            // Capture external checkpoint before resetting — preserve git state evidence.
            if let Some(ref repo) = self.repo {
                match SpiritCheckpoint::capture(repo) {
                    Ok(checkpoint) => {
                        let checkpoint: SpiritCheckpoint = checkpoint
                            .with_quest_id(&quest_id)
                            .with_spirit_name(format!("timeout-{}", quest_id))
                            .with_progress_notes(format!(
                                "Spirit timed out after {}s — checkpoint captured externally by scout",
                                self.spirit_timeout_secs
                            ));

                        let cp_path = SpiritCheckpoint::path_for_quest(repo, &quest_id);
                        if let Err(e) = checkpoint.write(&cp_path) {
                            warn!(domain = %self.domain_name, quest = %quest_id, error = %e, "failed to write timeout checkpoint");
                        } else {
                            info!(domain = %self.domain_name, quest = %quest_id, files = checkpoint.modified_files.len(), "timeout checkpoint captured");
                        }
                    }
                    Err(e) => {
                        warn!(domain = %self.domain_name, quest = %quest_id, error = %e, "failed to capture timeout checkpoint");
                    }
                }
            }

            {
                let mut store = self.beads.lock().await;
                let _ = store.update(&quest_id, |q| {
                    q.status = QuestStatus::Pending;
                    q.assignee = None;
                });
            }
            self.whisper_bus
                .send(Whisper::new_typed(
                    &format!("scout-{}", self.domain_name),
                    "familiar",
                    WhisperKind::SpiritCrashed {
                        domain: self.domain_name.clone(),
                        spirit: format!("timeout-{}", quest_id),
                        error: format!(
                            "Spirit timed out after {}s on quest {}",
                            self.spirit_timeout_secs, quest_id
                        ),
                    },
                ))
                .await;
        }

        // 2. Handle blocked beads — attempt resolution or escalate.
        self.handle_blocked_beads().await;

        // 3. Assign + launch ready beads as background tasks.
        let ready_quests = {
            let store = self.beads.lock().await;
            store.ready().into_iter().cloned().collect::<Vec<_>>()
        };

        for bead in ready_quests {
            if self.running_tasks.len() >= self.max_workers as usize {
                break;
            }

            if bead.assignee.is_some() {
                continue;
            }

            // Budget check: don't spawn if global daily budget or domain budget is exhausted.
            if let Some(ref ledger) = self.cost_ledger
                && !ledger.can_afford_domain(&self.domain_name) {
                    let (spent, budget, _) = ledger.domain_budget_status(&self.domain_name);
                    warn!(
                        domain = %self.domain_name,
                        spent_usd = spent,
                        budget_usd = budget,
                        "budget exhausted for domain, skipping quest"
                    );
                    break;
                }

            let spirit_idx = self.running_tasks.len() + 1;
            let spirit_name = format!("{}-worker-{}", self.domain_name, spirit_idx);
            info!(
                domain = %self.domain_name,
                worker = %spirit_name,
                bead = %bead.id,
                subject = %bead.subject,
                mode = ?self.execution_mode,
                "assigning work"
            );

            let mut worker = self.create_worker(spirit_name);

            // If there's a previous external checkpoint for this quest, inject it into the
            // quest description so the new spirit has context about the prior attempt's git state.
            if let Some(ref repo) = self.repo {
                let cp_path = SpiritCheckpoint::path_for_quest(repo, &bead.id.0);
                match SpiritCheckpoint::read(&cp_path) {
                    Ok(Some(checkpoint)) => {
                        let checkpoint_ctx: String = checkpoint.as_context();
                        let mut store = self.beads.lock().await;
                        let _ = store.update(&bead.id.0, |b| {
                            b.description.push_str(&format!(
                                "\n\n---\n{checkpoint_ctx}"
                            ));
                        });
                        info!(
                            domain = %self.domain_name,
                            quest = %bead.id,
                            "injected external checkpoint context from previous spirit"
                        );
                        // Remove the checkpoint file — it's been consumed.
                        let _ = SpiritCheckpoint::remove(&cp_path);
                    }
                    Ok(None) => {} // No checkpoint — normal launch.
                    Err(e) => {
                        warn!(
                            domain = %self.domain_name,
                            quest = %bead.id,
                            error = %e,
                            "failed to read checkpoint — launching without it"
                        );
                    }
                }
            }

            worker.assign(&bead);

            let quest_id = bead.id.0.clone();

            // Fire-and-forget: worker handles its own bead updates + mail notifications.
            let domain_name_task = self.domain_name.clone();
            let quest_id_task = quest_id.clone();
            let cost_ledger = self.cost_ledger.clone();
            let metrics = self.metrics.clone();
            if let Some(ref m) = self.metrics {
                m.spirits_spawned.inc();
            }
            let handle = tokio::spawn(async move {
                let start = std::time::Instant::now();
                match worker.execute().await {
                    Ok((outcome, cost_usd, turns)) => {
                        let duration_secs = start.elapsed().as_secs_f64();
                        debug!(
                            domain = %domain_name_task,
                            outcome = ?std::mem::discriminant(&outcome),
                            cost_usd,
                            turns,
                            duration_secs,
                            "worker finished"
                        );

                        // Record to cost ledger.
                        if let Some(ref ledger) = cost_ledger {
                            let _ = ledger.record(CostEntry {
                                domain: domain_name_task.clone(),
                                quest_id: quest_id_task.clone(),
                                spirit: "spirit".to_string(),
                                cost_usd,
                                turns,
                                timestamp: chrono::Utc::now(),
                            });
                        }

                        // Record metrics.
                        if let Some(ref m) = metrics {
                            m.spirit_duration_seconds.observe(duration_secs);
                            m.quest_cost_usd.observe(cost_usd);
                            match &outcome {
                                QuestOutcome::Done(_) => m.quests_completed.inc(),
                                QuestOutcome::Blocked { .. } | QuestOutcome::Handoff { .. } => m.quests_blocked.inc(),
                                QuestOutcome::Failed(_) => m.quests_failed.inc(),
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            domain = %domain_name_task,
                            error = %e,
                            "worker execution error"
                        );
                        if let Some(ref m) = metrics {
                            m.quests_failed.inc();
                        }
                    }
                }
            });
            self.running_tasks.push(TrackedSpirit {
                handle,
                quest_id,
                started_at: std::time::Instant::now(),
            });
        }

        // 4. Report to Familiar (only on state change).
        let active = self.running_tasks.len();
        let pending = {
            let store = self.beads.lock().await;
            store.ready().len()
        };

        let current = (active, pending);
        if current != self.last_report && (active > 0 || pending > 0) {
            self.last_report = current;
            self.whisper_bus
                .send(Whisper::new_typed(
                    &format!("scout-{}", self.domain_name),
                    "familiar",
                    WhisperKind::PatrolReport {
                        domain: self.domain_name.clone(),
                        active,
                        pending,
                    },
                ))
                .await;
        } else if current != self.last_report {
            self.last_report = current;
        }

        // 5. Record patrol metrics.
        if let Some(ref m) = self.metrics {
            m.patrol_cycles.inc();
            m.patrol_cycle_seconds.observe(patrol_start.elapsed().as_secs_f64());
            m.spirits_active.set(active as f64);
            m.quests_pending.set(pending as f64);
        }

        Ok(())
    }

    /// Handle blocked beads: attempt rig-level resolution or escalate to Familiar.
    ///
    /// Escalation chain:
    ///   1. Spirit BLOCKED → Scout spawns resolver worker (same rig, has full codebase access)
    ///   2. Resolver answers → Scout appends answer to bead, resets to Pending for re-attempt
    ///   3. Resolver also blocked → Scout escalates to Familiar via mail
    ///   4. Familiar tries (has KNOWLEDGE.md + cross-rig context)
    ///   5. Familiar resolves → sends RESOLVED mail back → Scout re-opens bead
    ///   6. Familiar stuck → routes to human via Telegram
    async fn handle_blocked_beads(&mut self) {
        let blocked_beads = {
            let store = self.beads.lock().await;
            store.all().into_iter()
                .filter(|b| b.status == QuestStatus::Blocked)
                .cloned()
                .collect::<Vec<_>>()
        };

        for bead in blocked_beads {
            let escalation_depth = Self::get_escalation_depth(&bead.labels);

            if escalation_depth >= MAX_RIG_RESOLUTION_ATTEMPTS {
                // Already tried rig-level resolution. Escalate to Familiar.
                self.escalate_to_familiar(&bead).await;
            } else {
                // Attempt rig-level resolution: re-open as Pending with resolution context.
                self.attempt_rig_resolution(&bead, escalation_depth).await;
            }
        }
    }

    /// Attempt to resolve a blocker at the rig level.
    ///
    /// Increments escalation depth, appends the blocker question to the bead
    /// description as resolution context, and resets to Pending so a new worker
    /// picks it up with the full context.
    async fn attempt_rig_resolution(
        &self,
        bead: &realm_quests::Quest,
        current_depth: u32,
    ) {
        let new_depth = current_depth + 1;
        let new_label = format!("{ESCALATION_LABEL_PREFIX}{new_depth}");

        // Extract the blocker question from closed_reason or description.
        let blocker_context = bead.closed_reason.as_deref()
            .unwrap_or("(no blocker details captured)");

        info!(
            domain = %self.domain_name,
            bead = %bead.id,
            depth = new_depth,
            "attempting rig-level resolution"
        );

        // Update bead: append resolution context, increment depth, reset to Pending.
        let mut store = self.beads.lock().await;
        let _ = store.update(&bead.id.0, |b| {
            // Append blocker context to description so the next worker sees it.
            b.description.push_str(&format!(
                "\n\n---\n## Resolution Attempt {new_depth}\n\n\
                 A previous worker was blocked on this task. \
                 Before continuing the original task, first try to answer this question \
                 using the codebase, documentation, and your knowledge. \
                 If you can answer it, proceed with the original task using that answer. \
                 If you genuinely cannot determine the answer, respond with BLOCKED: again.\n\n\
                 **Blocker question:**\n{blocker_context}\n"
            ));

            // Track escalation depth.
            b.labels.retain(|l| !l.starts_with(ESCALATION_LABEL_PREFIX));
            b.labels.push(new_label);

            // Reset to Pending so patrol picks it up for a new worker.
            b.status = QuestStatus::Pending;
            b.assignee = None;
        });
    }

    /// Escalate a blocked bead to the Familiar for cross-rig resolution.
    ///
    /// The Familiar has KNOWLEDGE.md with operational learnings and cross-rig
    /// awareness. If it can't resolve either, it routes to human via Telegram.
    async fn escalate_to_familiar(&self, bead: &realm_quests::Quest) {
        // Only escalate once — check if we already sent an ESCALATE mail for this bead.
        if bead.labels.iter().any(|l| l == "escalated_to_familiar") {
            return;
        }

        info!(
            domain = %self.domain_name,
            bead = %bead.id,
            "escalating to familiar — rig-level resolution exhausted"
        );

        // Mark bead as escalated + record metric.
        if let Some(ref m) = self.metrics {
            m.escalations_total.inc();
        }
        {
            let mut store = self.beads.lock().await;
            let _ = store.update(&bead.id.0, |b| {
                b.labels.push("escalated_to_familiar".to_string());
            });
        }

        // Send escalation mail to Familiar with full context.
        self.whisper_bus
            .send(Whisper::new_typed(
                &format!("scout-{}", self.domain_name),
                "familiar",
                WhisperKind::Escalation {
                    domain: self.domain_name.clone(),
                    quest_id: bead.id.to_string(),
                    subject: bead.subject.clone(),
                    description: format!(
                        "Priority: {}\n\nFull description:\n{}\n\n\
                         This quest has been blocked after {} resolution attempt(s) at the domain level. \
                         Please try to resolve using your cross-domain knowledge (KNOWLEDGE.md). \
                         If you can answer the blocker question, send a RESOLVED whisper back to \
                         scout-{} with the answer. If you cannot resolve it, escalate to the \
                         human operator via Telegram.",
                        bead.priority,
                        bead.description,
                        Self::get_escalation_depth(&bead.labels),
                        self.domain_name,
                    ),
                    attempts: Self::get_escalation_depth(&bead.labels),
                },
            ))
            .await;
    }

    /// Process a RESOLVED mail from the Familiar: re-open the blocked bead
    /// with the answer appended to the description.
    pub async fn handle_resolution(&self, quest_id: &str, answer: &str) {
        info!(
            domain = %self.domain_name,
            bead = %quest_id,
            "received resolution from familiar"
        );

        let mut store = self.beads.lock().await;
        let _ = store.update(quest_id, |b| {
            b.description.push_str(&format!(
                "\n\n---\n## Resolution (from Familiar)\n\n{answer}\n\n\
                 **Now proceed with the original task using this answer.**\n"
            ));
            b.status = QuestStatus::Pending;
            b.assignee = None;
            // Remove escalation labels — fresh start with the answer.
            b.labels.retain(|l| {
                !l.starts_with(ESCALATION_LABEL_PREFIX) && l != "escalated_to_familiar"
            });
        });
    }

    /// Get escalation depth from bead labels.
    fn get_escalation_depth(labels: &[String]) -> u32 {
        labels.iter()
            .filter_map(|l| l.strip_prefix(ESCALATION_LABEL_PREFIX))
            .filter_map(|n| n.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
    }

    /// Get worker count by state: (idle, running, 0).
    /// Spirits launch immediately as background tasks — no "hooked" state.
    pub fn spirit_counts(&self) -> (usize, usize, usize) {
        let running = self.running_tasks.iter().filter(|t| !t.handle.is_finished()).count();
        let capacity = self.max_workers as usize;
        let idle = capacity.saturating_sub(running);
        (idle, running, 0)
    }
}
