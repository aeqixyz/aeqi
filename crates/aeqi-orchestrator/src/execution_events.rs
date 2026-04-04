use aeqi_core::chat_stream::ChatStreamEvent;
/// Real-time execution events streamed from workers to observers.
///
/// Workers publish events through an [`EventBroadcaster`] during quest execution.
/// Dashboard WebSocket handlers, logging pipelines, and other consumers subscribe
/// to receive events as they happen. This replaces polling-based progress tracking
/// with push-based streaming.
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::debug;

use crate::runtime::{RuntimeExecution, RuntimeSession};

// ---------------------------------------------------------------------------
// ExecutionEvent
// ---------------------------------------------------------------------------

/// An event emitted during worker quest execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type")]
pub enum ExecutionEvent {
    /// Worker has begun executing a quest.
    QuestStarted {
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
    /// Quest completed successfully.
    QuestCompleted {
        task_id: String,
        outcome: String,
        confidence: f32,
        cost_usd: f64,
        turns: u32,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime: Option<RuntimeExecution>,
    },
    /// Quest failed.
    QuestFailed {
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
    /// A note entry was posted.
    NotePosted {
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
    AgentIdle { agent_id: String, idle_secs: u64 },
    /// A dispatch was received.
    DispatchReceived {
        from_agent: String,
        to_agent: String,
        kind: String,
    },
    /// A message was posted to a conversation channel.
    ChannelMessage {
        channel_name: String,
        chat_id: i64,
        from_agent: String,
        content_preview: String,
    },
    /// A department-wide broadcast message (Phase 9).
    ///
    /// Emitted when an agent posts to a department conversation channel,
    /// allowing all department members to observe cross-agent communication.
    DepartmentMessage {
        department_id: String,
        department_name: String,
        from_agent: String,
        content: String,
    },
}

impl ExecutionEvent {
    /// Extract event type, agent_id, quest_id, and content for EventStore persistence.
    fn to_event_fields(&self) -> (String, Option<String>, Option<String>, serde_json::Value) {
        let content = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        match self {
            Self::QuestStarted { task_id, agent, .. } => (
                "execution.quest_started".into(),
                Some(agent.clone()),
                Some(task_id.clone()),
                content,
            ),
            Self::QuestCompleted { task_id, .. } => (
                "execution.quest_completed".into(),
                None,
                Some(task_id.clone()),
                content,
            ),
            Self::QuestFailed { task_id, .. } => (
                "execution.quest_failed".into(),
                None,
                Some(task_id.clone()),
                content,
            ),
            Self::Progress { task_id, .. } => (
                "execution.progress".into(),
                None,
                Some(task_id.clone()),
                content,
            ),
            Self::ToolCallStarted { task_id, .. } | Self::ToolCallCompleted { task_id, .. } => (
                "execution.tool_call".into(),
                None,
                Some(task_id.clone()),
                content,
            ),
            Self::CheckpointCreated { task_id, .. } => (
                "execution.checkpoint".into(),
                None,
                Some(task_id.clone()),
                content,
            ),
            Self::ApprovalRequired { task_id, .. } | Self::ClarificationNeeded { task_id, .. } => (
                "execution.blocked".into(),
                None,
                Some(task_id.clone()),
                content,
            ),
            Self::ChatStream { task_id, .. } => {
                // High-volume — skip persistence for chat stream events.
                (
                    "execution.chat_stream".into(),
                    None,
                    Some(task_id.clone()),
                    serde_json::Value::Null,
                )
            }
            Self::DispatchReceived {
                from_agent: _,
                to_agent,
                ..
            } => (
                "execution.dispatch".into(),
                Some(to_agent.clone()),
                None,
                content,
            ),
            _ => ("execution.other".into(), None, None, content),
        }
    }
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
    /// Optional event store for persistence. When set, every published event
    /// is also written to the events table (fire-and-forget).
    event_store: Option<std::sync::Arc<crate::event_store::EventStore>>,
}

impl EventBroadcaster {
    /// Create a new broadcaster with capacity 256.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self {
            sender,
            event_store: None,
        }
    }

    /// Attach an EventStore for persistence. Every publish() will also emit
    /// to the events table.
    pub fn set_event_store(&mut self, store: std::sync::Arc<crate::event_store::EventStore>) {
        self.event_store = Some(store);
    }

    /// Subscribe to receive execution events.
    pub fn subscribe(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.sender.subscribe()
    }

    /// Publish an event to all subscribers and persist to the events table.
    /// Non-blocking; ignores lag errors and silently drops events when there
    /// are no subscribers.
    pub fn publish(&self, event: ExecutionEvent) {
        // Persist to events table (fire-and-forget).
        if let Some(ref store) = self.event_store {
            let store = store.clone();
            let (event_type, agent_id, quest_id, content) = event.to_event_fields();
            tokio::spawn(async move {
                let _ = store
                    .emit(
                        &event_type,
                        agent_id.as_deref(),
                        None,
                        quest_id.as_deref(),
                        &content,
                    )
                    .await;
            });
        }

        // Broadcast to in-process subscribers.
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

        broadcaster.publish(ExecutionEvent::QuestStarted {
            task_id: "t-1".into(),
            agent: "engineer".into(),
            project: "aeqi".into(),
            runtime_session: None,
        });

        let event = rx.recv().await.unwrap();
        match event {
            ExecutionEvent::QuestStarted {
                task_id,
                agent,
                project,
                ..
            } => {
                assert_eq!(task_id, "t-1");
                assert_eq!(agent, "engineer");
                assert_eq!(project, "aeqi");
            }
            _ => panic!("expected QuestStarted"),
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
        broadcaster.publish(ExecutionEvent::QuestFailed {
            task_id: "t-3".into(),
            reason: "build error".into(),
            artifacts_preserved: false,
            runtime: None,
        });

        // Still functional after publishing to zero subscribers.
        let mut rx = broadcaster.subscribe();
        broadcaster.publish(ExecutionEvent::QuestCompleted {
            task_id: "t-4".into(),
            outcome: "done".into(),
            confidence: 0.95,
            cost_usd: 0.1,
            turns: 5,
            duration_ms: 30000,
            runtime: None,
        });

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, ExecutionEvent::QuestCompleted { .. }));
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
