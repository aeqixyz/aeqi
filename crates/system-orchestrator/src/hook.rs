use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use system_tasks::TaskId;

/// A Hook pins a bead to a worker. GUPP: "If there is work on your hook,
/// you MUST run it." Spirits discover their work via hooks on startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub task_id: TaskId,
    pub subject: String,
    pub assigned_at: DateTime<Utc>,
}

impl Hook {
    pub fn new(task_id: TaskId, subject: String) -> Self {
        Self {
            task_id,
            subject,
            assigned_at: Utc::now(),
        }
    }
}
