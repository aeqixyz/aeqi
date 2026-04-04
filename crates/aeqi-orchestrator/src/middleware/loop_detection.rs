//! Loop Detection Middleware — detects repetitive tool call patterns.
//!
//! Hashes each tool call (name + input) into a fingerprint and tracks them
//! in a sliding window. When the same fingerprint appears repeatedly, it
//! first injects a warning message (at `warn_threshold`), then halts
//! execution entirely (at `halt_threshold`).

use std::collections::{HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Mutex;

use async_trait::async_trait;
use tracing::warn;

use super::{
    Middleware, MiddlewareAction, ORDER_LOOP_DETECTION, ToolCall, ToolResult, WorkerContext,
};

/// Loop detection middleware configuration and state.
pub struct LoopDetectionMiddleware {
    /// Size of the sliding window of recent tool call hashes.
    window_size: usize,
    /// Number of identical calls before injecting a warning.
    warn_threshold: usize,
    /// Number of identical calls before halting execution.
    halt_threshold: usize,
    /// Interior-mutable state: sliding window of hashes and repeat counts.
    state: Mutex<LoopState>,
}

#[derive(Debug)]
struct LoopState {
    /// Sliding window of recent tool call fingerprints.
    window: VecDeque<u64>,
    /// Count of each fingerprint currently in the window.
    counts: HashMap<u64, usize>,
}

impl LoopDetectionMiddleware {
    /// Create with default thresholds: window=10, warn=3, halt=5.
    pub fn new() -> Self {
        Self::with_thresholds(10, 3, 5)
    }

    /// Create with custom thresholds.
    pub fn with_thresholds(
        window_size: usize,
        warn_threshold: usize,
        halt_threshold: usize,
    ) -> Self {
        Self {
            window_size,
            warn_threshold,
            halt_threshold,
            state: Mutex::new(LoopState {
                window: VecDeque::with_capacity(window_size),
                counts: HashMap::new(),
            }),
        }
    }

    /// Compute a fingerprint hash for a tool call.
    fn fingerprint(call: &ToolCall) -> u64 {
        let mut hasher = DefaultHasher::new();
        call.name.hash(&mut hasher);
        call.input.hash(&mut hasher);
        hasher.finish()
    }

    /// Record a tool call and return its current count in the window.
    fn record(&self, call: &ToolCall) -> usize {
        let hash = Self::fingerprint(call);
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Recover from lock poisoning: take the inner state and continue.
                // This prevents cascade failure if another thread panicked while holding the lock.
                warn!("loop detection lock was poisoned, recovering");
                poisoned.into_inner()
            }
        };

        // Evict oldest entry if window is full.
        if state.window.len() >= self.window_size
            && let Some(old) = state.window.pop_front()
            && let Some(entry) = state.counts.get_mut(&old)
        {
            *entry -= 1;
            if *entry == 0 {
                state.counts.remove(&old);
            }
            // If the entry was missing (shouldn't happen), we just skip — no panic.
        }

        // Add new entry.
        state.window.push_back(hash);
        let count = state.counts.entry(hash).or_insert(0);
        *count += 1;
        *count
    }
}

impl Default for LoopDetectionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for LoopDetectionMiddleware {
    fn name(&self) -> &str {
        "loop_detection"
    }

    fn order(&self) -> u32 {
        ORDER_LOOP_DETECTION
    }

