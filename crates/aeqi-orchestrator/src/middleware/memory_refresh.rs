//! Memory Refresh Middleware — periodically suggests memory re-search during execution.
//!
//! Workers start with an initial memory context, but on long-running tasks the
//! initial context becomes stale. This middleware fires every N tool calls,
//! building a query from recent tool activity and injecting a refresh hint into
//! the worker context. The actual memory search is performed by the caller —
//! this middleware stores the query in [`WorkerContext::metadata`] under the key
//! `"memory_refresh_query"` for the executor to act on.
//!
//! Implements Priority 5 from the AEQI v4 synthesis: "Memory During Execution."

use async_trait::async_trait;
use tracing::{debug, info};

use super::{
    Middleware, MiddlewareAction, ORDER_MEMORY_REFRESH, ToolCall, ToolResult, WorkerContext,
};

/// Memory refresh middleware configuration.
pub struct MemoryRefreshMiddleware {
    /// Fire a refresh every N tool calls. Default: 5.
    refresh_interval: usize,
    /// Maximum number of lines to inject per refresh. Default: 20.
    max_refresh_lines: usize,
}

impl MemoryRefreshMiddleware {
    /// Create with default configuration (interval=5, max_lines=20).
    pub fn new() -> Self {
        Self {
            refresh_interval: 5,
            max_refresh_lines: 20,
        }
    }

    /// Create with custom configuration.
    pub fn with_config(refresh_interval: usize, max_refresh_lines: usize) -> Self {
        Self {
            refresh_interval,
            max_refresh_lines,
        }
    }

