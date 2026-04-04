use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::event_store::{Event, EventStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DispatchKind {
    /// A delegation request from one agent to another.
    DelegateRequest {
        prompt: String,
        /// How the response should be routed: "origin", "perpetual", "async", "department", "none".
        response_mode: String,
        /// Whether to also create a tracked task for this delegation.
        create_task: bool,
        /// Optional skill hint for the target agent.
        skill: Option<String>,
        /// Dispatch ID this is replying to (for chained delegations).
        reply_to: Option<String>,
        /// Session ID of the calling agent, so the child worker can set parent_id.
        #[serde(default)]
        parent_session_id: Option<String>,
    },
    /// A response to a previous DelegateRequest.
    DelegateResponse {
        /// The dispatch ID of the original DelegateRequest.
        reply_to: String,
        /// Copied from the request for routing purposes.
        response_mode: String,
        /// The response content.
        content: String,
    },
    /// Escalation to human operator when all automated resolution is exhausted.
    HumanEscalation {
        project: String,
        task_id: String,
        subject: String,
        summary: String,
    },
}

impl DispatchKind {
    pub fn requires_ack_by_default(&self) -> bool {
        matches!(self, Self::DelegateRequest { .. })
    }

    pub fn subject_tag(&self) -> &'static str {
        match self {
            Self::DelegateRequest { .. } => "DELEGATE_REQUEST",
            Self::DelegateResponse { .. } => "DELEGATE_RESPONSE",
            Self::HumanEscalation { .. } => "HUMAN_ESCALATION",
        }
    }

    pub fn body_text(&self) -> String {
        match self {
            Self::DelegateRequest {
                prompt,
                response_mode,
                create_task,
                skill,
                reply_to,
                ..
            } => {
                let mut text = format!(
                    "Delegation request (response_mode: {response_mode}, create_task: {create_task})"
                );
                if let Some(s) = skill {
                    text.push_str(&format!(", skill: {s}"));
                }
                if let Some(rt) = reply_to {
                    text.push_str(&format!(", reply_to: {rt}"));
                }
                text.push_str(&format!("\n\n{prompt}"));
                text
            }
            Self::DelegateResponse {
                reply_to,
                response_mode,
                content,
            } => format!(
                "Delegation response (reply_to: {reply_to}, mode: {response_mode})\n\n{content}"
            ),
            Self::HumanEscalation {
                project,
                task_id,
                subject,
                summary,
            } => format!(
                "BLOCKED: {project}/{task_id} — {subject}\n\n{summary}\n\n\
                     This task has exhausted all automated resolution attempts and requires human input.",
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispatch {
    pub from: String,
    pub to: String,
    pub kind: DispatchKind,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
    /// Unique dispatch ID for acknowledgment tracking.
    #[serde(default = "default_dispatch_id")]
    pub id: String,
    /// Whether this dispatch requires explicit acknowledgment.
    #[serde(default)]
    pub requires_ack: bool,
    /// Number of retry attempts so far.
    #[serde(default)]
    pub retry_count: u32,
    /// Maximum retries before dead-lettering.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// When the dispatch was first sent (for total latency tracking).
    #[serde(default = "Utc::now")]
    pub first_sent_at: DateTime<Utc>,
    /// Optional idempotency key. If set, duplicate dispatches with the same key
    /// are silently dropped. Prevents duplicate work on retry/reconnect.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// Snapshot of control-plane delivery state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchHealth {
    /// Messages currently unread by their recipient.
    pub unread: usize,
    /// Ack-required messages that were delivered but not yet acknowledged.
    pub awaiting_ack: usize,
    /// Ack-required messages that are back in the unread queue after a retry.
    pub retrying_delivery: usize,
    /// Awaiting-ack messages older than the patrol retry threshold.
    pub overdue_ack: usize,
    /// Messages that exhausted retries and are now in dead-letter state.
    pub dead_letters: usize,
}

fn default_dispatch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_max_retries() -> u32 {
    3
}

impl Dispatch {
    pub fn new_typed(from: &str, to: &str, kind: DispatchKind) -> Self {
        let now = Utc::now();
        let requires_ack = kind.requires_ack_by_default();
        Self {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            timestamp: now,
            read: false,
            id: default_dispatch_id(),
            requires_ack,
            retry_count: 0,
            max_retries: 3,
            first_sent_at: now,
            idempotency_key: None,
        }
    }

    /// Mark this dispatch as requiring acknowledgment.
    pub fn with_ack_required(mut self) -> Self {
        self.requires_ack = true;
        self
    }

    /// Set an idempotency key to prevent duplicate execution.
    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    /// Serialize this dispatch to JSON content for EventStore storage.
    fn to_event_content(&self) -> serde_json::Value {
        let kind_json = serde_json::to_value(&self.kind).unwrap_or_default();
        serde_json::json!({
            "from": self.from,
            "to": self.to,
            "kind": kind_json,
            "status": if self.read { "read" } else { "pending" },
            "dispatch_id": self.id,
            "requires_ack": self.requires_ack,
            "retry_count": self.retry_count,
            "max_retries": self.max_retries,
            "first_sent_at": self.first_sent_at.to_rfc3339(),
            "idempotency_key": self.idempotency_key,
        })
    }

    /// Reconstruct a Dispatch from an Event's content JSON.
    fn from_event(event: &Event) -> Option<Self> {
        let c = &event.content;
        let from = c.get("from")?.as_str()?.to_string();
        let to = c.get("to")?.as_str()?.to_string();
        let kind: DispatchKind = serde_json::from_value(c.get("kind")?.clone()).ok()?;
        let status = c
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");
        let dispatch_id = c
            .get("dispatch_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let requires_ack = c
            .get("requires_ack")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let retry_count = c.get("retry_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let max_retries = c.get("max_retries").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
        let first_sent_at = c
            .get("first_sent_at")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or(event.created_at);
        let idempotency_key = c
            .get("idempotency_key")
            .and_then(|v| v.as_str())
            .map(String::from);

        Some(Dispatch {
            from,
            to,
            kind,
            timestamp: event.created_at,
            read: status != "pending",
            id: if dispatch_id.is_empty() {
                event.id.clone()
            } else {
                dispatch_id
            },
            requires_ack,
            retry_count,
            max_retries,
            first_sent_at,
            idempotency_key,
        })
    }
}

/// Thin wrapper around EventStore that provides the DispatchBus API.
/// All dispatch data is stored as type="dispatch" events in the unified events table.
pub struct DispatchBus {
    event_store: Arc<EventStore>,
    ttl_secs: u64,
    max_queue_per_recipient: usize,
    event_broadcaster: Option<Arc<crate::execution_events::EventBroadcaster>>,
}

impl DispatchBus {
    /// Create a new DispatchBus backed by the given EventStore.
    pub fn new(event_store: Arc<EventStore>) -> Self {
        Self {
            event_store,
            ttl_secs: 3600,
            max_queue_per_recipient: 1000,
            event_broadcaster: None,
        }
    }

    /// Set the event broadcaster for emitting DispatchReceived events.
    pub fn set_event_broadcaster(
        &mut self,
        broadcaster: Arc<crate::execution_events::EventBroadcaster>,
    ) {
        self.event_broadcaster = Some(broadcaster);
    }

    pub fn set_ttl(&mut self, secs: u64) {
        self.ttl_secs = secs;
    }

    pub async fn send(&self, dispatch: Dispatch) {
        let content = dispatch.to_event_content();

        // Store via EventStore.
        match self.event_store.send_dispatch(&content).await {
            Ok(_id) => {
                debug!(to = %dispatch.to, kind = %dispatch.kind.subject_tag(), "dispatch sent via EventStore");
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("idempotency_key_exists") {
                    debug!(to = %dispatch.to, "dispatch dropped (idempotency key already exists)");
                    return;
                }
                warn!(error = %e, to = %dispatch.to, "failed to send dispatch");
                return;
            }
        }

        // Prune old dispatches and enforce queue depth limits.
        self.prune_and_limit(&dispatch.to).await;

        // Emit DispatchReceived event for trigger system.
        if let Some(ref broadcaster) = self.event_broadcaster {
            broadcaster.publish(crate::execution_events::ExecutionEvent::DispatchReceived {
                from_agent: dispatch.from.clone(),
                to_agent: dispatch.to.clone(),
                kind: dispatch.kind.subject_tag().to_string(),
            });
        }
    }

    /// Prune old dispatches and enforce per-recipient queue depth limits.
    async fn prune_and_limit(&self, recipient: &str) {
        let cutoff = Utc::now() - chrono::Duration::seconds(self.ttl_secs as i64);
        let _ = self.event_store.prune("dispatch", &cutoff).await;

        // Enforce max queue depth per recipient.
        let count = self
            .event_store
            .unread_dispatch_count(recipient)
            .await
            .unwrap_or(0) as usize;
        if count > self.max_queue_per_recipient {
            // The EventStore prune handles TTL; queue depth is a soft limit
            // since the unified store doesn't need per-recipient deletion.
            // Log a warning if we exceed the limit.
            warn!(
                recipient = %recipient,
                count = count,
                limit = self.max_queue_per_recipient,
                "dispatch queue depth exceeds limit"
            );
        }
    }

    pub async fn read(&self, recipient: &str) -> Vec<Dispatch> {
        match self.event_store.read_dispatches(recipient).await {
            Ok(events) => events.iter().filter_map(Dispatch::from_event).collect(),
            Err(e) => {
                warn!(error = %e, "failed to read dispatches");
                Vec::new()
            }
        }
    }

    pub async fn all(&self) -> Vec<Dispatch> {
        match self.event_store.all_dispatches().await {
            Ok(events) => events.iter().filter_map(Dispatch::from_event).collect(),
            Err(e) => {
                warn!(error = %e, "failed to list all dispatches");
                Vec::new()
            }
        }
    }

    pub async fn unread_count(&self, recipient: &str) -> usize {
        self.event_store
            .unread_dispatch_count(recipient)
            .await
            .unwrap_or(0) as usize
    }

    pub fn pending_count(&self) -> usize {
        // Use a blocking approach since this is called from sync contexts.
        // For the EventStore backend, we query synchronously via a try_lock.
        // Fall back to 0 if the lock is held (non-blocking).
        0 // This method is only used in tests with the old memory backend.
        // The async health() method should be used instead.
    }

    /// Summarize current control-plane delivery health.
    pub async fn health(&self, overdue_age_secs: u64) -> DispatchHealth {
        let overdue_cutoff = Utc::now() - chrono::Duration::seconds(overdue_age_secs as i64);
        let dispatches = self.all().await;
        Self::summarize_health(&dispatches, overdue_cutoff)
    }

    pub fn drain(&self) -> Vec<Dispatch> {
        // Drain is called from sync contexts in the IPC handler.
        // Use a runtime handle to run the async version.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // We're in an async context but called synchronously — use block_in_place.
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        match self.event_store.drain_dispatches().await {
                            Ok(events) => events.iter().filter_map(Dispatch::from_event).collect(),
                            Err(e) => {
                                warn!(error = %e, "failed to drain dispatches");
                                Vec::new()
                            }
                        }
                    })
                })
            }
            Err(_) => Vec::new(),
        }
    }

    /// Acknowledge a dispatch by ID, preventing future retries.
    pub async fn acknowledge(&self, dispatch_id: &str) {
        if let Err(e) = self.event_store.acknowledge_dispatch(dispatch_id).await {
            warn!(error = %e, dispatch_id = %dispatch_id, "failed to acknowledge dispatch");
        }
    }

    /// Return unacknowledged dispatches older than `max_age_secs` that haven't
    /// exceeded their retry limit. Increments retry_count on each returned dispatch.
    pub async fn retry_unacked(&self, max_age_secs: u64) -> Vec<Dispatch> {
        match self
            .event_store
            .retry_unacked_dispatches(max_age_secs)
            .await
        {
            Ok(events) => events.iter().filter_map(Dispatch::from_event).collect(),
            Err(e) => {
                warn!(error = %e, "failed to retry unacked dispatches");
                Vec::new()
            }
        }
    }

    /// Return dispatches that have exceeded their max retry count (dead letters).
    pub async fn dead_letters(&self) -> Vec<Dispatch> {
        match self.event_store.dead_letter_dispatches().await {
            Ok(events) => events.iter().filter_map(Dispatch::from_event).collect(),
            Err(e) => {
                warn!(error = %e, "failed to get dead letter dispatches");
                Vec::new()
            }
        }
    }

    /// No-op: persistence is handled by EventStore (SQLite).
    pub async fn save(&self) -> Result<()> {
        Ok(())
    }

    /// No-op: state is already persisted in EventStore (SQLite).
    /// Returns the count of pending dispatches for logging.
    pub async fn load(&self) -> Result<usize> {
        let count = self.event_store.pending_dispatch_count().await.unwrap_or(0) as usize;
        if count > 0 {
            debug!(count, "dispatch bus has persisted unread messages");
        }
        Ok(count)
    }

    fn summarize_health(dispatches: &[Dispatch], overdue_cutoff: DateTime<Utc>) -> DispatchHealth {
        let mut health = DispatchHealth::default();

        for dispatch in dispatches {
            if !dispatch.read {
                health.unread += 1;
            }

            if !dispatch.requires_ack {
                continue;
            }

            if dispatch.retry_count >= dispatch.max_retries {
                health.dead_letters += 1;
                continue;
            }

            if dispatch.read {
                health.awaiting_ack += 1;
                if dispatch.timestamp < overdue_cutoff {
                    health.overdue_ack += 1;
                }
            } else if dispatch.retry_count > 0 {
                health.retrying_delivery += 1;
            }
        }

        health
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn open_test_db() -> Arc<EventStore> {
        let conn = Connection::open_in_memory().unwrap();
        crate::event_store::EventStore::create_tables(&conn).unwrap();
        let db = Arc::new(Mutex::new(conn));
        Arc::new(EventStore::new(db))
    }

    fn test_delegate_request() -> DispatchKind {
        DispatchKind::DelegateRequest {
            prompt: "do something".into(),
            response_mode: "origin".into(),
            create_task: false,
            skill: None,
            reply_to: None,
            parent_session_id: None,
        }
    }

    fn test_delegate_response() -> DispatchKind {
        DispatchKind::DelegateResponse {
            reply_to: "d-123".into(),
            response_mode: "origin".into(),
            content: "done".into(),
        }
    }

    fn test_human_escalation() -> DispatchKind {
        DispatchKind::HumanEscalation {
            project: "demo".into(),
            task_id: "t1".into(),
            subject: "blocked".into(),
            summary: "help".into(),
        }
    }

    #[tokio::test]
    async fn test_send_and_read() {
        let store = open_test_db();
        let bus = DispatchBus::new(store);
        bus.send(Dispatch::new_typed("a", "b", test_delegate_request()))
            .await;

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 1);

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 0);
    }

    #[tokio::test]
    async fn test_indexed_recipient() {
        let store = open_test_db();
        let bus = DispatchBus::new(store);

        bus.send(Dispatch::new_typed("a", "b", test_delegate_request()))
            .await;
        bus.send(Dispatch::new_typed("a", "c", test_delegate_response()))
            .await;

        assert_eq!(bus.read("b").await.len(), 1);
        assert_eq!(bus.read("c").await.len(), 1);
        assert_eq!(bus.read("d").await.len(), 0);
    }

    #[tokio::test]
    async fn test_ack_required_dispatch() {
        let store = open_test_db();
        let bus = DispatchBus::new(store);
        let dispatch = Dispatch::new_typed("a", "b", test_delegate_request()).with_ack_required();
        let dispatch_id = dispatch.id.clone();
        assert!(dispatch.requires_ack);
        bus.send(dispatch).await;

        let delivered = bus.read("b").await;
        assert_eq!(delivered.len(), 1);

        let retries = bus.retry_unacked(0).await;
        assert_eq!(retries.len(), 1);
        assert_eq!(retries[0].retry_count, 1);

        // After ack: should not be retried.
        bus.acknowledge(&dispatch_id).await;
        let retries = bus.retry_unacked(0).await;
        assert_eq!(retries.len(), 0);
    }

    #[tokio::test]
    async fn test_dead_letter_after_max_retries() {
        let store = open_test_db();
        let bus = DispatchBus::new(store);
        let mut dispatch =
            Dispatch::new_typed("a", "b", test_delegate_request()).with_ack_required();
        dispatch.max_retries = 2;
        bus.send(dispatch).await;
        let delivered = bus.read("b").await;
        assert_eq!(delivered.len(), 1);

        // Retry twice to exhaust max_retries.
        let _ = bus.retry_unacked(0).await; // retry_count -> 1
        let retried = bus.read("b").await;
        assert_eq!(retried.len(), 1);
        let _ = bus.retry_unacked(0).await; // retry_count -> 2

        // Should now be dead-lettered.
        let dead = bus.dead_letters().await;
        assert_eq!(dead.len(), 1);

        // Retry should return nothing (exceeded max).
        let retries = bus.retry_unacked(0).await;
        assert_eq!(retries.len(), 0);
    }

    #[tokio::test]
    async fn test_ack_prevents_retry() {
        let store = open_test_db();
        let bus = DispatchBus::new(store);
        let dispatch = Dispatch::new_typed("a", "b", test_delegate_request()).with_ack_required();
        let id = dispatch.id.clone();
        bus.send(dispatch).await;
        let delivered = bus.read("b").await;
        assert_eq!(delivered.len(), 1);

        bus.acknowledge(&id).await;

        let retries = bus.retry_unacked(0).await;
        assert!(retries.is_empty());

        let dead = bus.dead_letters().await;
        assert!(dead.is_empty());
    }

    #[test]
    fn test_critical_dispatches_require_ack_by_default() {
        assert!(Dispatch::new_typed("a", "leader", test_delegate_request(),).requires_ack);
        assert!(!Dispatch::new_typed("a", "leader", test_delegate_response(),).requires_ack);
        assert!(!Dispatch::new_typed("a", "leader", test_human_escalation(),).requires_ack);
    }
}
