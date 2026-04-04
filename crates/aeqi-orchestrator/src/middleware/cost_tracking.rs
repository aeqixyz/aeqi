//! Cost Tracking Middleware — enforces per-task budget ceilings.
//!
//! Tracks accumulated cost throughout a worker execution and halts if
//! the configured budget ceiling is exceeded. Cost is read from the
//! [`WorkerContext::cost_usd`] field, which should be updated by the
//! executor as streaming cost data arrives.

use async_trait::async_trait;
use tracing::{info, warn};

use super::{
    Middleware, MiddlewareAction, ORDER_COST_TRACKING, Outcome, ToolCall, ToolResult, WorkerContext,
};

/// Cost tracking middleware with a configurable budget ceiling.
pub struct CostTrackingMiddleware {
    /// Maximum allowed cost in USD. Execution halts if exceeded.
    budget_usd: f64,
}

impl CostTrackingMiddleware {
    /// Create with the given budget ceiling in USD.
    pub fn new(budget_usd: f64) -> Self {
        Self { budget_usd }
    }

    /// Check current cost against budget, returning Halt if exceeded.
    fn check_budget(&self, ctx: &WorkerContext) -> MiddlewareAction {
        if ctx.cost_usd > self.budget_usd {
            warn!(
                cost_usd = ctx.cost_usd,
                budget_usd = self.budget_usd,
                task_id = %ctx.task_id,
                "budget exceeded — halting execution"
            );
            return MiddlewareAction::Halt(format!(
                "Budget exceeded: ${:.4} spent, ${:.4} budget. Execution halted.",
                ctx.cost_usd, self.budget_usd
            ));
        }
        MiddlewareAction::Continue
    }
}

#[async_trait]
impl Middleware for CostTrackingMiddleware {
    fn name(&self) -> &str {
        "cost_tracking"
    }

    fn order(&self) -> u32 {
        ORDER_COST_TRACKING
    }

    async fn before_model(&self, ctx: &mut WorkerContext) -> MiddlewareAction {
        self.check_budget(ctx)
    }

    async fn after_tool(
        &self,
        ctx: &mut WorkerContext,
        _call: &ToolCall,
        _result: &ToolResult,
    ) -> MiddlewareAction {
        self.check_budget(ctx)
    }

    async fn on_complete(&self, ctx: &mut WorkerContext, outcome: &Outcome) -> MiddlewareAction {
        info!(
            task_id = %ctx.task_id,
            cost_usd = outcome.cost_usd,
            budget_usd = self.budget_usd,
            "task completed — final cost"
        );
        MiddlewareAction::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx_with_cost(cost: f64) -> WorkerContext {
        let mut ctx = WorkerContext::new("task-1", "test", "engineer", "aeqi");
        ctx.cost_usd = cost;
        ctx
    }

    fn make_call() -> ToolCall {
        ToolCall {
            name: "Bash".into(),
            input: "ls".into(),
        }
    }

    fn make_result() -> ToolResult {
        ToolResult {
            success: true,
            output: "ok".into(),
        }
    }

    #[tokio::test]
    async fn under_budget_continues() {
        let mw = CostTrackingMiddleware::new(1.0);
        let mut ctx = test_ctx_with_cost(0.5);

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));

        let action = mw.after_tool(&mut ctx, &make_call(), &make_result()).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn over_budget_halts_before_model() {
        let mw = CostTrackingMiddleware::new(1.0);
        let mut ctx = test_ctx_with_cost(1.5);

        let action = mw.before_model(&mut ctx).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("Budget exceeded")),
            "expected Halt, got {action:?}"
        );
    }

    #[tokio::test]
    async fn over_budget_halts_after_tool() {
        let mw = CostTrackingMiddleware::new(0.50);
        let mut ctx = test_ctx_with_cost(0.75);

        let action = mw.after_tool(&mut ctx, &make_call(), &make_result()).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("Budget exceeded")),
            "expected Halt, got {action:?}"
        );
    }

    #[tokio::test]
    async fn exact_budget_continues() {
        let mw = CostTrackingMiddleware::new(1.0);
        let mut ctx = test_ctx_with_cost(1.0);

        // At exactly the budget — not over, so continue.
        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn zero_budget_halts_any_cost() {
        let mw = CostTrackingMiddleware::new(0.0);
        let mut ctx = test_ctx_with_cost(0.001);

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));
    }
}
