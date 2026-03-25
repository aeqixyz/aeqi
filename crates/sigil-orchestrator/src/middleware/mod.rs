//! Middleware system for composable agent execution behavior.
//!
//! Every feature around the core agent loop — guardrails, cost tracking, loop
//! detection, context budgeting — is a [`Middleware`] implementation. Middleware
//! instances are composed into a [`MiddlewareChain`] that wraps the agent
//! execution core, providing hook points before/after model calls, tool calls,
//! and at start/complete/error boundaries.
//!
//! This is the architectural foundation for Sigil v4's composable execution layer.

pub mod context_budget;
pub mod cost_tracking;
pub mod guardrails;
pub mod loop_detection;

pub use context_budget::ContextBudgetMiddleware;
pub use cost_tracking::CostTrackingMiddleware;
pub use guardrails::GuardrailsMiddleware;
pub use loop_detection::LoopDetectionMiddleware;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Result of a middleware hook invocation.
#[derive(Debug, Clone)]
pub enum MiddlewareAction {
    /// Proceed to the next middleware in the chain.
    Continue,
    /// Skip remaining middleware, proceed directly to the core agent loop.
    Skip,
    /// Stop execution entirely with a structured reason.
    Halt(String),
    /// Inject additional messages into the worker context.
    Inject(Vec<String>),
}

/// Simplified representation of a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name (e.g. "Bash", "Read", "Edit").
    pub name: String,
    /// Serialized input parameters.
    pub input: String,
}

/// Simplified representation of a tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool succeeded.
    pub success: bool,
    /// Output text (truncated for storage).
    pub output: String,
}

/// Completion status of a worker execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OutcomeStatus {
    Done,
    DoneWithConcerns,
    Blocked,
    NeedsContext,
    Handoff,
    Failed,
}

/// Structured outcome from an agent execution, enriched by middleware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    /// What happened.
    pub status: OutcomeStatus,
    /// Worker's confidence in the result (0.0 - 1.0).
    pub confidence: f32,
    /// Produced artifacts (file paths, commit hashes, URLs).
    pub artifacts: Vec<String>,
    /// Total cost in USD for this execution.
    pub cost_usd: f64,
    /// Number of agentic turns used.
    pub turns: u32,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Human-readable reason (especially for non-Done outcomes).
    pub reason: Option<String>,
}

impl Default for Outcome {
    fn default() -> Self {
        Self {
            status: OutcomeStatus::Done,
            confidence: 1.0,
            artifacts: Vec::new(),
            cost_usd: 0.0,
            turns: 0,
            duration_ms: 0,
            reason: None,
        }
    }
}

/// Mutable context threaded through the middleware chain during execution.
///
/// Middleware can read and mutate this to influence execution behavior.
#[derive(Debug, Clone)]
pub struct WorkerContext {
    /// Task identifier.
    pub task_id: String,
    /// Task description / prompt.
    pub task_description: String,
    /// Agent name executing this task.
    pub agent_name: String,
    /// Project name the task belongs to.
    pub project_name: String,
    /// Messages buffer — system prompt fragments, injected messages, etc.
    pub messages: Vec<String>,
    /// History of tool calls made during this execution.
    pub tool_call_history: Vec<ToolCall>,
    /// Accumulated cost in USD for this execution.
    pub cost_usd: f64,
    /// Arbitrary metadata that middleware can share.
    pub metadata: HashMap<String, String>,
}

