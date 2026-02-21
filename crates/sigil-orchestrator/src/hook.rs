use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sigil_beads::BeadId;

/// A Hook pins a bead to a worker. GUPP: "If there is work on your hook,
/// you MUST run it." Workers discover their work via hooks on startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub bead_id: BeadId,
    pub subject: String,
    pub assigned_at: DateTime<Utc>,
}

impl Hook {
    pub fn new(bead_id: BeadId, subject: String) -> Self {
        Self {
            bead_id,
            subject,
            assigned_at: Utc::now(),
        }
    }
}
