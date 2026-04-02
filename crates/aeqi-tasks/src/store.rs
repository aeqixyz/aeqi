use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::task::{Task, TaskId, TaskOutcomeKind, TaskOutcomeRecord, TaskStatus};

/// Valid transitions for the task state machine.
fn valid_transition(from: &TaskStatus, to: &TaskStatus) -> bool {
    use TaskStatus::*;
    matches!(
        (from, to),
        // Normal forward flow
        (Pending, InProgress)
            | (InProgress, Done)
            | (InProgress, Blocked)
            | (InProgress, Cancelled)
            // Retry/re-queue (from worker failure handling)
            | (InProgress, Pending)
            | (Blocked, Pending)
            // Cancellation from any non-terminal state
            | (Pending, Cancelled)
            | (Blocked, Cancelled)
            // Same-state (no-op)
            | (Pending, Pending)
            | (InProgress, InProgress)
    )
}

/// JSONL-based task store. One file per prefix, git-native.
pub struct TaskBoard {
    dir: PathBuf,
    /// In-memory index: all tasks keyed by ID.
    tasks: HashMap<String, Task>,
    /// Next sequence number per prefix.
    sequences: HashMap<String, u32>,
}

impl TaskBoard {
    /// Open or create a task store in the given directory.
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create tasks dir: {}", dir.display()))?;

        let mut store = Self {
            dir: dir.to_path_buf(),
            tasks: HashMap::new(),
            sequences: HashMap::new(),
        };

