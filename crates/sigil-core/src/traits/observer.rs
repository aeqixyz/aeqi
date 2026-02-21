use async_trait::async_trait;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Event types for observability.
#[derive(Debug, Clone)]
pub enum Event {
    AgentStart { agent_name: String },
    AgentEnd { agent_name: String, iterations: u32 },
    LlmRequest { model: String, tokens: u32 },
    LlmResponse { model: String, prompt_tokens: u32, completion_tokens: u32 },
    ToolCall { tool_name: String, duration_ms: u64 },
    ToolError { tool_name: String, error: String },
    Custom { name: String, data: Value },
}

/// Observability trait for metrics, logging, tracing.
#[async_trait]
pub trait Observer: Send + Sync {
    /// Record an event.
    async fn record(&self, event: Event);

    /// Observer name.
    fn name(&self) -> &str;
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
            Event::AgentEnd { agent_name, iterations } => {
                tracing::info!(agent = %agent_name, iterations, "agent completed");
            }
            Event::LlmRequest { model, tokens } => {
                tracing::debug!(model = %model, tokens, "LLM request");
            }
            Event::LlmResponse { model, prompt_tokens, completion_tokens } => {
                tracing::debug!(model = %model, prompt_tokens, completion_tokens, "LLM response");
            }
            Event::ToolCall { tool_name, duration_ms } => {
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
            Event::LlmResponse { prompt_tokens, completion_tokens, .. } => {
                self.llm_prompt_tokens.fetch_add(*prompt_tokens as u64, Ordering::Relaxed);
                self.llm_completion_tokens.fetch_add(*completion_tokens as u64, Ordering::Relaxed);
            }
            Event::ToolCall { duration_ms, .. } => {
                self.tool_calls.fetch_add(1, Ordering::Relaxed);
                self.tool_duration_ms.fetch_add(*duration_ms, Ordering::Relaxed);
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
