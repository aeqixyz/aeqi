//! Context Compression Middleware — compresses middle messages when context grows too large.
//!
//! Inspired by Hermes' context_compressor.py. When the messages buffer exceeds a
//! configurable threshold (percentage of a line budget), the middleware replaces
//! the middle portion of messages with a single compressed summary, preserving the
//! first N and last N messages intact.
//!
//! This prevents workers from hitting context limits on long-running tasks while
//! retaining the most relevant context (initial instructions and recent activity).
//!
//! ## Context Tier Stepping
//!
//! When an error containing context-length indicators is received via `on_error`,
//! the middleware reduces `max_context_lines` by 40% (e.g., 500 -> 300 -> 180 -> 108).
//! After 3 step-downs, execution is halted to prevent infinite shrinking.

use async_trait::async_trait;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use tracing::{debug, info, warn};

use super::{Middleware, MiddlewareAction, ORDER_CONTEXT_COMPRESSION, WorkerContext};

/// Maximum number of context tier step-downs before halting.
const MAX_TIER_STEPDOWNS: u32 = 3;

/// Factor by which `max_context_lines` is reduced on each step-down (40% reduction).
const STEPDOWN_FACTOR: f32 = 0.60;

/// Error message substrings that indicate a context-length problem.
const CONTEXT_LENGTH_INDICATORS: &[&str] = &[
    "context length",
    "token limit",
    "exceeds",
    "too long",
    "maximum context",
];

/// Context compression middleware configuration.
pub struct ContextCompressionMiddleware {
    /// Compress when messages count exceeds `threshold_percent * max_context_lines`.
    /// Default: 0.50 (50% of budget).
    threshold_percent: f32,
    /// Maximum context lines used as the budget reference.
    /// Default: 500. Reduced by 40% on each tier step-down.
    max_context_lines: AtomicUsize,
    /// Number of initial messages to always preserve.
    /// Default: 3.
    protect_first_n: usize,
    /// Number of trailing messages to always preserve.
    /// Default: 5.
    protect_last_n: usize,
    /// How many tier step-downs have occurred (max 3, then halt).
    stepdown_count: AtomicU32,
}

