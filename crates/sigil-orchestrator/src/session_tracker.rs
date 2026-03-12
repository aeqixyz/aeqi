use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use crate::registry::ProjectRegistry;
use sigil_core::traits::{Channel, OutgoingMessage};

/// Session alarm and progress heartbeat system.
///
/// Runs in a dedicated `tokio::spawn` (not inline in the patrol loop).
/// Fires `Channel::send()` to Telegram for:
/// - Periodic sprint check-ins while workers are working
/// - Idle alarm "get back to Architect" when queue is empty
/// - State transitions (active→idle, idle→active)
/// - One-shot deadline alarm when configured session time elapses
///
/// Anti-flood: at most one notification per `min_flood_interval`.
pub struct SessionTracker {
    pub channel: Arc<dyn Channel>,
    pub chat_id: i64,
    pub registry: Arc<ProjectRegistry>,
    pub checkin_interval: Duration,
    pub alarm_interval: Duration,
    pub min_flood_interval: Duration,
    pub deadline: Option<Duration>,
}

impl SessionTracker {
    pub async fn run(self, shutdown: Arc<Notify>) {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let session_start = Instant::now();
        let mut last_sent: Option<Instant> = None;
        let mut last_checkin: Option<Instant> = None;
        let mut last_alarm: Option<Instant> = None;
        let mut was_active = false;
        let mut deadline_fired = false;

        info!("session tracker started");

        loop {
            tokio::select! {
                biased;
                _ = shutdown.notified() => {
                    debug!("session tracker shutting down");
                    break;
                }
                _ = ticker.tick() => {}
            }

            let status = self.registry.status().await;
            let elapsed = session_start.elapsed();
            let (total_working, total_pending, active_projects) = aggregate_status(&status);
            let is_active = total_working > 0 || total_pending > 0;

            let can_send = last_sent.is_none_or(|t| t.elapsed() >= self.min_flood_interval);

            // Determine message (priority order: deadline > transition > periodic).
            let msg: Option<String> = if can_send {
                // 1. One-shot deadline alarm.
                if !deadline_fired
                    && let Some(deadline) = self.deadline
                    && elapsed >= deadline
                {
                    deadline_fired = true;
                    Some(format!(
                        "⏰ Session deadline reached — {} elapsed. Time to check in, Architect.",
                        fmt_duration(elapsed)
                    ))

                // 2. Transition: idle → active (queue filled after empty).
                } else if !was_active && is_active {
                    last_checkin = Some(Instant::now());
                    Some(format!(
                        "🚀 Workers awakened — {} {} queued across {}. Session: {}.",
                        total_pending + total_working,
                        if total_pending + total_working == 1 {
                            "task"
                        } else {
                            "tasks"
                        },
                        active_projects,
                        fmt_duration(elapsed)
                    ))

                // 3. Transition: active → idle (queue emptied).
                } else if was_active && !is_active {
                    last_alarm = Some(Instant::now());
                    Some(format!(
                        "💤 Queue empty — all workers at rest. Session: {}.",
                        fmt_duration(elapsed)
                    ))

                // 4. Periodic check-in while active.
                } else if is_active
                    && last_checkin.is_none_or(|t| t.elapsed() >= self.checkin_interval)
                {
                    last_checkin = Some(Instant::now());
                    Some(format!(
                        "⏱ Sprint check-in: {} {} working across {}. Session: {}.",
                        total_working,
                        if total_working == 1 {
                            "worker"
                        } else {
                            "workers"
                        },
                        active_projects,
                        fmt_duration(elapsed)
                    ))

                // 5. Periodic idle alarm — "come back" reminder.
                } else if !is_active
                    && last_alarm.is_none_or(|t| t.elapsed() >= self.alarm_interval)
                {
                    last_alarm = Some(Instant::now());
                    Some(format!(
                        "🔔 System idle — {} elapsed. Ready for your next command, Architect.",
                        fmt_duration(elapsed)
                    ))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(text) = msg {
                info!(elapsed = ?elapsed, is_active, "session tracker notifying");
                let message = OutgoingMessage {
                    channel: "telegram".to_string(),
                    recipient: "architect".to_string(),
                    text,
                    metadata: serde_json::json!({ "chat_id": self.chat_id }),
                };
                match self.channel.send(message).await {
                    Ok(()) => {
                        last_sent = Some(Instant::now());
                    }
                    Err(e) => {
                        warn!(error = %e, "session tracker failed to send notification");
                    }
                }
            }

            was_active = is_active;
        }

        info!("session tracker stopped");
    }
}

/// Summarise total working/pending counts and active project names.
fn aggregate_status(status: &crate::registry::RegistryStatus) -> (usize, usize, String) {
    let mut total_working = 0usize;
    let mut total_pending = 0usize;
    let mut active_names: Vec<&str> = Vec::new();

    for d in &status.projects {
        let working = d.workers_working + d.workers_bonded;
        let pending = d.open_tasks + d.ready_tasks;
        if working > 0 || pending > 0 {
            active_names.push(&d.name);
        }
        total_working += working;
        total_pending += pending;
    }

    let projects_str = match active_names.len() {
        0 => "no projects".to_string(),
        1 => active_names[0].to_string(),
        n => format!("{n} projects"),
    };

    (total_working, total_pending, projects_str)
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}
