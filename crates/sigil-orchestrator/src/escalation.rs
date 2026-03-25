//! Escalation policy and tracker for task failure recovery.
//!
//! Implements three-strikes escalation per the architecture doc (Layer 4: Verify):
//!   1st failure: retry with the same agent
//!   2nd failure: retry with a different agent
//!   3rd failure: escalate to a more capable model (if configured) or require human
//!   4th+:       always require human intervention
//!
//! The tracker maintains per-task state including failure count, cooldown timing,
//! and which agents have already been tried.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// EscalationAction
// ---------------------------------------------------------------------------

/// What the supervisor should do after a task failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EscalationAction {
    /// Retry the task with the same agent.
    Retry,
    /// Retry the task with a different agent.
    RetryDifferentAgent,
    /// Escalate to a more capable model.
    EscalateModel,
    /// Require human intervention — no more automated retries.
    RequireHuman,
}

// ---------------------------------------------------------------------------
// EscalationPolicy
// ---------------------------------------------------------------------------

/// Configuration for escalation behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationPolicy {
    /// Maximum retries before requiring human intervention.
    pub max_retries: u32,
    /// Cooldown in seconds between retries for the same task.
    pub cooldown_secs: u64,
    /// Model to escalate to on 3rd failure (e.g. "claude-opus-4-6").
    /// If None, 3rd failure goes straight to RequireHuman.
    pub escalate_model: Option<String>,
}

impl Default for EscalationPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            cooldown_secs: 30,
            escalate_model: None,
        }
    }
}

// ---------------------------------------------------------------------------
// EscalationState
// ---------------------------------------------------------------------------

/// Per-task escalation tracking state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationState {
    /// Number of failures recorded for this task.
    pub failures: u32,
    /// Timestamp of the most recent failure.
    pub last_failure: DateTime<Utc>,
    /// Agents that have already attempted this task.
    pub agents_tried: Vec<String>,
}

// ---------------------------------------------------------------------------
// EscalationTracker
// ---------------------------------------------------------------------------

/// Tracks per-task failure counts and decides escalation actions.
pub struct EscalationTracker {
    policy: EscalationPolicy,
    states: HashMap<String, EscalationState>,
}

impl EscalationTracker {
    /// Create a tracker with the given escalation policy.
    pub fn new(policy: EscalationPolicy) -> Self {
        Self {
            policy,
            states: HashMap::new(),
        }
    }

    /// Create a tracker with default policy.
    pub fn with_defaults() -> Self {
        Self::new(EscalationPolicy::default())
    }

    /// Decide what to do for a failed task based on its escalation history.
    pub fn decide(&self, task_id: &str) -> EscalationAction {
        let Some(state) = self.states.get(task_id) else {
            // No prior failures — this would be the first, so retry.
            debug!(task_id = %task_id, "no escalation state — recommending Retry");
            return EscalationAction::Retry;
        };

        let action = match state.failures {
            0 => EscalationAction::Retry,
            1 => EscalationAction::Retry,
            2 => EscalationAction::RetryDifferentAgent,
            3 => {
                if self.policy.escalate_model.is_some() {
                    EscalationAction::EscalateModel
                } else {
                    EscalationAction::RequireHuman
                }
            }
            _ => EscalationAction::RequireHuman,
        };

        debug!(
            task_id = %task_id,
            failures = state.failures,
            action = ?action,
            "escalation decision"
        );

        action
    }

    /// Record a failure for a task, updating the escalation state.
    pub fn record_failure(&mut self, task_id: &str, agent: &str) {
        let state = self
            .states
            .entry(task_id.to_string())
            .or_insert_with(|| EscalationState {
                failures: 0,
                last_failure: Utc::now(),
                agents_tried: Vec::new(),
            });

        state.failures += 1;
        state.last_failure = Utc::now();

        if !state.agents_tried.contains(&agent.to_string()) {
            state.agents_tried.push(agent.to_string());
        }

        info!(
            task_id = %task_id,
            agent = %agent,
            failures = state.failures,
            "recorded task failure"
        );
    }

    /// Record a success for a task, clearing its escalation state.
    pub fn record_success(&mut self, task_id: &str) {
        if self.states.remove(task_id).is_some() {
            info!(task_id = %task_id, "cleared escalation state on success");
        }
    }

    /// Check if a task is in its cooldown period.
    pub fn is_cooling_down(&self, task_id: &str) -> bool {
        let Some(state) = self.states.get(task_id) else {
            return false;
        };

        let elapsed = Utc::now()
            .signed_duration_since(state.last_failure)
            .num_seconds();
        let cooling = elapsed < self.policy.cooldown_secs as i64;

        if cooling {
            debug!(
                task_id = %task_id,
                elapsed_secs = elapsed,
                cooldown_secs = self.policy.cooldown_secs,
                "task is cooling down"
            );
        }

        cooling
    }

