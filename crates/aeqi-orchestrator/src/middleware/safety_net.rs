//! Safety Net Middleware — preserves partial work on failure.
//!
//! When a worker fails or errors out, inspects the [`WorkerContext`] for any
//! artifacts that were produced (git diffs, new files, output). If partial work
//! is found, annotates the outcome reason with "[partial work preserved]" and
//! logs what was recovered.
//!
//! This prevents silent loss of work when an agent fails partway through.

use async_trait::async_trait;
use tracing::{info, warn};

use super::{
    Middleware, MiddlewareAction, ORDER_SAFETY_NET, Outcome, OutcomeStatus, WorkerContext,
};

/// Safety net middleware that detects and preserves partial work on failure.
pub struct SafetyNetMiddleware;

impl SafetyNetMiddleware {
    pub fn new() -> Self {
        Self
    }

    /// Inspect the worker context for evidence of partial work.
    ///
    /// Returns a list of artifact descriptions found, or empty if none.
    fn detect_artifacts(ctx: &WorkerContext) -> Vec<String> {
        let mut found = Vec::new();

        // Check tool call history for git diff presence (non-empty diff output).
        for tool_call in &ctx.tool_call_history {
            let name_lower = tool_call.name.to_lowercase();
            let input_lower = tool_call.input.to_lowercase();

            if (name_lower.contains("bash") || name_lower.contains("shell"))
                && (input_lower.contains("git diff") || input_lower.contains("git commit"))
            {
                found.push(format!("git activity: {}", tool_call.input));
            }

            if name_lower == "edit" || name_lower == "write" {
                found.push(format!("file modification: {}", tool_call.input));
            }
        }

        // Check metadata for any artifacts the executor reported.
        if let Some(artifacts) = ctx.metadata.get("artifacts")
            && !artifacts.is_empty()
        {
            found.push(format!("reported artifacts: {artifacts}"));
        }

        // Check if there's any output collected in metadata.
        if let Some(output) = ctx.metadata.get("output")
            && !output.is_empty()
        {
            found.push("collected output present".into());
        }

        found
    }

    /// Annotate an outcome reason with partial work information.
    fn annotate_reason(existing: Option<&str>, artifacts: &[String]) -> String {
        let artifact_summary = artifacts.join("; ");
        let prefix = "[partial work preserved]";
        match existing {
            Some(reason) => format!("{prefix} {reason} | Found: {artifact_summary}"),
            None => format!("{prefix} Found: {artifact_summary}"),
        }
    }
}

impl Default for SafetyNetMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for SafetyNetMiddleware {
    fn name(&self) -> &str {
        "safety_net"
    }

    fn order(&self) -> u32 {
        ORDER_SAFETY_NET
    }

    async fn on_complete(&self, ctx: &mut WorkerContext, outcome: &Outcome) -> MiddlewareAction {
        // Only intervene on failure outcomes.
        if outcome.status != OutcomeStatus::Failed {
            return MiddlewareAction::Continue;
        }

        let artifacts = Self::detect_artifacts(ctx);
        if artifacts.is_empty() {
            return MiddlewareAction::Continue;
        }

        let annotated = Self::annotate_reason(outcome.reason.as_deref(), &artifacts);
        warn!(
            task_id = %ctx.task_id,
            artifacts_found = artifacts.len(),
            "partial work detected on failure — preserving"
        );
        for artifact in &artifacts {
            info!(task_id = %ctx.task_id, artifact = %artifact, "preserved artifact");
        }

        // Store the annotated reason in metadata so the worker_pool can pick it up.
        ctx.metadata.insert("safety_net_reason".into(), annotated);
        ctx.metadata
            .insert("safety_net_artifacts".into(), artifacts.join("\n"));

        MiddlewareAction::Continue
    }