impl ContextCompressionMiddleware {
    /// Create with default configuration.
    pub fn new() -> Self {
        Self {
            threshold_percent: 0.50,
            max_context_lines: AtomicUsize::new(500),
            protect_first_n: 3,
            protect_last_n: 5,
            stepdown_count: AtomicU32::new(0),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(
        threshold_percent: f32,
        max_context_lines: usize,
        protect_first_n: usize,
        protect_last_n: usize,
    ) -> Self {
        Self {
            threshold_percent,
            max_context_lines: AtomicUsize::new(max_context_lines),
            protect_first_n,
            protect_last_n,
            stepdown_count: AtomicU32::new(0),
        }
    }

    /// Current max context lines (may have been reduced by tier stepping).
    pub fn current_max_context_lines(&self) -> usize {
        self.max_context_lines.load(Ordering::Relaxed)
    }

    /// How many tier step-downs have occurred.
    pub fn stepdown_count(&self) -> u32 {
        self.stepdown_count.load(Ordering::Relaxed)
    }

    /// Compute the message count threshold that triggers compression.
    fn threshold(&self) -> usize {
        let max_lines = self.max_context_lines.load(Ordering::Relaxed);
        (max_lines as f32 * self.threshold_percent) as usize
    }

    /// Check whether an error message indicates a context-length problem.
    fn is_context_length_error(error: &str) -> bool {
        let lower = error.to_lowercase();
        CONTEXT_LENGTH_INDICATORS
            .iter()
            .any(|indicator| lower.contains(indicator))
    }

    /// Step down the context tier: reduce max_context_lines by 40%.
    /// Returns the new value, or None if max step-downs exceeded.
    fn step_down_tier(&self) -> Option<usize> {
        let count = self.stepdown_count.fetch_add(1, Ordering::Relaxed);
        if count >= MAX_TIER_STEPDOWNS {
            // Undo the increment — we're not actually stepping down.
            self.stepdown_count.fetch_sub(1, Ordering::Relaxed);
            return None;
        }

        let old = self.max_context_lines.load(Ordering::Relaxed);
        let new = ((old as f32) * STEPDOWN_FACTOR) as usize;
        // Ensure we don't go below a minimum useful size.
        let new = new.max(10);
        self.max_context_lines.store(new, Ordering::Relaxed);
        Some(new)
    }

    /// Build a compressed summary from a slice of messages.
    ///
    /// Extracts the first 200 characters of each message and joins them into
    /// a single summary string.
    fn build_summary(messages: &[String]) -> String {
        let count = messages.len();
        let key_points: Vec<String> = messages
            .iter()
            .map(|m| {
                let trimmed = m.trim();
                if trimmed.len() <= 200 {
                    trimmed.to_string()
                } else {
                    let end = trimmed
                        .char_indices()
                        .nth(200)
                        .map(|(i, _)| i)
                        .unwrap_or(trimmed.len());
                    format!("{}...", &trimmed[..end])
                }
            })
            .collect();

        format!(
            "[Context compressed: {} messages summarized. Key points: {}]",
            count,
            key_points.join(" | ")
        )
    }

    /// Compress the messages buffer in-place, preserving head and tail.
    fn compress_messages(
        messages: &[String],
        protect_first: usize,
        protect_last: usize,
    ) -> Vec<String> {
        let len = messages.len();
        let protected_total = protect_first + protect_last;

        // Not enough messages to have a compressible middle section.
        if len <= protected_total {
            return messages.to_vec();
        }

        let head = &messages[..protect_first];
        let middle = &messages[protect_first..len - protect_last];
        let tail = &messages[len - protect_last..];

        let summary = Self::build_summary(middle);

        let mut result = Vec::with_capacity(protected_total + 1);
        result.extend_from_slice(head);
        result.push(summary);
        result.extend_from_slice(tail);
        result
    }
}

impl Default for ContextCompressionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for ContextCompressionMiddleware {
    fn name(&self) -> &str {
        "context-compression"
    }

    fn order(&self) -> u32 {
        ORDER_CONTEXT_COMPRESSION
    }

    async fn before_model(&self, ctx: &mut WorkerContext) -> MiddlewareAction {
        // Defer to the agent loop's token-aware compaction when active.
        if ctx.agent_compaction_active {
            return MiddlewareAction::Continue;
        }

        let msg_count = ctx.messages.len();
        let threshold = self.threshold();

        if msg_count <= threshold {
            return MiddlewareAction::Continue;
        }

        let original_count = msg_count;
        ctx.messages =
            Self::compress_messages(&ctx.messages, self.protect_first_n, self.protect_last_n);

        let compressed_count = original_count - ctx.messages.len();
        let max_lines = self.max_context_lines.load(Ordering::Relaxed);
        info!(
            task_id = %ctx.task_id,
            original_messages = original_count,
            compressed_messages = compressed_count,
            remaining_messages = ctx.messages.len(),
            threshold,
            max_context_lines = max_lines,
            "context compressed — middle messages summarized"
        );
        debug!(
            protect_first = self.protect_first_n,
            protect_last = self.protect_last_n,
            "compression boundaries"
        );

        MiddlewareAction::Continue
    }

    async fn on_error(&self, ctx: &mut WorkerContext, error: &str) -> MiddlewareAction {
        // When the agent loop handles compaction, it also handles context-length
        // errors (compact + retry). Defer to avoid conflicting recovery.
        if ctx.agent_compaction_active {
            return MiddlewareAction::Continue;
        }

        if !Self::is_context_length_error(error) {
            return MiddlewareAction::Continue;
        }

        let old_max = self.current_max_context_lines();

        match self.step_down_tier() {
            Some(new_max) => {
                let stepdowns = self.stepdown_count();
                warn!(
                    task_id = %ctx.task_id,
                    old_max_context_lines = old_max,
                    new_max_context_lines = new_max,
                    stepdown = stepdowns,
                    max_stepdowns = MAX_TIER_STEPDOWNS,
                    "context tier step-down — reducing max_context_lines by 40% after context-length error"
                );
                MiddlewareAction::Continue
            }
            None => {
                let stepdowns = self.stepdown_count();
                warn!(
                    task_id = %ctx.task_id,
                    max_context_lines = old_max,
                    stepdowns,
                    "context tier step-down limit reached — halting execution"
                );
                MiddlewareAction::Halt(format!(
                    "context tier exhausted after {} step-downs (max_context_lines={}): {}",
                    stepdowns, old_max, error
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "test task", "engineer", "aeqi")
    }

    fn make_messages(count: usize) -> Vec<String> {
        (0..count)
            .map(|i| format!("Message {i}: some content here"))
            .collect()
    }

    #[tokio::test]
    async fn below_threshold_no_compression() {
        // threshold = 0.50 * 500 = 250; 10 messages < 250
        let mw = ContextCompressionMiddleware::new();
        let mut ctx = test_ctx();
        ctx.messages = make_messages(10);

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(ctx.messages.len(), 10);
        // Verify no compression marker present.
        for msg in &ctx.messages {
            assert!(!msg.contains("[Context compressed"));
        }
    }

    #[tokio::test]
    async fn above_threshold_compresses_middle() {
        // threshold_percent=0.50, max_context_lines=20 → threshold=10
        // protect_first=2, protect_last=2
        let mw = ContextCompressionMiddleware::with_config(0.50, 20, 2, 2);
        let mut ctx = test_ctx();
        ctx.messages = make_messages(15); // 15 > 10 → triggers compression

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));

        // Result: 2 head + 1 summary + 2 tail = 5
        assert_eq!(ctx.messages.len(), 5);

        // Head preserved.
        assert_eq!(ctx.messages[0], "Message 0: some content here");
        assert_eq!(ctx.messages[1], "Message 1: some content here");

        // Summary in the middle.
        assert!(ctx.messages[2].contains("[Context compressed: 11 messages summarized"));

        // Tail preserved.
        assert_eq!(ctx.messages[3], "Message 13: some content here");
        assert_eq!(ctx.messages[4], "Message 14: some content here");
    }

    #[tokio::test]
    async fn exact_threshold_no_compression() {
        // threshold_percent=0.50, max_context_lines=20 → threshold=10
        let mw = ContextCompressionMiddleware::with_config(0.50, 20, 2, 2);
        let mut ctx = test_ctx();
        ctx.messages = make_messages(10); // exactly 10 = threshold → no compression

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(ctx.messages.len(), 10);
    }

    #[tokio::test]
    async fn empty_messages_no_op() {
        let mw = ContextCompressionMiddleware::new();
        let mut ctx = test_ctx();
        // No messages at all.

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert!(ctx.messages.is_empty());
    }

    #[tokio::test]
    async fn few_messages_no_compression() {
        // protect_first=3, protect_last=5 → need > 8 messages to have a middle
        // But threshold also matters: threshold=0.50*500=250
        // With only 7 messages, we're below threshold, so no compression.
        let mw = ContextCompressionMiddleware::new();
        let mut ctx = test_ctx();
        ctx.messages = make_messages(7);

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(ctx.messages.len(), 7);
    }

    #[tokio::test]
    async fn messages_equal_to_protected_no_middle_to_compress() {
        // threshold_percent=0.50, max_context_lines=6 → threshold=3
        // protect_first=2, protect_last=2 → need > 4 for a middle
        let mw = ContextCompressionMiddleware::with_config(0.50, 6, 2, 2);
        let mut ctx = test_ctx();
        ctx.messages = make_messages(4); // 4 > 3 (threshold), but 4 <= 2+2 (protected)

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        // compress_messages returns original when len <= protected_total
        assert_eq!(ctx.messages.len(), 4);
    }

    #[tokio::test]
    async fn summary_contains_key_points() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 10, 1, 1);
        let mut ctx = test_ctx();
        ctx.messages = make_messages(8); // 8 > 5 → triggers

        mw.before_model(&mut ctx).await;

        // Result: 1 head + 1 summary + 1 tail = 3
        assert_eq!(ctx.messages.len(), 3);

        // Summary should mention the compressed message contents.
        let summary = &ctx.messages[1];
        assert!(summary.contains("Key points:"));
        assert!(summary.contains("Message 1:"));
        assert!(summary.contains("Message 6:"));
    }

    #[tokio::test]
    async fn long_messages_truncated_in_summary() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 4, 1, 1);
        let mut ctx = test_ctx();

