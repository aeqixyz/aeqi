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

/// What happens when the user interrupts during tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptBehavior {
    /// Stop the tool and discard its result.
    Cancel,
    /// Keep running; the interruption waits until the tool finishes.
    Block,
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

    /// Whether this tool is safe to run concurrently with other concurrent-safe tools.
    /// Read-only tools (file reads, searches, greps) should return true.
    /// Write tools (file edits, shell commands that mutate) should return false.
    /// The agent runs concurrent-safe tools in parallel and exclusive tools sequentially.
    fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
        true
    }

    /// Whether this tool performs irreversible operations (delete, overwrite, send).
    /// Used by permission systems and safety checks.
    fn is_destructive(&self, _input: &serde_json::Value) -> bool {
        false
    }

    /// What should happen when the user interrupts while this tool is running.
    /// Default: Block (keep running).
    fn interrupt_behavior(&self) -> InterruptBehavior {
        InterruptBehavior::Block
    }

    /// Maximum result size in characters before the result is persisted to disk.
    /// Returns None to use the agent's default (50K chars).
    /// Tools that self-bound their output (e.g., file read with token limit)
    /// can return Some(usize::MAX) to opt out of persistence.
    fn max_result_size_chars(&self) -> Option<usize> {
        None
    }

    /// Human-readable activity description for spinner/status display.
    /// e.g., "Reading src/main.rs", "Searching for pattern".
    fn activity_description(&self, _input: &serde_json::Value) -> Option<String> {
        None
    }
}
