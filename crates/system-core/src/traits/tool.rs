use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::provider::ToolSpec;

/// Result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: false,
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
        }
    }
}

/// Tool execution trait. Each tool implements this.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Execute the tool with given arguments.
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult>;

    /// Return the tool specification for LLM function calling.
    fn spec(&self) -> ToolSpec;

    /// Tool name (must match spec().name).
    fn name(&self) -> &str;
}
