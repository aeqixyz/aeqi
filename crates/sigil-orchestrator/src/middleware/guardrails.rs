//! Guardrails Middleware — blocks dangerous tool calls before execution.
//!
//! Maintains a deny list of dangerous patterns (e.g. `rm -rf`, `git push --force`,
//! `DROP TABLE`). Before each tool call, the tool name and input are checked against
//! the deny list. Matches halt execution with a structured explanation.
//!
//! The deny list is configurable per project/agent role.

use async_trait::async_trait;
use tracing::warn;

use super::{Middleware, MiddlewareAction, ToolCall, WorkerContext};

/// A pattern to deny in tool calls.
#[derive(Debug, Clone)]
pub struct DenyPattern {
    /// The string pattern to search for (case-insensitive substring match).
    pub pattern: String,
    /// Human-readable reason why this pattern is blocked.
    pub reason: String,
}

impl DenyPattern {
    pub fn new(pattern: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            reason: reason.into(),
        }
    }
}

/// Guardrails middleware with configurable deny patterns.
pub struct GuardrailsMiddleware {
    deny_patterns: Vec<DenyPattern>,
}

impl GuardrailsMiddleware {
    /// Create with the given deny patterns.
    pub fn new(deny_patterns: Vec<DenyPattern>) -> Self {
        Self { deny_patterns }
    }

    /// Create with a sensible set of default deny patterns.
    pub fn with_defaults() -> Self {
        Self::new(vec![
            DenyPattern::new(
                "rm -rf /",
                "Recursive deletion of root filesystem is prohibited",
            ),
            DenyPattern::new(
                "rm -rf ~",
                "Recursive deletion of home directory is prohibited",
            ),
            DenyPattern::new(
                "rm -rf *",
                "Wildcard recursive deletion is prohibited",
            ),
            DenyPattern::new(
                "git push --force",
                "Force push is prohibited — use --force-with-lease if necessary",
            ),
            DenyPattern::new(
                "git push -f",
                "Force push is prohibited — use --force-with-lease if necessary",
            ),
            DenyPattern::new("DROP TABLE", "SQL DROP TABLE is prohibited"),
            DenyPattern::new("DROP DATABASE", "SQL DROP DATABASE is prohibited"),
            DenyPattern::new("TRUNCATE TABLE", "SQL TRUNCATE TABLE is prohibited"),
            DenyPattern::new(
                ":(){ :|:& };:",
                "Fork bomb is prohibited",
            ),
            DenyPattern::new(
                "mkfs.",
                "Filesystem formatting is prohibited",
            ),
            DenyPattern::new(
                "dd if=/dev/zero",
                "Disk overwrite with dd is prohibited",
            ),
            DenyPattern::new(
                "> /dev/sda",
                "Direct disk device write is prohibited",
            ),
            DenyPattern::new(
                "chmod -R 777",
                "Recursive world-writable permissions are prohibited",
            ),
        ])
    }

    /// Check a tool call against the deny list.
    fn check_call(&self, call: &ToolCall) -> Option<&DenyPattern> {
        let name_lower = call.name.to_lowercase();
        let input_lower = call.input.to_lowercase();
        let combined = format!("{name_lower} {input_lower}");

        self.deny_patterns
            .iter()
            .find(|dp| combined.contains(&dp.pattern.to_lowercase()))
    }
}

#[async_trait]
impl Middleware for GuardrailsMiddleware {
    fn name(&self) -> &str {
        "guardrails"
    }

    fn order(&self) -> u32 {
        200
    }

    async fn before_tool(
        &self,
        _ctx: &mut WorkerContext,
        call: &ToolCall,
    ) -> MiddlewareAction {
        if let Some(denied) = self.check_call(call) {
            warn!(
                tool = %call.name,
                pattern = %denied.pattern,
                reason = %denied.reason,
                "guardrails blocked dangerous tool call"
            );
            return MiddlewareAction::Halt(format!(
                "Guardrails blocked: tool '{}' matched deny pattern '{}'. Reason: {}",
                call.name, denied.pattern, denied.reason
            ));
        }
        MiddlewareAction::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "test", "engineer", "sigil")
    }

    #[tokio::test]
    async fn safe_command_passes() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "cargo test --workspace".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn rm_rf_root_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "rm -rf /".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("rm -rf /")),
            "expected Halt for rm -rf /, got {action:?}"
        );
    }

    #[tokio::test]
    async fn force_push_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "git push --force origin main".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("Force push")),
            "expected Halt for force push, got {action:?}"
        );
    }

    #[tokio::test]
    async fn force_push_short_flag_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "git push -f origin main".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));
    }

    #[tokio::test]
    async fn drop_table_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "sqlite3 db.sqlite 'DROP TABLE users;'".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("DROP TABLE")),
            "expected Halt for DROP TABLE, got {action:?}"
        );
    }

    #[tokio::test]
    async fn case_insensitive_matching() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "drop table users".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));
    }

    #[tokio::test]
    async fn custom_deny_patterns() {
        let mw = GuardrailsMiddleware::new(vec![DenyPattern::new(
            "sudo reboot",
            "Rebooting is not allowed",
        )]);
        let mut ctx = test_ctx();

        // Blocked.
        let call = ToolCall {
            name: "Bash".into(),
            input: "sudo reboot now".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));

        // Not blocked (different command).
        let call = ToolCall {
            name: "Bash".into(),
            input: "sudo apt update".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn read_tool_passes() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Read".into(),
            input: "/etc/passwd".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn empty_deny_list_passes_all() {
        let mw = GuardrailsMiddleware::new(vec![]);
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "rm -rf /".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }
}
