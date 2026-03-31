use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use crate::identity::Identity;
use crate::traits::{
    ChatRequest, ChatResponse, ContentPart, ContextAttachment, Event, LoopAction, Memory,
    MemoryCategory, MemoryQuery, MemoryScope, Message, MessageContent, Observer, Provider, Role,
    StopReason, Tool, ToolResult, ToolSpec, Usage,
};

/// Generic notification that can be injected into the agent loop between turns.
/// Used by background agents to deliver results to the parent.
#[derive(Debug, Clone)]
pub struct LoopNotification {
    /// Content to inject as a user-role message (e.g., XML task-notification).
    pub content: String,
}

/// Sender half for injecting notifications into an agent loop.
pub type NotificationSender = mpsc::UnboundedSender<LoopNotification>;
/// Receiver half for draining notifications inside the agent loop.
pub type NotificationReceiver = mpsc::UnboundedReceiver<LoopNotification>;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default per-tool result size limit (characters).
const DEFAULT_MAX_TOOL_RESULT_CHARS: usize = 50_000;

/// Default aggregate tool results limit per turn (characters).
const DEFAULT_MAX_TOOL_RESULTS_PER_TURN: usize = 200_000;

/// Characters-per-token estimate for context size calculations.
const CHARS_PER_TOKEN: usize = 4;

/// Maximum compaction attempts per agent run to prevent infinite loops.
const MAX_COMPACTIONS_PER_RUN: u32 = 3;

/// Maximum mid-loop memory recalls.
const MAX_MID_LOOP_RECALLS: u32 = 2;

/// Microcompact: keep the N most recent compactable tool results.
const MICROCOMPACT_KEEP_RECENT: usize = 5;

/// Tool names whose results can be cleared by microcompact.
const COMPACTABLE_TOOLS: &[&str] = &[
    "read", "read_file", "readfile", "cat",
    "shell", "bash",
    "grep",
    "glob",
    "web_search", "websearch",
    "web_fetch", "webfetch",
    "edit", "edit_file", "fileedit",
    "write", "write_file", "filewrite",
];

/// Cleared content marker for microcompacted tool results.
const MICROCOMPACT_CLEARED: &str = "[Old tool result content cleared]";

/// Consecutive failures before switching to fallback model.
const FALLBACK_TRIGGER_COUNT: u32 = 3;

/// Preview size for persisted tool results (bytes).
const PERSIST_PREVIEW_SIZE: usize = 2000;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Session type — determines what the agent loop is allowed to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SessionType {
    /// Perpetual session (Telegram/Discord channel). Never ends.
    /// Can self-delegate async sessions. The only session type that
    /// can spawn background work on itself.
    Perpetual,
    /// Async session (CLI chat, task, cron). Runs to completion.
    /// Cannot spawn async sessions on itself. Can delegate to
    /// ephemeral subagents (different identity) but they also
    /// cannot delegate (no recursion).
    #[default]
    Async,
}

/// Configuration for an agent loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Model to use (provider-specific format).
    pub model: String,
    /// Maximum iterations (LLM round-trips) before stopping.
    pub max_iterations: u32,
    /// Maximum tokens per LLM response.
    pub max_tokens: u32,
    /// Temperature for generation.
    pub temperature: f32,
    /// Name of this agent (for logging).
    pub name: String,
    /// Entity ID for scoped memory queries. None = domain scope.
    pub entity_id: Option<String>,
    /// Model's context window size in tokens. Drives compaction decisions.
    pub context_window: u32,
    /// Maximum characters per individual tool result before persistence/truncation.
    pub max_tool_result_chars: usize,
    /// Maximum aggregate tool result characters per turn.
    pub max_tool_results_per_turn: usize,
    /// Loop-level retries on transient API errors. Default: 0.
    /// Retries should normally be handled by the Provider layer (ReliableProvider,
    /// FallbackChain). Set >0 only when your provider lacks built-in retry.
    /// Context-length errors are always handled by the loop (compact + retry)
    /// regardless of this setting.
    pub max_retries: u32,
    /// Base delay for exponential backoff on loop-level retries (ms).
    pub retry_base_delay_ms: u64,
    /// Auto-continue attempts when output is truncated (MaxTokens stop reason).
    pub max_output_recovery: u32,
    /// Compact context when estimated tokens exceed this fraction of context_window.
    pub compact_threshold: f32,
    /// Initial messages to preserve during compaction.
    pub compact_preserve_head: usize,
    /// Trailing messages to preserve during compaction.
    pub compact_preserve_tail: usize,
    /// Loop-level fallback model on consecutive failures. None = no fallback.
    /// Prefer using FallbackChain at the Provider layer instead. This field
    /// exists for simple setups (e.g., `sigil run`) where the provider isn't
    /// wrapped in a chain.
    pub fallback_model: Option<String>,
    /// Directory for persisting large tool results. None = use temp dir on demand.
    pub persist_dir: Option<PathBuf>,
    /// File path for session state persistence. When set, the agent saves its
    /// conversation state after each compaction. On restart, if the file exists,
    /// the agent resumes from the saved state instead of starting fresh.
    pub session_file: Option<PathBuf>,
    /// Session type — Perpetual (never ends, can self-delegate) or Async (runs to completion).
    pub session_type: SessionType,
    /// Optional token budget for auto-continuation. When set, the agent continues
    /// automatically after end-turn if total output tokens < budget * 0.9.
    /// Parsed from "+500k" or "use 2m tokens" syntax in the user prompt.
    pub token_budget: Option<u32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-sonnet-4.6".to_string(),
            max_iterations: 20,
            max_tokens: 8192,
            temperature: 0.0,
            name: "agent".to_string(),
            entity_id: None,
            context_window: 200_000,
            max_tool_result_chars: DEFAULT_MAX_TOOL_RESULT_CHARS,
            max_tool_results_per_turn: DEFAULT_MAX_TOOL_RESULTS_PER_TURN,
            max_retries: 0,
            retry_base_delay_ms: 500,
            max_output_recovery: 3,
            compact_threshold: 0.80,
            compact_preserve_head: 3,
            compact_preserve_tail: 6,
            fallback_model: None,
            persist_dir: None,
            session_file: None,
            session_type: SessionType::Async,
            token_budget: None,
        }
    }
}

impl AgentConfig {
    /// Parse a token budget from the user prompt. Recognizes:
    /// - "+500k", "+2m" (at start or end of prompt)
    /// - "use 500k tokens", "spend 2m tokens"
    pub fn parse_token_budget(prompt: &str) -> Option<u32> {
        let lower = prompt.to_lowercase();

        // Pattern: +Nk or +Nm at start or end.
        for word in lower.split_whitespace() {
            let word = word.trim_start_matches('+');
            if let Some(n) = Self::parse_token_shorthand(word) {
                return Some(n);
            }
        }

        // Pattern: "use Nk tokens" or "spend Nm tokens".
        if let Some(pos) = lower.find("use ").or_else(|| lower.find("spend ")) {
            let after = &lower[pos..];
            for word in after.split_whitespace().skip(1) {
                if let Some(n) = Self::parse_token_shorthand(word) {
                    return Some(n);
                }
            }
        }

        None
    }

    fn parse_token_shorthand(s: &str) -> Option<u32> {
        let s = s.trim_end_matches("tokens").trim_end_matches("token").trim();
        if let Some(n) = s.strip_suffix('k') {
            n.parse::<f32>().ok().map(|v| (v * 1000.0) as u32)
        } else if let Some(n) = s.strip_suffix('m') {
            n.parse::<f32>().ok().map(|v| (v * 1_000_000.0) as u32)
        } else {
            s.parse::<u32>().ok().filter(|&n| n > 1000)
        }
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Why the agent stopped.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStopReason {
    /// Normal completion — LLM returned end_turn with no tool calls.
    EndTurn,
    /// Hit max_iterations limit.
    MaxIterations,
    /// Halted by observer/middleware.
    Halted(String),
    /// All API retries exhausted.
    ApiError(String),
    /// Context window exhausted after compaction attempts.
    ContextExhausted,
    /// Model switched to fallback due to consecutive errors.
    FallbackActivated,
}

/// Result from an agent run.
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub text: String,
    pub total_prompt_tokens: u32,
    pub total_completion_tokens: u32,
    pub iterations: u32,
    pub model: String,
    pub stop_reason: AgentStopReason,
}

// ---------------------------------------------------------------------------
// Session state — serializable checkpoint for resume
// ---------------------------------------------------------------------------

/// Serializable snapshot of agent loop state for session resume.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionState {
    /// Conversation messages at checkpoint time.
    pub messages: Vec<Message>,
    /// Iterations completed.
    pub iterations: u32,
    /// Total prompt tokens consumed.
    pub total_prompt_tokens: u32,
    /// Total completion tokens consumed.
    pub total_completion_tokens: u32,
    /// Number of compactions performed.
    pub compactions: u32,
    /// Active model at checkpoint (may differ from config if fallback was triggered).
    pub active_model: String,
    /// Timestamp of checkpoint (epoch millis).
    pub timestamp_ms: u64,
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// Tracks token usage and compaction state across loop iterations.
#[derive(Debug, Default)]
struct ContextTracker {
    total_prompt_tokens: u32,
    total_completion_tokens: u32,
    /// Prompt tokens from the most recent API response.
    last_prompt_tokens: u32,
    compactions: u32,
}

impl ContextTracker {
    fn update(&mut self, usage: &Usage) {
        self.total_prompt_tokens += usage.prompt_tokens;
        self.total_completion_tokens += usage.completion_tokens;
        self.last_prompt_tokens = usage.prompt_tokens;
    }

