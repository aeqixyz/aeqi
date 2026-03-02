use anyhow::Result;
use async_trait::async_trait;
use system_core::traits::{ToolResult, ToolSpec};
use system_core::traits::Tool;
use system_tasks::{TaskBoard, Priority};
use std::path::PathBuf;
use std::sync::Mutex;

/// Tool for creating beads (tasks).
pub struct BeadsCreateTool {
    store: Mutex<TaskBoard>,
    prefix: String,
}

impl BeadsCreateTool {
    pub fn new(quests_dir: PathBuf, prefix: String) -> Result<Self> {
        let store = TaskBoard::open(&quests_dir)?;
        Ok(Self { store: Mutex::new(store), prefix })
    }
}

#[async_trait]
impl Tool for BeadsCreateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let subject = args.get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("untitled");
        let description = args.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let priority = args.get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");

        let mut store = self.store.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut bead = store.create(&self.prefix, subject)?;

        if !description.is_empty() || priority != "normal" {
            bead = store.update(&bead.id.0, |b| {
                if !description.is_empty() {
                    b.description = description.to_string();
                }
                b.priority = match priority {
                    "low" => Priority::Low,
                    "high" => Priority::High,
                    "critical" => Priority::Critical,
                    _ => Priority::Normal,
                };
            })?;
        }

        Ok(ToolResult::success(format!(
            "Created bead {} [{}] {}",
            bead.id, bead.priority, bead.subject
        )))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "quests_create".to_string(),
            description: "Create a new task (bead) with a subject and optional description/priority.".to_string(),
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

    fn name(&self) -> &str { "quests_create" }
}

/// Tool for listing ready (unblocked) beads.
pub struct BeadsReadyTool {
    store: Mutex<TaskBoard>,
}

impl BeadsReadyTool {
    pub fn new(quests_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&quests_dir)?;
        Ok(Self { store: Mutex::new(store) })
    }
}

#[async_trait]
impl Tool for BeadsReadyTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let store = self.store.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let ready = store.ready();

        if ready.is_empty() {
            return Ok(ToolResult::success("No ready work."));
        }

        let mut output = String::new();
        for bead in ready {
            output.push_str(&format!(
                "{} [{}] {} — {}\n",
                bead.id, bead.priority, bead.subject,
                if bead.description.is_empty() { "(no description)" } else { &bead.description }
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "quests_ready".to_string(),
            description: "List all unblocked tasks (beads) that are ready to work on.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str { "quests_ready" }
}

/// Tool for updating a bead's status.
pub struct BeadsUpdateTool {
    store: Mutex<TaskBoard>,
}

impl BeadsUpdateTool {
    pub fn new(quests_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&quests_dir)?;
        Ok(Self { store: Mutex::new(store) })
    }
}

#[async_trait]
impl Tool for BeadsUpdateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let status = args.get("status")
            .and_then(|v| v.as_str());
        let assignee = args.get("assignee")
            .and_then(|v| v.as_str());

        let mut store = self.store.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        let bead = store.update(id, |b| {
            if let Some(s) = status {
                b.status = match s {
                    "in_progress" => system_tasks::TaskStatus::InProgress,
                    "done" => system_tasks::TaskStatus::Done,
                    "blocked" => system_tasks::TaskStatus::Blocked,
                    "cancelled" => system_tasks::TaskStatus::Cancelled,
                    _ => system_tasks::TaskStatus::Pending,
                };
            }
            if let Some(a) = assignee {
                b.assignee = Some(a.to_string());
            }
        })?;

        Ok(ToolResult::success(format!("Updated {} [{}] {}", bead.id, bead.status, bead.subject)))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "quests_update".to_string(),
            description: "Update a bead's status or assignee.".to_string(),
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

    fn name(&self) -> &str { "quests_update" }
}

/// Tool for closing a bead.
pub struct BeadsCloseTool {
    store: Mutex<TaskBoard>,
}

impl BeadsCloseTool {
    pub fn new(quests_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&quests_dir)?;
        Ok(Self { store: Mutex::new(store) })
    }
}

#[async_trait]
impl Tool for BeadsCloseTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let reason = args.get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("completed");

        let mut store = self.store.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let bead = store.close(id, reason)?;
        Ok(ToolResult::success(format!("Closed {} — {}", bead.id, bead.subject)))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "quests_close".to_string(),
            description: "Close (complete) a bead with an optional reason.".to_string(),
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

    fn name(&self) -> &str { "quests_close" }
}

/// Tool for showing bead details.
pub struct BeadsShowTool {
    store: Mutex<TaskBoard>,
}

impl BeadsShowTool {
    pub fn new(quests_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&quests_dir)?;
        Ok(Self { store: Mutex::new(store) })
    }
}

#[async_trait]
impl Tool for BeadsShowTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;

        let store = self.store.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        if let Some(bead) = store.get(id) {
            let deps = if bead.depends_on.is_empty() {
                "none".to_string()
            } else {
                bead.depends_on.iter().map(|d| d.0.as_str()).collect::<Vec<_>>().join(", ")
            };
            let blocks = if bead.blocks.is_empty() {
                "none".to_string()
            } else {
                bead.blocks.iter().map(|b| b.0.as_str()).collect::<Vec<_>>().join(", ")
            };
            let assignee = bead.assignee.as_deref().unwrap_or("unassigned");

            let output = format!(
                "ID: {}\nSubject: {}\nStatus: {}\nPriority: {}\nAssignee: {}\nDescription: {}\nDepends on: {}\nBlocks: {}\nCreated: {}",
                bead.id, bead.subject, bead.status, bead.priority, assignee,
                if bead.description.is_empty() { "(none)" } else { &bead.description },
                deps, blocks, bead.created_at
            );
            Ok(ToolResult::success(output))
        } else {
            Ok(ToolResult::error(format!("Task not found: {id}")))
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "quests_show".to_string(),
            description: "Show detailed information about a specific bead.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Task ID to show" }
                },
                "required": ["id"]
            }),
        }
    }

    fn name(&self) -> &str { "quests_show" }
}

/// Tool for adding a dependency between beads.
pub struct BeadsDepTool {
    store: Mutex<TaskBoard>,
}

impl BeadsDepTool {
    pub fn new(quests_dir: PathBuf) -> Result<Self> {
        let store = TaskBoard::open(&quests_dir)?;
        Ok(Self { store: Mutex::new(store) })
    }
}

#[async_trait]
impl Tool for BeadsDepTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let id = args.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let depends_on = args.get("depends_on")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing depends_on"))?;

        let mut store = self.store.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        store.add_dependency(id, depends_on)?;

        Ok(ToolResult::success(format!("{id} now depends on {depends_on}")))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "quests_dep".to_string(),
            description: "Add a dependency between two beads. The first bead will be blocked until the second is closed.".to_string(),
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

    fn name(&self) -> &str { "quests_dep" }
}