impl WorkerContext {
    pub fn new(
        task_id: impl Into<String>,
        task_description: impl Into<String>,
        agent_name: impl Into<String>,
        project_name: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            task_description: task_description.into(),
            agent_name: agent_name.into(),
            project_name: project_name.into(),
            messages: Vec::new(),
            tool_call_history: Vec::new(),
            cost_usd: 0.0,
            metadata: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// The Middleware trait
// ---------------------------------------------------------------------------

/// A composable behavior layer around the agent execution core.
///
/// Implementations hook into the worker lifecycle at well-defined points.
/// All hooks return [`MiddlewareAction`] to control flow. Default
/// implementations return `Continue` so middleware only needs to override
/// the hooks it cares about.
#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    /// Human-readable name for logging and diagnostics.
    fn name(&self) -> &str;

    /// Execution priority — lower values run earlier in the chain.
    fn order(&self) -> u32;

    /// Called once when the worker starts, before any model interaction.
    async fn on_start(&self, _ctx: &mut WorkerContext) -> MiddlewareAction {
        MiddlewareAction::Continue
    }

    /// Called before each model (LLM) invocation.
    async fn before_model(&self, _ctx: &mut WorkerContext) -> MiddlewareAction {
        MiddlewareAction::Continue
    }

    /// Called after each model (LLM) response is received.
    async fn after_model(&self, _ctx: &mut WorkerContext) -> MiddlewareAction {
        MiddlewareAction::Continue
    }

    /// Called before each tool execution.
    async fn before_tool(
        &self,
        _ctx: &mut WorkerContext,
        _call: &ToolCall,
    ) -> MiddlewareAction {
        MiddlewareAction::Continue
    }

    /// Called after each tool execution completes.
    async fn after_tool(
        &self,
        _ctx: &mut WorkerContext,
        _call: &ToolCall,
        _result: &ToolResult,
    ) -> MiddlewareAction {
        MiddlewareAction::Continue
    }

    /// Called when execution completes (any outcome).
    async fn on_complete(
        &self,
        _ctx: &mut WorkerContext,
        _outcome: &Outcome,
    ) -> MiddlewareAction {
        MiddlewareAction::Continue
    }

    /// Called when an error occurs during execution.
    async fn on_error(
        &self,
        _ctx: &mut WorkerContext,
        _error: &str,
    ) -> MiddlewareAction {
        MiddlewareAction::Continue
    }
}

// ---------------------------------------------------------------------------
// MiddlewareChain
// ---------------------------------------------------------------------------

/// Ordered chain of middleware that wraps agent execution.
///
/// Middleware is sorted by `order()` at construction time (lower = earlier).
/// Each `run_*` method iterates through the chain, short-circuiting on
/// `Halt` or `Skip` actions.
pub struct MiddlewareChain {
    layers: Vec<Box<dyn Middleware>>,
}

impl MiddlewareChain {
    /// Create a new chain, sorted by middleware order (ascending).
    pub fn new(mut layers: Vec<Box<dyn Middleware>>) -> Self {
        layers.sort_by_key(|m| m.order());
        Self { layers }
    }

    /// Create an empty chain (no middleware).
    pub fn empty() -> Self {
        Self {
            layers: Vec::new(),
        }
    }

    /// Number of middleware in the chain.
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Whether the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Run `on_start` across all middleware.
    pub async fn run_on_start(&self, ctx: &mut WorkerContext) -> MiddlewareAction {
        for mw in &self.layers {
            let action = mw.on_start(ctx).await;
            match action {
                MiddlewareAction::Continue => continue,
                MiddlewareAction::Inject(msgs) => {
                    ctx.messages.extend(msgs);
                    continue;
                }
                other => {
                    debug!(
                        middleware = mw.name(),
                        action = ?other,
                        "on_start short-circuited"
                    );
                    return other;
                }
            }
        }
        MiddlewareAction::Continue
    }

    /// Run `before_model` across all middleware.
    pub async fn run_before_model(&self, ctx: &mut WorkerContext) -> MiddlewareAction {
        for mw in &self.layers {
            let action = mw.before_model(ctx).await;
            match action {
                MiddlewareAction::Continue => continue,
                MiddlewareAction::Inject(msgs) => {
                    ctx.messages.extend(msgs);
                    continue;
                }
                other => {
                    debug!(
                        middleware = mw.name(),
                        action = ?other,
                        "before_model short-circuited"
                    );
                    return other;
                }
            }
        }
        MiddlewareAction::Continue
    }

    /// Run `after_model` across all middleware.
    pub async fn run_after_model(&self, ctx: &mut WorkerContext) -> MiddlewareAction {
        for mw in &self.layers {
            let action = mw.after_model(ctx).await;
            match action {
                MiddlewareAction::Continue => continue,
                MiddlewareAction::Inject(msgs) => {
                    ctx.messages.extend(msgs);
                    continue;
                }
                other => {
                    debug!(
                        middleware = mw.name(),
                        action = ?other,
                        "after_model short-circuited"
                    );
                    return other;
                }
            }
        }
        MiddlewareAction::Continue
    }

    /// Run `before_tool` across all middleware for a specific tool call.
    pub async fn run_before_tool(
        &self,
        ctx: &mut WorkerContext,
        call: &ToolCall,
    ) -> MiddlewareAction {
        for mw in &self.layers {
            let action = mw.before_tool(ctx, call).await;
            match action {
                MiddlewareAction::Continue => continue,
                MiddlewareAction::Inject(msgs) => {
                    ctx.messages.extend(msgs);
                    continue;
                }
                other => {
                    debug!(
                        middleware = mw.name(),
                        action = ?other,
                        "before_tool short-circuited"
                    );
                    return other;
                }
            }
        }
        MiddlewareAction::Continue
    }