    fn estimated_context_tokens(&self) -> u32 {
        self.last_prompt_tokens
    }
}

/// Why the loop continued to the next iteration — for debugging and analytics.
#[derive(Debug, Clone)]
pub enum LoopTransition {
    Initial,
    ToolUse,
    OutputTruncated { attempt: u32 },
    ContextCompacted,
    ContextLengthRecovery,
    /// Reactive compaction: 413/context-length error recovered via emergency compact.
    ReactiveCompact,
    /// Snip compaction removed old rounds (no API call).
    SnipCompacted { tokens_freed: u32 },
    FallbackModelSwitch,
    AfterTurnContinue,
}

/// A skill that was invoked during this session — tracked for post-compact restoration.
#[derive(Debug, Clone)]
struct InvokedSkill {
    name: String,
    content: String,
    invoked_at: std::time::Instant,
}

/// Intermediate tool result during processing.
struct ProcessedToolResult {
    id: String,
    name: String,
    output: String,
    is_error: bool,
}

/// Post-compact restoration constants.
const POST_COMPACT_MAX_FILES: usize = 5;
const POST_COMPACT_MAX_TOKENS_PER_FILE: usize = 5_000;
const POST_COMPACT_FILE_BUDGET: usize = 50_000;
const POST_COMPACT_MAX_TOKENS_PER_SKILL: usize = 5_000;
const POST_COMPACT_SKILLS_BUDGET: usize = 25_000;

/// Snip compaction: early threshold factor. Fires at threshold * SNIP_FACTOR
/// before full compaction at threshold * 1.0.
const SNIP_THRESHOLD_FACTOR: f32 = 0.85;

/// Minimum tokens per continuation to consider productive. 3+ continuations
/// below this threshold trigger diminishing returns detection.
const DIMINISHING_RETURNS_THRESHOLD: u32 = 500;
const DIMINISHING_RETURNS_COUNT: u32 = 3;

/// Token budget auto-continuation: stop when this fraction of budget is used.
const TOKEN_BUDGET_COMPLETION_THRESHOLD: f32 = 0.90;

/// A recently-read file tracked for post-compact restoration and change detection.
#[derive(Debug, Clone)]
struct RecentFile {
    path: String,
    content: String,
    /// File modification time at the point we read it (epoch secs).
    mtime_secs: u64,
}

/// Tool_use/tool_result pairing repair marker.
const SYNTHETIC_TOOL_RESULT: &str = "[Tool result unavailable — context was compacted]";

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// Autonomous agent loop — the core execution engine of Sigil's native runtime.
///
/// ## Layer Responsibilities
///
/// The agent loop is ONE layer in Sigil's execution stack. It owns what only
/// the loop can do. Everything else is delegated:
///
/// | Concern | Owner | Not the loop's job |
/// |---------|-------|--------------------|
/// | Message history | **Agent loop** | |
/// | Tool execution | **Agent loop** | |
/// | Context compaction | **Agent loop** | Middleware also compresses, but at a different level |
/// | Tool result persistence | **Agent loop** | |
/// | MaxTokens recovery | **Agent loop** | |
/// | Context-length recovery | **Agent loop** | |
/// | Observer hooks | **Agent loop** | |
/// | Transient error retry | Provider layer | Loop has opt-in safety net (max_retries) |
/// | Model fallback | Provider layer | Loop has opt-in escape hatch (fallback_model) |
/// | Cost tracking | Middleware | via after_model hook |
/// | Guardrails | Middleware | via before_tool hook |
/// | Loop detection | Middleware | via after_model hook |
/// | Memory refresh | Middleware | via after_tool hook |
/// | Budget enforcement | Middleware | via before_model hook |
///
/// ## Key Design Choices (Sigil-specific, not copied from Claude Code)
///
/// - **Tool result persistence over truncation**: Large outputs are written to disk
///   with a preview. The model can re-read the full output via file tools. Data is
///   never lost — critical for autonomous agents that can't ask the user to re-run.
///
/// - **Multi-stage compaction**: (1) Digest old tool results cheaply, (2) LLM-based
///   structured summary focused on task state, not conversation. The compaction prompt
///   is tuned for autonomous execution: what's done, what's remaining, what to do next.
///
/// - **Tool concurrency via trait**: `Tool::is_concurrent_safe()` lets each tool
///   declare its safety. Safe tools run in parallel, unsafe tools run sequentially.
///
/// - **after_turn hook**: Enables Sigil's verification pipeline to validate the
///   agent's work before accepting a "done" signal.
pub struct Agent {
    config: AgentConfig,
    provider: Arc<dyn Provider>,
    tools: Vec<Arc<dyn Tool>>,
    observer: Arc<dyn Observer>,
    identity: Identity,
    memory: Option<Arc<dyn Memory>>,
    chat_stream: Option<crate::chat_stream::ChatStreamSender>,
    /// Receiver for notifications from background agents. Drained between turns.
    notification_rx: Option<Arc<Mutex<NotificationReceiver>>>,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        observer: Arc<dyn Observer>,
        identity: Identity,
    ) -> Self {
        Self {
            config,
            provider,
            tools,
            observer,
            identity,
            memory: None,
            chat_stream: None,
            notification_rx: None,
        }
    }

    /// Attach a memory backend for context recall.
    pub fn with_memory(mut self, memory: Arc<dyn Memory>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Attach a chat stream sender for real-time event streaming to clients.
    pub fn with_chat_stream(mut self, sender: crate::chat_stream::ChatStreamSender) -> Self {
        self.chat_stream = Some(sender);
        self
    }

    /// Attach a notification receiver for background agent results.
    /// Notifications are drained between turns and injected as user-role messages.
    pub fn with_notification_rx(mut self, rx: NotificationReceiver) -> Self {
        self.notification_rx = Some(Arc::new(Mutex::new(rx)));
        self
    }

    /// Emit a chat stream event if a sender is attached.
    fn emit(&self, event: crate::chat_stream::ChatStreamEvent) {
        if let Some(ref tx) = self.chat_stream {
            tx.send(event);
        }
    }

    // -----------------------------------------------------------------------
    // Main loop
    // -----------------------------------------------------------------------

    /// Run the agent with a user prompt.
    pub async fn run(&self, prompt: &str) -> anyhow::Result<AgentResult> {
        self.observer
            .record(Event::AgentStart {
                agent_name: self.config.name.clone(),
            })
            .await;

        // Build initial messages.
        let system_prompt = self.identity.system_prompt();
        let mut messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::text(&system_prompt),
            },
            Message {
                role: Role::User,
                content: MessageContent::text(prompt),
            },
        ];

        self.inject_initial_memory(&mut messages, prompt).await;

        let tool_specs: Vec<ToolSpec> = self.tools.iter().map(|t| t.spec()).collect();

        let mut tracker = ContextTracker::default();
        let mut iterations = 0u32;
        let mut final_text = String::new();
        let mut recent_files: Vec<RecentFile> = Vec::new();
        let mut consecutive_low_output: u32 = 0;

        // --- Session resume: restore from checkpoint if available ---
        if let Some(ref session_file) = self.config.session_file
            && let Some(state) = Self::load_session(session_file).await
        {
            messages = state.messages;
            iterations = state.iterations;
            tracker.total_prompt_tokens = state.total_prompt_tokens;
            tracker.total_completion_tokens = state.total_completion_tokens;
            tracker.compactions = state.compactions;
            // Inject a resume prompt so the model knows it's continuing.
            messages.push(Message {
                role: Role::User,
                content: MessageContent::text(
                    "Session resumed from checkpoint. Continue where you left off. \
                     Do not repeat completed work.",
                ),
            });
        }
        let mut mid_loop_recalls = 0u32;
        let mut output_recovery_count = 0u32;
        let mut stop_reason = AgentStopReason::EndTurn;
        let mut transition = LoopTransition::Initial;
        let mut consecutive_errors = 0u32;
        let mut active_model = self.config.model.clone();
        let mut persist_dir_created: Option<PathBuf> = None;
        let invoked_skills: Vec<InvokedSkill> = Vec::new();
        let mut has_attempted_reactive_compact = false;