    async fn after_tool(
        &self,
        _ctx: &mut WorkerContext,
        call: &ToolCall,
        _result: &ToolResult,
    ) -> MiddlewareAction {
        let count = self.record(call);

        if count >= self.halt_threshold {
            warn!(
                tool = %call.name,
                count,
                threshold = self.halt_threshold,
                "loop detected — halting execution"
            );
            return MiddlewareAction::Halt(format!(
                "Loop detected: tool '{}' called {} times in last {} calls. \
                 Execution halted to prevent infinite loop.",
                call.name, count, self.window_size
            ));
        }

        if count >= self.warn_threshold {
            warn!(
                tool = %call.name,
                count,
                threshold = self.warn_threshold,
                "possible loop detected — injecting warning"
            );
            return MiddlewareAction::Inject(vec![format!(
                "WARNING: You have called '{}' with identical arguments {} times \
                 in the last {} calls. This looks like a loop. Change your approach \
                 or you will be terminated.",
                call.name, count, self.window_size
            )]);
        }

        MiddlewareAction::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_call(name: &str, input: &str) -> ToolCall {
        ToolCall {
            name: name.into(),
            input: input.into(),
        }
    }

    fn make_result() -> ToolResult {
        ToolResult {
            success: true,
            output: "ok".into(),
        }
    }

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "test task", "engineer", "aeqi")
    }

    #[tokio::test]
    async fn no_loop_continues() {
        let mw = LoopDetectionMiddleware::new();
        let mut ctx = test_ctx();
        let call = make_call("Read", "/some/file.rs");
        let result = make_result();

        let action = mw.after_tool(&mut ctx, &call, &result).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn warn_at_threshold() {
        let mw = LoopDetectionMiddleware::with_thresholds(10, 3, 5);
        let mut ctx = test_ctx();
        let call = make_call("Bash", "ls -la");
        let result = make_result();

        // First two calls: Continue.
        for _ in 0..2 {
            let action = mw.after_tool(&mut ctx, &call, &result).await;
            assert!(matches!(action, MiddlewareAction::Continue));
        }

        // Third call: Inject warning.
        let action = mw.after_tool(&mut ctx, &call, &result).await;
        assert!(
            matches!(action, MiddlewareAction::Inject(ref msgs) if msgs[0].contains("WARNING")),
            "expected Inject(warning), got {action:?}"
        );
    }

    #[tokio::test]
    async fn halt_at_threshold() {
        let mw = LoopDetectionMiddleware::with_thresholds(10, 3, 5);
        let mut ctx = test_ctx();
        let call = make_call("Bash", "cat /dev/null");
        let result = make_result();

        // Calls 1-4: Continue or Inject.
        for _ in 0..4 {
            let _ = mw.after_tool(&mut ctx, &call, &result).await;
        }

        // Call 5: Halt.
        let action = mw.after_tool(&mut ctx, &call, &result).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("Loop detected")),
            "expected Halt, got {action:?}"
        );
    }

    #[tokio::test]
    async fn different_calls_dont_trigger() {
        let mw = LoopDetectionMiddleware::with_thresholds(10, 3, 5);
        let mut ctx = test_ctx();
        let result = make_result();

        // 10 different calls — no loops.
        for i in 0..10 {
            let call = make_call("Read", &format!("/file_{i}.rs"));
            let action = mw.after_tool(&mut ctx, &call, &result).await;
            assert!(matches!(action, MiddlewareAction::Continue));
        }
    }

    #[tokio::test]
    async fn sliding_window_evicts_old() {
        // Window of 5, warn at 3, halt at 4.
        let mw = LoopDetectionMiddleware::with_thresholds(5, 3, 4);
        let mut ctx = test_ctx();
        let target = make_call("Bash", "echo hi");
        let filler = make_call("Read", "/other.rs");
        let result = make_result();

        // Add target twice.
        mw.after_tool(&mut ctx, &target, &result).await;
        mw.after_tool(&mut ctx, &target, &result).await;

        // Fill with 3 different calls to push old target out of window.
        for _ in 0..3 {
            mw.after_tool(&mut ctx, &filler, &result).await;
        }

        // Now target only appears once in window (second was evicted by filler).
        // Actually both targets were evicted — window is [target, target, filler, filler, filler]
        // No wait: window size 5. After 5 calls: [target, target, filler, filler, filler].
        // After adding one more filler, the first target is evicted.
        // Let me re-check: we did 2 target + 3 filler = 5 calls. Window is full.
        // Next target should evict the first target.
        let action = mw.after_tool(&mut ctx, &target, &result).await;
        // Window: [target, filler, filler, filler, target]. Target count = 2.
        assert!(
            matches!(action, MiddlewareAction::Continue),
            "expected Continue after eviction, got {action:?}"
        );
    }
}
