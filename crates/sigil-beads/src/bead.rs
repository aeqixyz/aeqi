use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A hierarchical bead ID: "as-001", "as-001.1", "as-001.1.3"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BeadId(pub String);

impl BeadId {
    /// Create a new root-level bead ID with the given prefix and sequence number.
    pub fn root(prefix: &str, seq: u32) -> Self {
        Self(format!("{prefix}-{seq:03}"))
    }

    /// Create a child bead ID: "as-001" + 2 → "as-001.2"
    pub fn child(&self, child_seq: u32) -> Self {
        Self(format!("{}.{child_seq}", self.0))
    }

    /// Get the prefix (e.g., "as" from "as-001.2").
    pub fn prefix(&self) -> &str {
        self.0.split('-').next().unwrap_or("")
    }

    /// Get the parent ID, if this is a child bead.
    pub fn parent(&self) -> Option<Self> {
        let last_dot = self.0.rfind('.')?;
        Some(Self(self.0[..last_dot].to_string()))
    }

    /// Depth: "as-001" = 0, "as-001.1" = 1, "as-001.1.3" = 2
    pub fn depth(&self) -> usize {
        self.0.matches('.').count()
    }

    /// Check if this bead is an ancestor of another.
    pub fn is_ancestor_of(&self, other: &BeadId) -> bool {
        other.0.starts_with(&self.0) && other.0.len() > self.0.len()
    }
}

impl fmt::Display for BeadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for BeadId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for BeadId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
    Cancelled,
}

impl fmt::Display for BeadStatus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
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

/// A single bead (task) in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bead {
    pub id: BeadId,
    pub subject: String,
    #[serde(default)]
    pub description: String,
    pub status: BeadStatus,
    #[serde(default)]
    pub priority: Priority,
    /// Who is working on this bead.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Bead IDs that must be completed before this one can start.
    #[serde(default)]
    pub depends_on: Vec<BeadId>,
    /// Bead IDs that this bead blocks.
    #[serde(default)]
    pub blocks: Vec<BeadId>,
    /// Labels for categorization (e.g., "sg:mail", "molecule:feature-dev").
    #[serde(default)]
    pub labels: Vec<String>,
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
}

impl Bead {
    /// Create a new bead with minimal fields.
    pub fn new(id: BeadId, subject: impl Into<String>) -> Self {
        Self {
            id,
            subject: subject.into(),
            description: String::new(),
            status: BeadStatus::Pending,
            priority: Priority::Normal,
            assignee: None,
            depends_on: Vec::new(),
            blocks: Vec::new(),
            labels: Vec::new(),
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
            updated_at: None,
            closed_at: None,
            closed_reason: None,
        }
    }

    /// Is this bead in a terminal state?
    pub fn is_closed(&self) -> bool {
        matches!(self.status, BeadStatus::Done | BeadStatus::Cancelled)
    }

    /// Is this bead ready to work on? (pending + no unresolved dependencies)
    pub fn is_ready(&self, resolved: &dyn Fn(&BeadId) -> bool) -> bool {
        self.status == BeadStatus::Pending
            && self.depends_on.iter().all(|dep| resolved(dep))
    }
}
