//! Context Budget Middleware — trims worker context to fit within a line budget.
//!
//! On `on_start`, examines the messages buffer and trims to fit within
//! `max_lines`. Priority order: task description first, then memory
//! context, then blackboard/knowledge. Overflow content is truncated
//! with a marker indicating how much was omitted.

use async_trait::async_trait;
use tracing::debug;

use super::{Middleware, MiddlewareAction, WorkerContext};

/// Context budget middleware — enforces a maximum line count on the messages buffer.
pub struct ContextBudgetMiddleware {
    /// Maximum total lines allowed across all messages.
    max_lines: usize,
}

impl ContextBudgetMiddleware {
    /// Create with the given line budget.
    pub fn new(max_lines: usize) -> Self {
        Self { max_lines }
    }

    /// Trim a list of messages to fit within the line budget.
    ///
    /// Messages are prioritized by position: earlier messages (task description,
    /// identity) are preserved first. Later messages (memory, blackboard) are
    /// truncated or dropped.
    fn trim_messages(messages: &[String], max_lines: usize) -> Vec<String> {
        let mut result = Vec::new();
        let mut lines_remaining = max_lines;

        for msg in messages {
            if lines_remaining == 0 {
                break;
            }

            let line_count = msg.lines().count().max(1);

            if line_count <= lines_remaining {
                result.push(msg.clone());
                lines_remaining -= line_count;
            } else {
                // Partial fit: take as many lines as we can.
                let truncated: String = msg
                    .lines()
                    .take(lines_remaining)
                    .collect::<Vec<_>>()
                    .join("\n");
                let omitted = line_count - lines_remaining;
                result.push(format!(
                    "{truncated}\n[... truncated, {omitted} lines omitted]"
                ));
                lines_remaining = 0;
            }
        }

        result
    }
}

#[async_trait]
impl Middleware for ContextBudgetMiddleware {
    fn name(&self) -> &str {
        "context_budget"
    }

    fn order(&self) -> u32 {
        100
    }

    async fn on_start(&self, ctx: &mut WorkerContext) -> MiddlewareAction {
        let total_lines: usize = ctx
            .messages
            .iter()
            .map(|m| m.lines().count().max(1))
            .sum();

        if total_lines > self.max_lines {
            debug!(
                total_lines,
                max_lines = self.max_lines,
                messages = ctx.messages.len(),
                "trimming context to fit budget"
            );
            ctx.messages = Self::trim_messages(&ctx.messages, self.max_lines);
        }

        MiddlewareAction::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "test task", "engineer", "sigil")
    }

    #[tokio::test]
    async fn under_budget_unchanged() {
        let mw = ContextBudgetMiddleware::new(100);
        let mut ctx = test_ctx();
        ctx.messages = vec!["line 1\nline 2\nline 3".into()];

        let action = mw.on_start(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(ctx.messages.len(), 1);
        assert!(!ctx.messages[0].contains("truncated"));
    }

    #[tokio::test]
    async fn over_budget_trimmed() {
        let mw = ContextBudgetMiddleware::new(5);
        let mut ctx = test_ctx();
        ctx.messages = vec![
            "task line 1\ntask line 2\ntask line 3".into(), // 3 lines — priority
            "memory line 1\nmemory line 2\nmemory line 3".into(), // 3 lines — trimmed
            "blackboard line 1\nblackboard line 2".into(),  // 2 lines — dropped
        ];

        let action = mw.on_start(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));

        // First message (3 lines) fully preserved.
        assert_eq!(ctx.messages.len(), 2);
        assert!(ctx.messages[0].contains("task line 1"));
        assert!(ctx.messages[0].contains("task line 3"));

        // Second message trimmed to 2 lines (5 - 3 = 2 remaining).
        assert!(ctx.messages[1].contains("memory line 1"));
        assert!(ctx.messages[1].contains("memory line 2"));
        assert!(ctx.messages[1].contains("truncated"));
        assert!(!ctx.messages[1].contains("memory line 3"));
    }

    #[tokio::test]
    async fn exact_budget_unchanged() {
        let mw = ContextBudgetMiddleware::new(3);
        let mut ctx = test_ctx();
        ctx.messages = vec!["a\nb\nc".into()]; // exactly 3 lines

        mw.on_start(&mut ctx).await;
        assert_eq!(ctx.messages.len(), 1);
        assert!(!ctx.messages[0].contains("truncated"));
    }

    #[tokio::test]
    async fn zero_lines_drops_all() {
        let mw = ContextBudgetMiddleware::new(0);
        let mut ctx = test_ctx();
        ctx.messages = vec!["stuff".into()];

        mw.on_start(&mut ctx).await;
        assert!(ctx.messages.is_empty());
    }

    #[tokio::test]
    async fn priority_preserves_first_messages() {
        let mw = ContextBudgetMiddleware::new(4);
        let mut ctx = test_ctx();
        ctx.messages = vec![
            "task: build the thing\nstep 1\nstep 2\nstep 3".into(), // 4 lines
            "memory: old context".into(),                            // would be dropped
        ];

        mw.on_start(&mut ctx).await;
        assert_eq!(ctx.messages.len(), 1);
        assert!(ctx.messages[0].contains("task: build the thing"));
        assert!(ctx.messages[0].contains("step 3"));
    }

    #[test]
    fn trim_messages_empty() {
        let result = ContextBudgetMiddleware::trim_messages(&[], 10);
        assert!(result.is_empty());
    }
}
