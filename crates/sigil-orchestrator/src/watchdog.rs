//! Watchdog Triggers — event-driven automation for orchestration events.
//!
//! Evaluates conditions against the audit trail (Phase 1) and fires actions
//! like creating tasks, sending dispatches, or pausing projects when thresholds
//! are met. Examples: "3 failures in 1 hour" or "budget > 80%".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::audit::{AuditLog, DecisionType};

/// Events that can trigger watchdog rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchdogEvent {
    TaskCompleted,
    TaskFailed,
    TaskBlocked,
    BudgetThresholdReached,
    CostSpike,
    WorkerTimeout,
    EscalationCreated,
}

/// Condition that must be met for a watchdog rule to fire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogCondition {
    pub event: WatchdogEvent,
    #[serde(default)]
    pub project_filter: Option<String>,
    #[serde(default)]
    pub label_filter: Option<String>,
    #[serde(default = "default_count_threshold")]
    pub count_threshold: u32,
    #[serde(default = "default_window_secs")]
    pub window_secs: u64,
    #[serde(default)]
    pub budget_threshold: Option<f64>,
}

fn default_count_threshold() -> u32 {
    3
}
fn default_window_secs() -> u64 {
    3600
}

/// Action to take when a watchdog rule fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchdogAction {
    CreateTask {
        project: String,
        subject: String,
        description: String,
    },
    SendDispatch {
        to: String,
        message: String,
    },
    Escalate {
        message: String,
    },
    PauseProject {
        project: String,
    },
    RunCommand {
        command: String,
    },
}

/// A complete watchdog rule: condition + action + cooldown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogRule {
    pub name: String,
    pub condition: WatchdogCondition,
    pub action: WatchdogAction,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_cooldown() -> u64 {
    3600
}
fn default_enabled() -> bool {
    true
}

/// Engine that evaluates watchdog rules against audit events.
pub struct WatchdogEngine {
    rules: Vec<WatchdogRule>,
    last_fired: HashMap<String, DateTime<Utc>>,
}

impl WatchdogEngine {
    pub fn new(rules: Vec<WatchdogRule>) -> Self {
        Self {
            rules,
            last_fired: HashMap::new(),
        }
    }

    /// Evaluate all rules against the audit log. Returns names of fired rules
    /// and their actions.
    pub fn evaluate(
        &mut self,
        audit_log: &AuditLog,
        budget_pct: Option<f64>,
    ) -> Vec<(String, WatchdogAction)> {
        let mut fired = Vec::new();
        let now = Utc::now();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            // Check cooldown.
            if let Some(last) = self.last_fired.get(&rule.name) {
                let elapsed = (now - *last).num_seconds() as u64;
                if elapsed < rule.cooldown_secs {
                    continue;
                }
            }

            let condition_met = self.check_condition(&rule.condition, audit_log, budget_pct);
            if condition_met {
                fired.push((rule.name.clone(), rule.action.clone()));
                self.last_fired.insert(rule.name.clone(), now);
            }
        }

