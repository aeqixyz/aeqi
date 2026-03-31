//! Streaming tool executor — starts executing tools as they stream in from the provider.
//!
//! Concurrency-safe tools run in parallel during streaming. Non-concurrent tools
//! queue behind the parallel batch. Results are buffered and emitted in tool-order
//! (not completion order) for deterministic API message construction.

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::traits::{Tool, ToolResult};

/// Status of a tracked tool in the executor queue.
#[derive(Debug, Clone, PartialEq)]
enum ToolStatus {
    /// Tool has been queued but not started.
    Queued,
    /// Tool is executing.
    Executing,
    /// Tool has completed (result available).
    Completed,
    /// Tool was cancelled (sibling error or user abort).
    Cancelled,
}

/// A tool tracked by the streaming executor.
struct TrackedTool {
    id: String,
    name: String,
    input: serde_json::Value,
    status: ToolStatus,
    is_concurrent_safe: bool,
    result: Option<ToolResult>,
    join_handle: Option<JoinHandle<Result<ToolResult, String>>>,
}

/// Result from a completed tool, in order.
#[derive(Debug)]
pub struct CompletedTool {
    pub id: String,
    pub name: String,
    pub result: ToolResult,
    pub duration_ms: u64,
}

/// Executes tools as they stream in with concurrency control.
///
/// - Concurrent-safe tools can execute in parallel with other concurrent-safe tools
/// - Non-concurrent tools must execute alone (exclusive access)
/// - Results are buffered and emitted in the order tools were received
///
/// # Usage
///
/// ```ignore
/// let mut executor = StreamingToolExecutor::new(tools);
/// // During streaming:
/// executor.add_tool("id", "name", input).await;
/// for result in executor.drain_completed() { /* process */ }
/// // After streaming:
/// let remaining = executor.finish_all().await;
/// ```
pub struct StreamingToolExecutor {
    tools_defs: Vec<Arc<dyn Tool>>,
    queue: Vec<TrackedTool>,
    /// Shared flag — set when a tool errors, signals siblings to abort.
    sibling_errored: Arc<Mutex<bool>>,
}

impl StreamingToolExecutor {
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Self {
        Self {
            tools_defs: tools,
            queue: Vec::new(),
            sibling_errored: Arc::new(Mutex::new(false)),
        }
    }

    /// Add a tool to the execution queue. Starts executing immediately if concurrency allows.
    pub async fn add_tool(&mut self, id: String, name: String, input: serde_json::Value) {
        let is_safe = self
            .tools_defs
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.is_concurrent_safe(&input))
            .unwrap_or(false);

        self.queue.push(TrackedTool {
            id,
            name,
            input,
            status: ToolStatus::Queued,
            is_concurrent_safe: is_safe,
            result: None,
            join_handle: None,
        });

