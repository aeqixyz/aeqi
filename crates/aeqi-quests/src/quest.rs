use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A hierarchical quest ID: "as-001", "as-001.1", "as-001.1.3"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QuestId(pub String);

impl QuestId {
    /// Create a new root-level quest ID with the given prefix and sequence number.
    pub fn root(prefix: &str, seq: u32) -> Self {
        Self(format!("{prefix}-{seq:03}"))
    }

    /// Create a child quest ID: "as-001" + 2 → "as-001.2"
    pub fn child(&self, child_seq: u32) -> Self {
        Self(format!("{}.{child_seq}", self.0))
    }

    /// Get the prefix (e.g., "as" from "as-001.2").
    pub fn prefix(&self) -> &str {
        self.0.split('-').next().unwrap_or("")
    }

    /// Get the parent ID, if this is a child quest.
    pub fn parent(&self) -> Option<Self> {
        let last_dot = self.0.rfind('.')?;
        Some(Self(self.0[..last_dot].to_string()))
    }

    /// Depth: "as-001" = 0, "as-001.1" = 1, "as-001.1.3" = 2
    pub fn depth(&self) -> usize {
        self.0.matches('.').count()
    }

    /// Check if this quest is an ancestor of another.
    pub fn is_ancestor_of(&self, other: &QuestId) -> bool {
        other.0.starts_with(&self.0) && other.0.len() > self.0.len()
    }
}

impl fmt::Display for QuestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for QuestId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for QuestId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
    Cancelled,
}