        let long_msg = "x".repeat(300);
        ctx.messages = vec!["head".into(), long_msg.clone(), long_msg, "tail".into()];
        // 4 messages, threshold=2, protect 1+1=2, middle=2

        mw.before_model(&mut ctx).await;

        // 1 head + 1 summary + 1 tail = 3
        assert_eq!(ctx.messages.len(), 3);
        let summary = &ctx.messages[1];
        assert!(
            summary.contains("..."),
            "long messages should be truncated with ..."
        );
    }

    #[test]
    fn build_summary_format() {
        let messages = vec![
            "First middle message".to_string(),
            "Second middle message".to_string(),
        ];
        let summary = ContextCompressionMiddleware::build_summary(&messages);
        assert!(summary.starts_with("[Context compressed: 2 messages summarized"));
        assert!(summary.contains("First middle message"));
        assert!(summary.contains("Second middle message"));
    }

    // -----------------------------------------------------------------------
    // Context tier stepping tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_context_length_error_detects_indicators() {
        assert!(ContextCompressionMiddleware::is_context_length_error(
            "Request exceeds maximum context length"
        ));
        assert!(ContextCompressionMiddleware::is_context_length_error(
            "token limit reached for model"
        ));
        assert!(ContextCompressionMiddleware::is_context_length_error(
            "Input too long for this model"
        ));
        assert!(ContextCompressionMiddleware::is_context_length_error(
            "Exceeds the maximum allowed tokens"
        ));
        assert!(ContextCompressionMiddleware::is_context_length_error(
            "Maximum context window exceeded"
        ));
    }

    #[test]
    fn is_context_length_error_ignores_unrelated() {
        assert!(!ContextCompressionMiddleware::is_context_length_error(
            "network timeout"
        ));
        assert!(!ContextCompressionMiddleware::is_context_length_error(
            "authentication failed"
        ));
        assert!(!ContextCompressionMiddleware::is_context_length_error(
            "rate limited"
        ));
    }

    #[tokio::test]
    async fn on_error_steps_down_on_context_length_error() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 500, 2, 2);
        let mut ctx = test_ctx();

        // First step-down: 500 * 0.60 = 300
        let action = mw
            .on_error(&mut ctx, "request exceeds maximum context length")
            .await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(mw.current_max_context_lines(), 300);
        assert_eq!(mw.stepdown_count(), 1);
    }

    #[tokio::test]
    async fn on_error_progressive_stepdown() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 500, 2, 2);
        let mut ctx = test_ctx();

        // Step 1: 500 -> 300
        mw.on_error(&mut ctx, "token limit exceeded").await;
        assert_eq!(mw.current_max_context_lines(), 300);

        // Step 2: 300 -> 180
        mw.on_error(&mut ctx, "token limit exceeded").await;
        assert_eq!(mw.current_max_context_lines(), 180);

        // Step 3: 180 -> 108
        mw.on_error(&mut ctx, "token limit exceeded").await;
        assert_eq!(mw.current_max_context_lines(), 108);
        assert_eq!(mw.stepdown_count(), 3);
    }

    #[tokio::test]
    async fn on_error_halts_after_max_stepdowns() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 500, 2, 2);
        let mut ctx = test_ctx();

        // Exhaust all 3 step-downs.
        mw.on_error(&mut ctx, "context length exceeded").await;
        mw.on_error(&mut ctx, "context length exceeded").await;
        mw.on_error(&mut ctx, "context length exceeded").await;

        // 4th attempt should halt.
        let action = mw.on_error(&mut ctx, "context length exceeded").await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));
        if let MiddlewareAction::Halt(msg) = action {
            assert!(msg.contains("context tier exhausted"));
            assert!(msg.contains("3 step-downs"));
        }
    }

    #[tokio::test]
    async fn defers_to_agent_compaction_before_model() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 10, 1, 1);
        let mut ctx = test_ctx();
        ctx.messages = make_messages(20); // Way above threshold
        ctx.agent_compaction_active = true;

        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(ctx.messages.len(), 20); // No compression
    }

    #[tokio::test]
    async fn defers_to_agent_compaction_on_error() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 500, 2, 2);
        let mut ctx = test_ctx();
        ctx.agent_compaction_active = true;

        let action = mw.on_error(&mut ctx, "context length exceeded").await;
        assert!(matches!(action, MiddlewareAction::Continue));
        // No tier step-down
        assert_eq!(mw.current_max_context_lines(), 500);
        assert_eq!(mw.stepdown_count(), 0);
    }

    #[tokio::test]
    async fn on_error_ignores_non_context_errors() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 500, 2, 2);
        let mut ctx = test_ctx();

        let action = mw.on_error(&mut ctx, "network timeout").await;
        assert!(matches!(action, MiddlewareAction::Continue));
        // No step-down should have occurred.
        assert_eq!(mw.current_max_context_lines(), 500);
        assert_eq!(mw.stepdown_count(), 0);
    }

    #[tokio::test]
    async fn stepdown_affects_threshold() {
        let mw = ContextCompressionMiddleware::with_config(0.50, 20, 2, 2);
        let mut ctx = test_ctx();

        // Original threshold: 0.50 * 20 = 10
        ctx.messages = make_messages(8); // 8 < 10, no compression
        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        assert_eq!(ctx.messages.len(), 8);

        // Step down: 20 -> 12, new threshold: 0.50 * 12 = 6
        mw.on_error(&mut ctx, "context length exceeded").await;
        assert_eq!(mw.current_max_context_lines(), 12);

        // Now 8 > 6, should trigger compression
        ctx.messages = make_messages(8);
        let action = mw.before_model(&mut ctx).await;
        assert!(matches!(action, MiddlewareAction::Continue));
        // 2 head + 1 summary + 2 tail = 5
        assert_eq!(ctx.messages.len(), 5);
    }
}
