use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use system_tasks::TaskId;
use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

/// A raid tracks work across multiple projects.
/// It monitors a set of beads from different projects and
/// auto-closes when all tracked beads are completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub name: String,
    pub tasks: Vec<OperationTask>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTask {
    pub task_id: TaskId,
    #[serde(alias = "rig")]
    pub project: String,
    pub closed: bool,
}

impl Operation {
    pub fn new(name: &str, tasks: Vec<(TaskId, String)>) -> Self {
        let id = format!("raid-{}", uuid::Uuid::new_v4().as_simple());
        Self {
            id,
            name: name.to_string(),
            tasks: tasks.into_iter().map(|(task_id, project)| OperationTask {
                task_id,
                project,
                closed: false,
            }).collect(),
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    /// Mark a bead as closed in this raid.
    pub fn mark_closed(&mut self, task_id: &TaskId) {
        for b in &mut self.tasks {
            if b.task_id == *task_id {
                b.closed = true;
            }
        }
    }

    /// Check if all beads in the raid are closed.
    pub fn is_complete(&self) -> bool {
        self.tasks.iter().all(|b| b.closed)
    }

    /// Count completed vs total.
    pub fn progress(&self) -> (usize, usize) {
        let done = self.tasks.iter().filter(|b| b.closed).count();
        (done, self.tasks.len())
    }
}

/// Persistent raid store.
pub struct OperationStore {
    path: std::path::PathBuf,
    pub raids: Vec<Operation>,
}

impl OperationStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut store = Self {
            path: path.to_path_buf(),
            raids: Vec::new(),
        };

        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read raid store: {}", path.display()))?;
            store.raids = serde_json::from_str(&content).unwrap_or_default();
        }

        Ok(store)
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.raids)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    /// Create a new raid.
    pub fn create(&mut self, name: &str, tasks: Vec<(TaskId, String)>) -> Result<&Operation> {
        let raid = Operation::new(name, tasks);
        info!(id = %raid.id, name = %name, tasks = raid.tasks.len(), "operation created");
        self.raids.push(raid);
        self.save()?;
        Ok(self.raids.last().unwrap())
    }

    /// Mark a bead as closed across all active raids.
    pub fn mark_bead_closed(&mut self, task_id: &TaskId) -> Result<Vec<String>> {
        let mut completed_raids = Vec::new();

        for raid in &mut self.raids {
            if raid.closed_at.is_some() {
                continue;
            }
            raid.mark_closed(task_id);
            if raid.is_complete() {
                raid.closed_at = Some(Utc::now());
                info!(id = %raid.id, name = %raid.name, "raid completed");
                completed_raids.push(raid.id.clone());
            }
        }

        if !completed_raids.is_empty() {
            self.save()?;
        }

        Ok(completed_raids)
    }

    /// Get a raid by ID.
    pub fn get(&self, id: &str) -> Option<&Operation> {
        self.raids.iter().find(|c| c.id == id)
    }

    /// List active (unclosed) raids.
    pub fn active(&self) -> Vec<&Operation> {
        self.raids.iter().filter(|c| c.closed_at.is_none()).collect()
    }

    /// Remove completed raids older than the specified days.
    pub fn cleanup(&mut self, max_age_days: i64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days);
        let before = self.raids.len();
        self.raids.retain(|c| {
            c.closed_at.map(|t| t > cutoff).unwrap_or(true)
        });
        let removed = before - self.raids.len();
        if removed > 0 {
            self.save()?;
        }
        Ok(removed)
    }
}
