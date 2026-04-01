use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{Tool, ToolResult, ToolSpec};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::delegate::{AgentHandle, AgentStatus};

/// Tool for continuing or querying a running/completed background agent.
///
/// Routes by agent ID:
/// - **Running agent**: Message is acknowledged (agent will see it on next poll —
///   requires agent-side message queue, which is a future enhancement).
/// - **Completed/Failed agent**: Returns the agent's result text.
/// - **Unknown ID**: Returns an error.
///
/// This is the Sigil equivalent of Claude Code's SendMessage tool.
pub struct ContinueAgentTool {
    registry: Arc<Mutex<HashMap<String, AgentHandle>>>,
}

impl ContinueAgentTool {
    pub fn new(registry: Arc<Mutex<HashMap<String, AgentHandle>>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ContinueAgentTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let to = args
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'to' (agent ID)"))?;
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let reg = self.registry.lock().await;

        match reg.get(to) {
            Some(handle) => match &handle.status {
                AgentStatus::Running => {
                    // Agent is still running. In a full implementation, this would
                    // queue the message via a per-agent channel. For now, acknowledge
                    // that the agent is running and the message cannot be delivered yet.
                    Ok(ToolResult::success(format!(
                        "Agent \"{}\" ({}) is still running. Message noted but cannot be \
                         delivered mid-execution yet. You will be notified when it completes.",
                        handle.description, to
                    )))
                }
                AgentStatus::Completed => {
                    if message.is_empty() {
                        Ok(ToolResult::success(format!(
                            "Agent \"{}\" ({}) has completed. Its result was already delivered \
                             via task-notification.",
                            handle.description, to
                        )))
                    } else {
                        // In a full implementation, this would resume the agent from
                        // its saved session state with the new message appended.
                        Ok(ToolResult::success(format!(
                            "Agent \"{}\" ({}) has completed. Resuming completed agents with \
                             new instructions is not yet supported. Consider launching a new \
                             delegate with run_in_background=true.",
                            handle.description, to
                        )))
                    }
                }
                AgentStatus::Failed => Ok(ToolResult::error(format!(
                    "Agent \"{}\" ({}) has failed. Cannot continue a failed agent. \
                     Consider launching a new delegate.",
                    handle.description, to
                ))),
            },
            None => Ok(ToolResult::error(format!(
                "No agent found with ID '{to}'. Use the ID from the delegate tool's \
                 launch response (e.g., 'a-12345678abcd')."
            ))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "continue_agent".to_string(),
            description: "Send a follow-up message to a background agent by ID. \
                Use this to check status or (in the future) send new instructions \
                to a running agent."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Agent ID (from delegate tool's launch response)"
                    },
                    "message": {
                        "type": "string",
                        "description": "Follow-up message or instructions for the agent"
                    }
                },
                "required": ["to"]
            }),
        }
    }

    fn name(&self) -> &str {
        "continue_agent"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_continue_unknown_agent() {
        let registry = Arc::new(Mutex::new(HashMap::new()));
        let tool = ContinueAgentTool::new(registry);
        let result = tool
            .execute(serde_json::json!({"to": "a-nonexistent"}))
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("No agent found"));
    }

    #[tokio::test]
    async fn test_continue_running_agent() {
        let registry = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut reg = registry.lock().await;
            reg.insert(
                "a-test123".to_string(),
                AgentHandle {
                    id: "a-test123".to_string(),
                    description: "Test agent".to_string(),
                    status: AgentStatus::Running,
                    notified: false,
                },
            );
        }
        let tool = ContinueAgentTool::new(registry);
        let result = tool
            .execute(serde_json::json!({"to": "a-test123", "message": "keep going"}))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("still running"));
    }

    #[tokio::test]
    async fn test_continue_completed_agent() {
        let registry = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut reg = registry.lock().await;
            reg.insert(
                "a-done456".to_string(),
                AgentHandle {
                    id: "a-done456".to_string(),
                    description: "Done agent".to_string(),
                    status: AgentStatus::Completed,
                    notified: true,
                },
            );
        }
        let tool = ContinueAgentTool::new(registry);
        let result = tool
            .execute(serde_json::json!({"to": "a-done456"}))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("has completed"));
    }

    #[tokio::test]
    async fn test_continue_failed_agent() {
        let registry = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut reg = registry.lock().await;
            reg.insert(
                "a-fail789".to_string(),
                AgentHandle {
                    id: "a-fail789".to_string(),
                    description: "Failed agent".to_string(),
                    status: AgentStatus::Failed,
                    notified: true,
                },
            );
        }
        let tool = ContinueAgentTool::new(registry);
        let result = tool
            .execute(serde_json::json!({"to": "a-fail789"}))
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("has failed"));
    }
}