impl fmt::Display for QuestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Done => write!(f, "done"),
            Self::Blocked => write!(f, "blocked"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestOutcomeKind {
    Done,
    Blocked,
    Handoff,
    Failed,
    Cancelled,
}

impl fmt::Display for QuestOutcomeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Done => write!(f, "done"),
            Self::Blocked => write!(f, "blocked"),
            Self::Handoff => write!(f, "handoff"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Normal => write!(f, "normal"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// A checkpoint recording incremental progress on a quest.
/// Saved when a worker completes, blocks, or fails — so the next worker
/// can skip work that's already done.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub timestamp: DateTime<Utc>,
    pub worker: String,
    pub progress: String,
    pub cost_usd: f64,
    pub turns_used: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestOutcomeRecord {
    pub kind: QuestOutcomeKind,
    pub summary: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub next_action: Option<String>,
}

impl QuestOutcomeRecord {
    pub fn new(kind: QuestOutcomeKind, summary: impl Into<String>) -> Self {
        Self {
            kind,
            summary: summary.into(),
            reason: None,
            next_action: None,
        }
    }
}

/// A single quest in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest {
    pub id: QuestId,
    #[serde(alias = "subject")]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub status: QuestStatus,
    #[serde(default)]
    pub priority: Priority,
    /// Who is working on this quest.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Persistent agent UUID that owns this quest. None = legacy/unbound.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Quest IDs that must be completed before this one can start.
    #[serde(default)]
    pub depends_on: Vec<QuestId>,
    /// Quest IDs that this quest blocks.
    #[serde(default)]
    pub blocks: Vec<QuestId>,
    /// Skill to apply when executing this quest (loaded from project skills dir).
    #[serde(default)]
    pub skill: Option<String>,
    /// Labels for categorization.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Number of times this quest has been retried after failure/handoff.
    #[serde(default)]
    pub retry_count: u32,
    /// Incremental progress checkpoints from previous worker attempts.
    #[serde(default)]
    pub checkpoints: Vec<Checkpoint>,
    /// Arbitrary metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub closed_reason: Option<String>,
    /// What "done" looks like — worker validates output against this.
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    /// Worker that currently holds the execution lock.
    #[serde(default)]
    pub locked_by: Option<String>,
    /// When the execution lock was acquired.
    #[serde(default)]
    pub locked_at: Option<DateTime<Utc>>,
}

impl Quest {
    /// Create a new quest with minimal fields.
    pub fn new(id: QuestId, name: impl Into<String>) -> Self {
        Self::with_agent(id, name, None)
    }

    /// Create a new quest bound to a specific agent.
    pub fn with_agent(id: QuestId, name: impl Into<String>, agent_id: Option<&str>) -> Self {
        Self {
            id,
            name: name.into(),
            description: String::new(),
            status: QuestStatus::Pending,
            priority: Priority::Normal,
            assignee: None,
            agent_id: agent_id.map(|s| s.to_string()),
            depends_on: Vec::new(),
            blocks: Vec::new(),
            skill: None,
            labels: Vec::new(),
            retry_count: 0,
            checkpoints: Vec::new(),
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
            updated_at: None,
            closed_at: None,
            closed_reason: None,
            acceptance_criteria: None,
            locked_by: None,
            locked_at: None,
        }
    }

    /// Whether this quest is bound to a persistent agent.
    pub fn is_agent_bound(&self) -> bool {
        self.agent_id.is_some()
    }

    /// Is this quest in a terminal state?
    pub fn is_closed(&self) -> bool {
        matches!(self.status, QuestStatus::Done | QuestStatus::Cancelled)
    }

    /// Is this quest ready to work on? (pending + no unresolved dependencies)
    pub fn is_ready(&self, resolved: &dyn Fn(&QuestId) -> bool) -> bool {
        self.status == QuestStatus::Pending && self.depends_on.iter().all(resolved)
    }

    /// Whether the scheduler should temporarily hold this quest from execution.
    pub fn is_scheduler_held(&self) -> bool {
        self.metadata
            .pointer("/aeqi/hold")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn aeqi_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata
            .as_object()
            .and_then(|meta| meta.get("aeqi"))
            .and_then(|aeqi| aeqi.as_object())
            .and_then(|aeqi| aeqi.get(key))
    }

    pub fn set_aeqi_metadata(&mut self, key: &str, value: serde_json::Value) {
        let mut metadata = match std::mem::take(&mut self.metadata) {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("_legacy".to_string(), other);
                map
            }
        };

        let aeqi_value = metadata
            .entry("aeqi".to_string())
            .or_insert_with(|| serde_json::json!({}));

        if !aeqi_value.is_object() {
            *aeqi_value = serde_json::json!({});
        }

        if let Some(aeqi_meta) = aeqi_value.as_object_mut() {
            aeqi_meta.insert(key.to_string(), value);
        }

        self.metadata = if metadata.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(metadata)
        };
    }

    pub fn task_outcome(&self) -> Option<QuestOutcomeRecord> {
        self.aeqi_metadata("task_outcome")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
    }

    pub fn set_task_outcome(&mut self, outcome: &QuestOutcomeRecord) {
        if let Ok(value) = serde_json::to_value(outcome) {
            self.set_aeqi_metadata("task_outcome", value);
        }
    }

    pub fn runtime(&self) -> Option<serde_json::Value> {
        self.aeqi_metadata("runtime").cloned()
    }

    pub fn outcome_summary(&self) -> Option<String> {
        self.task_outcome()
            .map(|outcome| outcome.summary)
            .filter(|summary| !summary.trim().is_empty())
            .or_else(|| self.closed_reason.clone())
    }

    pub fn blocker_context(&self) -> Option<String> {
        self.task_outcome()
            .and_then(|outcome| {
                outcome
                    .reason
                    .filter(|reason| !reason.trim().is_empty())
                    .or_else(|| (!outcome.summary.trim().is_empty()).then_some(outcome.summary))
            })
            .or_else(|| self.closed_reason.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{Quest, QuestId, QuestOutcomeKind, QuestOutcomeRecord};

    #[test]
    fn quest_outcome_round_trips_through_aeqi_metadata() {
        let mut quest = Quest::new(QuestId::from("sg-001"), "Outcome");
        let outcome = QuestOutcomeRecord {
            kind: QuestOutcomeKind::Blocked,
            summary: "Waiting on staging credentials".to_string(),
            reason: Some("Which staging account should be used?".to_string()),
            next_action: Some("await_operator_input".to_string()),
        };

        quest.set_task_outcome(&outcome);

        assert_eq!(quest.task_outcome(), Some(outcome));
    }

    #[test]
    fn set_aeqi_metadata_preserves_legacy_metadata() {
        let mut quest = Quest::new(QuestId::from("sg-002"), "Legacy");
        quest.metadata = serde_json::json!("legacy");

        quest.set_aeqi_metadata("runtime", serde_json::json!({"phase": "act"}));

        assert_eq!(
            quest
                .metadata
                .pointer("/_legacy")
                .and_then(|value| value.as_str()),
            Some("legacy")
        );
        assert_eq!(
            quest
                .metadata
                .pointer("/aeqi/runtime/phase")
                .and_then(|value| value.as_str()),
            Some("act")
        );
    }
}