    /// Build a search query from the last N tool names in the history.
    fn build_query(history: &[ToolCall], last_n: usize) -> String {
        let tools: Vec<&str> = history
            .iter()
            .rev()
            .take(last_n)
            .map(|tc| tc.name.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        format!("Context for: {}", tools.join(", "))
    }

    /// Build the injection message listing recent tool names.
    fn build_inject_message(history: &[ToolCall], last_n: usize) -> String {
        let tools: Vec<&str> = history
            .iter()
            .rev()
            .take(last_n)
            .map(|tc| tc.name.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        format!(
            "[Memory refresh suggested based on tools: {}]",
            tools.join(", ")
        )
    }
}

impl Default for MemoryRefreshMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for MemoryRefreshMiddleware {
    fn name(&self) -> &str {
        "memory-refresh"
    }

    fn order(&self) -> u32 {
        ORDER_MEMORY_REFRESH
    }

    async fn after_tool(
        &self,
        ctx: &mut WorkerContext,
        _call: &ToolCall,
        _result: &ToolResult,
    ) -> MiddlewareAction {
        let call_count = ctx.tool_call_history.len();

        // No-op when history is empty or not at an interval boundary.
        if call_count == 0 || !call_count.is_multiple_of(self.refresh_interval) {
            return MiddlewareAction::Continue;
        }

        let query = Self::build_query(&ctx.tool_call_history, 3);
        let inject_msg = Self::build_inject_message(&ctx.tool_call_history, 3);

        info!(
            task_id = %ctx.task_id,
            call_count,
            interval = self.refresh_interval,
            max_lines = self.max_refresh_lines,
            "memory refresh triggered"
        );
        debug!(query = %query, "memory refresh query");

        ctx.metadata.insert("memory_refresh_query".into(), query);

        MiddlewareAction::Inject(vec![inject_msg])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "test task", "engineer", "aeqi")
    }

    fn make_call(name: &str) -> ToolCall {
        ToolCall {
            name: name.into(),
            input: "test input".into(),
        }
    }

    fn make_result() -> ToolResult {
        ToolResult {
            success: true,
            output: "ok".into(),
        }
    }

    #[tokio::test]
    async fn fires_at_interval() {
        let mw = MemoryRefreshMiddleware::with_config(5, 20);
        let mut ctx = test_ctx();
        let call = make_call("Bash");
        let result = make_result();

        // Simulate 5 tool calls in history.
        for _ in 0..5 {
            ctx.tool_call_history.push(make_call("Bash"));
        }

        let action = mw.after_tool(&mut ctx, &call, &result).await;
        assert!(
            matches!(action, MiddlewareAction::Inject(ref msgs) if !msgs.is_empty()),
            "expected Inject at interval 5, got {action:?}"
        );
    }

    #[tokio::test]
    async fn fires_at_second_interval() {
        let mw = MemoryRefreshMiddleware::with_config(5, 20);
        let mut ctx = test_ctx();
        let call = make_call("Read");
        let result = make_result();

        // Simulate 10 tool calls in history.
        for _ in 0..10 {
            ctx.tool_call_history.push(make_call("Edit"));
        }

        let action = mw.after_tool(&mut ctx, &call, &result).await;
        assert!(
            matches!(action, MiddlewareAction::Inject(_)),
            "expected Inject at interval 10, got {action:?}"
        );
    }

    #[tokio::test]
    async fn does_not_fire_between_intervals() {
        let mw = MemoryRefreshMiddleware::with_config(5, 20);
        let mut ctx = test_ctx();
        let call = make_call("Bash");
        let result = make_result();

        // Test counts 1, 2, 3, 4 — none should trigger.
        for i in 1..5 {
            ctx.tool_call_history.clear();
            for _ in 0..i {
                ctx.tool_call_history.push(make_call("Bash"));
            }
            let action = mw.after_tool(&mut ctx, &call, &result).await;
            assert!(
                matches!(action, MiddlewareAction::Continue),
                "expected Continue at count {i}, got {action:?}"
            );
        }
    }

    #[tokio::test]
    async fn empty_tool_history_no_op() {
        let mw = MemoryRefreshMiddleware::new();
        let mut ctx = test_ctx();
        let call = make_call("Bash");
        let result = make_result();

        let action = mw.after_tool(&mut ctx, &call, &result).await;
        assert!(
            matches!(action, MiddlewareAction::Continue),
            "expected Continue for empty history, got {action:?}"
        );
    }

    #[tokio::test]
    async fn inject_message_contains_tool_names() {
        let mw = MemoryRefreshMiddleware::with_config(3, 20);
        let mut ctx = test_ctx();
        let call = make_call("Bash");
        let result = make_result();

        ctx.tool_call_history.push(make_call("Read"));
        ctx.tool_call_history.push(make_call("Edit"));
        ctx.tool_call_history.push(make_call("Bash"));

        let action = mw.after_tool(&mut ctx, &call, &result).await;
        match action {
            MiddlewareAction::Inject(msgs) => {
                assert_eq!(msgs.len(), 1);
                assert!(msgs[0].contains("Read"));
                assert!(msgs[0].contains("Edit"));
                assert!(msgs[0].contains("Bash"));
                assert!(msgs[0].contains("[Memory refresh suggested"));
            }
            other => panic!("expected Inject, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn metadata_key_set_correctly() {
        let mw = MemoryRefreshMiddleware::with_config(2, 20);
        let mut ctx = test_ctx();
        let call = make_call("Bash");
        let result = make_result();

        ctx.tool_call_history.push(make_call("Read"));
        ctx.tool_call_history.push(make_call("Bash"));

        let _ = mw.after_tool(&mut ctx, &call, &result).await;

        let query = ctx.metadata.get("memory_refresh_query");
        assert!(query.is_some(), "memory_refresh_query should be set");
        let query = query.unwrap();
        assert!(query.starts_with("Context for:"));
        assert!(query.contains("Read"));
        assert!(query.contains("Bash"));
    }

    #[tokio::test]
    async fn query_uses_last_3_tools() {
        let mw = MemoryRefreshMiddleware::with_config(5, 20);
        let mut ctx = test_ctx();
        let call = make_call("Bash");
        let result = make_result();

        ctx.tool_call_history.push(make_call("Alpha"));
        ctx.tool_call_history.push(make_call("Beta"));
        ctx.tool_call_history.push(make_call("Gamma"));
        ctx.tool_call_history.push(make_call("Delta"));
        ctx.tool_call_history.push(make_call("Epsilon"));

        let _ = mw.after_tool(&mut ctx, &call, &result).await;

        let query = ctx.metadata.get("memory_refresh_query").unwrap();
        // Last 3 should be Gamma, Delta, Epsilon.
        assert!(query.contains("Gamma"));
        assert!(query.contains("Delta"));
        assert!(query.contains("Epsilon"));
        assert!(!query.contains("Alpha"));
    }

    #[test]
    fn default_impl() {
        let mw = MemoryRefreshMiddleware::default();
        assert_eq!(mw.refresh_interval, 5);
        assert_eq!(mw.max_refresh_lines, 20);
    }
}