        loop {
            iterations += 1;
            if iterations > self.config.max_iterations {
                warn!(
                    agent = %self.config.name,
                    max = self.config.max_iterations,
                    "agent hit max iterations"
                );
                stop_reason = AgentStopReason::MaxIterations;
                break;
            }

            // --- before_model hook ---
            match self.observer.before_model(iterations).await {
                LoopAction::Halt(reason) => {
                    warn!(agent = %self.config.name, reason = %reason, "before_model halted");
                    final_text = format!("HALTED by middleware: {reason}");
                    stop_reason = AgentStopReason::Halted(reason);
                    break;
                }
                LoopAction::Inject(msgs) => {
                    for msg in msgs {
                        messages.push(Message {
                            role: Role::System,
                            content: MessageContent::text(&msg),
                        });
                    }
                }
                LoopAction::Continue => {}
            }

            // --- Context window management ---
            let estimated_tokens = if tracker.estimated_context_tokens() > 0 {
                tracker.estimated_context_tokens()
            } else {
                Self::estimate_tokens_from_messages(&messages)
            };

            let full_threshold =
                (self.config.context_window as f32 * self.config.compact_threshold) as u32;
            let snip_threshold =
                (self.config.context_window as f32 * self.config.compact_threshold * SNIP_THRESHOLD_FACTOR) as u32;
            let protected =
                self.config.compact_preserve_head + self.config.compact_preserve_tail;

            // --- Stage 0: Snip — remove entire old API rounds (no API call, ~free) ---
            if estimated_tokens > snip_threshold && messages.len() > protected {
                let freed = Self::snip_compact(
                    &mut messages,
                    self.config.compact_preserve_head,
                    self.config.compact_preserve_tail,
                );
                if freed > 0 {
                    debug!(
                        agent = %self.config.name,
                        tokens_freed = freed,
                        "snip compaction freed tokens"
                    );
                    transition = LoopTransition::SnipCompacted { tokens_freed: freed };
                }
            }

            // Re-estimate after snip.
            let estimated_tokens = if matches!(transition, LoopTransition::SnipCompacted { .. }) {
                Self::estimate_tokens_from_messages(&messages)
            } else {
                estimated_tokens
            };

            // --- Stage 1: Microcompact — clear old tool results by name + keep recent N ---
            if estimated_tokens > snip_threshold && messages.len() > protected {
                Self::microcompact(
                    &mut messages,
                    self.config.compact_preserve_tail,
                    MICROCOMPACT_KEEP_RECENT,
                );
            }

            // Re-estimate after microcompact.
            let estimated_tokens = Self::estimate_tokens_from_messages(&messages);

            // --- Stage 2: Full compaction (LLM summary + restoration) ---
            if estimated_tokens > full_threshold
                && tracker.compactions < MAX_COMPACTIONS_PER_RUN
                && messages.len() > protected
            {
                info!(
                    agent = %self.config.name,
                    estimated_tokens,
                    threshold = full_threshold,
                    compaction = tracker.compactions + 1,
                    "context approaching limit, compacting"
                );
                self.compact_messages(&mut messages, &recent_files, &invoked_skills)
                    .await;
                tracker.compactions += 1;
                has_attempted_reactive_compact = false; // Reset after successful compact
                transition = LoopTransition::ContextCompacted;

                // Save session checkpoint after compaction.
                if let Some(ref sf) = self.config.session_file {
                    Self::save_session(
                        &messages,
                        &tracker,
                        iterations,
                        &active_model,
                        sf,
                    )
                    .await;
                }
            }

            // --- Conversation repair: ensure tool_use/tool_result pairing ---
            // Only needed after compaction which may drop half of a use/result pair.
            if matches!(transition, LoopTransition::ContextCompacted | LoopTransition::ContextLengthRecovery) {
                Self::repair_tool_pairing(&mut messages);
            }

            // Build request.
            let request = ChatRequest {
                model: active_model.clone(),
                messages: messages.clone(),
                tools: tool_specs.clone(),
                max_tokens: self.config.max_tokens,
                temperature: self.config.temperature,
            };

            self.observer
                .record(Event::LlmRequest {
                    model: active_model.clone(),
                    tokens: estimated_tokens,
                })
                .await;

            self.emit(crate::chat_stream::ChatStreamEvent::TurnStart {
                turn: iterations,
                model: active_model.clone(),
            });

            // --- Call provider with retry ---
            let response = match self.call_with_retry(&request).await {
                Ok(resp) => {
                    consecutive_errors = 0;
                    resp
                }
                Err(e) => {
                    let err_str = e.to_string();
                    consecutive_errors += 1;

                    // Context-length error → reactive compact and retry.
                    if Self::is_context_length_error(&err_str)
                        && !has_attempted_reactive_compact
                        && tracker.compactions < MAX_COMPACTIONS_PER_RUN
                    {
                        let protected =
                            self.config.compact_preserve_head + self.config.compact_preserve_tail;
                        if messages.len() > protected {
                            warn!(
                                agent = %self.config.name,
                                "reactive compact: context too long, emergency compaction"
                            );
                            // Full pipeline: snip → microcompact → full compact.
                            Self::snip_compact(
                                &mut messages,
                                self.config.compact_preserve_head,
                                self.config.compact_preserve_tail,
                            );
                            Self::microcompact(
                                &mut messages,
                                self.config.compact_preserve_tail,
                                MICROCOMPACT_KEEP_RECENT,
                            );
                            self.compact_messages(&mut messages, &recent_files, &invoked_skills)
                                .await;
                            tracker.compactions += 1;
                            has_attempted_reactive_compact = true;
                            iterations -= 1;
                            transition = LoopTransition::ReactiveCompact;
                            continue;
                        }
                    }

                    // Fallback model — switch on consecutive failures.
                    if consecutive_errors >= FALLBACK_TRIGGER_COUNT
                        && let Some(ref fallback) = self.config.fallback_model
                        && active_model != *fallback
                    {
                        warn!(
                            agent = %self.config.name,
                            consecutive_errors,
                            from = %active_model,
                            to = %fallback,
                            "switching to fallback model"
                        );
                        active_model = fallback.clone();
                        consecutive_errors = 0;
                        iterations -= 1;
                        transition = LoopTransition::FallbackModelSwitch;
                        stop_reason = AgentStopReason::FallbackActivated;
                        continue;
                    }

                    // Notify observer.
                    let action = self.observer.on_error(iterations, &err_str).await;
                    match action {
                        LoopAction::Halt(reason) => {
                            stop_reason = AgentStopReason::Halted(reason);
                        }
                        _ => {
                            if Self::is_context_length_error(&err_str) {
                                stop_reason = AgentStopReason::ContextExhausted;
                            } else {
                                stop_reason = AgentStopReason::ApiError(err_str);
                            }
                        }
                    }
                    break;
                }
            };

            tracker.update(&response.usage);

            self.observer
                .record(Event::LlmResponse {
                    model: active_model.clone(),
                    prompt_tokens: response.usage.prompt_tokens,
                    completion_tokens: response.usage.completion_tokens,
                })
                .await;

            // Emit text delta for chat stream.
            if let Some(ref text) = response.content
                && !text.is_empty()
            {
                self.emit(crate::chat_stream::ChatStreamEvent::TextDelta {
                    text: text.clone(),
                });
            }

            self.emit(crate::chat_stream::ChatStreamEvent::TurnComplete {
                turn: iterations,
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
            });

            // --- after_model hook ---
            match self
                .observer
                .after_model(
                    iterations,
                    response.usage.prompt_tokens,
                    response.usage.completion_tokens,
                )
                .await
            {
                LoopAction::Halt(reason) => {
                    warn!(agent = %self.config.name, reason = %reason, "after_model halted");
                    final_text = format!("HALTED by middleware: {reason}");
                    stop_reason = AgentStopReason::Halted(reason);
                    break;
                }
                LoopAction::Inject(msgs) => {
                    for msg in msgs {
                        messages.push(Message {
                            role: Role::System,
                            content: MessageContent::text(&msg),
                        });
                    }
                }
                LoopAction::Continue => {}
            }

            debug!(
                agent = %self.config.name,
                iteration = iterations,
                tool_calls = response.tool_calls.len(),
                stop_reason = ?response.stop_reason,
                prompt_tokens = response.usage.prompt_tokens,
                completion_tokens = response.usage.completion_tokens,
                transition = ?transition,
                "LLM response"
            );

            // Accumulate text.
            if let Some(ref text) = response.content {
                final_text = text.clone();
            }

            // --- Turn completion: no tool calls ---
            if response.tool_calls.is_empty() {
                // MaxTokens recovery: output was truncated, auto-continue.
                if response.stop_reason == StopReason::MaxTokens
                    && output_recovery_count < self.config.max_output_recovery
                {
                    output_recovery_count += 1;
                    info!(
                        agent = %self.config.name,
                        attempt = output_recovery_count,
                        max = self.config.max_output_recovery,
                        "output truncated (MaxTokens), auto-continuing"
                    );

                    if let Some(ref text) = response.content {
                        messages.push(Message {
                            role: Role::Assistant,
                            content: MessageContent::text(text),
                        });
                    }

                    messages.push(Message {
                        role: Role::User,
                        content: MessageContent::text(
                            "Output was truncated. Continue executing the task from where \
                             you left off. Do not repeat completed work. If the remaining \
                             work is large, break it into smaller tool calls.",
                        ),
                    });

                    transition = LoopTransition::OutputTruncated {
                        attempt: output_recovery_count,
                    };
                    continue;
                }

                // --- after_turn hook ---
                let stop_str = format!("{:?}", response.stop_reason);
                match self
                    .observer
                    .after_turn(iterations, &final_text, &stop_str)
                    .await
                {
                    LoopAction::Inject(msgs) => {
                        info!(
                            agent = %self.config.name,
                            injected = msgs.len(),
                            "after_turn forcing continuation"
                        );
                        // Add assistant response + injected messages to continue.
                        if let Some(ref text) = response.content {
                            messages.push(Message {
                                role: Role::Assistant,
                                content: MessageContent::text(text),
                            });
                        }
                        for msg in msgs {
                            messages.push(Message {
                                role: Role::User,
                                content: MessageContent::text(&msg),
                            });
                        }
                        transition = LoopTransition::AfterTurnContinue;
                        continue;
                    }
                    LoopAction::Halt(reason) => {
                        stop_reason = AgentStopReason::Halted(reason);
                        break;
                    }
                    LoopAction::Continue => {
                        // Token budget auto-continuation: if budget set and not exhausted,
                        // inject nudge message and keep going.
                        if let Some(budget) = self.config.token_budget {
                            let used = tracker.total_completion_tokens;
                            let threshold =
                                (budget as f32 * TOKEN_BUDGET_COMPLETION_THRESHOLD) as u32;
                            if used < threshold {
                                let pct = (used as f32 / budget as f32 * 100.0) as u32;
                                info!(
                                    agent = %self.config.name,
                                    used, budget, pct,
                                    "token budget not exhausted, auto-continuing"
                                );
                                if let Some(ref text) = response.content {
                                    messages.push(Message {
                                        role: Role::Assistant,
                                        content: MessageContent::text(text),
                                    });
                                }
                                messages.push(Message {
                                    role: Role::User,
                                    content: MessageContent::text(format!(
                                        "Stopped at {pct}% of token target ({used} / {budget}). \
                                         Keep working — do not summarize or ask if you should continue."
                                    )),
                                });
                                transition = LoopTransition::AfterTurnContinue;
                                continue;
                            }
                        }
                        // Accept the stop.
                        break;
                    }
                }
            }

            // Reset output recovery counter on tool-use turns.
            output_recovery_count = 0;

            // --- Diminishing returns detection ---
            if response.usage.completion_tokens < DIMINISHING_RETURNS_THRESHOLD {
                consecutive_low_output += 1;
                if consecutive_low_output >= DIMINISHING_RETURNS_COUNT {
                    warn!(
                        agent = %self.config.name,
                        consecutive = consecutive_low_output,
                        threshold = DIMINISHING_RETURNS_THRESHOLD,
                        "diminishing returns detected — stopping"
                    );
                    stop_reason = AgentStopReason::Halted(
                        "Diminishing returns: agent producing minimal output".to_string(),
                    );
                    break;
                }
            } else {
                consecutive_low_output = 0;
            }

            // --- Build assistant message ---
            let mut assistant_parts: Vec<ContentPart> = Vec::new();
            if let Some(ref text) = response.content {
                assistant_parts.push(ContentPart::Text { text: text.clone() });
            }
            for tc in &response.tool_calls {
                assistant_parts.push(ContentPart::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
                self.emit(crate::chat_stream::ChatStreamEvent::ToolStart {
                    tool_use_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                });
            }

            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Parts(assistant_parts),
            });

            // --- before_tool hooks (sequential, per-call) ---
            let mut allowed_calls = Vec::new();
            let mut tool_result_parts: Vec<ContentPart> = Vec::new();
            let mut loop_halted = false;

