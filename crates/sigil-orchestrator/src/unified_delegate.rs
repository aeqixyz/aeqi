//! Unified delegate tool — consolidates subagent spawning, dispatch sending,
//! task assignment, and channel posting into a single `delegate` tool with
//! routing determined by the `to` parameter.
//!
//! Response modes:
//! - `origin` — response injected back into the caller's conversation
//! - `perpetual` — response delivered to the caller's perpetual session
//! - `async` — fire-and-forget; caller notified on completion
//! - `department` — response posted to the department channel
//! - `none` — no response expected

use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{Tool, ToolResult, ToolSpec};
use std::sync::Arc;
use tracing::info;

use crate::message::{Dispatch, DispatchBus, DispatchKind};

// ---------------------------------------------------------------------------
// UnifiedDelegateTool
// ---------------------------------------------------------------------------

/// Unified tool for delegating work to subagents, named agents, or departments.
///
/// Routing is determined by the `to` parameter:
/// - `"subagent"` — spawn an ephemeral subagent (reuses existing DelegateTool logic)
/// - `"dept:<name>"` — post to a department conversation channel
/// - `<agent_name>` — send a DelegateRequest dispatch to a named agent
pub struct UnifiedDelegateTool {
    dispatch_bus: Arc<DispatchBus>,
    /// The name of the calling agent (used as the "from" field in dispatches).
    agent_name: String,
}

impl UnifiedDelegateTool {
    pub fn new(dispatch_bus: Arc<DispatchBus>, agent_name: String) -> Self {
        Self {
            dispatch_bus,
            agent_name,
        }
    }

    /// Parse a response mode string, defaulting to "origin".
    fn parse_response_mode(args: &serde_json::Value) -> String {
        args.get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("origin")
            .to_string()
    }

    /// Handle delegation to a named agent via DelegateRequest dispatch.
    async fn delegate_to_agent(
        &self,
        to: &str,
        prompt: &str,
        response_mode: &str,
        create_task: bool,
        skill: Option<String>,
    ) -> Result<ToolResult> {
        let kind = DispatchKind::DelegateRequest {
            prompt: prompt.to_string(),
            response_mode: response_mode.to_string(),
            create_task,
            skill: skill.clone(),
            reply_to: None,
        };

        let dispatch = Dispatch::new_typed(&self.agent_name, to, kind);
        let dispatch_id = dispatch.id.clone();

        info!(
            from = %self.agent_name,
            to = %to,
            response_mode = %response_mode,
            create_task = create_task,
            dispatch_id = %dispatch_id,
            "sending DelegateRequest dispatch"
        );

        self.dispatch_bus.send(dispatch).await;

        let mut msg = format!(
            "Delegation sent to '{to}' (dispatch_id: {dispatch_id}, response_mode: {response_mode})"
        );
        if create_task {
            msg.push_str("\nTask creation requested — target agent will pick up via task queue.");
        }
        if let Some(s) = &skill {
            msg.push_str(&format!("\nSkill hint: {s}"));
        }

        Ok(ToolResult::success(msg))
    }

    /// Handle delegation to a department channel.
    async fn delegate_to_department(
        &self,
        dept: &str,
        prompt: &str,
        response_mode: &str,
    ) -> Result<ToolResult> {
        // Send a DelegateRequest dispatch addressed to the department.
        // The trigger/routing system will pick it up and deliver to appropriate agents.
        let kind = DispatchKind::DelegateRequest {
            prompt: prompt.to_string(),
            response_mode: response_mode.to_string(),
            create_task: false,
            skill: None,
            reply_to: None,
        };

        let to = format!("dept:{dept}");
        let dispatch = Dispatch::new_typed(&self.agent_name, &to, kind);
        let dispatch_id = dispatch.id.clone();

        info!(
            from = %self.agent_name,
            department = %dept,
            dispatch_id = %dispatch_id,
            "sending DelegateRequest to department"
        );

        self.dispatch_bus.send(dispatch).await;

        Ok(ToolResult::success(format!(
            "Delegation posted to department '{dept}' (dispatch_id: {dispatch_id}, response_mode: {response_mode})"
        )))
    }
}

