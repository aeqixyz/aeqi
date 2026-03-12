use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::Tool;
use sigil_core::traits::{ToolResult, ToolSpec};
use sigil_tasks::{Priority, TaskBoard};
use std::path::PathBuf;
use std::sync::Mutex;

/// Tool for creating tasks.
pub struct TaskCreateTool {
    store: Mutex<TaskBoard>,
    prefix: String,
}

impl TaskCreateTool {
    pub fn new(tasks_dir: PathBuf, prefix: String) -> Result<Self> {
        let store = TaskBoard::open(&tasks_dir)?;
        Ok(Self {
            store: Mutex::new(store),
            prefix,
        })
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let subject = args
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("untitled");
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let priority = args
            .get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");

        let mut store = self
            .store
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut task = store.create(&self.prefix, subject)?;

        if !description.is_empty() || priority != "normal" {
            task = store.update(&task.id.0, |t| {
                if !description.is_empty() {
                    t.description = description.to_string();
                }
                t.priority = match priority {
                    "low" => Priority::Low,
                    "high" => Priority::High,
                    "critical" => Priority::Critical,
                    _ => Priority::Normal,
                };
            })?;
        }

        Ok(ToolResult::success(format!(
            "Created task {} [{}] {}",
            task.id, task.priority, task.subject
        )))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_create".to_string(),
            description: "Create a new task with a subject and optional description/priority."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Task title" },
                    "description": { "type": "string", "description": "Detailed description" },
                    "priority": { "type": "string", "enum": ["low", "normal", "high", "critical"], "default": "normal" }
                },
                "required": ["subject"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_create"
    }
}

/// Tool for listing ready (unblocked) tasks.
pub struct TaskReadyTool {
    store: Mutex<TaskBoard>,
}

impl TaskReadyTool {
    pub fn new(tasks_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&tasks_dir)?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

#[async_trait]
impl Tool for TaskReadyTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let store = self
            .store
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let ready = store.ready();

        if ready.is_empty() {
            return Ok(ToolResult::success("No ready work."));
        }

        let mut output = String::new();
        for task in ready {
            output.push_str(&format!(
                "{} [{}] {} — {}\n",
                task.id,
                task.priority,
                task.subject,
                if task.description.is_empty() {
                    "(no description)"
                } else {
                    &task.description
                }
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_ready".to_string(),
            description: "List all unblocked tasks that are ready to work on.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str {
        "task_ready"
    }
}

/// Tool for updating a task's status.
pub struct TaskUpdateTool {
    store: Mutex<TaskBoard>,
}

impl TaskUpdateTool {
    pub fn new(tasks_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&tasks_dir)?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let status = args.get("status").and_then(|v| v.as_str());
        let assignee = args.get("assignee").and_then(|v| v.as_str());

        let mut store = self
            .store
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        let task = store.update(id, |t| {
            if let Some(s) = status {
                t.status = match s {
                    "in_progress" => sigil_tasks::TaskStatus::InProgress,
                    "done" => sigil_tasks::TaskStatus::Done,
                    "blocked" => sigil_tasks::TaskStatus::Blocked,
                    "cancelled" => sigil_tasks::TaskStatus::Cancelled,
                    _ => sigil_tasks::TaskStatus::Pending,
                };
            }
            if let Some(a) = assignee {
                t.assignee = Some(a.to_string());
            }
        })?;

        Ok(ToolResult::success(format!(
            "Updated {} [{}] {}",
            task.id, task.status, task.subject
        )))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_update".to_string(),
            description: "Update a task's status or assignee.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Task ID (e.g. as-001)" },
                    "status": { "type": "string", "enum": ["pending", "in_progress", "done", "blocked", "cancelled"] },
                    "assignee": { "type": "string", "description": "Agent name to assign" }
                },
                "required": ["id"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_update"
    }
}

/// Tool for closing a task.
pub struct TaskCloseTool {
    store: Mutex<TaskBoard>,
}

impl TaskCloseTool {
    pub fn new(tasks_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&tasks_dir)?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

#[async_trait]
impl Tool for TaskCloseTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("completed");

        let mut store = self
            .store
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let task = store.close(id, reason)?;
        Ok(ToolResult::success(format!(
            "Closed {} — {}",
            task.id, task.subject
        )))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_close".to_string(),
            description: "Close (complete) a task with an optional reason.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Task ID to close" },
                    "reason": { "type": "string", "description": "Completion reason", "default": "completed" }
                },
                "required": ["id"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_close"
    }
}

/// Tool for showing task details.
pub struct TaskShowTool {
    store: Mutex<TaskBoard>,
}

impl TaskShowTool {
    pub fn new(tasks_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&tasks_dir)?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

#[async_trait]
impl Tool for TaskShowTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;

        let store = self
            .store
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        if let Some(task) = store.get(id) {
            let deps = if task.depends_on.is_empty() {
                "none".to_string()
            } else {
                task.depends_on
                    .iter()
                    .map(|d| d.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let blocks = if task.blocks.is_empty() {
                "none".to_string()
            } else {
                task.blocks
                    .iter()
                    .map(|b| b.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let assignee = task.assignee.as_deref().unwrap_or("unassigned");

            let output = format!(
                "ID: {}\nSubject: {}\nStatus: {}\nPriority: {}\nAssignee: {}\nDescription: {}\nDepends on: {}\nBlocks: {}\nCreated: {}",
                task.id,
                task.subject,
                task.status,
                task.priority,
                assignee,
                if task.description.is_empty() {
                    "(none)"
                } else {
                    &task.description
                },
                deps,
                blocks,
                task.created_at
            );
            Ok(ToolResult::success(output))
        } else {
            Ok(ToolResult::error(format!("Task not found: {id}")))
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_show".to_string(),
            description: "Show detailed information about a specific task.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Task ID to show" }
                },
                "required": ["id"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_show"
    }
}

/// Tool for adding a dependency between tasks.
pub struct TaskDepTool {
    store: Mutex<TaskBoard>,
}

impl TaskDepTool {
    pub fn new(tasks_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&tasks_dir)?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

#[async_trait]
impl Tool for TaskDepTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let depends_on = args
            .get("depends_on")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing depends_on"))?;

        let mut store = self
            .store
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        store.add_dependency(id, depends_on)?;

        Ok(ToolResult::success(format!(
            "{id} now depends on {depends_on}"
        )))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "task_dep".to_string(),
            description: "Add a dependency between two tasks. The first task will be blocked until the second is closed.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Task that will be blocked" },
                    "depends_on": { "type": "string", "description": "Task that must complete first" }
                },
                "required": ["id", "depends_on"]
            }),
        }
    }

    fn name(&self) -> &str {
        "task_dep"
    }
}