    /// Get the current escalation state for a task, if any.
    pub fn get_state(&self, task_id: &str) -> Option<&EscalationState> {
        self.states.get(task_id)
    }

    /// Get the configured policy.
    pub fn policy(&self) -> &EscalationPolicy {
        &self.policy
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_strikes_progression() {
        let policy = EscalationPolicy {
            max_retries: 3,
            cooldown_secs: 0, // disable cooldown for testing
            escalate_model: Some("claude-opus-4-6".into()),
        };
        let mut tracker = EscalationTracker::new(policy);

        // Before any failures: Retry.
        assert_eq!(tracker.decide("task-1"), EscalationAction::Retry);

        // 1st failure: next decision should still be Retry.
        tracker.record_failure("task-1", "engineer");
        assert_eq!(tracker.decide("task-1"), EscalationAction::Retry);

        // 2nd failure: RetryDifferentAgent.
        tracker.record_failure("task-1", "engineer");
        assert_eq!(tracker.decide("task-1"), EscalationAction::RetryDifferentAgent);

        // 3rd failure: EscalateModel (because escalate_model is configured).
        tracker.record_failure("task-1", "researcher");
        assert_eq!(tracker.decide("task-1"), EscalationAction::EscalateModel);

        // 4th failure: RequireHuman always.
        tracker.record_failure("task-1", "researcher");
        assert_eq!(tracker.decide("task-1"), EscalationAction::RequireHuman);

        // 5th failure: still RequireHuman.
        tracker.record_failure("task-1", "researcher");
        assert_eq!(tracker.decide("task-1"), EscalationAction::RequireHuman);
    }

    #[test]
    fn three_strikes_without_escalate_model() {
        let policy = EscalationPolicy {
            max_retries: 3,
            cooldown_secs: 0,
            escalate_model: None, // no model to escalate to
        };
        let mut tracker = EscalationTracker::new(policy);

        tracker.record_failure("task-1", "engineer");
        tracker.record_failure("task-1", "engineer");
        tracker.record_failure("task-1", "researcher");

        // 3rd failure without escalate_model: RequireHuman.
        assert_eq!(tracker.decide("task-1"), EscalationAction::RequireHuman);
    }

    #[test]
    fn success_clears_state() {
        let mut tracker = EscalationTracker::with_defaults();

        tracker.record_failure("task-1", "engineer");
        tracker.record_failure("task-1", "engineer");
        assert!(tracker.get_state("task-1").is_some());

        tracker.record_success("task-1");
        assert!(tracker.get_state("task-1").is_none());

        // After clearing, next decision is Retry (fresh start).
        assert_eq!(tracker.decide("task-1"), EscalationAction::Retry);
    }

    #[test]
    fn cooldown_active_after_failure() {
        let policy = EscalationPolicy {
            max_retries: 3,
            cooldown_secs: 3600, // 1 hour cooldown
            escalate_model: None,
        };
        let mut tracker = EscalationTracker::new(policy);

        tracker.record_failure("task-1", "engineer");
        assert!(tracker.is_cooling_down("task-1"));
    }

    #[test]
    fn cooldown_not_active_when_zero() {
        let policy = EscalationPolicy {
            max_retries: 3,
            cooldown_secs: 0,
            escalate_model: None,
        };
        let mut tracker = EscalationTracker::new(policy);

        tracker.record_failure("task-1", "engineer");
        assert!(!tracker.is_cooling_down("task-1"));
    }

    #[test]
    fn cooldown_not_active_for_unknown_task() {
        let tracker = EscalationTracker::with_defaults();
        assert!(!tracker.is_cooling_down("nonexistent"));
    }

    #[test]
    fn different_agents_tracked() {
        let mut tracker = EscalationTracker::with_defaults();

        tracker.record_failure("task-1", "engineer");
        tracker.record_failure("task-1", "researcher");
        tracker.record_failure("task-1", "engineer"); // duplicate agent

        let state = tracker.get_state("task-1").unwrap();
        assert_eq!(state.failures, 3);
        assert_eq!(state.agents_tried, vec!["engineer", "researcher"]);
    }

    #[test]
    fn independent_tasks() {
        let mut tracker = EscalationTracker::with_defaults();

        tracker.record_failure("task-1", "engineer");
        tracker.record_failure("task-1", "engineer");
        tracker.record_failure("task-2", "designer");

        // task-1 has 2 failures, task-2 has 1.
        let s1 = tracker.get_state("task-1").unwrap();
        let s2 = tracker.get_state("task-2").unwrap();
        assert_eq!(s1.failures, 2);
        assert_eq!(s2.failures, 1);

        // Clear task-1, task-2 unaffected.
        tracker.record_success("task-1");
        assert!(tracker.get_state("task-1").is_none());
        assert!(tracker.get_state("task-2").is_some());
    }

    #[test]
    fn default_policy_values() {
        let tracker = EscalationTracker::with_defaults();
        let policy = tracker.policy();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.cooldown_secs, 30);
        assert!(policy.escalate_model.is_none());
    }
}
