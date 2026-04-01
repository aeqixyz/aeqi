//! Execute Plan Tool — Programmatic Tool Calling (PTC) for context compression.
//!
//! The LLM provides a list of tool calls as a JSON array. All steps execute
//! in-process sequentially (parallel-safe steps can run concurrently). Intermediate
//! results never enter the context window — only the final summary is returned.
//!
//! This collapses multi-step tool chains into a single inference turn with zero
//! context cost. For a 10-step research phase, this saves 9 inference turns and
//! keeps all intermediate file contents out of context.
//!
//! Inspired by Hermes Agent's execute_code PTC pattern, but implemented natively
//! in Rust — no subprocess, no RPC, no Python dependency. Simpler and faster.

use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{Tool, ToolResult, ToolSpec};
use std::sync::Arc;
use tracing::debug;

/// Maximum steps in a single plan (prevents runaway execution).
const MAX_PLAN_STEPS: usize = 50;

/// Maximum characters per step result kept for the summary.
const MAX_RESULT_CHARS_PER_STEP: usize = 2_000;

/// Maximum total summary characters returned to the LLM.
const MAX_SUMMARY_CHARS: usize = 20_000;

/// Tool that executes a plan of tool calls without intermediate context growth.
///
/// The LLM calls this with a `steps` array of `{tool, args}` objects.
/// Each step is dispatched to the matching tool from the shared tool registry.
/// Results are accumulated internally and returned as a single summary.
pub struct ExecutePlanTool {
    tools: Vec<Arc<dyn Tool>>,
}

impl ExecutePlanTool {
    /// Create with a reference to all available tools.
    /// Typically cloned from the same tool list the agent uses.
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Self {
        Self { tools }
    }

    fn find_tool(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == name)
    }
}

/// A single step result for internal tracking.
struct StepResult {
    step: usize,
    tool: String,
    output: String,
    is_error: bool,
}

