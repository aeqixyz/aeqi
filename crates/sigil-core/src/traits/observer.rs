use async_trait::async_trait;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// A typed context attachment for mid-turn enrichment.
///
/// Observers collect these between tool execution and the next model call.
/// The agent loop applies per-type token budgets, sorts by priority, and
/// injects surviving attachments as system messages.
#[derive(Debug, Clone)]
pub struct ContextAttachment {
    /// Source identifier (e.g., "memory", "file_changes", "skills", "blackboard").
    pub source: String,
    /// Content to inject as a system message.
    pub content: String,
    /// Priority — lower values survive budget trimming first.
    pub priority: u32,
    /// Maximum token budget for this attachment (chars / 4 estimate).
    pub max_tokens: u32,
}

/// Action returned by observer hooks to influence agent loop execution.
#[derive(Debug, Clone, Default)]
pub enum LoopAction {
    /// Continue execution normally.
    #[default]
    Continue,
    /// Stop the agent loop with a reason.
    Halt(String),
    /// Inject system messages into the conversation before the next LLM call.
    Inject(Vec<String>),
}

/// Event types for observability.
#[derive(Debug, Clone)]
pub enum Event {
    AgentStart {
        agent_name: String,
    },
    AgentEnd {
        agent_name: String,
        iterations: u32,
    },
    LlmRequest {
        model: String,
        tokens: u32,
    },
    LlmResponse {
        model: String,
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    ToolCall {
        tool_name: String,
        duration_ms: u64,
    },
    ToolError {
        tool_name: String,
        error: String,
    },
    Custom {
        name: String,
        data: Value,
    },
}

/// Observability and interception trait for the agent loop.
///
/// The `record` method is one-way logging. The `before_*`/`after_*` hooks can
/// influence execution by returning [`LoopAction`]. Default implementations
/// return `Continue`, so existing observers are unaffected.
#[async_trait]
pub trait Observer: Send + Sync {
    /// Record an event (one-way, cannot influence execution).
    async fn record(&self, event: Event);

    /// Observer name.
    fn name(&self) -> &str;

    /// Called before each LLM call. Return Halt to stop, Inject to add system messages.
    async fn before_model(&self, _iteration: u32) -> LoopAction {
        LoopAction::Continue
    }

    /// Called after each LLM response. Return Halt to stop, Inject to add messages.
    async fn after_model(
        &self,
        _iteration: u32,
        _prompt_tokens: u32,
        _completion_tokens: u32,
    ) -> LoopAction {
        LoopAction::Continue
    }

    /// Called before each tool execution. Return Halt to block the tool call.
    async fn before_tool(&self, _tool_name: &str, _input: &Value) -> LoopAction {
        LoopAction::Continue
    }

    /// Called after each tool execution completes.
    async fn after_tool(
        &self,
        _tool_name: &str,
        _output: &str,
        _is_error: bool,
    ) -> LoopAction {
        LoopAction::Continue
    }

    /// Called when the agent encounters an API or execution error.
    /// Return Halt to stop, Continue to let the agent's built-in recovery handle it.
    async fn on_error(&self, _iteration: u32, _error: &str) -> LoopAction {
        LoopAction::Continue
    }

    /// Called when the model finishes with no tool calls (end of turn).
    /// Return Continue to accept the stop. Return Inject to add messages and
    /// force the agent to continue (e.g., for validation or correction).
    /// Return Halt to stop with a specific reason.
    async fn after_turn(
        &self,
        _iteration: u32,
        _response_text: &str,
        _stop_reason: &str,
    ) -> LoopAction {
        LoopAction::Continue
    }

    /// Collect context enrichments to inject before the next model call.
    /// Returns typed attachments with token budgets that the agent loop
    /// manages and prioritizes. Called after tool execution, before before_model.
    async fn collect_attachments(&self, _iteration: u32) -> Vec<ContextAttachment> {
        Vec::new()
    }

    // --- Extended lifecycle hooks (Phase 5, CC-parity) ---
    // All have default no-op implementations for backward compatibility.

    /// Called before context compaction. Can provide custom instructions or a display message.
    async fn pre_compact(&self) -> CompactInstructions {
        CompactInstructions::default()
    }

    /// Called after context compaction completes.
    async fn post_compact(&self) {}

    /// Called when a subagent is spawned.
    async fn subagent_start(&self, _agent_id: &str, _description: &str) {}

    /// Called when a subagent completes or fails.
    async fn subagent_stop(&self, _agent_id: &str, _status: &str) {}

    /// Called when a file is modified externally (detected between turns).
    async fn file_changed(&self, _path: &str) {}

    /// Called when a task is created.
    async fn task_created(&self, _task_id: &str, _subject: &str) {}

    /// Called when a task is completed.
    async fn task_completed(&self, _task_id: &str, _outcome: &str) {}

    /// Called when the user submits a prompt. Can return modified prompt.
    async fn user_prompt_submit(&self, _prompt: &str) -> Option<String> {
        None
    }

