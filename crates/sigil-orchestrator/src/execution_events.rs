/// Real-time execution events streamed from workers to observers.
///
/// Workers publish events through an [`EventBroadcaster`] during task execution.
/// Dashboard WebSocket handlers, logging pipelines, and other consumers subscribe
/// to receive events as they happen. This replaces polling-based progress tracking
/// with push-based streaming.
use serde::Serialize;
use sigil_core::chat_stream::ChatStreamEvent;
use tokio::sync::broadcast;
use tracing::debug;

use crate::runtime::{RuntimeExecution, RuntimeSession};

// ---------------------------------------------------------------------------
// ExecutionEvent
// ---------------------------------------------------------------------------

/// An event emitted during worker task execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type")]
pub enum ExecutionEvent {
    /// Worker has begun executing a task.
    TaskStarted {
        task_id: String,
        agent: String,
        project: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime_session: Option<RuntimeSession>,
    },
    /// Periodic progress update during execution.
    Progress {
        task_id: String,
        turns: u32,
        cost_usd: f64,
        last_tool: Option<String>,
    },
    /// A tool call has started.
    ToolCallStarted { task_id: String, tool_name: String },
    /// A tool call has completed.
    ToolCallCompleted {
        task_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },
    /// A checkpoint was captured during execution.
    CheckpointCreated {
        task_id: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime: Option<RuntimeExecution>,
    },
    /// Task completed successfully.
    TaskCompleted {
        task_id: String,
        outcome: String,
        confidence: f32,
        cost_usd: f64,
        turns: u32,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime: Option<RuntimeExecution>,
    },
    /// Task failed.
    TaskFailed {
        task_id: String,
        reason: String,
        artifacts_preserved: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime: Option<RuntimeExecution>,
    },
    /// An approval is required before the worker can continue.
    ApprovalRequired {
        task_id: String,
        pattern: String,
        description: String,
    },
    /// The worker needs clarification before continuing.
    ClarificationNeeded {
        task_id: String,
        question: String,
        options: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime: Option<RuntimeExecution>,
    },
    /// Real-time chat stream event from the agent loop.
    /// Used by CLI TUI and WebSocket chat clients for token-by-token rendering.
    ChatStream {
        task_id: String,
        chat_id: i64,
        event: ChatStreamEvent,
    },
    // --- Extended event types for richer trigger patterns ---
    /// A memory entry was stored.
    MemoryStored {
        key: String,
        scope: String,
        project: Option<String>,
    },
    /// A blackboard entry was posted.
    BlackboardPosted {
        key: String,
        project: String,
        agent: String,
    },
    /// Project cost exceeded a threshold.
    BudgetExceeded {
        project: String,
        current_usd: f64,
        threshold_usd: f64,
    },
    /// A persistent agent has been idle.
    AgentIdle {
        agent_id: String,
        idle_secs: u64,
    },
    /// A dispatch was received.
    DispatchReceived {
        from_agent: String,
        to_agent: String,
        kind: String,
    },
}

// ---------------------------------------------------------------------------
// EventBroadcaster
// ---------------------------------------------------------------------------

/// Broadcast channel for distributing execution events to multiple subscribers.
///
/// Uses `tokio::sync::broadcast` with a fixed capacity. Slow consumers that
/// fall behind will experience lag (missed events) rather than blocking the
/// publisher.
pub struct EventBroadcaster {
    sender: broadcast::Sender<ExecutionEvent>,
}

impl EventBroadcaster {
    /// Create a new broadcaster with capacity 256.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    /// Subscribe to receive execution events.
    pub fn subscribe(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.sender.subscribe()
    }

    /// Publish an event to all subscribers. Non-blocking; ignores lag errors
    /// and silently drops events when there are no subscribers.
    pub fn publish(&self, event: ExecutionEvent) {
        match self.sender.send(event) {
            Ok(n) => {
                debug!(subscribers = n, "event published");
            }
            Err(_) => {
                // No active receivers — that's fine, event is dropped.
            }
        }
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_subscribe_single() {
        let broadcaster = EventBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        broadcaster.publish(ExecutionEvent::TaskStarted {
            task_id: "t-1".into(),
            agent: "engineer".into(),
            project: "sigil".into(),
            runtime_session: None,
        });

        let event = rx.recv().await.unwrap();
        match event {
            ExecutionEvent::TaskStarted {
                task_id,
                agent,
                project,
                ..
            } => {
                assert_eq!(task_id, "t-1");
                assert_eq!(agent, "engineer");
                assert_eq!(project, "sigil");
            }
            _ => panic!("expected TaskStarted"),
        }
    }

    #[tokio::test]
    async fn publish_subscribe_multiple() {
        let broadcaster = EventBroadcaster::new();
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();

        assert_eq!(broadcaster.subscriber_count(), 2);

        broadcaster.publish(ExecutionEvent::Progress {
            task_id: "t-2".into(),
            turns: 3,
            cost_usd: 0.05,
            last_tool: Some("Bash".into()),
        });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();

        // Both subscribers should receive the same event.
        match (&e1, &e2) {
            (
                ExecutionEvent::Progress {
                    task_id: id1,
                    turns: t1,
                    ..
                },
                ExecutionEvent::Progress {
                    task_id: id2,
                    turns: t2,
                    ..
                },
            ) => {
                assert_eq!(id1, "t-2");
                assert_eq!(id2, "t-2");
                assert_eq!(*t1, 3);
                assert_eq!(*t2, 3);
            }
            _ => panic!("expected Progress events"),
        }
    }

    #[tokio::test]
    async fn publish_no_subscribers_does_not_panic() {
        let broadcaster = EventBroadcaster::new();
        assert_eq!(broadcaster.subscriber_count(), 0);

        // This must not panic even with zero subscribers.
        broadcaster.publish(ExecutionEvent::TaskFailed {
            task_id: "t-3".into(),
            reason: "build error".into(),
            artifacts_preserved: false,
            runtime: None,
        });

        // Still functional after publishing to zero subscribers.
        let mut rx = broadcaster.subscribe();
        broadcaster.publish(ExecutionEvent::TaskCompleted {
            task_id: "t-4".into(),
            outcome: "done".into(),
            confidence: 0.95,
            cost_usd: 0.1,
            turns: 5,
            duration_ms: 30000,
            runtime: None,
        });

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, ExecutionEvent::TaskCompleted { .. }));
    }

    #[tokio::test]
    async fn subscriber_count_tracks_correctly() {
        let broadcaster = EventBroadcaster::new();
        assert_eq!(broadcaster.subscriber_count(), 0);

        let rx1 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 1);

        let rx2 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 2);

        drop(rx1);
        assert_eq!(broadcaster.subscriber_count(), 1);

        drop(rx2);
        assert_eq!(broadcaster.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn serialization_round_trip() {
        let event = ExecutionEvent::ToolCallCompleted {
            task_id: "t-5".into(),
            tool_name: "Read".into(),
            success: true,
            duration_ms: 42,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event_type\":\"ToolCallCompleted\""));
        assert!(json.contains("\"tool_name\":\"Read\""));
        assert!(json.contains("\"success\":true"));
    }

    #[tokio::test]
    async fn default_creates_broadcaster() {
        let broadcaster = EventBroadcaster::default();
        assert_eq!(broadcaster.subscriber_count(), 0);

        let mut rx = broadcaster.subscribe();
        broadcaster.publish(ExecutionEvent::CheckpointCreated {
            task_id: "t-6".into(),
            message: "captured git state".into(),
            runtime: None,
        });
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, ExecutionEvent::CheckpointCreated { .. }));
    }
}