        self.try_start_queued().await;
    }

    /// Check if any queued tools can start executing based on current concurrency state.
    async fn try_start_queued(&mut self) {
        let executing_all_safe = {
            let executing: Vec<bool> = self
                .queue
                .iter()
                .filter(|t| t.status == ToolStatus::Executing)
                .map(|t| t.is_concurrent_safe)
                .collect();
            (executing.is_empty(), executing.iter().all(|&s| s))
        };

        // Collect indices of tools to start.
        let mut to_start = Vec::new();
        for (i, tool) in self.queue.iter().enumerate() {
            if tool.status != ToolStatus::Queued {
                continue;
            }
            let can_execute = executing_all_safe.0
                || (tool.is_concurrent_safe && executing_all_safe.1);
            if can_execute {
                to_start.push(i);
            } else if !tool.is_concurrent_safe {
                break;
            }
        }

        // Start tools by index (no overlapping borrows).
        for i in to_start {
            let tool_def = self
                .tools_defs
                .iter()
                .find(|t| t.name() == self.queue[i].name)
                .cloned();
            let Some(tool_def) = tool_def else {
                self.queue[i].status = ToolStatus::Completed;
                self.queue[i].result = Some(ToolResult::error(format!(
                    "Unknown tool: {}",
                    self.queue[i].name
                )));
                continue;
            };

            let input = self.queue[i].input.clone();
            let sibling_errored = self.sibling_errored.clone();
            let tool_name = self.queue[i].name.clone();

            let handle = tokio::spawn(async move {
                if *sibling_errored.lock().await {
                    return Err("Cancelled: sibling tool errored".to_string());
                }
                match tool_def.execute(input).await {
                    Ok(result) => {
                        if result.is_error {
                            *sibling_errored.lock().await = true;
                            debug!(tool = %tool_name, "tool errored — signaling siblings");
                        }
                        Ok(result)
                    }
                    Err(e) => {
                        *sibling_errored.lock().await = true;
                        Err(e.to_string())
                    }
                }
            });

            self.queue[i].status = ToolStatus::Executing;
            self.queue[i].join_handle = Some(handle);
        }
    }

    /// Collect completed results without blocking. Returns results in tool-order
    /// for tools at the front of the queue that have finished.
    pub fn drain_completed(&mut self) -> Vec<CompletedTool> {
        let mut results = Vec::new();

        // Only drain from the front — maintain order.
        while let Some(tool) = self.queue.first() {
            match tool.status {
                ToolStatus::Completed | ToolStatus::Cancelled => {
                    let mut tool = self.queue.remove(0);
                    let result = tool
                        .result
                        .take()
                        .unwrap_or_else(|| ToolResult::error("Tool cancelled"));
                    results.push(CompletedTool {
                        id: tool.id,
                        name: tool.name,
                        result,
                        duration_ms: 0, // TODO: track start time
                    });
                }
                _ => break, // Not yet complete — stop draining.
            }
        }

        results
    }

    /// Await ALL remaining tools. Called after streaming completes.
    /// Returns results in tool-order.
    pub async fn finish_all(&mut self) -> Vec<CompletedTool> {
        // First, start any remaining queued tools.
        self.try_start_queued().await;

        // Await all executing tools.
        for tool in self.queue.iter_mut() {
            if let Some(handle) = tool.join_handle.take() {
                match handle.await {
                    Ok(Ok(result)) => {
                        tool.result = Some(result);
                        tool.status = ToolStatus::Completed;
                    }
                    Ok(Err(err_msg)) => {
                        tool.result = Some(ToolResult::error(err_msg));
                        tool.status = ToolStatus::Cancelled;
                    }
                    Err(join_err) => {
                        tool.result =
                            Some(ToolResult::error(format!("Tool panicked: {join_err}")));
                        tool.status = ToolStatus::Cancelled;
                    }
                }
            }
        }

        // Start any newly-unblocked tools and await them too (recursive unblock).
        let mut had_queued = true;
        while had_queued {
            had_queued = false;
            self.try_start_queued().await;
            for tool in self.queue.iter_mut() {
                if let Some(handle) = tool.join_handle.take() {
                    had_queued = true;
                    match handle.await {
                        Ok(Ok(result)) => {
                            tool.result = Some(result);
                            tool.status = ToolStatus::Completed;
                        }
                        Ok(Err(err_msg)) => {
                            tool.result = Some(ToolResult::error(err_msg));
                            tool.status = ToolStatus::Cancelled;
                        }
                        Err(join_err) => {
                            tool.result =
                                Some(ToolResult::error(format!("Tool panicked: {join_err}")));
                            tool.status = ToolStatus::Cancelled;
                        }
                    }
                }
            }
        }

        // Drain everything.
        let mut results = Vec::new();
        for tool in self.queue.drain(..) {
            let result = tool
                .result
                .unwrap_or_else(|| ToolResult::error("Tool never completed"));
            results.push(CompletedTool {
                id: tool.id,
                name: tool.name,
                result,
                duration_ms: 0,
            });
        }
        results
    }

    /// Discard all pending tools. Called on streaming fallback or abort.
    pub fn discard(&mut self) {
        for tool in self.queue.iter_mut() {
            if tool.status == ToolStatus::Queued {
                tool.status = ToolStatus::Cancelled;
                tool.result = Some(ToolResult::error("Discarded: streaming fallback"));
            }
            // Executing tools will be cleaned up when their JoinHandle is dropped.
        }
    }

    /// Number of tools currently in the queue.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Number of tools currently executing.
    pub fn executing_count(&self) -> usize {
        self.queue
            .iter()
            .filter(|t| t.status == ToolStatus::Executing)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{ToolResult, ToolSpec};
    use async_trait::async_trait;

    /// Test tool that returns its name as output.
    struct EchoTool {
        tool_name: String,
        concurrent_safe: bool,
        delay_ms: u64,
    }

    #[async_trait]
    impl Tool for EchoTool {
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            if self.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            }
            Ok(ToolResult::success(format!("echo:{}", self.tool_name)))
        }

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.tool_name.clone(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }
        }

        fn name(&self) -> &str {
            &self.tool_name
        }

        fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
            self.concurrent_safe
        }
    }

    /// Test tool that always errors.
    struct ErrorTool;

    #[async_trait]
    impl Tool for ErrorTool {
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::error("intentional error"))
        }

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "error_tool".into(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }
        }

        fn name(&self) -> &str {
            "error_tool"
        }

        fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_single_tool_execution() {
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(EchoTool {
            tool_name: "read".into(),
            concurrent_safe: true,
            delay_ms: 0,
        })];

        let mut executor = StreamingToolExecutor::new(tools);
        executor
            .add_tool("t1".into(), "read".into(), serde_json::json!({}))
            .await;

        let results = executor.finish_all().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "t1");
        assert_eq!(results[0].result.output, "echo:read");
        assert!(!results[0].result.is_error);
    }

    #[tokio::test]
    async fn test_concurrent_safe_tools_parallel() {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(EchoTool {
                tool_name: "read".into(),
                concurrent_safe: true,
                delay_ms: 50,
            }),
            Arc::new(EchoTool {
                tool_name: "grep".into(),
                concurrent_safe: true,
                delay_ms: 50,
            }),
        ];

        let mut executor = StreamingToolExecutor::new(tools);
        executor
            .add_tool("t1".into(), "read".into(), serde_json::json!({}))
            .await;
        executor
            .add_tool("t2".into(), "grep".into(), serde_json::json!({}))
            .await;

        // Both should be executing in parallel.
        assert_eq!(executor.executing_count(), 2);

        let results = executor.finish_all().await;
        assert_eq!(results.len(), 2);
        // Results in tool-order (not completion order).
        assert_eq!(results[0].id, "t1");
        assert_eq!(results[1].id, "t2");
    }

    #[tokio::test]
    async fn test_non_concurrent_tool_blocks_queue() {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(EchoTool {
                tool_name: "read".into(),
                concurrent_safe: true,
                delay_ms: 0,
            }),
            Arc::new(EchoTool {
                tool_name: "edit".into(),
                concurrent_safe: false,
                delay_ms: 0,
            }),
            Arc::new(EchoTool {
                tool_name: "grep".into(),
                concurrent_safe: true,
                delay_ms: 0,
            }),
        ];

        let mut executor = StreamingToolExecutor::new(tools);
        executor
            .add_tool("t1".into(), "read".into(), serde_json::json!({}))
            .await;
        executor
            .add_tool("t2".into(), "edit".into(), serde_json::json!({}))
            .await;
        executor
            .add_tool("t3".into(), "grep".into(), serde_json::json!({}))
            .await;

        let results = executor.finish_all().await;
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "read");
        assert_eq!(results[1].name, "edit");
        assert_eq!(results[2].name, "grep");
        assert!(results.iter().all(|r| !r.result.is_error));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let tools: Vec<Arc<dyn Tool>> = vec![];
        let mut executor = StreamingToolExecutor::new(tools);
        executor
            .add_tool("t1".into(), "nonexistent".into(), serde_json::json!({}))
            .await;

        let results = executor.finish_all().await;
        assert_eq!(results.len(), 1);
        assert!(results[0].result.is_error);
        assert!(results[0].result.output.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_sibling_error_signaling() {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(ErrorTool),
            Arc::new(EchoTool {
                tool_name: "read".into(),
                concurrent_safe: true,
                delay_ms: 100, // Delayed so error tool finishes first.
            }),
        ];

        let mut executor = StreamingToolExecutor::new(tools);
        executor
            .add_tool("t1".into(), "error_tool".into(), serde_json::json!({}))
            .await;
        executor
            .add_tool("t2".into(), "read".into(), serde_json::json!({}))
            .await;

        let results = executor.finish_all().await;
        assert_eq!(results.len(), 2);
        assert!(results[0].result.is_error); // error_tool errored
    }

    #[tokio::test]
    async fn test_discard() {
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(EchoTool {
            tool_name: "read".into(),
            concurrent_safe: true,
            delay_ms: 1000,
        })];

        let mut executor = StreamingToolExecutor::new(tools);
        executor
            .add_tool("t1".into(), "read".into(), serde_json::json!({}))
            .await;
        executor.discard();

        // Queued tools should be cancelled.
        assert_eq!(executor.queue_len(), 1);
    }
}
