use crate::runtime::{RuntimeOutcome, RuntimeOutcomeStatus};

/// Parsed outcome from a worker's result text.
#[derive(Debug, Clone)]
pub enum TaskOutcome {
    /// Task completed successfully.
    Done(String),
    /// Worker is blocked and needs input to continue.
    Blocked {
        /// The specific question or information needed.
        question: String,
        /// Full result text including work done so far.
        full_text: String,
    },
    /// Worker hit context exhaustion but made progress. Re-queue with checkpoint.
    Handoff {
        /// Summary of progress made and what remains.
        checkpoint: String,
    },
    /// Task failed due to a technical error.
    Failed(String),
}

impl TaskOutcome {
    /// Legacy compatibility parser while callers migrate to runtime-first outcomes.
    pub fn parse(result_text: &str) -> Self {
        let runtime = RuntimeOutcome::from_agent_response(result_text, Vec::new());
        Self::from_runtime_outcome(&runtime)
    }

    pub fn from_runtime_outcome(runtime: &RuntimeOutcome) -> Self {
        match runtime.status {
            RuntimeOutcomeStatus::Done => Self::Done(runtime.summary.clone()),
            RuntimeOutcomeStatus::Blocked => Self::Blocked {
                question: runtime
                    .reason
                    .clone()
                    .unwrap_or_else(|| runtime.summary.clone()),
                full_text: runtime.summary.clone(),
            },
            RuntimeOutcomeStatus::Handoff => Self::Handoff {
                checkpoint: runtime.summary.clone(),
            },
            RuntimeOutcomeStatus::Failed => Self::Failed(
                runtime
                    .reason
                    .clone()
                    .unwrap_or_else(|| runtime.summary.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_done_output() {
        let outcome = TaskOutcome::parse("I fixed the bug and committed to feat/fix-pms.");
        assert!(matches!(outcome, TaskOutcome::Done(_)));
    }

    #[test]
    fn parses_blocked_output() {
        let text = "BLOCKED:\nShould the new endpoint require auth?\n\nI implemented the handler.";
        let outcome = TaskOutcome::parse(text);
        match outcome {
            TaskOutcome::Blocked { question, .. } => {
                assert_eq!(question, "Should the new endpoint require auth?");
            }
            _ => panic!("expected blocked"),
        }
    }

    #[test]
    fn parses_failed_output() {
        let outcome =
            TaskOutcome::parse("FAILED:\ncargo build returned 3 errors in pms/src/main.rs");
        assert!(matches!(outcome, TaskOutcome::Failed(_)));
    }

    #[test]
    fn empty_output_is_failure() {
        let outcome = TaskOutcome::parse("");
        assert!(matches!(outcome, TaskOutcome::Failed(_)));

        let outcome = TaskOutcome::parse("   \n  \n  ");
        assert!(matches!(outcome, TaskOutcome::Failed(_)));
    }

    #[test]
    fn parses_handoff_output() {
        let text = "HANDOFF:\nImplemented the worker queue, remaining: metrics wiring.";
        let outcome = TaskOutcome::parse(text);
        match outcome {
            TaskOutcome::Handoff { checkpoint } => {
                assert!(checkpoint.contains("Implemented the worker queue"));
            }
            _ => panic!("expected handoff"),
        }
    }

    #[test]
    fn parses_structured_json_output() {
        let outcome = TaskOutcome::parse(
            r#"{"status":"failed","summary":"cargo test failed","reason":"workspace has compile errors"}"#,
        );

        match outcome {
            TaskOutcome::Failed(reason) => {
                assert_eq!(reason, "workspace has compile errors");
            }
            _ => panic!("expected failed"),
        }
    }
}