        fired
    }

    fn check_condition(
        &self,
        condition: &WatchdogCondition,
        audit_log: &AuditLog,
        budget_pct: Option<f64>,
    ) -> bool {
        // Budget threshold check (doesn't need audit trail).
        if condition.event == WatchdogEvent::BudgetThresholdReached {
            if let (Some(threshold), Some(pct)) = (condition.budget_threshold, budget_pct) {
                return pct >= threshold;
            }
            return false;
        }

        // Map watchdog event to decision type for audit query.
        let decision_type = match condition.event {
            WatchdogEvent::TaskCompleted => DecisionType::TaskCompleted,
            WatchdogEvent::TaskFailed => DecisionType::TaskFailed,
            WatchdogEvent::TaskBlocked => DecisionType::TaskBlocked,
            WatchdogEvent::WorkerTimeout => DecisionType::WorkerTimedOut,
            WatchdogEvent::EscalationCreated => DecisionType::TaskEscalated,
            WatchdogEvent::CostSpike => DecisionType::BudgetBlocked,
            WatchdogEvent::BudgetThresholdReached => return false,
        };

        // Query recent events from audit log.
        let events = if let Some(ref project) = condition.project_filter {
            audit_log.query_by_project(project).unwrap_or_default()
        } else {
            audit_log.query_recent(1000).unwrap_or_default()
        };

        // Filter by time window and decision type.
        let cutoff = Utc::now() - chrono::Duration::seconds(condition.window_secs as i64);
        let matching_count = events
            .iter()
            .filter(|e| e.timestamp > cutoff && e.decision_type == decision_type)
            .count() as u32;

        matching_count >= condition.count_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditEvent, AuditLog};
    use tempfile::TempDir;

    fn temp_audit() -> (AuditLog, TempDir) {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::open(&dir.path().join("audit.db")).unwrap();
        (log, dir)
    }

    #[test]
    fn test_condition_met_count_threshold() {
        let (audit, _dir) = temp_audit();

        // Record 3 timeout events
        for i in 0..3 {
            audit
                .record(
                    &AuditEvent::new("proj", DecisionType::WorkerTimedOut, format!("timeout {i}"))
                        .with_task(format!("t-{i:03}")),
                )
                .unwrap();
        }

        let rules = vec![WatchdogRule {
            name: "timeout-alert".to_string(),
            condition: WatchdogCondition {
                event: WatchdogEvent::WorkerTimeout,
                project_filter: Some("proj".to_string()),
                label_filter: None,
                count_threshold: 3,
                window_secs: 3600,
                budget_threshold: None,
            },
            action: WatchdogAction::Escalate {
                message: "Too many timeouts".to_string(),
            },
            cooldown_secs: 3600,
            enabled: true,
        }];

        let mut engine = WatchdogEngine::new(rules);
        let fired = engine.evaluate(&audit, None);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].0, "timeout-alert");
    }

    #[test]
    fn test_condition_not_met_below_threshold() {
        let (audit, _dir) = temp_audit();

        // Only 2 events (threshold is 3)
        for i in 0..2 {
            audit
                .record(&AuditEvent::new(
                    "proj",
                    DecisionType::WorkerTimedOut,
                    format!("timeout {i}"),
                ))
                .unwrap();
        }

        let rules = vec![WatchdogRule {
            name: "timeout-alert".to_string(),
            condition: WatchdogCondition {
                event: WatchdogEvent::WorkerTimeout,
                project_filter: Some("proj".to_string()),
                label_filter: None,
                count_threshold: 3,
                window_secs: 3600,
                budget_threshold: None,
            },
            action: WatchdogAction::Escalate {
                message: "Too many timeouts".to_string(),
            },
            cooldown_secs: 3600,
            enabled: true,
        }];

        let mut engine = WatchdogEngine::new(rules);
        let fired = engine.evaluate(&audit, None);
        assert!(fired.is_empty());
    }

    #[test]
    fn test_cooldown_prevents_refire() {
        let (audit, _dir) = temp_audit();

        for i in 0..5 {
            audit
                .record(&AuditEvent::new(
                    "proj",
                    DecisionType::WorkerTimedOut,
                    format!("timeout {i}"),
                ))
                .unwrap();
        }

        let rules = vec![WatchdogRule {
            name: "timeout-alert".to_string(),
            condition: WatchdogCondition {
                event: WatchdogEvent::WorkerTimeout,
                project_filter: Some("proj".to_string()),
                label_filter: None,
                count_threshold: 3,
                window_secs: 3600,
                budget_threshold: None,
            },
            action: WatchdogAction::Escalate {
                message: "Too many timeouts".to_string(),
            },
            cooldown_secs: 3600,
            enabled: true,
        }];

        let mut engine = WatchdogEngine::new(rules);

        // First evaluation fires
        let fired1 = engine.evaluate(&audit, None);
        assert_eq!(fired1.len(), 1);

        // Second evaluation within cooldown does NOT fire
        let fired2 = engine.evaluate(&audit, None);
        assert!(fired2.is_empty());
    }

    #[test]
    fn test_budget_threshold() {
        let (audit, _dir) = temp_audit();

        let rules = vec![WatchdogRule {
            name: "budget-warning".to_string(),
            condition: WatchdogCondition {
                event: WatchdogEvent::BudgetThresholdReached,
                project_filter: None,
                label_filter: None,
                count_threshold: 1,
                window_secs: 3600,
                budget_threshold: Some(0.8),
            },
            action: WatchdogAction::Escalate {
                message: "Budget > 80%".to_string(),
            },
            cooldown_secs: 3600,
            enabled: true,
        }];

        let mut engine = WatchdogEngine::new(rules);

        // Below threshold
        let fired1 = engine.evaluate(&audit, Some(0.5));
        assert!(fired1.is_empty());

        // Above threshold
        let fired2 = engine.evaluate(&audit, Some(0.85));
        assert_eq!(fired2.len(), 1);
    }

    #[test]
    fn test_project_filter() {
        let (audit, _dir) = temp_audit();

        audit
            .record(&AuditEvent::new(
                "proj-a",
                DecisionType::WorkerTimedOut,
                "timeout",
            ))
            .unwrap();
        audit
            .record(&AuditEvent::new(
                "proj-a",
                DecisionType::WorkerTimedOut,
                "timeout",
            ))
            .unwrap();
        audit
            .record(&AuditEvent::new(
                "proj-a",
                DecisionType::WorkerTimedOut,
                "timeout",
            ))
            .unwrap();
        audit
            .record(&AuditEvent::new(
                "proj-b",
                DecisionType::WorkerTimedOut,
                "timeout",
            ))
            .unwrap();

        let rules = vec![WatchdogRule {
            name: "proj-b-alert".to_string(),
            condition: WatchdogCondition {
                event: WatchdogEvent::WorkerTimeout,
                project_filter: Some("proj-b".to_string()),
                label_filter: None,
                count_threshold: 3,
                window_secs: 3600,
                budget_threshold: None,
            },
            action: WatchdogAction::Escalate {
                message: "alert".to_string(),
            },
            cooldown_secs: 3600,
            enabled: true,
        }];

        let mut engine = WatchdogEngine::new(rules);
        let fired = engine.evaluate(&audit, None);
        // proj-b only has 1 event, threshold is 3
        assert!(fired.is_empty());
    }

    #[test]
    fn test_create_task_action() {
        let action = WatchdogAction::CreateTask {
            project: "proj".to_string(),
            subject: "Investigate failures".to_string(),
            description: "Multiple failures detected".to_string(),
        };
        let json = serde_json::to_value(&action).unwrap();
        assert!(json.get("create_task").is_some());
    }

    #[test]
    fn test_watchdog_rule_serde() {
        let rule = WatchdogRule {
            name: "test-rule".to_string(),
            condition: WatchdogCondition {
                event: WatchdogEvent::TaskFailed,
                project_filter: None,
                label_filter: None,
                count_threshold: 5,
                window_secs: 1800,
                budget_threshold: None,
            },
            action: WatchdogAction::Escalate {
                message: "alert".to_string(),
            },
            cooldown_secs: 600,
            enabled: true,
        };

        let json = serde_json::to_string(&rule).unwrap();
        let parsed: WatchdogRule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test-rule");
        assert_eq!(parsed.condition.count_threshold, 5);
        assert_eq!(parsed.cooldown_secs, 600);
    }

    #[test]
    fn test_task_completed_counts_only_completion_events() {
        let (audit, _dir) = temp_audit();

        audit
            .record(&AuditEvent::new(
                "proj",
                DecisionType::TaskAssigned,
                "assigned",
            ))
            .unwrap();
        audit
            .record(&AuditEvent::new(
                "proj",
                DecisionType::TaskCompleted,
                "done",
            ))
            .unwrap();

        let rules = vec![WatchdogRule {
            name: "done-alert".to_string(),
            condition: WatchdogCondition {
                event: WatchdogEvent::TaskCompleted,
                project_filter: Some("proj".to_string()),
                label_filter: None,
                count_threshold: 1,
                window_secs: 3600,
                budget_threshold: None,
            },
            action: WatchdogAction::Escalate {
                message: "completed".to_string(),
            },
            cooldown_secs: 3600,
            enabled: true,
        }];

        let mut engine = WatchdogEngine::new(rules);
        let fired = engine.evaluate(&audit, None);
        assert_eq!(fired.len(), 1);
    }
}
