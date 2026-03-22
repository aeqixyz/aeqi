use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A hierarchical task ID: "as-001", "as-001.1", "as-001.1.3"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    /// Create a new root-level task ID with the given prefix and sequence number.
    pub fn root(prefix: &str, seq: u32) -> Self {
        Self(format!("{prefix}-{seq:03}"))
    }

    /// Create a child task ID: "as-001" + 2 → "as-001.2"
    pub fn child(&self, child_seq: u32) -> Self {
        Self(format!("{}.{child_seq}", self.0))
    }

    /// Get the prefix (e.g., "as" from "as-001.2").
    pub fn prefix(&self) -> &str {
        self.0.split('-').next().unwrap_or("")
    }

    /// Get the parent ID, if this is a child task.
    pub fn parent(&self) -> Option<Self> {
        let last_dot = self.0.rfind('.')?;
        Some(Self(self.0[..last_dot].to_string()))
    }

    /// Depth: "as-001" = 0, "as-001.1" = 1, "as-001.1.3" = 2
    pub fn depth(&self) -> usize {
        self.0.matches('.').count()
    }

    /// Check if this task is an ancestor of another.
    pub fn is_ancestor_of(&self, other: &TaskId) -> bool {
        other.0.starts_with(&self.0) && other.0.len() > self.0.len()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
    Cancelled,
}

impl fmt::Display for TaskStatus {
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

/// A checkpoint recording incremental progress on a task.
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

/// A single task in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub subject: String,
    #[serde(default)]
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub priority: Priority,
    /// Who is working on this task.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Task IDs that must be completed before this one can start.
    #[serde(default)]
    pub depends_on: Vec<TaskId>,
    /// Task IDs that this task blocks.
    #[serde(default)]
    pub blocks: Vec<TaskId>,
    /// Mission this task belongs to (groups related tasks).
    #[serde(default)]
    pub mission_id: Option<String>,
    /// Skill to apply when executing this task (loaded from project skills dir).
    #[serde(default)]
    pub skill: Option<String>,
    /// Labels for categorization.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Number of times this task has been retried after failure/handoff.
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
}

impl Task {
    /// Create a new task with minimal fields.
    pub fn new(id: TaskId, subject: impl Into<String>) -> Self {
        Self {
            id,
            subject: subject.into(),
            description: String::new(),
            status: TaskStatus::Pending,
            priority: Priority::Normal,
            assignee: None,
            depends_on: Vec::new(),
            blocks: Vec::new(),
            mission_id: None,
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
        }
    }

    /// Is this task in a terminal state?
    pub fn is_closed(&self) -> bool {
        matches!(self.status, TaskStatus::Done | TaskStatus::Cancelled)
    }

    /// Is this task ready to work on? (pending + no unresolved dependencies)
    pub fn is_ready(&self, resolved: &dyn Fn(&TaskId) -> bool) -> bool {
        self.status == TaskStatus::Pending && self.depends_on.iter().all(resolved)
    }
}
