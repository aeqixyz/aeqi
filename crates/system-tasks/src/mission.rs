use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::task::TaskStatus;

/// A mission groups related tasks within a project.
///
/// Hierarchy: Project → Mission → Task
/// Tasks reference their mission via `task.mission_id`.
/// A mission auto-closes when all its tasks are done.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub status: TaskStatus,
    /// Project prefix this mission belongs to (e.g., "as" for algostaking).
    pub project_prefix: String,
    #[serde(default)]
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
}

impl Mission {
    /// Create a new mission.
    pub fn new(id: impl Into<String>, name: impl Into<String>, project_prefix: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            status: TaskStatus::Pending,
            project_prefix: project_prefix.into(),
            labels: Vec::new(),
            created_at: Utc::now(),
            updated_at: None,
            closed_at: None,
        }
    }

    /// Is this mission in a terminal state?
    pub fn is_closed(&self) -> bool {
        matches!(self.status, TaskStatus::Done | TaskStatus::Cancelled)
    }

    /// Generate a mission ID from a prefix and sequence number.
    pub fn make_id(prefix: &str, seq: u32) -> String {
        format!("{prefix}-m{seq:03}")
    }

    /// Check completion: returns (done_count, total_count).
    pub fn check_progress(mission_id: &str, tasks: &[&crate::task::Task]) -> (usize, usize) {
        let mission_tasks: Vec<_> = tasks
            .iter()
            .filter(|t| t.mission_id.as_deref() == Some(mission_id))
            .collect();
        let done = mission_tasks.iter().filter(|t| t.is_closed()).count();
        (done, mission_tasks.len())
    }
}

impl std::fmt::Display for Mission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {} ({})", self.id, self.name, self.status)
    }
}