        store.load_all()?;
        Ok(store)
    }

    /// Load all JSONL files from the store directory.
    fn load_all(&mut self) -> Result<()> {
        let entries = std::fs::read_dir(&self.dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                self.load_file(&path)?;
            }
        }
        Ok(())
    }

    /// Load tasks from a single JSONL file.
    fn load_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match serde_json::from_str::<Task>(line) {
                Ok(task) => {
                    // Track max sequence for this prefix.
                    let prefix = task.id.prefix().to_string();
                    if task.id.depth() == 0
                        && let Some(seq_str) = task.id.0.split('-').nth(1)
                    {
                        // Handle dotted children: take only the root part.
                        let root_seq = seq_str.split('.').next().unwrap_or(seq_str);
                        if let Ok(seq) = root_seq.parse::<u32>() {
                            let entry = self.sequences.entry(prefix).or_insert(0);
                            *entry = (*entry).max(seq);
                        }
                    }
                    self.tasks.insert(task.id.0.clone(), task);
                }
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "skipping malformed task line");
                }
            }
        }

        Ok(())
    }

    /// Persist a task to its prefix JSONL file (append).
    fn persist(&self, task: &Task) -> Result<()> {
        let prefix = task.id.prefix();
        let path = self.dir.join(format!("{prefix}.jsonl"));

        let line = serde_json::to_string(task)? + "\n";

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.write_all(line.as_bytes())?;

        Ok(())
    }

    /// Rewrite the entire JSONL file for a prefix (after updates).
    fn rewrite_prefix(&self, prefix: &str) -> Result<()> {
        let path = self.dir.join(format!("{prefix}.jsonl"));

        let mut tasks: Vec<&Task> = self
            .tasks
            .values()
            .filter(|b| b.id.prefix() == prefix)
            .collect();
        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let mut content = String::new();
        for task in tasks {
            content.push_str(&serde_json::to_string(task)?);
            content.push('\n');
        }

        std::fs::write(&path, &content)
            .with_context(|| format!("failed to write {}", path.display()))?;

        Ok(())
    }

    /// Deprecated: prefer create_with_agent() which binds to a persistent agent.
    pub fn create_unbound(&mut self, prefix: &str, subject: &str) -> Result<Task> {
        self.create_with_agent(prefix, subject, None)
    }

    /// Create a new task with auto-generated ID and optional agent binding.
    pub fn create_with_agent(
        &mut self,
        prefix: &str,
        subject: &str,
        agent_id: Option<&str>,
    ) -> Result<Task> {
        let seq = self.sequences.entry(prefix.to_string()).or_insert(0);
        *seq += 1;
        let id = TaskId::root(prefix, *seq);

        let task = Task::with_agent(id, subject, agent_id);
        self.persist(&task)?;
        self.tasks.insert(task.id.0.clone(), task.clone());

        Ok(task)
    }

    /// Create a child task under a parent. Inherits the parent's `agent_id`.
    pub fn create_child(&mut self, parent_id: &TaskId, subject: &str) -> Result<Task> {
        let parent_agent_id = self
            .tasks
            .get(&parent_id.0)
            .and_then(|p| p.agent_id.clone());

        // Count existing children to determine next child seq.
        let child_count = self
            .tasks
            .values()
            .filter(|b| b.id.parent().as_ref() == Some(parent_id))
            .count() as u32;

        let id = parent_id.child(child_count + 1);
        let mut task = Task::new(id, subject);
        // Inherit agent_id from parent.
        task.agent_id = parent_agent_id;
        task.depends_on = Vec::new();

        self.persist(&task)?;
        self.tasks.insert(task.id.0.clone(), task.clone());

        Ok(task)
    }

    /// Get a task by ID.
    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Update a task. Returns the updated task.
    ///
    /// Uses append-only persistence: the updated task is appended to the JSONL
    /// file rather than rewriting all tasks for the prefix. On reload, later
    /// entries overwrite earlier ones (last-write-wins dedup in load_file).
    pub fn update(&mut self, id: &str, f: impl FnOnce(&mut Task)) -> Result<Task> {
        let task = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {id}"))?;

        f(task);
        task.updated_at = Some(chrono::Utc::now());

        let task = task.clone();
        self.persist(&task)?;

        Ok(task)
    }

    /// Update a task with state transition validation.
    ///
    /// Like `update()`, but logs a warning if the status change is not a valid
    /// transition in the task state machine. Does NOT block the update — callers
    /// can migrate from `update()` over time.
    pub fn validated_update(&mut self, id: &str, f: impl FnOnce(&mut Task)) -> Result<Task> {
        let old_status = self
            .tasks
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {id}"))?
            .status;

        let task = self.update(id, f)?;

        if !valid_transition(&old_status, &task.status) {
            tracing::warn!(
                task = %id,
                from = ?old_status,
                to = ?task.status,
                "invalid task state transition (allowed for backwards compat)"
            );
        }

        Ok(task)
    }

    /// Atomically claim a task for execution. Returns Err if already locked.
    pub fn checkout(&mut self, id: &str, worker_id: &str) -> Result<Task> {
        let task = self
            .tasks
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {id}"))?;

        if task.status != TaskStatus::Pending {
            anyhow::bail!("task {} is not Pending (status: {:?})", id, task.status);
        }

        if let Some(ref locked_by) = task.locked_by {
            anyhow::bail!("task {} already locked by {}", id, locked_by);
        }

        self.update(id, |t| {
            t.locked_by = Some(worker_id.to_string());
            t.locked_at = Some(chrono::Utc::now());
            t.status = TaskStatus::InProgress;
        })
    }

    /// Release the execution lock (on completion or failure).
    pub fn release(&mut self, id: &str) -> Result<Task> {
        self.update(id, |t| {
            t.locked_by = None;
            t.locked_at = None;
        })
    }

    /// Close a task (mark as done with reason).
    /// Automatically cascades: if all sibling children of a parent are now closed,
    /// the parent is auto-closed too (pipeline auto-progression).
    pub fn close(&mut self, id: &str, reason: &str) -> Result<Task> {
        let task = self.update(id, |b| {
            b.status = TaskStatus::Done;
            b.closed_at = Some(chrono::Utc::now());
            b.closed_reason = Some(reason.to_string());
            b.set_task_outcome(&TaskOutcomeRecord::new(TaskOutcomeKind::Done, reason));
        })?;

        self.cascade_parent_close(&task.id);

        Ok(task)
    }

    fn cascade_parent_close(&mut self, child_id: &TaskId) {
        let Some(parent_id) = child_id.parent() else {
            return;
        };
        let Some(parent) = self.tasks.get(&parent_id.0) else {
            return;
        };
        if parent.is_closed() {
            return;
        }

        let children: Vec<String> = self
            .tasks
            .values()
            .filter(|b| b.id.parent().as_ref() == Some(&parent_id))
            .map(|b| b.id.0.clone())
            .collect();

        if children.is_empty() {
            return;
        }

        let all_closed = children
            .iter()
            .all(|cid| self.tasks.get(cid).is_some_and(|b| b.is_closed()));

        if all_closed {
            // Check if ALL children were cancelled — parent should be Cancelled, not Done.
            let all_cancelled = children.iter().all(|cid| {
                self.tasks
                    .get(cid)
                    .is_some_and(|b| b.status == TaskStatus::Cancelled)
            });

            let (outcome_status, outcome_kind, verb) = if all_cancelled {
                (
                    TaskStatus::Cancelled,
                    TaskOutcomeKind::Cancelled,
                    "cancelled",
                )
            } else {
                (TaskStatus::Done, TaskOutcomeKind::Done, "completed")
            };

            let child_summaries: Vec<String> = children
                .iter()
                .filter_map(|cid| {
                    self.tasks.get(cid).map(|b| {
                        let reason = b.closed_reason.as_deref().unwrap_or(verb);
                        format!("  {} — {}", b.subject, reason)
                    })
                })
                .collect();

            let reason = format!(
                "All {} steps {}:\n{}",
                children.len(),
                verb,
                child_summaries.join("\n")
            );

            if let Err(e) = self.update(&parent_id.0, |b| {
                b.status = outcome_status;
                b.closed_at = Some(chrono::Utc::now());
                b.closed_reason = Some(reason.clone());
                b.set_task_outcome(&TaskOutcomeRecord::new(outcome_kind, reason.clone()));
            }) {
                debug!(parent = %parent_id, error = %e, "failed to auto-close parent task");
                return;
            }

            debug!(parent = %parent_id, children = children.len(), status = ?outcome_status, "auto-closed parent (all children closed)");

            self.cascade_parent_close(&parent_id);
        }
    }

    /// Cancel a task.
    pub fn cancel(&mut self, id: &str, reason: &str) -> Result<Task> {
        self.update(id, |b| {
            b.status = TaskStatus::Cancelled;
            b.closed_at = Some(chrono::Utc::now());
            b.closed_reason = Some(reason.to_string());
            b.set_task_outcome(&TaskOutcomeRecord::new(TaskOutcomeKind::Cancelled, reason));
        })
    }

    /// Detect if adding `from` depends-on `to` would create a cycle.
    /// Check if adding "`id` depends on `dep_id`" would create a cycle.
    /// Follows depends_on edges from dep_id; if we reach id, it's a cycle.
    fn would_cycle(&self, id: &str, dep_id: &str) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![dep_id.to_string()];
        while let Some(node) = stack.pop() {
            if node == id {
                return true;
            }
            if visited.insert(node.clone())
                && let Some(task_entry) = self.tasks.get(&node)
            {
                for dep in &task_entry.depends_on {
                    stack.push(dep.0.clone());
                }
            }
        }
        false
    }

    /// Add a dependency: `id` depends on `dep_id`.
    pub fn add_dependency(&mut self, id: &str, dep_id: &str) -> Result<()> {
        if id == dep_id {
            anyhow::bail!("task cannot depend on itself: {id}");
        }
        if self.would_cycle(id, dep_id) {
            anyhow::bail!("circular dependency detected: {id} → {dep_id} would create a cycle");
        }

        let dep_task_id = TaskId::from(dep_id);

        self.update(id, |b| {
            if !b.depends_on.contains(&dep_task_id) {
                b.depends_on.push(dep_task_id.clone());
            }
        })?;

        // Add to blocks on the dependency.
        let blocker_id = TaskId::from(id);
        if self.tasks.contains_key(dep_id) {
            self.update(dep_id, |b| {
                if !b.blocks.contains(&blocker_id) {
                    b.blocks.push(blocker_id.clone());
                }
            })?;
        }

        Ok(())
    }

    /// Get all tasks that are ready (pending + all deps resolved).
    pub fn ready(&self) -> Vec<&Task> {
        let resolved =
            |id: &TaskId| -> bool { self.tasks.get(&id.0).is_some_and(|b| b.is_closed()) };

        let mut ready: Vec<&Task> = self
            .tasks
            .values()
            .filter(|b| b.is_ready(&resolved) && !b.is_scheduler_held())
            .collect();

        // Sort by priority (highest first), then by creation time.
        ready.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        ready
    }

    /// Get all tasks matching a prefix.
    pub fn by_prefix(&self, prefix: &str) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .values()
            .filter(|b| b.id.prefix() == prefix)
            .collect();
        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        tasks
    }

    /// Get all tasks.
    pub fn all(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.tasks.values().collect();
        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        tasks
    }

    /// Get all tasks assigned to a specific agent.
    pub fn assigned_to(&self, assignee: &str) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|b| b.assignee.as_deref() == Some(assignee) && !b.is_closed())
            .collect()
    }

    /// Get children of a task.
    pub fn children(&self, parent_id: &TaskId) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|b| b.id.parent().as_ref() == Some(parent_id))
            .collect()
    }

    /// Count open tasks by prefix.
    pub fn open_count_by_prefix(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for task in self.tasks.values() {
            if !task.is_closed() {
                *counts.entry(task.id.prefix().to_string()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Reload all tasks from disk, picking up externally-created tasks.
    /// Compacts all prefix files after reload to remove duplicate entries.
    pub fn reload(&mut self) -> Result<()> {
        self.tasks.clear();
        self.sequences.clear();
        self.load_all()?;
        self.compact_all()
    }

    /// Rewrite all prefix files to contain only the latest version of each task.
    /// This deduplicates append-only entries accumulated during updates.
    fn compact_all(&self) -> Result<()> {
        let mut prefixes: std::collections::HashSet<String> = std::collections::HashSet::new();
        for task in self.tasks.values() {
            prefixes.insert(task.id.prefix().to_string());
        }
        for prefix in prefixes {
            self.rewrite_prefix(&prefix)?;
        }
        Ok(())
    }

    // ── Dependency Inference ─────────────────────────────────────

    /// Suggest dependencies between open tasks based on entity overlap.
    pub fn suggest_dependencies(
        &self,
        threshold: f64,
    ) -> Vec<crate::dependency_inference::InferredDependency> {
        let open_tasks: Vec<&Task> = self.tasks.values().filter(|t| !t.is_closed()).collect();
        crate::dependency_inference::infer_dependencies(&open_tasks, threshold)
    }

    /// Apply inferred dependencies above the given confidence threshold.
    /// Skips any that would create cycles. Returns count of applied dependencies.
    pub fn apply_inferred_dependencies(&mut self, threshold: f64) -> Result<usize> {
        let deps = self.suggest_dependencies(threshold);
        let mut applied = 0;
        for dep in deps {
            if !self.would_cycle(&dep.from.0, &dep.to.0)
                && self.add_dependency(&dep.from.0, &dep.to.0).is_ok()
            {
                applied += 1;
            }
        }
        Ok(applied)
    }

    // ── General ────────────────────────────────────────────────────

    /// Store directory path.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Total task count.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TaskBoard, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = TaskBoard::open(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn test_create_and_get() {
        let (mut store, _dir) = temp_store();
        let task = store.create_unbound("as", "Fix login bug").unwrap();
        assert_eq!(task.id.0, "as-001");
        assert_eq!(task.subject, "Fix login bug");

        let task2 = store.create_unbound("as", "Add logout button").unwrap();
        assert_eq!(task2.id.0, "as-002");

        assert!(store.get("as-001").is_some());
        assert!(store.get("as-002").is_some());
        assert!(store.get("as-003").is_none());
    }

    #[test]
    fn test_children() {
        let (mut store, _dir) = temp_store();
        let parent = store.create_unbound("as", "Feature X").unwrap();
        let child1 = store.create_child(&parent.id, "Step 1").unwrap();
        let child2 = store.create_child(&parent.id, "Step 2").unwrap();

        assert_eq!(child1.id.0, "as-001.1");
        assert_eq!(child2.id.0, "as-001.2");
        assert_eq!(child1.id.parent().unwrap(), parent.id);
    }

    #[test]
    fn test_dependencies_and_ready() {
        let (mut store, _dir) = temp_store();
        let b1 = store.create_unbound("as", "Task 1").unwrap();
        let b2 = store.create_unbound("as", "Task 2").unwrap();

        store.add_dependency(&b2.id.0, &b1.id.0).unwrap();

        // b1 is ready, b2 is blocked.
        let ready = store.ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, b1.id);

        // Close b1 → b2 becomes ready.
        store.close(&b1.id.0, "completed").unwrap();
        let ready = store.ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, b2.id);
    }

    #[test]
    fn test_scheduler_hold_excludes_task_from_ready() {
        let (mut store, _dir) = temp_store();
        let held = store.create_unbound("as", "Held task").unwrap();
        let free = store.create_unbound("as", "Free task").unwrap();

        store
            .update(&held.id.0, |b| {
                b.metadata = serde_json::json!({
                    "aeqi": {
                        "hold": true,
                        "hold_reason": "awaiting_council"
                    }
                });
            })
            .unwrap();

        let ready = store.ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, free.id);
        assert!(store.get(&held.id.0).unwrap().is_scheduler_held());
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();

        {
            let mut store = TaskBoard::open(dir.path()).unwrap();
            store.create_unbound("rd", "Price check").unwrap();
            store.create_unbound("rd", "Inventory update").unwrap();
        }

        // Reopen and verify data persisted.
        let store = TaskBoard::open(dir.path()).unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.get("rd-001").is_some());
        assert!(store.get("rd-002").is_some());
    }

    #[test]
    fn test_self_dependency_rejected() {
        let (mut store, _dir) = temp_store();
        let b1 = store.create_unbound("as", "Task 1").unwrap();
        assert!(store.add_dependency(&b1.id.0, &b1.id.0).is_err());
    }

    #[test]
    fn test_circular_dependency_rejected() {
        let (mut store, _dir) = temp_store();
        let b1 = store.create_unbound("as", "Task A").unwrap();
        let b2 = store.create_unbound("as", "Task B").unwrap();
        let b3 = store.create_unbound("as", "Task C").unwrap();

        store.add_dependency(&b2.id.0, &b1.id.0).unwrap();
        store.add_dependency(&b3.id.0, &b2.id.0).unwrap();
        // b3 → b2 → b1. Adding b1 → b3 would create a cycle.
        assert!(store.add_dependency(&b1.id.0, &b3.id.0).is_err());
    }

    #[test]
    fn test_append_only_update_persists() {
        let dir = TempDir::new().unwrap();

        {
            let mut store = TaskBoard::open(dir.path()).unwrap();
            store.create_unbound("as", "Task 1").unwrap();
            store
                .update("as-001", |b| {
                    b.status = TaskStatus::InProgress;
                    b.assignee = Some("worker-1".to_string());
                })
                .unwrap();
        }

        // Reopen — load_file deduplicates by last-write-wins.
        let store = TaskBoard::open(dir.path()).unwrap();
        assert_eq!(store.len(), 1);
        let task = store.get("as-001").unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.assignee.as_deref(), Some("worker-1"));
    }

    #[test]
    fn test_reload_compacts() {
        let dir = TempDir::new().unwrap();

        let mut store = TaskBoard::open(dir.path()).unwrap();
        store.create_unbound("as", "Task 1").unwrap();
        // Multiple updates = multiple append lines.
        for i in 0..5 {
            store
                .update("as-001", |b| {
                    b.subject = format!("Task 1 v{}", i + 1);
                })
                .unwrap();
        }

        // Before reload, file has 6 lines (1 create + 5 updates).
        let path = dir.path().join("as.jsonl");
        let lines_before = std::fs::read_to_string(&path).unwrap().lines().count();
        assert_eq!(lines_before, 6);

        // Reload compacts to 1 line.
        store.reload().unwrap();
        let lines_after = std::fs::read_to_string(&path).unwrap().lines().count();
        assert_eq!(lines_after, 1);

        let task = store.get("as-001").unwrap();
        assert_eq!(task.subject, "Task 1 v5");
    }

    #[test]
    fn test_auto_close_parent_when_all_children_done() {
        let (mut store, _dir) = temp_store();
        let parent = store.create_unbound("as", "Pipeline: Deploy").unwrap();
        let c1 = store.create_child(&parent.id, "Step 1: Build").unwrap();
        let c2 = store.create_child(&parent.id, "Step 2: Test").unwrap();
        let c3 = store.create_child(&parent.id, "Step 3: Ship").unwrap();

        store.close(&c1.id.0, "built").unwrap();
        assert_eq!(store.get(&parent.id.0).unwrap().status, TaskStatus::Pending);

        store.close(&c2.id.0, "tested").unwrap();
        assert_eq!(store.get(&parent.id.0).unwrap().status, TaskStatus::Pending);

        store.close(&c3.id.0, "shipped").unwrap();
        assert_eq!(store.get(&parent.id.0).unwrap().status, TaskStatus::Done);
        assert!(
            store
                .get(&parent.id.0)
                .unwrap()
                .closed_reason
                .as_ref()
                .unwrap()
                .contains("3 steps")
        );
    }

    #[test]
    fn test_auto_close_cascades_upward() {
        let (mut store, _dir) = temp_store();
        let grandparent = store.create_unbound("as", "Epic").unwrap();
        let parent = store.create_child(&grandparent.id, "Feature").unwrap();
        let child = store.create_child(&parent.id, "Task").unwrap();

        store.close(&child.id.0, "done").unwrap();
        assert_eq!(store.get(&parent.id.0).unwrap().status, TaskStatus::Done);
        assert_eq!(
            store.get(&grandparent.id.0).unwrap().status,
            TaskStatus::Done
        );
    }

}
