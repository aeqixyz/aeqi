//! Mission Auto-Decomposition.
//!
//! Automatically decomposes missions into a task DAG with dependencies and
//! critical path identification. Tasks on the critical path get elevated priority.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sigil_tasks::{Priority, TaskBoard, TaskId};

/// A decomposed sub-task with dependency references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecomposedTask {
    pub subject: String,
    pub description: String,
    pub priority: Priority,
    /// Indices into the parent DecompositionResult.tasks array.
    pub depends_on_indices: Vec<usize>,
    pub labels: Vec<String>,
}

/// Result of decomposing a mission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompositionResult {
    pub tasks: Vec<DecomposedTask>,
    /// Indices of tasks on the critical path (longest dependency chain).
    pub critical_path: Vec<usize>,
    pub cost_usd: f64,
}

impl DecompositionResult {
    /// Parse a decomposition result from LLM response text.
    /// Expected format:
    /// ```text
    /// TASK: Set up database schema
    /// DESC: Create initial migration files
    /// DEPS:
    /// LABELS: database, setup
    /// ---
    /// TASK: Implement user model
    /// DESC: Create User struct and CRUD operations
    /// DEPS: 0
    /// LABELS: backend, database
    /// ---
    /// TASK: Add authentication middleware
    /// DESC: JWT-based auth middleware
    /// DEPS: 1
    /// LABELS: backend, auth
    /// ```
    pub fn parse(text: &str) -> Self {
        let mut tasks = Vec::new();
        let mut current_subject = String::new();
        let mut current_desc = String::new();
        let mut current_deps: Vec<usize> = Vec::new();
        let mut current_labels: Vec<String> = Vec::new();

        let flush = |tasks: &mut Vec<DecomposedTask>,
                     subject: &mut String,
                     desc: &mut String,
                     deps: &mut Vec<usize>,
                     labels: &mut Vec<String>| {
            if !subject.is_empty() {
                tasks.push(DecomposedTask {
                    subject: std::mem::take(subject),
                    description: std::mem::take(desc),
                    priority: Priority::Normal,
                    depends_on_indices: std::mem::take(deps),
                    labels: std::mem::take(labels),
                });
            }
        };

        for line in text.lines() {
            let line = line.trim();
            if line == "---" {
                flush(
                    &mut tasks,
                    &mut current_subject,
                    &mut current_desc,
                    &mut current_deps,
                    &mut current_labels,
                );
            } else if let Some(rest) = line.strip_prefix("TASK:") {
                current_subject = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("DESC:") {
                current_desc = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("DEPS:") {
                current_deps = rest
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<usize>().ok())
                    .collect();
            } else if let Some(rest) = line.strip_prefix("LABELS:") {
                current_labels = rest
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        // Flush last task.
        flush(
            &mut tasks,
            &mut current_subject,
            &mut current_desc,
            &mut current_deps,
            &mut current_labels,
        );

        let critical_path = Self::compute_critical_path(&tasks);

        Self {
            tasks,
            critical_path,
            cost_usd: 0.0,
        }
    }

    /// Compute the critical path (longest chain of dependencies).
    fn compute_critical_path(tasks: &[DecomposedTask]) -> Vec<usize> {
        let n = tasks.len();
        if n == 0 {
            return Vec::new();
        }

        // dp[i] = length of longest path ending at task i
        let mut dp = vec![1usize; n];
        let mut parent = vec![None::<usize>; n];

        // Topological order: since deps reference earlier indices, iterate in order.
        for i in 0..n {
            for &dep in &tasks[i].depends_on_indices {
                if dep < n && dp[dep] + 1 > dp[i] {
                    dp[i] = dp[dep] + 1;
                    parent[i] = Some(dep);
                }
            }
        }

        // Find the end of the longest path.
        let end = (0..n).max_by_key(|&i| dp[i]).unwrap_or(0);

        // Trace back.
        let mut path = vec![end];
        let mut current = end;
        while let Some(p) = parent[current] {
            path.push(p);
            current = p;
        }
        path.reverse();
        path
    }

    /// Materialize decomposed tasks into a TaskBoard, wiring dependencies.
    /// Returns the created task IDs. If `agent_id` is provided, all created
    /// tasks are bound to that agent.
    pub fn materialize(
        &mut self,
        board: &mut TaskBoard,
        prefix: &str,
        mission_id: &str,
        agent_id: Option<&str>,
    ) -> Result<Vec<TaskId>> {
        let mut created_ids: Vec<TaskId> = Vec::new();

        // Validate no cycles in depends_on_indices.
        for (i, task) in self.tasks.iter().enumerate() {
            for &dep in &task.depends_on_indices {
                if dep >= i {
                    anyhow::bail!(
                        "dependency cycle detected: task {i} depends on task {dep} (not yet created)"
                    );
                }
            }
        }

        // Elevate critical path tasks to High priority.
        for &idx in &self.critical_path {
            if idx < self.tasks.len() {
                self.tasks[idx].priority = Priority::High;
            }
        }

        // Create tasks.
        for task in &self.tasks {
            let mut created = board.create_with_agent(prefix, &task.subject, agent_id)?;
            created = board.update(&created.id.0, |t| {
                t.description = task.description.clone();
                t.priority = task.priority;
                t.labels = task.labels.clone();
                t.mission_id = Some(mission_id.to_string());
            })?;
            created_ids.push(created.id);
        }

        // Wire dependencies.
        for (i, task) in self.tasks.iter().enumerate() {
            for &dep_idx in &task.depends_on_indices {
                if dep_idx < created_ids.len() {
                    board.add_dependency(&created_ids[i].0, &created_ids[dep_idx].0)?;
                }
            }
        }

        Ok(created_ids)
    }

    /// Build an LLM prompt for mission decomposition.
    pub fn decomposition_prompt(name: &str, description: &str) -> String {
        format!(
            "Decompose this mission into a task DAG. Each task should be small enough \
             for a single worker to complete. Include dependencies between tasks.\n\n\
             Mission: {name}\n\
             Description: {description}\n\n\
             Respond with tasks in this EXACT format (use --- to separate tasks):\n\
             TASK: <short task subject>\n\
             DESC: <1-2 sentence description>\n\
             DEPS: <comma-separated indices of tasks this depends on, 0-indexed, or empty>\n\
             LABELS: <comma-separated categorization labels>\n\
             ---\n\
             TASK: <next task>\n\
             ...\n\n\
             Rules:\n\
             - Dependencies must reference earlier tasks (lower indices)\n\
             - Keep tasks atomic — one clear deliverable each\n\
             - 3-10 tasks is typical"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_decompose_parse() {
        let text = "TASK: Create database schema\nDESC: Set up migrations\nDEPS:\nLABELS: database\n---\nTASK: Build user model\nDESC: User CRUD\nDEPS: 0\nLABELS: backend\n---\nTASK: Add auth\nDESC: JWT auth\nDEPS: 1\nLABELS: auth";
        let result = DecompositionResult::parse(text);
        assert_eq!(result.tasks.len(), 3);
        assert_eq!(result.tasks[0].subject, "Create database schema");
        assert!(result.tasks[0].depends_on_indices.is_empty());
        assert_eq!(result.tasks[1].depends_on_indices, vec![0]);
        assert_eq!(result.tasks[2].depends_on_indices, vec![1]);
    }

    #[test]
    fn test_materialize_creates_tasks() {
        let dir = TempDir::new().unwrap();
        let mut board = TaskBoard::open(dir.path()).unwrap();

        let text = "TASK: Step A\nDESC: First\nDEPS:\nLABELS:\n---\nTASK: Step B\nDESC: Second\nDEPS: 0\nLABELS:";
        let mut result = DecompositionResult::parse(text);
        let ids = result.materialize(&mut board, "ts", "ts-m001", None).unwrap();

        assert_eq!(ids.len(), 2);
        let t1 = board.get(&ids[0].0).unwrap();
        let t2 = board.get(&ids[1].0).unwrap();
        assert_eq!(t1.subject, "Step A");
        assert_eq!(t2.subject, "Step B");
        assert_eq!(t2.depends_on, vec![ids[0].clone()]);
        assert_eq!(t2.mission_id.as_deref(), Some("ts-m001"));
    }

    #[test]
    fn test_critical_path_priority() {
        let text = "TASK: A\nDESC: base\nDEPS:\nLABELS:\n---\nTASK: B\nDESC: mid\nDEPS: 0\nLABELS:\n---\nTASK: C\nDESC: end\nDEPS: 1\nLABELS:\n---\nTASK: D\nDESC: parallel\nDEPS: 0\nLABELS:";
        let result = DecompositionResult::parse(text);
        // Critical path: A → B → C (length 3), D branches off (length 2)
        assert_eq!(result.critical_path, vec![0, 1, 2]);
    }

    #[test]
    fn test_cycle_detection_on_materialize() {
        let dir = TempDir::new().unwrap();
        let mut board = TaskBoard::open(dir.path()).unwrap();

        let mut result = DecompositionResult {
            tasks: vec![
                DecomposedTask {
                    subject: "A".to_string(),
                    description: String::new(),
                    priority: Priority::Normal,
                    depends_on_indices: vec![1], // forward reference = cycle
                    labels: vec![],
                },
                DecomposedTask {
                    subject: "B".to_string(),
                    description: String::new(),
                    priority: Priority::Normal,
                    depends_on_indices: vec![],
                    labels: vec![],
                },
            ],
            critical_path: vec![],
            cost_usd: 0.0,
        };

        assert!(result.materialize(&mut board, "ts", "ts-m001", None).is_err());
    }

    #[test]
    fn test_mission_association() {
        let dir = TempDir::new().unwrap();
        let mut board = TaskBoard::open(dir.path()).unwrap();

        let text = "TASK: Only task\nDESC: solo\nDEPS:\nLABELS:";
        let mut result = DecompositionResult::parse(text);
        let ids = result.materialize(&mut board, "ts", "ts-m042", None).unwrap();

        let task = board.get(&ids[0].0).unwrap();
        assert_eq!(task.mission_id.as_deref(), Some("ts-m042"));
    }
}