#[async_trait]
impl Tool for UnifiedDelegateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let to = args
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter 'to'"))?;
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter 'prompt'"))?;

        let response_mode = Self::parse_response_mode(&args);
        let create_task = args
            .get("create_task")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let skill = args
            .get("skill")
            .and_then(|v| v.as_str())
            .map(String::from);

        match to {
            // Pattern 1: Subagent — ephemeral in-process agent
            "subagent" => {
                // For now, subagent spawning delegates to the existing DelegateTool.
                // The existing tool is registered separately and handles sync/async subagents.
                // This path returns an informative message directing to the existing tool.
                Ok(ToolResult::success(
                    "Subagent spawning via unified delegate is not yet wired. \
                     Use the existing 'delegate' tool (with to='subagent' omitted) for subagent spawning. \
                     This will be connected in a follow-up phase."
                ))
            }

            // Pattern 3: Department — post to department channel
            dept_target if dept_target.starts_with("dept:") => {
                let dept_name = &dept_target[5..]; // strip "dept:" prefix
                if dept_name.is_empty() {
                    return Ok(ToolResult::error(
                        "Department name cannot be empty. Use 'dept:<name>' format.",
                    ));
                }
                self.delegate_to_department(dept_name, prompt, &response_mode)
                    .await
            }

            // Pattern 2 & 4: Named agent (or fallback for unknown targets)
            agent_name => {
                self.delegate_to_agent(agent_name, prompt, &response_mode, create_task, skill)
                    .await
            }
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "unified_delegate".to_string(),
            description: "Delegate work to subagents, named agents, or departments. \
                Routes based on the 'to' parameter: \
                'subagent' spawns an ephemeral sub-agent, \
                'dept:<name>' posts to a department channel, \
                or any other value sends a delegation request to a named agent. \
                Response mode controls how results are returned: \
                'origin' (inject back into caller), \
                'perpetual' (deliver to perpetual session), \
                'async' (fire-and-forget with notification), \
                'department' (post to department channel), \
                'none' (no response expected)."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Target: 'subagent' for ephemeral agent, 'dept:<name>' for department, or an agent name"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The task or message to delegate"
                    },
                    "response": {
                        "type": "string",
                        "enum": ["origin", "perpetual", "async", "department", "none"],
                        "default": "origin",
                        "description": "How the response should be routed back"
                    },
                    "create_task": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether to also create a tracked task for this delegation"
                    },
                    "skill": {
                        "type": "string",
                        "description": "Optional skill hint for the target agent"
                    },
                    "tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tool allowlist for subagent mode"
                    }
                },
                "required": ["to", "prompt"]
            }),
        }
    }

    fn name(&self) -> &str {
        "unified_delegate"
    }

    fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool() -> UnifiedDelegateTool {
        let bus = Arc::new(DispatchBus::new());
        UnifiedDelegateTool::new(bus, "test-agent".to_string())
    }

    #[test]
    fn test_parse_response_mode_default() {
        let args = serde_json::json!({});
        assert_eq!(UnifiedDelegateTool::parse_response_mode(&args), "origin");
    }

    #[test]
    fn test_parse_response_mode_explicit() {
        let args = serde_json::json!({"response": "async"});
        assert_eq!(UnifiedDelegateTool::parse_response_mode(&args), "async");
    }

    #[test]
    fn test_spec_has_required_fields() {
        let tool = make_tool();
        let spec = tool.spec();
        assert_eq!(spec.name, "unified_delegate");
        let required = spec.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("to")));
        assert!(required.contains(&serde_json::json!("prompt")));
    }

    #[test]
    fn test_name() {
        let tool = make_tool();
        assert_eq!(tool.name(), "unified_delegate");
    }

    #[tokio::test]
    async fn test_subagent_mode_detection() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "subagent",
            "prompt": "do something"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("Subagent spawning"));
    }

    #[tokio::test]
    async fn test_department_mode_detection() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "dept:engineering",
            "prompt": "review this PR"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("engineering"));
        assert!(result.output.contains("dispatch_id"));
    }

    #[tokio::test]
    async fn test_department_empty_name_rejected() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "dept:",
            "prompt": "review this PR"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_named_agent_dispatch() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "researcher",
            "prompt": "find the auth bug",
            "response": "async",
            "create_task": true,
            "skill": "code-review"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("researcher"));
        assert!(result.output.contains("dispatch_id"));
        assert!(result.output.contains("Task creation requested"));
        assert!(result.output.contains("code-review"));
    }

    #[tokio::test]
    async fn test_missing_to_param() {
        let tool = make_tool();
        let args = serde_json::json!({
            "prompt": "do something"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_prompt_param() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "researcher"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dispatch_actually_sent() {
        let bus = Arc::new(DispatchBus::new());
        let tool = UnifiedDelegateTool::new(bus.clone(), "sender".to_string());

        let args = serde_json::json!({
            "to": "receiver",
            "prompt": "hello agent"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        // Verify the dispatch landed in the bus.
        let messages = bus.read("receiver").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "sender");
        assert_eq!(messages[0].to, "receiver");
        assert_eq!(messages[0].kind.subject_tag(), "DELEGATE_REQUEST");
    }

    #[tokio::test]
    async fn test_department_dispatch_sent() {
        let bus = Arc::new(DispatchBus::new());
        let tool = UnifiedDelegateTool::new(bus.clone(), "leader".to_string());

        let args = serde_json::json!({
            "to": "dept:ops",
            "prompt": "check server health"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        // Verify dispatch was sent to "dept:ops".
        let messages = bus.read("dept:ops").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "leader");
        assert_eq!(messages[0].kind.subject_tag(), "DELEGATE_REQUEST");
    }
}
