use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{LogObserver, Observer, Provider, Tool, ToolResult, ToolSpec};
use sigil_core::{Agent, AgentConfig, Identity};
use std::sync::Arc;

/// Tool for spawning a sub-agent with a delegated task.
/// The sub-agent runs with its own tool allowlist and iteration limit.
pub struct DelegateTool {
    provider: Arc<dyn Provider>,
    tools: Vec<Arc<dyn Tool>>,
    identity: Identity,
    model: String,
}

impl DelegateTool {
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        identity: Identity,
        model: String,
    ) -> Self {
        Self {
            provider,
            tools,
            identity,
            model,
        }
    }
}

#[async_trait]
impl Tool for DelegateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing prompt"))?;
        let max_iterations = args
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        let agent_name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("delegate");

        // Filter tools if an allowlist is specified.
        let tools = if let Some(allow) = args.get("tools").and_then(|v| v.as_array()) {
            let allowed: Vec<String> = allow
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            self.tools
                .iter()
                .filter(|t| allowed.contains(&t.name().to_string()))
                .cloned()
                .collect()
        } else {
            self.tools.clone()
        };

        let observer: Arc<dyn Observer> = Arc::new(LogObserver);
        let config = AgentConfig {
            model: self.model.clone(),
            max_iterations,
            name: agent_name.to_string(),
            ..Default::default()
        };

        let agent = Agent::new(
            config,
            self.provider.clone(),
            tools,
            observer,
            self.identity.clone(),
        );

        match agent.run(prompt).await {
            Ok(result) => Ok(ToolResult::success(result.text)),
            Err(e) => Ok(ToolResult::error(format!("Sub-agent failed: {e}"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "delegate".to_string(),
            description: "Spawn a sub-agent to handle a delegated task. The sub-agent runs with the same tools and identity but its own iteration budget.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Task for the sub-agent" },
                    "name": { "type": "string", "description": "Sub-agent name for logging", "default": "delegate" },
                    "max_iterations": { "type": "integer", "description": "Max tool-call iterations", "default": 10 },
                    "tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tool allowlist. If omitted, all tools available."
                    }
                },
                "required": ["prompt"]
            }),
        }
    }

    fn name(&self) -> &str {
        "delegate"
    }
}