    /// Run `after_tool` across all middleware for a specific tool call/result.
    pub async fn run_after_tool(
        &self,
        ctx: &mut WorkerContext,
        call: &ToolCall,
        result: &ToolResult,
    ) -> MiddlewareAction {
        for mw in &self.layers {
            let action = mw.after_tool(ctx, call, result).await;
            match action {
                MiddlewareAction::Continue => continue,
                MiddlewareAction::Inject(msgs) => {
                    ctx.messages.extend(msgs);
                    continue;
                }
                other => {
                    debug!(
                        middleware = mw.name(),
                        action = ?other,
                        "after_tool short-circuited"
                    );
                    return other;
                }
            }
        }
        MiddlewareAction::Continue
    }

    /// Run `on_complete` across all middleware.
    pub async fn run_on_complete(
        &self,
        ctx: &mut WorkerContext,
        outcome: &Outcome,
    ) -> MiddlewareAction {
        for mw in &self.layers {
            let action = mw.on_complete(ctx, outcome).await;
            match action {
                MiddlewareAction::Continue => continue,
                MiddlewareAction::Inject(msgs) => {
                    ctx.messages.extend(msgs);
                    continue;
                }
                other => {
                    debug!(
                        middleware = mw.name(),
                        action = ?other,
                        "on_complete short-circuited"
                    );
                    return other;
                }
            }
        }
        MiddlewareAction::Continue
    }

    /// Run `on_error` across all middleware.
    pub async fn run_on_error(
        &self,
        ctx: &mut WorkerContext,
        error: &str,
    ) -> MiddlewareAction {
        for mw in &self.layers {
            let action = mw.on_error(ctx, error).await;
            match action {
                MiddlewareAction::Continue => continue,
                MiddlewareAction::Inject(msgs) => {
                    ctx.messages.extend(msgs);
                    continue;
                }
                other => {
                    debug!(
                        middleware = mw.name(),
                        action = ?other,
                        "on_error short-circuited"
                    );
                    return other;
                }
            }
        }
        MiddlewareAction::Continue
    }

}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct TestMiddleware {
        label: String,
        priority: u32,
        action: MiddlewareAction,
    }

    #[async_trait]
    impl Middleware for TestMiddleware {
        fn name(&self) -> &str {
            &self.label
        }
        fn order(&self) -> u32 {
            self.priority
        }
        async fn on_start(&self, _ctx: &mut WorkerContext) -> MiddlewareAction {
            self.action.clone()
        }
        async fn before_tool(
            &self,
            _ctx: &mut WorkerContext,
            _call: &ToolCall,
        ) -> MiddlewareAction {
            self.action.clone()
        }
    }

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "do something", "engineer", "sigil")
    }

    #[tokio::test]
    async fn chain_sorts_by_order() {
        let chain = MiddlewareChain::new(vec![
            Box::new(TestMiddleware {
                label: "second".into(),
                priority: 20,
                action: MiddlewareAction::Continue,
            }),
            Box::new(TestMiddleware {
                label: "first".into(),
                priority: 10,
                action: MiddlewareAction::Continue,
            }),
        ]);
        assert_eq!(chain.layers[0].name(), "first");
        assert_eq!(chain.layers[1].name(), "second");
    }

    #[tokio::test]
    async fn chain_halt_short_circuits() {
        let chain = MiddlewareChain::new(vec![
            Box::new(TestMiddleware {
                label: "halter".into(),
                priority: 10,
                action: MiddlewareAction::Halt("stop".into()),
            }),
            Box::new(TestMiddleware {
                label: "never_reached".into(),
                priority: 20,
                action: MiddlewareAction::Continue,
            }),
        ]);
        let mut ctx = test_ctx();
        let action = chain.run_on_start(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Halt(ref s) if s == "stop"));
    }

    #[tokio::test]
    async fn chain_inject_continues() {
        let chain = MiddlewareChain::new(vec![
            Box::new(TestMiddleware {
                label: "injector".into(),
                priority: 10,
                action: MiddlewareAction::Inject(vec!["warning".into()]),
            }),
            Box::new(TestMiddleware {
                label: "after_inject".into(),
                priority: 20,
                action: MiddlewareAction::Continue,
            }),
        ]);
        let mut ctx = test_ctx();
        let action = chain.run_on_start(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(ctx.messages, vec!["warning"]);
    }

    #[tokio::test]
    async fn chain_skip_short_circuits() {
        let chain = MiddlewareChain::new(vec![
            Box::new(TestMiddleware {
                label: "skipper".into(),
                priority: 10,
                action: MiddlewareAction::Skip,
            }),
            Box::new(TestMiddleware {
                label: "never_reached".into(),
                priority: 20,
                action: MiddlewareAction::Halt("should not happen".into()),
            }),
        ]);
        let mut ctx = test_ctx();
        let action = chain.run_on_start(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Skip));
    }

    #[tokio::test]
    async fn empty_chain_continues() {
        let chain = MiddlewareChain::empty();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        let mut ctx = test_ctx();
        let action = chain.run_on_start(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }
}