    /// Called at session end.
    async fn session_end(&self, _reason: &str) {}
}

/// Instructions returned by `pre_compact` to customize compaction behavior.
#[derive(Debug, Clone, Default)]
pub struct CompactInstructions {
    /// Custom instructions appended to the compaction prompt.
    pub custom_instructions: Option<String>,
    /// Message shown to the user during compaction.
    pub user_display_message: Option<String>,
}

/// Default observer that logs to tracing.
pub struct LogObserver;

#[async_trait]
impl Observer for LogObserver {
    async fn record(&self, event: Event) {
        match &event {
            Event::AgentStart { agent_name } => {
                tracing::info!(agent = %agent_name, "agent started");
            }
            Event::AgentEnd {
                agent_name,
                iterations,
            } => {
                tracing::info!(agent = %agent_name, iterations, "agent completed");
            }
            Event::LlmRequest { model, tokens } => {
                tracing::debug!(model = %model, tokens, "LLM request");
            }
            Event::LlmResponse {
                model,
                prompt_tokens,
                completion_tokens,
            } => {
                tracing::debug!(model = %model, prompt_tokens, completion_tokens, "LLM response");
            }
            Event::ToolCall {
                tool_name,
                duration_ms,
            } => {
                tracing::debug!(tool = %tool_name, duration_ms, "tool executed");
            }
            Event::ToolError { tool_name, error } => {
                tracing::warn!(tool = %tool_name, error = %error, "tool error");
            }
            Event::Custom { name, data } => {
                tracing::info!(event = %name, data = %data, "custom event");
            }
        }
    }

    fn name(&self) -> &str {
        "log"
    }
}

/// Prometheus-compatible metrics observer.
/// Exposes counters as a /metrics-style text format.
pub struct PrometheusObserver {
    pub agent_starts: AtomicU64,
    pub agent_ends: AtomicU64,
    pub llm_requests: AtomicU64,
    pub llm_prompt_tokens: AtomicU64,
    pub llm_completion_tokens: AtomicU64,
    pub tool_calls: AtomicU64,
    pub tool_errors: AtomicU64,
    pub tool_duration_ms: AtomicU64,
}

impl PrometheusObserver {
    pub fn new() -> Self {
        Self {
            agent_starts: AtomicU64::new(0),
            agent_ends: AtomicU64::new(0),
            llm_requests: AtomicU64::new(0),
            llm_prompt_tokens: AtomicU64::new(0),
            llm_completion_tokens: AtomicU64::new(0),
            tool_calls: AtomicU64::new(0),
            tool_errors: AtomicU64::new(0),
            tool_duration_ms: AtomicU64::new(0),
        }
    }

    /// Render metrics in Prometheus text exposition format.
    pub fn render(&self) -> String {
        format!(
            "# HELP sigil_agent_starts_total Total agent starts\n\
             # TYPE sigil_agent_starts_total counter\n\
             sigil_agent_starts_total {}\n\
             # HELP sigil_agent_ends_total Total agent completions\n\
             # TYPE sigil_agent_ends_total counter\n\
             sigil_agent_ends_total {}\n\
             # HELP sigil_llm_requests_total Total LLM requests\n\
             # TYPE sigil_llm_requests_total counter\n\
             sigil_llm_requests_total {}\n\
             # HELP sigil_llm_prompt_tokens_total Total prompt tokens\n\
             # TYPE sigil_llm_prompt_tokens_total counter\n\
             sigil_llm_prompt_tokens_total {}\n\
             # HELP sigil_llm_completion_tokens_total Total completion tokens\n\
             # TYPE sigil_llm_completion_tokens_total counter\n\
             sigil_llm_completion_tokens_total {}\n\
             # HELP sigil_tool_calls_total Total tool calls\n\
             # TYPE sigil_tool_calls_total counter\n\
             sigil_tool_calls_total {}\n\
             # HELP sigil_tool_errors_total Total tool errors\n\
             # TYPE sigil_tool_errors_total counter\n\
             sigil_tool_errors_total {}\n\
             # HELP sigil_tool_duration_ms_total Total tool execution time in ms\n\
             # TYPE sigil_tool_duration_ms_total counter\n\
             sigil_tool_duration_ms_total {}\n",
            self.agent_starts.load(Ordering::Relaxed),
            self.agent_ends.load(Ordering::Relaxed),
            self.llm_requests.load(Ordering::Relaxed),
            self.llm_prompt_tokens.load(Ordering::Relaxed),
            self.llm_completion_tokens.load(Ordering::Relaxed),
            self.tool_calls.load(Ordering::Relaxed),
            self.tool_errors.load(Ordering::Relaxed),
            self.tool_duration_ms.load(Ordering::Relaxed),
        )
    }
}

impl Default for PrometheusObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Observer for PrometheusObserver {
    async fn record(&self, event: Event) {
        match &event {
            Event::AgentStart { .. } => {
                self.agent_starts.fetch_add(1, Ordering::Relaxed);
            }
            Event::AgentEnd { .. } => {
                self.agent_ends.fetch_add(1, Ordering::Relaxed);
            }
            Event::LlmRequest { .. } => {
                self.llm_requests.fetch_add(1, Ordering::Relaxed);
            }
            Event::LlmResponse {
                prompt_tokens,
                completion_tokens,
                ..
            } => {
                self.llm_prompt_tokens
                    .fetch_add(*prompt_tokens as u64, Ordering::Relaxed);
                self.llm_completion_tokens
                    .fetch_add(*completion_tokens as u64, Ordering::Relaxed);
            }
            Event::ToolCall { duration_ms, .. } => {
                self.tool_calls.fetch_add(1, Ordering::Relaxed);
                self.tool_duration_ms
                    .fetch_add(*duration_ms, Ordering::Relaxed);
            }
            Event::ToolError { .. } => {
                self.tool_errors.fetch_add(1, Ordering::Relaxed);
            }
            Event::Custom { .. } => {}
        }
    }

    fn name(&self) -> &str {
        "prometheus"
    }
}
