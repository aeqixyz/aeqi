use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sigil_tasks::TaskId;
use std::path::Path;
use tracing::info;

/// An operation tracks work across multiple projects.
/// It monitors a set of tasks from different projects and
/// auto-closes when all tracked tasks are completed.
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
    pub project: String,
    pub closed: bool,
}

impl Operation {
    pub fn new(name: &str, tasks: Vec<(TaskId, String)>) -> Self {
        let id = format!("op-{}", uuid::Uuid::new_v4().as_simple());
        Self {
            id,
            name: name.to_string(),
            tasks: tasks
                .into_iter()
                .map(|(task_id, project)| OperationTask {
                    task_id,
                    project,
                    closed: false,
                })
                .collect(),
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    /// Mark a task as closed in this operation.
    pub fn mark_closed(&mut self, task_id: &TaskId) {
        for b in &mut self.tasks {
            if b.task_id == *task_id {
                b.closed = true;
            }
        }
    }

    /// Check if all tasks in the operation are closed.
    pub fn is_complete(&self) -> bool {
        self.tasks.iter().all(|b| b.closed)
    }

    /// Count completed vs total.
    pub fn progress(&self) -> (usize, usize) {
        let done = self.tasks.iter().filter(|b| b.closed).count();
        (done, self.tasks.len())
    }
}

/// Persistent operation store.
pub struct OperationStore {
    path: std::path::PathBuf,
    pub operations: Vec<Operation>,
}

impl OperationStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut store = Self {
            path: path.to_path_buf(),
            operations: Vec::new(),
        };

        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read operation store: {}", path.display()))?;
            store.operations = serde_json::from_str(&content).unwrap_or_default();
        }

        Ok(store)
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.operations)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    /// Create a new operation.
    pub fn create(&mut self, name: &str, tasks: Vec<(TaskId, String)>) -> Result<&Operation> {
        let op = Operation::new(name, tasks);
        info!(id = %op.id, name = %name, tasks = op.tasks.len(), "operation created");
        self.operations.push(op);
        self.save()?;
        Ok(self.operations.last().unwrap())
    }

    /// Mark a task as closed across all active operations.
    pub fn mark_task_closed(&mut self, task_id: &TaskId) -> Result<Vec<String>> {
        let mut completed_ops = Vec::new();

        for op in &mut self.operations {
            if op.closed_at.is_some() {
                continue;
            }
            op.mark_closed(task_id);
            if op.is_complete() {
                op.closed_at = Some(Utc::now());
                info!(id = %op.id, name = %op.name, "operation completed");
                completed_ops.push(op.id.clone());
            }
        }

        if !completed_ops.is_empty() {
            self.save()?;
        }

        Ok(completed_ops)
    }

    /// Get an operation by ID.
    pub fn get(&self, id: &str) -> Option<&Operation> {
        self.operations.iter().find(|c| c.id == id)
    }

    /// List active (unclosed) operations.
    pub fn active(&self) -> Vec<&Operation> {
        self.operations
            .iter()
            .filter(|c| c.closed_at.is_none())
            .collect()
    }

    /// Remove completed operations older than the specified days.
    pub fn cleanup(&mut self, max_age_days: i64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days);
        let before = self.operations.len();
        self.operations
            .retain(|c| c.closed_at.map(|t| t > cutoff).unwrap_or(true));
        let removed = before - self.operations.len();
        if removed > 0 {
            self.save()?;
        }
        Ok(removed)
    }
}