#[async_trait]
impl Tool for ExecutePlanTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let steps = args
            .get("steps")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing 'steps' array"))?;

        if steps.is_empty() {
            return Ok(ToolResult::error("Plan has no steps."));
        }
        if steps.len() > MAX_PLAN_STEPS {
            return Ok(ToolResult::error(format!(
                "Plan has {} steps, maximum is {MAX_PLAN_STEPS}.",
                steps.len()
            )));
        }

        let mut results: Vec<StepResult> = Vec::with_capacity(steps.len());
        let mut abort_reason: Option<String> = None;

        for (i, step) in steps.iter().enumerate() {
            let tool_name = step
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let tool_args = step
                .get("args")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            // Don't allow recursive execute_plan or delegate calls.
            if tool_name == "execute_plan" || tool_name == "delegate" {
                results.push(StepResult {
                    step: i + 1,
                    tool: tool_name.to_string(),
                    output: format!("Blocked: {tool_name} cannot be called from within a plan."),
                    is_error: true,
                });
                continue;
            }

            let Some(tool) = self.find_tool(tool_name) else {
                results.push(StepResult {
                    step: i + 1,
                    tool: tool_name.to_string(),
                    output: format!("Unknown tool: {tool_name}"),
                    is_error: true,
                });
                continue;
            };

            debug!(step = i + 1, tool = tool_name, "executing plan step");

            match tool.execute(tool_args).await {
                Ok(result) => {
                    let truncated = if result.output.len() > MAX_RESULT_CHARS_PER_STEP {
                        format!(
                            "{}... [truncated from {} chars]",
                            &result.output[..MAX_RESULT_CHARS_PER_STEP],
                            result.output.len()
                        )
                    } else {
                        result.output.clone()
                    };

                    results.push(StepResult {
                        step: i + 1,
                        tool: tool_name.to_string(),
                        output: truncated,
                        is_error: result.is_error,
                    });

                    // Abort on critical errors (tool execution failure, not tool-level errors).
                    if result.is_error && result.output.contains("FATAL") {
                        abort_reason = Some(format!("Step {} fatal error: {}", i + 1, result.output));
                        break;
                    }
                }
                Err(e) => {
                    let err_msg = format!("Execution error: {e}");
                    results.push(StepResult {
                        step: i + 1,
                        tool: tool_name.to_string(),
                        output: err_msg.clone(),
                        is_error: true,
                    });
                    abort_reason = Some(format!("Step {} crashed: {e}", i + 1));
                    break;
                }
            }
        }

        // Build summary.
        let mut summary = String::with_capacity(MAX_SUMMARY_CHARS);
        let total = results.len();
        let errors = results.iter().filter(|r| r.is_error).count();
        let successes = total - errors;

        summary.push_str(&format!(
            "Plan executed: {successes}/{total} steps succeeded"
        ));
        if let Some(ref reason) = abort_reason {
            summary.push_str(&format!(" (aborted: {reason})"));
        }
        summary.push_str(".\n\n");

        for r in &results {
            let status = if r.is_error { "ERROR" } else { "OK" };
            let header = format!("Step {}: {} [{}]\n", r.step, r.tool, status);
            let entry = format!("{header}{}\n\n", r.output);

            if summary.len() + entry.len() > MAX_SUMMARY_CHARS {
                summary.push_str("... [remaining steps truncated]\n");
                break;
            }
            summary.push_str(&entry);
        }

        let has_errors = errors > 0;
        if has_errors {
            Ok(ToolResult {
                output: summary,
                is_error: false, // Plan itself succeeded even if individual steps had errors.
            })
        } else {
            Ok(ToolResult::success(summary))
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "execute_plan".to_string(),
            description: "Execute multiple tool calls in a single turn. Intermediate results \
                stay internal — only the final summary enters your context. Use this when you \
                need to run many sequential operations (read files, search, inspect) without \
                growing your context window. Much more efficient than calling tools one by one."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "description": "List of tool calls to execute sequentially",
                        "items": {
                            "type": "object",
                            "properties": {
                                "tool": {
                                    "type": "string",
                                    "description": "Tool name (e.g., 'read_file', 'grep', 'shell')"
                                },
                                "args": {
                                    "type": "object",
                                    "description": "Arguments for the tool call"
                                }
                            },
                            "required": ["tool"]
                        }
                    }
                },
                "required": ["steps"]
            }),
        }
    }

    fn name(&self) -> &str {
        "execute_plan"
    }

    fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
        false // Plans modify state, not safe to run concurrently.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_core::traits::{Tool, ToolResult, ToolSpec};

    /// Test tool that echoes its input.
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
            let msg = args
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("no message");
            Ok(ToolResult::success(format!("echo: {msg}")))
        }
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "echo".into(),
                description: "test echo".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }
        }
        fn name(&self) -> &str {
            "echo"
        }
    }

    /// Test tool that always errors.
    struct FailTool;

    #[async_trait]
    impl Tool for FailTool {
        async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
            Ok(ToolResult::error("intentional failure"))
        }
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "fail".into(),
                description: "test fail".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }
        }
        fn name(&self) -> &str {
            "fail"
        }
    }

    fn make_tools() -> Vec<Arc<dyn Tool>> {
        vec![Arc::new(EchoTool), Arc::new(FailTool)]
    }

    #[tokio::test]
    async fn test_basic_plan_execution() {
        let tool = ExecutePlanTool::new(make_tools());
        let result = tool
            .execute(serde_json::json!({
                "steps": [
                    {"tool": "echo", "args": {"message": "hello"}},
                    {"tool": "echo", "args": {"message": "world"}}
                ]
            }))
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.output.contains("2/2 steps succeeded"));
        assert!(result.output.contains("echo: hello"));
        assert!(result.output.contains("echo: world"));
    }

    #[tokio::test]
    async fn test_plan_with_error_step() {
        let tool = ExecutePlanTool::new(make_tools());
        let result = tool
            .execute(serde_json::json!({
                "steps": [
                    {"tool": "echo", "args": {"message": "before"}},
                    {"tool": "fail", "args": {}},
                    {"tool": "echo", "args": {"message": "after"}}
                ]
            }))
            .await
            .unwrap();

        // Plan continues past non-fatal errors.
        assert!(result.output.contains("2/3 steps succeeded"));
        assert!(result.output.contains("intentional failure"));
        assert!(result.output.contains("echo: after"));
    }

    #[tokio::test]
    async fn test_unknown_tool_in_plan() {
        let tool = ExecutePlanTool::new(make_tools());
        let result = tool
            .execute(serde_json::json!({
                "steps": [
                    {"tool": "nonexistent", "args": {}}
                ]
            }))
            .await
            .unwrap();

        assert!(result.output.contains("Unknown tool: nonexistent"));
    }

    #[tokio::test]
    async fn test_recursive_plan_blocked() {
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(EchoTool)];
        let tool = ExecutePlanTool::new(tools);
        let result = tool
            .execute(serde_json::json!({
                "steps": [
                    {"tool": "execute_plan", "args": {"steps": []}}
                ]
            }))
            .await
            .unwrap();

        assert!(result.output.contains("Blocked: execute_plan"));
    }

    #[tokio::test]
    async fn test_delegate_blocked_in_plan() {
        let tool = ExecutePlanTool::new(make_tools());
        let result = tool
            .execute(serde_json::json!({
                "steps": [
                    {"tool": "delegate", "args": {"prompt": "do stuff"}}
                ]
            }))
            .await
            .unwrap();

        assert!(result.output.contains("Blocked: delegate"));
    }

    #[tokio::test]
    async fn test_empty_plan() {
        let tool = ExecutePlanTool::new(make_tools());
        let result = tool
            .execute(serde_json::json!({"steps": []}))
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.output.contains("no steps"));
    }

    #[tokio::test]
    async fn test_too_many_steps() {
        let tool = ExecutePlanTool::new(make_tools());
        let steps: Vec<serde_json::Value> = (0..60)
            .map(|i| serde_json::json!({"tool": "echo", "args": {"message": format!("step {i}")}}))
            .collect();

        let result = tool
            .execute(serde_json::json!({"steps": steps}))
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.output.contains("maximum is 50"));
    }
}