            for tc in &response.tool_calls {
                match self.observer.before_tool(&tc.name, &tc.arguments).await {
                    LoopAction::Halt(reason) => {
                        warn!(agent = %self.config.name, tool = %tc.name, reason = %reason, "before_tool halted");
                        tool_result_parts.push(ContentPart::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: format!("Blocked by middleware: {reason}"),
                            is_error: true,
                        });
                        final_text = format!("HALTED by middleware: {reason}");
                        stop_reason = AgentStopReason::Halted(reason);
                        loop_halted = true;
                        break;
                    }
                    LoopAction::Inject(_) | LoopAction::Continue => {
                        allowed_calls.push(tc);
                    }
                }
            }

            if loop_halted {
                if !tool_result_parts.is_empty() {
                    messages.push(Message {
                        role: Role::Tool,
                        content: MessageContent::Parts(tool_result_parts),
                    });
                }
                break;
            }

            // --- Execute tools with concurrency classification ---
            let mut safe_calls = Vec::new();
            let mut unsafe_calls = Vec::new();

            for tc in &allowed_calls {
                let is_safe = self
                    .tools
                    .iter()
                    .find(|t| t.name() == tc.name)
                    .map(|t| t.is_concurrent_safe(&tc.arguments))
                    .unwrap_or(true);

                if is_safe {
                    safe_calls.push(*tc);
                } else {
                    unsafe_calls.push(*tc);
                }
            }

            let mut all_results: Vec<(String, String, Result<ToolResult, anyhow::Error>, u64)> =
                Vec::new();

            // Run safe tools in parallel.
            if !safe_calls.is_empty() {
                let futures: Vec<_> = safe_calls
                    .iter()
                    .map(|tc| {
                        let tools = self.tools.clone();
                        let name = tc.name.clone();
                        let args = tc.arguments.clone();
                        let id = tc.id.clone();
                        async move {
                            let start = std::time::Instant::now();
                            let result = Self::execute_tool_static(&tools, &name, args).await;
                            (id, name, result, start.elapsed().as_millis() as u64)
                        }
                    })
                    .collect();
                all_results.extend(futures::future::join_all(futures).await);
            }

            // Run unsafe tools sequentially.
            for tc in &unsafe_calls {
                let start = std::time::Instant::now();
                let result =
                    Self::execute_tool_static(&self.tools, &tc.name, tc.arguments.clone()).await;
                let duration_ms = start.elapsed().as_millis() as u64;
                all_results.push((tc.id.clone(), tc.name.clone(), result, duration_ms));
            }

            // --- Process results: observe, persist/truncate, budget ---
            let mut processed: Vec<ProcessedToolResult> = Vec::with_capacity(all_results.len());

            for (id, name, result, duration_ms) in all_results {
                match result {
                    Ok(tr) => {
                        self.observer
                            .record(Event::ToolCall {
                                tool_name: name.clone(),
                                duration_ms,
                            })
                            .await;

                        let _ = self
                            .observer
                            .after_tool(&name, &tr.output, tr.is_error)
                            .await;

                        self.emit(crate::chat_stream::ChatStreamEvent::ToolComplete {
                            tool_use_id: id.clone(),
                            tool_name: name.clone(),
                            success: !tr.is_error,
                            output_preview: tr.output.chars().take(500).collect(),
                            duration_ms,
                        });

                        // Empty result injection — prevents model confusion on turn boundaries.
                        let output = if tr.output.trim().is_empty() && !tr.is_error {
                            format!("({name} completed with no output)")
                        } else {
                            tr.output
                        };

                        processed.push(ProcessedToolResult {
                            id,
                            name,
                            output,
                            is_error: tr.is_error,
                        });
                    }
                    Err(e) => {
                        self.observer
                            .record(Event::ToolError {
                                tool_name: name.clone(),
                                error: e.to_string(),
                            })
                            .await;

                        let _ = self
                            .observer
                            .after_tool(&name, &e.to_string(), true)
                            .await;

                        processed.push(ProcessedToolResult {
                            id,
                            name,
                            output: format!("Tool execution error: {e}"),
                            is_error: true,
                        });
                    }
                }
            }

            // --- Persist or truncate oversized results ---
            for r in &mut processed {
                if r.is_error || r.output.len() <= self.config.max_tool_result_chars {
                    continue;
                }
                let original_len = r.output.len();

                // Try disk persistence first — model retains access via file read.
                if let Some(ref dir) = self.resolve_persist_dir(&mut persist_dir_created) {
                    match Self::persist_tool_result(dir, &r.id, &r.output).await {
                        Ok(persisted_msg) => {
                            debug!(
                                agent = %self.config.name,
                                tool = %r.name,
                                original = original_len,
                                "tool result persisted to disk"
                            );
                            r.output = persisted_msg;
                            continue;
                        }
                        Err(e) => {
                            warn!(agent = %self.config.name, "persist failed, falling back to truncation: {e}");
                        }
                    }
                }

                // Fallback: truncate with head+tail preview.
                r.output = Self::truncate_result(&r.output, self.config.max_tool_result_chars);
                debug!(
                    agent = %self.config.name,
                    tool = %r.name,
                    original = original_len,
                    truncated_to = r.output.len(),
                    "tool result truncated"
                );
            }

            // --- Enforce aggregate per-turn budget ---
            Self::enforce_result_budget(&mut processed, self.config.max_tool_results_per_turn);

            // --- Track recently-read files for post-compact restoration ---
            for r in &processed {
                if !r.is_error
                    && Self::is_file_read_tool(&r.name)
                    && let Some(path) = Self::extract_file_path_from_result(&r.output)
                {
                    // Dedup by path (keep most recent).
                    recent_files.retain(|f| f.path != path);
                    let mtime_secs = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    recent_files.push(RecentFile {
                        path,
                        content: r.output.clone(),
                        mtime_secs,
                    });
                    // Keep only the most recent N files.
                    if recent_files.len() > POST_COMPACT_MAX_FILES * 2 {
                        recent_files.drain(..recent_files.len() - POST_COMPACT_MAX_FILES);
                    }
                }
            }

            // Build tool result message.
            for r in &processed {
                tool_result_parts.push(ContentPart::ToolResult {
                    tool_use_id: r.id.clone(),
                    content: r.output.clone(),
                    is_error: r.is_error,
                });
            }

            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Parts(tool_result_parts),
            });

            // --- Mid-loop memory recall ---
            if mid_loop_recalls < MAX_MID_LOOP_RECALLS
                && let Some(ref mem) = self.memory
            {
                let tool_output: String = processed
                    .iter()
                    .filter(|r| !r.is_error)
                    .map(|r| r.output.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                if tool_output.len() > 200 && Self::has_novel_terms(&tool_output, prompt) {
                    let mq = MemoryQuery::new(&tool_output, 3).with_scope(MemoryScope::Domain);
                    if let Ok(entries) = mem.search(&mq).await
                        && !entries.is_empty()
                    {
                        let ctx = entries
                            .iter()
                            .map(|e| format!("[{}] {}: {}", e.scope, e.key, e.content))
                            .collect::<Vec<_>>()
                            .join("\n");
                        messages.push(Message {
                            role: Role::System,
                            content: MessageContent::text(format!(
                                "# Updated Memory Recall\n{ctx}"
                            )),
                        });
                        mid_loop_recalls += 1;
                        debug!(
                            agent = %self.config.name,
                            recall = mid_loop_recalls,
                            entries = entries.len(),
                            "mid-loop memory recall injected"
                        );
                    }
                }
            }

            // --- Detect file changes since last read (mid-turn enrichment) ---
            let file_change_msgs = Self::detect_file_changes(&recent_files).await;
            if !file_change_msgs.is_empty() {
                debug!(
                    agent = %self.config.name,
                    changes = file_change_msgs.len(),
                    "injecting file change notifications"
                );
                messages.extend(file_change_msgs);
            }

            // --- Collect enrichments from observers ---
            let attachments = self.observer.collect_attachments(iterations).await;
            if !attachments.is_empty() {
                Self::inject_enrichments(&mut messages, attachments, &self.config);
            }

            // --- Drain background agent notifications ---
            if let Some(ref rx) = self.notification_rx {
                let mut rx_guard = rx.lock().await;
                let mut notif_count = 0u32;
                while let Ok(notif) = rx_guard.try_recv() {
                    messages.push(Message {
                        role: Role::User,
                        content: MessageContent::text(&notif.content),
                    });
                    notif_count += 1;
                }
                if notif_count > 0 {
                    debug!(
                        agent = %self.config.name,
                        count = notif_count,
                        "injected background agent notifications"
                    );
                }
            }

            transition = LoopTransition::ToolUse;

            // If stop reason is EndTurn (not ToolUse), break after executing tools.
            if response.stop_reason == StopReason::EndTurn {
                break;
            }
        }

        self.reflect(&messages).await;

        // Final session checkpoint — save before cleanup.
        if let Some(ref sf) = self.config.session_file {
            Self::save_session(&messages, &tracker, iterations, &active_model, sf).await;
        }

        // Cleanup persisted tool results.
        if let Some(dir) = persist_dir_created
            && let Err(e) = tokio::fs::remove_dir_all(&dir).await
        {
            debug!(agent = %self.config.name, path = %dir.display(), "cleanup persist dir: {e}");
        }

        self.observer
            .record(Event::AgentEnd {
                agent_name: self.config.name.clone(),
                iterations,
            })
            .await;

        self.emit(crate::chat_stream::ChatStreamEvent::Complete {
            stop_reason: format!("{:?}", stop_reason),
            total_prompt_tokens: tracker.total_prompt_tokens,
            total_completion_tokens: tracker.total_completion_tokens,
            iterations,
            cost_usd: 0.0, // Calculated by orchestrator layer
        });

        info!(
            agent = %self.config.name,
            iterations,
            prompt_tokens = tracker.total_prompt_tokens,
            completion_tokens = tracker.total_completion_tokens,
            compactions = tracker.compactions,
            model = %active_model,
            stop = ?stop_reason,
            "agent completed"
        );

        Ok(AgentResult {
            text: final_text,
            total_prompt_tokens: tracker.total_prompt_tokens,
            total_completion_tokens: tracker.total_completion_tokens,
            iterations,
            model: active_model,
            stop_reason,
        })
    }

    // -----------------------------------------------------------------------
    // Memory
    // -----------------------------------------------------------------------

    async fn inject_initial_memory(&self, messages: &mut [Message], prompt: &str) {
        let Some(ref mem) = self.memory else { return };

        let mq = if let Some(ref cid) = self.config.entity_id {
            MemoryQuery::new(prompt, 5).with_entity(cid.clone())
        } else {
            MemoryQuery::new(prompt, 5).with_scope(MemoryScope::Domain)
        };

        match mem.search(&mq).await {
            Ok(entries) if !entries.is_empty() => {
                let ctx = entries
                    .iter()
                    .map(|e| format!("[{}] {}: {}", e.scope, e.key, e.content))
                    .collect::<Vec<_>>()
                    .join("\n");

                if let Some(msg) = messages.first_mut()
                    && let MessageContent::Text(t) = &mut msg.content
                {
                    *t = format!("{t}\n\n# Recalled Memory\n{ctx}");
                }

                debug!(agent = %self.config.name, count = entries.len(), "memory context injected");
            }
            Ok(_) => {}
            Err(e) => warn!(agent = %self.config.name, "memory recall failed: {e}"),
        }
    }

    // -----------------------------------------------------------------------
    // Provider call with retry
    // -----------------------------------------------------------------------

    async fn call_with_retry(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match self.provider.chat(request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    let err_str = e.to_string();

                    if Self::is_context_length_error(&err_str) {
                        return Err(e);
                    }

                    if !Self::is_retryable_error(&err_str) {
                        return Err(e);
                    }

                    if attempt < self.config.max_retries {
                        let delay = self.config.retry_base_delay_ms * 2u64.pow(attempt);
                        warn!(
                            agent = %self.config.name,
                            attempt = attempt + 1,
                            max = self.config.max_retries,
                            delay_ms = delay,
                            error = %err_str,
                            "retrying after transient error"
                        );
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }

                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("all retries exhausted")))
    }

    // -----------------------------------------------------------------------
    // Tool result persistence
    // -----------------------------------------------------------------------

    /// Resolve or create the persist directory.
    fn resolve_persist_dir(&self, created: &mut Option<PathBuf>) -> Option<PathBuf> {
        if let Some(ref dir) = *created {
            return Some(dir.clone());
        }
        let dir = self.config.persist_dir.clone().unwrap_or_else(|| {
            std::env::temp_dir()
                .join("sigil-tool-results")
                .join(format!("{}-{}", self.config.name, std::process::id()))
        });
        if std::fs::create_dir_all(&dir).is_ok() {
            *created = Some(dir.clone());
            Some(dir)
        } else {
            None
        }
    }

    /// Persist a tool result to disk and return a reference message with preview.
    async fn persist_tool_result(
        dir: &Path,
        tool_use_id: &str,
        content: &str,
    ) -> anyhow::Result<String> {
        let path = dir.join(format!("{tool_use_id}.txt"));

        tokio::fs::write(&path, content).await?;

        let preview = Self::generate_preview(content, PERSIST_PREVIEW_SIZE);

        Ok(format!(
            "<persisted-output>\n\
             Output too large ({} chars). Full output saved to: {}\n\
             Use the file read tool to access it if needed.\n\n\
             Preview (first ~{PERSIST_PREVIEW_SIZE} chars):\n\
             {preview}\n\
             </persisted-output>",
            content.len(),
            path.display(),
        ))
    }

    /// Generate a preview of content, cutting at a newline boundary when possible.
    fn generate_preview(content: &str, max_bytes: usize) -> String {
        if content.len() <= max_bytes {
            return content.to_string();
        }

        let truncated = &content[..max_bytes.min(content.len())];
        let last_newline = truncated.rfind('\n');

        // Cut at a newline if one exists in the back half.
        let cut_point = match last_newline {
            Some(pos) if pos > max_bytes / 2 => pos,
            _ => max_bytes,
        };

        // Find safe UTF-8 boundary.
        let safe_end = content
            .char_indices()
            .take_while(|(i, _)| *i < cut_point)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);

        format!("{}...", &content[..safe_end])
    }

    // -----------------------------------------------------------------------
    // Tool result truncation
    // -----------------------------------------------------------------------

    /// Truncate a tool result with head (40%) + tail (40%) preview.
    fn truncate_result(output: &str, max_chars: usize) -> String {
        if output.len() <= max_chars {
            return output.to_string();
        }

        let head_size = max_chars * 2 / 5;
        let tail_size = max_chars * 2 / 5;
        let omitted = output.len() - head_size - tail_size;

        let head_end = output
            .char_indices()
            .take_while(|(i, _)| *i < head_size)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);

        let tail_start = output
            .char_indices()
            .rev()
            .take_while(|(i, _)| output.len() - *i <= tail_size)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(output.len());

        format!(
            "{}\n\n[... {} characters truncated ...]\n\n{}",
            &output[..head_end],
            omitted,
            &output[tail_start..]
        )
    }

    /// Enforce aggregate character budget across all tool results in a turn.
    fn enforce_result_budget(results: &mut [ProcessedToolResult], max_chars: usize) {
        let total: usize = results.iter().map(|r| r.output.len()).sum();
        if total <= max_chars {
            return;
        }

        let mut indices: Vec<usize> = (0..results.len()).collect();
        indices.sort_by(|a, b| results[*b].output.len().cmp(&results[*a].output.len()));

        let mut current_total = total;
        for idx in indices {
            if current_total <= max_chars {
                break;
            }
            if results[idx].is_error {
                continue;
            }
            let old_len = results[idx].output.len();
            let overage = current_total - max_chars;
            let target_len = old_len.saturating_sub(overage).max(500);
            if target_len < old_len {
                results[idx].output = Self::truncate_result(&results[idx].output, target_len);
                current_total -= old_len - results[idx].output.len();
            }
        }
    }

    // -----------------------------------------------------------------------
    // Mid-turn context enrichment
    // -----------------------------------------------------------------------

    /// Apply token budgets to enrichment attachments and inject as system messages.
    ///
    /// Attachments arrive sorted by priority (lower = higher priority).
    /// Each attachment has its own max_tokens budget. We also enforce a global
    /// enrichment budget (5% of context_window) to prevent enrichments from
    /// consuming too much of the model's capacity.
    fn inject_enrichments(
        messages: &mut Vec<Message>,
        attachments: Vec<ContextAttachment>,
        config: &AgentConfig,
    ) {
        let global_budget = (config.context_window as usize) / 20; // 5% of window
        let mut total_tokens = 0usize;

        for att in &attachments {
            let att_tokens = att.content.len() / CHARS_PER_TOKEN;
            let capped = att_tokens.min(att.max_tokens as usize);

            if total_tokens + capped > global_budget {
                debug!(
                    source = %att.source,
                    "enrichment dropped — global budget exhausted"
                );
                continue;
            }

            let content = if att_tokens > att.max_tokens as usize {
                // Truncate to budget
                let max_chars = att.max_tokens as usize * CHARS_PER_TOKEN;
                format!(
                    "# {} (enrichment)\n{}",
                    att.source,
                    &att.content[..att.content.len().min(max_chars)]
                )
            } else {
                format!("# {} (enrichment)\n{}", att.source, att.content)
            };

            messages.push(Message {
                role: Role::System,
                content: MessageContent::text(content),
            });
            total_tokens += capped;
        }

        if total_tokens > 0 {
            debug!(
                injected = attachments.len(),
                total_tokens,
                "mid-turn enrichments injected"
            );
        }
    }

    // -----------------------------------------------------------------------
    // File tracking for post-compact restoration
    // -----------------------------------------------------------------------

    fn is_file_read_tool(name: &str) -> bool {
        matches!(
            name.to_lowercase().as_str(),
            "read" | "file_read" | "cat" | "readfile"
        )
    }

    fn extract_file_path_from_result(output: &str) -> Option<String> {
        // Common pattern: first line is the file path or "Contents of /path/to/file:"
        let first_line = output.lines().next()?;
        if first_line.contains('/') {
            // Strip common prefixes like "Contents of " or line number prefixes
            let cleaned = first_line
                .trim_start_matches("Contents of ")
                .trim_end_matches(':')
                .trim();
            if cleaned.starts_with('/') || cleaned.starts_with("./") {
                return Some(cleaned.to_string());
            }
        }
        None
    }

    /// Build post-compact file restoration messages from recently-read files.
    fn build_file_restoration(recent_files: &[RecentFile]) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut total_tokens = 0usize;

        // Take the most recent files first (end of vec = most recent).
        for file in recent_files.iter().rev().take(POST_COMPACT_MAX_FILES) {
            let file_tokens = file.content.len() / CHARS_PER_TOKEN;
            let capped = file_tokens.min(POST_COMPACT_MAX_TOKENS_PER_FILE);

            if total_tokens + capped > POST_COMPACT_FILE_BUDGET {
                break;
            }

            let content = if file_tokens > POST_COMPACT_MAX_TOKENS_PER_FILE {
                let max_chars = POST_COMPACT_MAX_TOKENS_PER_FILE * CHARS_PER_TOKEN;
                format!(
                    "# File (restored after compaction): {}\n{}... [truncated]",
                    file.path,
                    &file.content[..file.content.len().min(max_chars)]
                )
            } else {
                format!(
                    "# File (restored after compaction): {}\n{}",
                    file.path, file.content
                )
            };

            messages.push(Message {
                role: Role::System,
                content: MessageContent::text(content),
            });
            total_tokens += capped;
        }

        messages
    }

    /// Detect files that changed externally since we last read them.
    /// Returns system messages with change notifications for injection between turns.
    async fn detect_file_changes(recent_files: &[RecentFile]) -> Vec<Message> {
        let mut changes = Vec::new();

        for file in recent_files {
            if file.mtime_secs == 0 {
                continue; // No mtime recorded — skip.
            }

            let current_mtime = match tokio::fs::metadata(&file.path).await {
                Ok(meta) => meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                Err(_) => continue, // File deleted or inaccessible — skip silently.
            };

            if current_mtime > file.mtime_secs {
                // File was modified externally.
                let notice = format!(
                    "<system-reminder>\nFile modified externally: {}\n\
                     The file has changed since you last read it. \
                     Re-read it before making edits to avoid overwriting external changes.\n\
                     </system-reminder>",
                    file.path
                );
                changes.push(Message {
                    role: Role::User,
                    content: MessageContent::text(notice),
                });
            }
        }

        changes
    }

    /// Build post-compact skill restoration messages from invoked skills.
    /// Skills are sorted by invocation recency (most recent first) and truncated
    /// to per-skill and aggregate token budgets.
    fn build_skill_restoration(invoked_skills: &[InvokedSkill]) -> Vec<Message> {
        if invoked_skills.is_empty() {
            return Vec::new();
        }

        let mut messages = Vec::new();
        let mut total_tokens = 0usize;

        // Sort by recency (most recent first).
        let mut sorted: Vec<&InvokedSkill> = invoked_skills.iter().collect();
        sorted.sort_by(|a, b| b.invoked_at.cmp(&a.invoked_at));

        for skill in sorted {
            let skill_tokens = skill.content.len() / CHARS_PER_TOKEN;
            let capped = skill_tokens.min(POST_COMPACT_MAX_TOKENS_PER_SKILL);

            if total_tokens + capped > POST_COMPACT_SKILLS_BUDGET {
                break;
            }

            let content = if skill_tokens > POST_COMPACT_MAX_TOKENS_PER_SKILL {
                let max_chars = POST_COMPACT_MAX_TOKENS_PER_SKILL * CHARS_PER_TOKEN;
                format!(
                    "# Skill (restored after compaction): {}\n{}... [truncated]",
                    skill.name,
                    &skill.content[..skill.content.len().min(max_chars)]
                )
            } else {
                format!(
                    "# Skill (restored after compaction): {}\n{}",
                    skill.name, skill.content
                )
            };

            messages.push(Message {
                role: Role::System,
                content: MessageContent::text(content),
            });
            total_tokens += capped;
        }

        messages
    }

    // -----------------------------------------------------------------------
    // Conversation repair
    // -----------------------------------------------------------------------

    /// Ensure every tool_use has a matching tool_result and vice versa.
    /// Prevents API 400 errors after compaction drops messages.
    fn repair_tool_pairing(messages: &mut Vec<Message>) {
        // Collect all tool_use IDs from assistant messages.
        let mut tool_use_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        // Collect all tool_result IDs from tool messages.
        let mut tool_result_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        for msg in messages.iter() {
            if let MessageContent::Parts(parts) = &msg.content {
                for part in parts {
                    match part {
                        ContentPart::ToolUse { id, .. } => {
                            tool_use_ids.insert(id.clone());
                        }
                        ContentPart::ToolResult { tool_use_id, .. } => {
                            tool_result_ids.insert(tool_use_id.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        // Find dangling tool_uses (no matching result).
        let dangling: Vec<String> = tool_use_ids
            .difference(&tool_result_ids)
            .cloned()
            .collect();

        // Find orphan tool_results (no matching use).
        let orphans: Vec<String> = tool_result_ids
            .difference(&tool_use_ids)
            .cloned()
            .collect();

        if dangling.is_empty() && orphans.is_empty() {
            return;
        }

        // Add synthetic results for dangling tool_uses.
        if !dangling.is_empty() {
            debug!(
                count = dangling.len(),
                "injecting synthetic tool_results for dangling tool_uses"
            );
            let synthetic_parts: Vec<ContentPart> = dangling
                .iter()
                .map(|id| ContentPart::ToolResult {
                    tool_use_id: id.clone(),
                    content: SYNTHETIC_TOOL_RESULT.to_string(),
                    is_error: true,
                })
                .collect();

            // Find the last assistant message and insert results after it.
            if let Some(pos) = messages.iter().rposition(|m| m.role == Role::Assistant) {
                let insert_at = pos + 1;
                messages.insert(
                    insert_at.min(messages.len()),
                    Message {
                        role: Role::Tool,
                        content: MessageContent::Parts(synthetic_parts),
                    },
                );
            }
        }

        // Strip orphan tool_results.
        if !orphans.is_empty() {
            debug!(
                count = orphans.len(),
                "stripping orphan tool_results"
            );
            let orphan_set: std::collections::HashSet<&str> =
                orphans.iter().map(|s| s.as_str()).collect();

            for msg in messages.iter_mut() {
                if msg.role != Role::Tool {
                    continue;
                }
                if let MessageContent::Parts(ref mut parts) = msg.content {
                    parts.retain(|p| {
                        if let ContentPart::ToolResult { tool_use_id, .. } = p {
                            !orphan_set.contains(tool_use_id.as_str())
                        } else {
                            true
                        }
                    });
                }
            }

            // Remove empty tool messages.
            messages.retain(|m| {
                if m.role == Role::Tool
                    && let MessageContent::Parts(parts) = &m.content
                {
                    return !parts.is_empty();
                }
                true
            });
        }
    }

    // -----------------------------------------------------------------------
    // Multi-stage context compaction
    // -----------------------------------------------------------------------

    fn estimate_tokens_from_messages(messages: &[Message]) -> u32 {
        let total_chars: usize = messages
            .iter()
            .map(|m| match &m.content {
                MessageContent::Text(t) => t.len(),
                MessageContent::Parts(parts) => parts
                    .iter()
                    .map(|p| match p {
                        ContentPart::Text { text } => text.len(),
                        ContentPart::ToolUse { input, name, .. } => {
                            name.len() + input.to_string().len()
                        }
                        ContentPart::ToolResult { content, .. } => content.len(),
                    })
                    .sum(),
            })
            .sum();

        (total_chars / CHARS_PER_TOKEN) as u32
    }

    /// Snip compaction: remove entire old API rounds (assistant + tool messages)
    /// from the compactable window. No API call — purely token estimation.
    /// Returns estimated tokens freed.
    fn snip_compact(
        messages: &mut Vec<Message>,
        preserve_head: usize,
        preserve_tail: usize,
    ) -> u32 {
        if messages.len() <= preserve_head + preserve_tail {
            return 0;
        }
        let window_start = preserve_head;
        let window_end = messages.len().saturating_sub(preserve_tail);
        if window_start >= window_end {
            return 0;
        }

        // Find "API rounds" — sequences of (Assistant, Tool) messages.
        // Remove the oldest rounds first (from window_start forward).
        let mut remove_count = 0;
        let mut tokens_freed: u32 = 0;
        let mut i = window_start;

        // Remove at most half the window to avoid over-snipping.
        let max_remove = (window_end - window_start) / 2;

        while i < window_end && remove_count < max_remove {
            // A round starts with an Assistant message.
            if messages[i].role != Role::Assistant {
                i += 1;
                continue;
            }

            // Count the round: Assistant + following Tool messages.
            let round_start = i;
            let mut round_end = i + 1;
            while round_end < window_end && messages[round_end].role == Role::Tool {
                round_end += 1;
            }

            // Estimate tokens in this round.
            let round_tokens = Self::estimate_tokens_from_messages(&messages[round_start..round_end]);
            tokens_freed += round_tokens;
            remove_count += round_end - round_start;
            let _next = round_end; // consumed by break

            // Stop after removing one full round — snip is conservative.
            break;
        }

        if remove_count > 0 {
            // Remove the snipped messages.
            messages.drain(window_start..window_start + remove_count);
        }

        tokens_freed
    }

    /// Microcompact: clear old tool results by tool name, keeping the N most recent.
    /// More targeted than the old digest — only clears results from compactable tools
    /// (read, shell, grep, glob, web_search, web_fetch, edit, write).
    fn microcompact(
        messages: &mut [Message],
        preserve_tail: usize,
        keep_recent: usize,
    ) {
        if messages.len() <= preserve_tail {
            return;
        }
        let cutoff = messages.len() - preserve_tail;

        // Collect all compactable tool_use IDs and their associated tool_result IDs.
        // We need to match tool_use names to tool_result IDs.
        let mut compactable_ids: Vec<String> = Vec::new();

        // Pass 1: find tool_use blocks with compactable tool names.
        for msg in messages[..cutoff].iter() {
            if let MessageContent::Parts(parts) = &msg.content {
                for part in parts {
                    if let ContentPart::ToolUse { id, name, .. } = part {
                        let lower = name.to_lowercase();
                        if COMPACTABLE_TOOLS.iter().any(|t| lower.contains(t)) {
                            compactable_ids.push(id.clone());
                        }
                    }
                }
            }
        }

        if compactable_ids.len() <= keep_recent {
            return; // Nothing to clear.
        }

        // Keep the most recent N, clear the rest.
        let clear_set: std::collections::HashSet<&str> = compactable_ids
            [..compactable_ids.len() - keep_recent]
            .iter()
            .map(|s| s.as_str())
            .collect();

        if clear_set.is_empty() {
            return;
        }

        // Pass 2: clear tool_result content for the IDs to clear.
        let mut cleared = 0usize;
        for msg in messages[..cutoff].iter_mut() {
            if msg.role != Role::Tool {
                continue;
            }
            if let MessageContent::Parts(ref mut parts) = msg.content {
                for part in parts.iter_mut() {
                    if let ContentPart::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } = part
                        && clear_set.contains(tool_use_id.as_str())
                        && *content != MICROCOMPACT_CLEARED
                    {
                        *content = MICROCOMPACT_CLEARED.to_string();
                        cleared += 1;
                    }
                }
            }
        }

        if cleared > 0 {
            debug!(
                cleared,
                total = compactable_ids.len(),
                kept = keep_recent,
                "microcompact: cleared old tool results"
            );
        }
    }

    /// Stages 2+3: Compact conversation by summarizing middle messages.
    /// After compaction, restores: (1) active context (files/tools in use),
    /// (2) preserved skills, (3) recently-read file contents, (4) enrichments.
    async fn compact_messages(
        &self,
        messages: &mut Vec<Message>,
        recent_files: &[RecentFile],
        invoked_skills: &[InvokedSkill],
    ) {
        let head = self.config.compact_preserve_head.min(messages.len());
        let tail = self
            .config
            .compact_preserve_tail
            .min(messages.len().saturating_sub(head));

        if messages.len() <= head + tail {
            return;
        }

        let middle_end = messages.len() - tail;
        let middle = &messages[head..middle_end];
        let middle_count = middle.len();

        if middle.is_empty() {
            return;
        }

        // Extract active context BEFORE compacting (files, tools in use).
        let active_context = Self::extract_active_context(middle);

        // Extract skill-load messages — these survive compaction verbatim.
        // Skills are working instructions, not conversation history.
        let preserved_skills = Self::extract_skill_messages(middle);

        let transcript = Self::build_compaction_transcript(middle);

        let summary = match self.summarize_context(&transcript).await {
            Ok(s) => s,
            Err(e) => {
                warn!(agent = %self.config.name, "LLM compaction failed: {e}");
                Self::simple_compact_summary(middle)
            }
        };

        let mut compacted = Vec::with_capacity(head + 2 + tail);
        compacted.extend_from_slice(&messages[..head]);
        compacted.push(Message {
            role: Role::System,
            content: MessageContent::text(format!(
                "# Context Summary\n[{middle_count} messages compacted]\n\n{summary}"
            )),
        });

        // Re-inject preserved skill content — these are working instructions,
        // not history. They survive compaction verbatim.
        for skill_content in &preserved_skills {
            compacted.push(Message {
                role: Role::System,
                content: MessageContent::text(format!(
                    "# Skill (preserved through compaction)\n{skill_content}"
                )),
            });
        }

        // Post-compact restoration: inject active context so the model knows
        // what it was working on without re-reading the full summary.
        if !active_context.is_empty() {
            compacted.push(Message {
                role: Role::System,
                content: MessageContent::text(format!(
                    "# Active Context (restored after compaction)\n{active_context}"
                )),
            });
        }

        // Post-compact file restoration: re-inject recently-read file contents
        // so the model can continue working without re-reading them.
        let file_msgs = Self::build_file_restoration(recent_files);
        if !file_msgs.is_empty() {
            debug!(
                agent = %self.config.name,
                files = file_msgs.len(),
                "restoring file contents after compaction"
            );
            compacted.extend(file_msgs);
        }

        // Post-compact skill restoration: re-inject invoked skill content
        // so the model retains working instructions across compactions.
        let skill_msgs = Self::build_skill_restoration(invoked_skills);
        if !skill_msgs.is_empty() {
            debug!(
                agent = %self.config.name,
                skills = skill_msgs.len(),
                "restoring skill content after compaction"
            );
            compacted.extend(skill_msgs);
        }

        compacted.extend_from_slice(&messages[messages.len() - tail..]);

        self.observer
            .record(Event::Custom {
                name: "context_compacted".to_string(),
                data: serde_json::json!({
                    "original_messages": messages.len(),
                    "compacted_messages": middle_count,
                    "remaining_messages": compacted.len(),
                }),
            })
            .await;

        self.emit(crate::chat_stream::ChatStreamEvent::Compacted {
            original_messages: messages.len(),
            remaining_messages: compacted.len(),
            compaction_number: 0, // Caller tracks this
        });

        info!(
            agent = %self.config.name,
            original = messages.len(),
            removed = middle_count,
            remaining = compacted.len(),
            "context compacted"
        );

        *messages = compacted;
    }

    fn build_compaction_transcript(messages: &[Message]) -> String {
        // Larger budget — the structured summarizer needs detail to produce
        // a good 9-section summary. 16K chars ≈ 4K tokens of input.
        const MAX_TRANSCRIPT: usize = 16_000;
        const MAX_TEXT_BLOCK: usize = 500;
        const MAX_TOOL_RESULT: usize = 300;

        let mut transcript = String::with_capacity(MAX_TRANSCRIPT);

        for msg in messages {
            if transcript.len() >= MAX_TRANSCRIPT {
                break;
            }

            let role = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
                Role::Tool => "Tool",
            };

            let text = match &msg.content {
                MessageContent::Text(t) => {
                    if t.len() > MAX_TEXT_BLOCK {
                        format!("{}...", &t[..MAX_TEXT_BLOCK])
                    } else {
                        t.clone()
                    }
                }
                MessageContent::Parts(parts) => parts
                    .iter()
                    .map(|p| match p {
                        ContentPart::Text { text } => {
                            if text.len() > MAX_TEXT_BLOCK {
                                format!("{}...", &text[..MAX_TEXT_BLOCK])
                            } else {
                                text.clone()
                            }
                        }
                        ContentPart::ToolUse { name, input, .. } => {
                            // Include tool name + key input fields for context.
                            let input_preview = if let Some(obj) = input.as_object() {
                                obj.iter()
                                    .take(3)
                                    .map(|(k, v)| {
                                        let vs = v.to_string();
                                        if vs.len() > 80 {
                                            format!("{k}={:.80}...", vs)
                                        } else {
                                            format!("{k}={vs}")
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            } else {
                                String::new()
                            };
                            format!("[tool:{name}({input_preview})]")
                        }
                        ContentPart::ToolResult {
                            content, is_error, ..
                        } => {
                            let prefix = if *is_error { "ERROR: " } else { "" };
                            if content.len() > MAX_TOOL_RESULT {
                                format!("{prefix}{}...", &content[..MAX_TOOL_RESULT])
                            } else {
                                format!("{prefix}{content}")
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | "),
            };

            if !text.is_empty() {
                let entry = format!("{role}: {text}\n");
                if transcript.len() + entry.len() > MAX_TRANSCRIPT {
                    break;
                }
                transcript.push_str(&entry);
            }
        }

        transcript
    }

    /// Extract files and tools from messages being compacted.
    /// Returns a human-readable context string for post-compact restoration.
    fn extract_active_context(messages: &[Message]) -> String {
        let mut files: Vec<String> = Vec::new();
        let mut tools: Vec<String> = Vec::new();

        for msg in messages {
            let parts = match &msg.content {
                MessageContent::Parts(p) => p,
                _ => continue,
            };

            for part in parts {
                if let ContentPart::ToolUse { name, input, .. } = part {
                    if !tools.contains(name) {
                        tools.push(name.clone());
                    }

                    // Extract file paths from common tool input fields.
                    for key in &["file_path", "path", "file", "directory"] {
                        if let Some(path) = input.get(*key).and_then(|v| v.as_str()) {
                            let path = path.to_string();
                            if !files.contains(&path) {
                                files.push(path);
                            }
                        }
                    }

                    // Extract paths from glob/grep pattern fields.
                    if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str())
                        && (pattern.contains('/') || pattern.contains('.'))
                    {
                        let path = pattern.to_string();
                        if !files.contains(&path) && path.len() < 200 {
                            files.push(path);
                        }
                    }
                }
            }
        }

        // Cap at most recent to avoid overwhelming the context.
        let max_files = 10;
        let recent_files: Vec<&str> = files.iter().rev().take(max_files).map(|s| s.as_str()).collect();

        let mut context = String::new();
        if !recent_files.is_empty() {
            context.push_str("Files active before compaction:\n");
            for f in &recent_files {
                context.push_str(&format!("- {f}\n"));
            }
        }
        if !tools.is_empty() {
            context.push_str(&format!("Tools used: {}\n", tools.join(", ")));
        }
        context
    }

    /// Extract skill content from tool results in compacted messages.
    /// Skills are identified by ToolUse blocks calling "sigil_skills" with action="get",
    /// followed by their ToolResult. The result content (the skill text) is preserved.
    fn extract_skill_messages(messages: &[Message]) -> Vec<String> {
        let mut skill_tool_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut skill_contents: Vec<String> = Vec::new();

        // Pass 1: find tool_use IDs for sigil_skills get calls.
        for msg in messages {
            if let MessageContent::Parts(parts) = &msg.content {
                for part in parts {
                    if let ContentPart::ToolUse { id, name, input, .. } = part
                        && name == "sigil_skills"
                        && input
                            .get("action")
                            .and_then(|v| v.as_str())
                            .is_some_and(|a| a == "get")
                    {
                        skill_tool_ids.insert(id.clone());
                    }
                }
            }
        }

        if skill_tool_ids.is_empty() {
            return skill_contents;
        }

        // Pass 2: extract the tool result content for those IDs.
        for msg in messages {
            if let MessageContent::Parts(parts) = &msg.content {
                for part in parts {
                    if let ContentPart::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                        ..
                    } = part
                        && !is_error
                        && skill_tool_ids.contains(tool_use_id)
                        && !content.is_empty()
                    {
                        skill_contents.push(content.clone());
                    }
                }
            }
        }

        skill_contents
    }

    async fn summarize_context(&self, transcript: &str) -> anyhow::Result<String> {
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text(format!(
                    "CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.\n\
                     Tool calls will be REJECTED and will waste your only turn — you will fail the task.\n\
                     Your entire response must be plain text: an <analysis> block followed by a <summary> block.\n\n\
                     You are summarizing an autonomous agent's execution context. This summary \
                     replaces the compacted messages — the agent will use it to continue working. \
                     Anything you omit is lost forever.\n\n\
                     First write an <analysis> block as a drafting scratchpad (it will be stripped), \
                     then a <summary> block with ALL of these sections:\n\n\
                     1. **Primary Request and Intent** — What was the user's original request? \
                     What are the acceptance criteria? What is the end goal?\n\
                     2. **Key Technical Concepts** — Domain-specific terms, patterns, and \
                     constraints that affect the work. Include library versions, API contracts, \
                     architectural decisions.\n\
                     3. **Files and Code Sections** — Every file read, edited, or created. \
                     Include filenames with paths, what changed, and **full code snippets** for \
                     any code that is currently being worked on or was recently modified.\n\
                     4. **Errors and Fixes** — Every error encountered and exactly how it was \
                     resolved. Include error messages verbatim. This prevents re-encountering \
                     the same issues.\n\
                     5. **Problem Solving** — The reasoning chain: what was tried, what worked, \
                     what was rejected and why. Include rejected approaches to prevent retry.\n\
                     6. **All User Messages** — Reproduce every user instruction, clarification, \
                     or correction. Do not paraphrase — use the user's exact words for requests \
                     and corrections.\n\
                     7. **Pending Tasks** — What remains to be done, in dependency order. \
                     Include any task IDs, branch names, or tracking references.\n\
                     8. **Current Work** — What the agent was doing at the moment of compaction. \
                     Be precise: filename, function name, line range, what operation was in \
                     progress. Include enough detail to resume without re-reading.\n\
                     9. **Next Step** — The single immediate next action the agent should take. \
                     Include direct quotes from tool output or code that show where work left off.\n\n\
                     Be precise. Include filenames, function signatures, error messages, and \
                     code snippets where they affect the next action. Vague summaries cause \
                     the agent to redo work or make wrong assumptions.\n\n\
                     ## Execution Transcript\n\n{transcript}"
                )),
            }],
            tools: vec![],
            max_tokens: 8192,
            temperature: 0.0,
        };

        let response = self.provider.chat(&request).await?;
        let text = response
            .content
            .ok_or_else(|| anyhow::anyhow!("empty summary response"))?;

        // Strip the <analysis> scratchpad, keep only <summary> content.
        if let Some(start) = text.find("<summary>") {
            let content_start = start + "<summary>".len();
            let end = text.find("</summary>").unwrap_or(text.len());
            Ok(text[content_start..end].trim().to_string())
        } else {
            Ok(text)
        }
    }

    fn simple_compact_summary(messages: &[Message]) -> String {
        let key_points: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                let text = match &m.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::Parts(parts) => parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::Text { text } => Some(text.as_str()),
                            ContentPart::ToolUse { name, .. } => Some(name.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                if text.is_empty() {
                    None
                } else if text.len() > 200 {
                    Some(format!("{}...", &text[..200]))
                } else {
                    Some(text)
                }
            })
            .collect();

        format!(
            "[{} messages summarized. Key points: {}]",
            messages.len(),
            key_points.join(" | ")
        )
    }

    // -----------------------------------------------------------------------
    // Error classification
    // -----------------------------------------------------------------------

    fn is_retryable_error(error: &str) -> bool {
        let lower = error.to_lowercase();
        lower.contains("rate")
            || lower.contains("429")
            || lower.contains("overloaded")
            || lower.contains("503")
            || lower.contains("529")
            || lower.contains("timeout")
            || lower.contains("timed out")
            || lower.contains("connection")
            || lower.contains("server error")
            || lower.contains("500 ")
            || lower.contains("502")
    }

    fn is_context_length_error(error: &str) -> bool {
        let lower = error.to_lowercase();
        lower.contains("context length")
            || lower.contains("token limit")
            || lower.contains("prompt is too long")
            || lower.contains("maximum context")
            || (lower.contains("too long") && lower.contains("token"))
    }

    // -----------------------------------------------------------------------
    // Reflection
    // -----------------------------------------------------------------------

    async fn reflect(&self, messages: &[Message]) {
        let Some(ref mem) = self.memory else { return };

        let transcript = self.compact_transcript(messages);
        if transcript.len() < 50 {
            return;
        }

        let reflection_prompt = format!(
            "You are a memory extraction system. Analyze this conversation and extract ONLY \
             genuinely important insights worth remembering long-term. Output NOTHING if the \
             conversation is trivial (greetings, status checks, small talk).\n\n\
             For each insight, output exactly one line in this format:\n\
             SCOPE CATEGORY: key-slug | The insight content\n\n\
             Scopes (choose the most appropriate):\n\
             - DOMAIN: Technical facts about this specific project/codebase\n\
             - SYSTEM: Insights about the user (preferences, decisions, patterns that span projects)\n\
             - SELF: Your own observations, reflections, learnings as an agent\n\n\
             Categories:\n\
             - FACT: Factual information (technical details, architecture decisions, numbers)\n\
             - PROCEDURE: How something works or should be done\n\
             - PREFERENCE: User preferences, opinions, behavioral patterns\n\
             - CONTEXT: Decisions made, strategic shifts, project state changes\n\n\
             Rules:\n\
             - Maximum 5 insights per conversation\n\
             - Each insight must be self-contained (understandable without the conversation)\n\
             - key-slug: 2-4 lowercase hyphenated words\n\
             - Content: one concise sentence\n\
             - If nothing is worth remembering, output exactly: NONE\n\n\
             ## Conversation\n\n{}",
            transcript
        );

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text(&reflection_prompt),
            }],
            tools: vec![],
            max_tokens: 512,
            temperature: 0.0,
        };

        match self.provider.chat(&request).await {
            Ok(response) => {
                if let Some(text) = response.content {
                    self.store_insights(&text, mem).await;
                }
            }
            Err(e) => warn!(agent = %self.config.name, "reflection failed: {e}"),
        }
    }

    fn compact_transcript(&self, messages: &[Message]) -> String {
        let mut transcript = String::new();
        let max_len = 8000;

        for msg in messages {
            if transcript.len() >= max_len {
                break;
            }

            let role = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System | Role::Tool => continue,
            };

            let text = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            };

            if !text.is_empty() {
                let remaining = max_len.saturating_sub(transcript.len());
                if text.len() > remaining {
                    transcript.push_str(&format!("{role}: {}\n\n", &text[..remaining]));
                } else {
                    transcript.push_str(&format!("{role}: {text}\n\n"));
                }
            }
        }
        transcript
    }

    async fn store_insights(&self, text: &str, mem: &Arc<dyn Memory>) {
        for line in text.lines() {
            let line = line.trim();
            if line == "NONE" || line.is_empty() {
                continue;
            }

            let (scope, rest) = if let Some(r) = line.strip_prefix("DOMAIN ") {
                (MemoryScope::Domain, r)
            } else if let Some(r) = line.strip_prefix("SYSTEM ") {
                (MemoryScope::System, r)
            } else if let Some(r) = line.strip_prefix("SELF ") {
                (MemoryScope::Entity, r)
            } else if let Some((cat_str, _)) = line.split_once(':') {
                let cat_str = cat_str.trim();
                if matches!(cat_str, "FACT" | "PROCEDURE" | "PREFERENCE" | "CONTEXT") {
                    (MemoryScope::Domain, line)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let Some((cat_str, rest)) = rest.split_once(':') else {
                continue;
            };
            let Some((key, content)) = rest.split_once('|') else {
                continue;
            };

            let category = match cat_str.trim().to_uppercase().as_str() {
                "FACT" => MemoryCategory::Fact,
                "PROCEDURE" => MemoryCategory::Procedure,
                "PREFERENCE" => MemoryCategory::Preference,
                "CONTEXT" => MemoryCategory::Context,
                _ => continue,
            };

            let key = key.trim();
            let content = content.trim();
            if key.is_empty() || content.is_empty() {
                continue;
            }

            let entity_id = if scope == MemoryScope::Entity {
                self.config.entity_id.as_deref()
            } else {
                None
            };

            match mem.store(key, content, category, scope, entity_id).await {
                Ok(id) => {
                    debug!(agent = %self.config.name, id = %id, key = %key, scope = %scope, "insight stored")
                }
                Err(e) => {
                    warn!(agent = %self.config.name, key = %key, "failed to store insight: {e}")
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn has_novel_terms(tool_output: &str, original_prompt: &str) -> bool {
        let prompt_words: std::collections::HashSet<&str> = original_prompt
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 5)
            .collect();

        let novel_count = tool_output
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 5 && !prompt_words.contains(w))
            .take(3)
            .count();

        novel_count >= 3
    }

    async fn execute_tool_static(
        tools: &[Arc<dyn Tool>],
        name: &str,
        args: serde_json::Value,
    ) -> anyhow::Result<ToolResult> {
        for tool in tools {
            if tool.name() == name {
                return tool.execute(args).await;
            }
        }
        Ok(ToolResult::error(format!("Unknown tool: {name}")))
    }

    // -----------------------------------------------------------------------
    // Session persistence
    // -----------------------------------------------------------------------

    /// Save a session checkpoint to the configured session file.
    async fn save_session(
        messages: &[Message],
        tracker: &ContextTracker,
        iterations: u32,
        active_model: &str,
        session_file: &Path,
    ) {
        let state = SessionState {
            messages: messages.to_vec(),
            iterations,
            total_prompt_tokens: tracker.total_prompt_tokens,
            total_completion_tokens: tracker.total_completion_tokens,
            compactions: tracker.compactions,
            active_model: active_model.to_string(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        match serde_json::to_string(&state) {
            Ok(json) => {
                if let Some(parent) = session_file.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match tokio::fs::write(session_file, json).await {
                    Ok(()) => {
                        debug!(
                            path = %session_file.display(),
                            iterations,
                            messages = messages.len(),
                            "session checkpoint saved"
                        );
                    }
                    Err(e) => {
                        warn!(
                            path = %session_file.display(),
                            "failed to save session checkpoint: {e}"
                        );
                    }
                }
            }
            Err(e) => {
                warn!("failed to serialize session state: {e}");
            }
        }
    }

    /// Load a session checkpoint from the configured session file.
    /// Returns None if the file doesn't exist or can't be parsed.
    async fn load_session(session_file: &Path) -> Option<SessionState> {
        match tokio::fs::read_to_string(session_file).await {
            Ok(json) => match serde_json::from_str::<SessionState>(&json) {
                Ok(state) => {
                    info!(
                        path = %session_file.display(),
                        iterations = state.iterations,
                        messages = state.messages.len(),
                        age_ms = {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            now.saturating_sub(state.timestamp_ms)
                        },
                        "resuming from session checkpoint"
                    );
                    Some(state)
                }
                Err(e) => {
                    warn!(path = %session_file.display(), "corrupt session file, starting fresh: {e}");
                    None
                }
            },
            Err(_) => None, // File doesn't exist — normal case.
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_novel_terms_true() {
        let prompt = "Fix the authentication bug in login.rs";
        let output = "Found DatabaseConnection, TransactionPool, and ConnectionManager \
                       types in the codebase that handle connection lifecycle";
        assert!(Agent::has_novel_terms(output, prompt));
    }

    #[test]
    fn test_has_novel_terms_false_same_words() {
        let prompt = "Fix authentication and login handling";
        let output = "Checked authentication and login handling";
        assert!(!Agent::has_novel_terms(output, prompt));
    }

    #[test]
    fn test_has_novel_terms_false_short_output() {
        let prompt = "Fix the bug";
        let output = "ok done";
        assert!(!Agent::has_novel_terms(output, prompt));
    }

    #[test]
    fn test_truncate_result_below_limit() {
        let output = "short output";
        let result = Agent::truncate_result(output, 100);
        assert_eq!(result, output);
    }

    #[test]
    fn test_truncate_result_above_limit() {
        let output = "a".repeat(10_000);
        let result = Agent::truncate_result(&output, 1000);
        assert!(result.len() < output.len());
        assert!(result.contains("characters truncated"));
        assert!(result.starts_with("aaaa"));
        assert!(result.ends_with("aaaa"));
    }

    #[test]
    fn test_truncate_result_utf8_safe() {
        let output = "🦀".repeat(5000);
        let result = Agent::truncate_result(&output, 1000);
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_enforce_result_budget_under_budget() {
        let mut results = vec![
            ProcessedToolResult {
                id: "1".into(),
                name: "a".into(),
                output: "x".repeat(100),
                is_error: false,
            },
            ProcessedToolResult {
                id: "2".into(),
                name: "b".into(),
                output: "y".repeat(100),
                is_error: false,
            },
        ];
        Agent::enforce_result_budget(&mut results, 1000);
        assert_eq!(results[0].output.len(), 100);
        assert_eq!(results[1].output.len(), 100);
    }

    #[test]
    fn test_enforce_result_budget_over_budget() {
        let mut results = vec![
            ProcessedToolResult {
                id: "1".into(),
                name: "small".into(),
                output: "x".repeat(100),
                is_error: false,
            },
            ProcessedToolResult {
                id: "2".into(),
                name: "big".into(),
                output: "y".repeat(10_000),
                is_error: false,
            },
        ];
        Agent::enforce_result_budget(&mut results, 5000);
        let total: usize = results.iter().map(|r| r.output.len()).sum();
        assert!(total <= 5500, "total was {total}");
        assert_eq!(results[0].output.len(), 100);
        assert!(results[1].output.len() < 10_000);
    }

    #[test]
    fn test_enforce_result_budget_skips_errors() {
        let mut results = vec![ProcessedToolResult {
            id: "1".into(),
            name: "err".into(),
            output: "x".repeat(10_000),
            is_error: true,
        }];
        Agent::enforce_result_budget(&mut results, 100);
        assert_eq!(results[0].output.len(), 10_000);
    }

    #[test]
    fn test_estimate_tokens() {
        let messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::text("a".repeat(400)),
            },
            Message {
                role: Role::User,
                content: MessageContent::text("b".repeat(400)),
            },
        ];
        let tokens = Agent::estimate_tokens_from_messages(&messages);
        assert_eq!(tokens, 200);
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(Agent::is_retryable_error("rate limit exceeded"));
        assert!(Agent::is_retryable_error("HTTP 429 Too Many Requests"));
        assert!(Agent::is_retryable_error("server is overloaded"));
        assert!(Agent::is_retryable_error("connection reset"));
        assert!(Agent::is_retryable_error("request timed out"));
        assert!(!Agent::is_retryable_error("invalid API key"));
        assert!(!Agent::is_retryable_error("malformed request body"));
    }

    #[test]
    fn test_is_context_length_error() {
        assert!(Agent::is_context_length_error(
            "request exceeds maximum context length"
        ));
        assert!(Agent::is_context_length_error("token limit reached"));
        assert!(Agent::is_context_length_error("prompt is too long"));
        assert!(!Agent::is_context_length_error("network timeout"));
        assert!(!Agent::is_context_length_error("rate limited"));
    }

    #[test]
    fn test_simple_compact_summary() {
        let messages = vec![
            Message {
                role: Role::User,
                content: MessageContent::text("First message"),
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::text("Second message"),
            },
        ];
        let summary = Agent::simple_compact_summary(&messages);
        assert!(summary.contains("2 messages summarized"));
        assert!(summary.contains("First message"));
        assert!(summary.contains("Second message"));
    }

    #[test]
    fn test_default_config_values() {
        let config = AgentConfig::default();
        assert_eq!(config.context_window, 200_000);
        assert_eq!(config.max_tool_result_chars, 50_000);
        assert_eq!(config.max_tool_results_per_turn, 200_000);
        assert_eq!(config.max_retries, 0);
        assert_eq!(config.max_output_recovery, 3);
        assert_eq!(config.max_tokens, 8192);
        assert!(config.fallback_model.is_none());
        assert!(config.persist_dir.is_none());
        assert!((config.compact_threshold - 0.80).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_token_budget_shorthand() {
        assert_eq!(AgentConfig::parse_token_budget("+500k"), Some(500_000));
        assert_eq!(AgentConfig::parse_token_budget("+2m"), Some(2_000_000));
        assert_eq!(AgentConfig::parse_token_budget("fix the bug +500k"), Some(500_000));
        assert_eq!(AgentConfig::parse_token_budget("+1.5m"), Some(1_500_000));
    }

    #[test]
    fn test_parse_token_budget_verbose() {
        assert_eq!(
            AgentConfig::parse_token_budget("use 500k tokens"),
            Some(500_000)
        );
        assert_eq!(
            AgentConfig::parse_token_budget("spend 2m tokens on this"),
            Some(2_000_000)
        );
    }

    #[test]
    fn test_parse_token_budget_none() {
        assert_eq!(AgentConfig::parse_token_budget("fix the bug"), None);
        assert_eq!(AgentConfig::parse_token_budget("hello world"), None);
    }

    #[test]
    fn test_agent_stop_reason_eq() {
        assert_eq!(AgentStopReason::EndTurn, AgentStopReason::EndTurn);
        assert_eq!(
            AgentStopReason::MaxIterations,
            AgentStopReason::MaxIterations
        );
        assert_ne!(AgentStopReason::EndTurn, AgentStopReason::MaxIterations);
    }

    #[test]
    fn test_generate_preview_short() {
        let content = "hello world";
        let preview = Agent::generate_preview(content, 100);
        assert_eq!(preview, "hello world");
    }

    #[test]
    fn test_generate_preview_cuts_at_newline() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let preview = Agent::generate_preview(content, 20);
        // Should cut at a newline boundary, not mid-line.
        assert!(preview.ends_with("..."));
        assert!(!preview.contains("line5"));
    }

    #[test]
    fn test_microcompact() {
        let mut messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::text("system"),
            },
            // Old assistant + tool round with compactable tool.
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![ContentPart::ToolUse {
                    id: "t1".into(),
                    name: "read_file".into(),
                    input: serde_json::json!({"file_path": "/src/main.rs"}),
                }]),
            },
            Message {
                role: Role::Tool,
                content: MessageContent::Parts(vec![ContentPart::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "x".repeat(1000),
                    is_error: false,
                }]),
            },
            // Recent assistant + tool round with compactable tool.
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![ContentPart::ToolUse {
                    id: "t2".into(),
                    name: "grep".into(),
                    input: serde_json::json!({"pattern": "foo"}),
                }]),
            },
            Message {
                role: Role::Tool,
                content: MessageContent::Parts(vec![ContentPart::ToolResult {
                    tool_use_id: "t2".into(),
                    content: "y".repeat(1000),
                    is_error: false,
                }]),
            },
        ];

        // keep_recent=1 means only the most recent compactable tool is kept.
        Agent::microcompact(&mut messages, 0, 1);

        // First tool result (t1) should be cleared.
        if let MessageContent::Parts(parts) = &messages[2].content
            && let ContentPart::ToolResult { content, .. } = &parts[0]
        {
            assert_eq!(content, "[Old tool result content cleared]");
        }

        // Second tool result (t2, most recent) should be preserved.
        if let MessageContent::Parts(parts) = &messages[4].content
            && let ContentPart::ToolResult { content, .. } = &parts[0]
        {
            assert_eq!(content.len(), 1000, "recent result should be preserved");
        }
    }

    #[test]
    fn test_snip_compact() {
        let mut messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::text("system prompt"),
            },
            Message {
                role: Role::User,
                content: MessageContent::text("user request"),
            },
            // Old round to snip.
            Message {
                role: Role::Assistant,
                content: MessageContent::text("a".repeat(400)),
            },
            Message {
                role: Role::Tool,
                content: MessageContent::Parts(vec![ContentPart::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "b".repeat(400),
                    is_error: false,
                }]),
            },
            // Recent round to preserve.
            Message {
                role: Role::Assistant,
                content: MessageContent::text("recent work"),
            },
        ];

        let freed = Agent::snip_compact(&mut messages, 2, 1);
        assert!(freed > 0, "should have freed tokens");
        // Head (2) + tail (1) preserved, middle snipped.
        assert_eq!(messages.len(), 3, "should have 3 messages after snip (was 5)");
        // Head preserved.
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        // Tail preserved.
        assert_eq!(messages[2].role, Role::Assistant);
    }

    #[test]
    fn test_loop_transition_debug() {
        let t = LoopTransition::ToolUse;
        assert_eq!(format!("{t:?}"), "ToolUse");
        let t = LoopTransition::OutputTruncated { attempt: 2 };
        assert!(format!("{t:?}").contains("2"));
    }

    #[test]
    fn test_extract_active_context_finds_files() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![ContentPart::ToolUse {
                    id: "t1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({"file_path": "/src/main.rs"}),
                }]),
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![ContentPart::ToolUse {
                    id: "t2".into(),
                    name: "Edit".into(),
                    input: serde_json::json!({"file_path": "/src/lib.rs", "old_string": "a", "new_string": "b"}),
                }]),
            },
        ];
        let ctx = Agent::extract_active_context(&messages);
        assert!(ctx.contains("/src/main.rs"));
        assert!(ctx.contains("/src/lib.rs"));
        assert!(ctx.contains("Read"));
        assert!(ctx.contains("Edit"));
    }

    #[test]
    fn test_extract_active_context_deduplicates() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![
                    ContentPart::ToolUse {
                        id: "t1".into(),
                        name: "Read".into(),
                        input: serde_json::json!({"file_path": "/src/main.rs"}),
                    },
                    ContentPart::ToolUse {
                        id: "t2".into(),
                        name: "Read".into(),
                        input: serde_json::json!({"file_path": "/src/main.rs"}),
                    },
                ]),
            },
        ];
        let ctx = Agent::extract_active_context(&messages);
        // Should only appear once.
        assert_eq!(ctx.matches("/src/main.rs").count(), 1);
        assert_eq!(ctx.matches("Read").count(), 1);
    }

    #[test]
    fn test_extract_active_context_empty() {
        let messages = vec![Message {
            role: Role::User,
            content: MessageContent::text("hello"),
        }];
        let ctx = Agent::extract_active_context(&messages);
        assert!(ctx.is_empty());
    }
}