    async fn on_error(&self, ctx: &mut WorkerContext, error: &str) -> MiddlewareAction {
        let artifacts = Self::detect_artifacts(ctx);
        if artifacts.is_empty() {
            return MiddlewareAction::Continue;
        }

        let annotated = Self::annotate_reason(Some(error), &artifacts);
        warn!(
            task_id = %ctx.task_id,
            artifacts_found = artifacts.len(),
            "partial work detected on error — preserving"
        );
        for artifact in &artifacts {
            info!(task_id = %ctx.task_id, artifact = %artifact, "preserved artifact");
        }

        ctx.metadata.insert("safety_net_reason".into(), annotated);
        ctx.metadata
            .insert("safety_net_artifacts".into(), artifacts.join("\n"));

        MiddlewareAction::Continue
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::{ToolCall, WorkerContext};

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "do something", "engineer", "aeqi")
    }

    fn failed_outcome() -> Outcome {
        Outcome {
            status: OutcomeStatus::Failed,
            confidence: 0.0,
            artifacts: Vec::new(),
            cost_usd: 0.5,
            turns: 3,
            duration_ms: 5000,
            reason: Some("compilation error".into()),
            runtime: None,
        }
    }

    fn success_outcome() -> Outcome {
        Outcome {
            status: OutcomeStatus::Done,
            confidence: 1.0,
            artifacts: vec!["main.rs".into()],
            cost_usd: 0.1,
            turns: 1,
            duration_ms: 1000,
            reason: None,
            runtime: None,
        }
    }

    #[tokio::test]
    async fn detects_artifacts_on_failure() {
        let mw = SafetyNetMiddleware::new();
        let mut ctx = test_ctx();

        // Simulate git activity in tool history.
        ctx.tool_call_history.push(ToolCall {
            name: "Bash".into(),
            input: "git diff HEAD".into(),
        });
        ctx.tool_call_history.push(ToolCall {
            name: "Edit".into(),
            input: "src/main.rs".into(),
        });

        let outcome = failed_outcome();
        let action = mw.on_complete(&mut ctx, &outcome).await;
        assert!(matches!(action, MiddlewareAction::Continue));

        let reason = ctx.metadata.get("safety_net_reason").unwrap();
        assert!(reason.contains("[partial work preserved]"));
        assert!(reason.contains("git activity"));
        assert!(reason.contains("file modification"));
    }

    #[tokio::test]
    async fn no_op_on_success() {
        let mw = SafetyNetMiddleware::new();
        let mut ctx = test_ctx();

        // Even with tool history, should not intervene on success.
        ctx.tool_call_history.push(ToolCall {
            name: "Bash".into(),
            input: "git diff HEAD".into(),
        });

        let outcome = success_outcome();
        let action = mw.on_complete(&mut ctx, &outcome).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert!(!ctx.metadata.contains_key("safety_net_reason"));
    }

    #[tokio::test]
    async fn no_op_on_failure_without_artifacts() {
        let mw = SafetyNetMiddleware::new();
        let mut ctx = test_ctx();

        // No tool history, no metadata — nothing to preserve.
        let outcome = failed_outcome();
        let action = mw.on_complete(&mut ctx, &outcome).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert!(!ctx.metadata.contains_key("safety_net_reason"));
    }

    #[tokio::test]
    async fn detects_artifacts_on_error() {
        let mw = SafetyNetMiddleware::new();
        let mut ctx = test_ctx();

        ctx.tool_call_history.push(ToolCall {
            name: "Write".into(),
            input: "new_file.rs".into(),
        });

        let action = mw.on_error(&mut ctx, "process crashed").await;
        assert!(matches!(action, MiddlewareAction::Continue));

        let reason = ctx.metadata.get("safety_net_reason").unwrap();
        assert!(reason.contains("[partial work preserved]"));
        assert!(reason.contains("process crashed"));
    }

    #[tokio::test]
    async fn detects_metadata_artifacts() {
        let mw = SafetyNetMiddleware::new();
        let mut ctx = test_ctx();

        ctx.metadata
            .insert("artifacts".into(), "src/lib.rs, src/main.rs".into());

        let outcome = failed_outcome();
        let action = mw.on_complete(&mut ctx, &outcome).await;
        assert!(matches!(action, MiddlewareAction::Continue));

        let reason = ctx.metadata.get("safety_net_reason").unwrap();
        assert!(reason.contains("[partial work preserved]"));
        assert!(reason.contains("reported artifacts"));
    }

    #[tokio::test]
    async fn detects_output_metadata() {
        let mw = SafetyNetMiddleware::new();
        let mut ctx = test_ctx();

        ctx.metadata
            .insert("output".into(), "partial build output here".into());

        let outcome = failed_outcome();
        let action = mw.on_complete(&mut ctx, &outcome).await;
        assert!(matches!(action, MiddlewareAction::Continue));

        let reason = ctx.metadata.get("safety_net_reason").unwrap();
        assert!(reason.contains("collected output present"));
    }

    #[test]
    fn annotate_reason_with_existing() {
        let artifacts = vec!["git diff found".into(), "file edit found".into()];
        let result = SafetyNetMiddleware::annotate_reason(Some("build failed"), &artifacts);
        assert!(result.starts_with("[partial work preserved]"));
        assert!(result.contains("build failed"));
        assert!(result.contains("git diff found"));
    }

    #[test]
    fn annotate_reason_without_existing() {
        let artifacts = vec!["output present".into()];
        let result = SafetyNetMiddleware::annotate_reason(None, &artifacts);
        assert!(result.starts_with("[partial work preserved]"));
        assert!(result.contains("output present"));
    }

    #[test]
    fn default_impl() {
        let _mw = SafetyNetMiddleware;
    }
}
