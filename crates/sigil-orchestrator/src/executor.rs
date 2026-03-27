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
    /// Parse a worker's result text into a structured outcome.
    ///
    /// Checks the first non-empty line for BLOCKED:, HANDOFF:, or FAILED:.
    /// This prevents false positives from code blocks that happen to
    /// contain these words in the middle of output.
    pub fn parse(result_text: &str) -> Self {
        let trimmed = result_text.trim();

        // Empty or whitespace-only responses are failures — never silently mark done.
        if trimmed.is_empty() {
            return Self::Failed("Worker returned empty response".to_string());
        }

        let first_line = trimmed
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();

        if first_line.starts_with("BLOCKED:") {
            let after_prefix = if first_line == "BLOCKED:" {
                trimmed.strip_prefix("BLOCKED:").unwrap_or(trimmed).trim()
            } else {
                first_line.strip_prefix("BLOCKED:").unwrap_or("").trim()
            };
            let question = after_prefix
                .split("\n\n")
                .next()
                .unwrap_or(after_prefix)
                .trim()
                .to_string();
            Self::Blocked {
                question,
                full_text: result_text.to_string(),
            }
        } else if first_line.starts_with("HANDOFF:") {
            let checkpoint = trimmed
                .strip_prefix("HANDOFF:")
                .unwrap_or(trimmed)
                .trim()
                .to_string();
            Self::Handoff { checkpoint }
        } else if first_line.starts_with("FAILED:") {
            Self::Failed(result_text.to_string())
        } else {
            Self::Done(result_text.to_string())
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
}
