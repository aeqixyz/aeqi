//! Persistent Agent Registry — lifecycle management for long-lived agent identities.
//!
//! Each persistent agent has a stable UUID, entity-scoped memory, and can be
//! attached to a project (project-scoped) or run at root (cross-project).
//! The registry stores agent metadata in SQLite alongside the daemon's other state.
//!
//! Persistent agents are NOT running processes — they are identities that get
//! loaded into fresh sessions on demand. Their "persistence" comes from:
//! 1. Stable UUID → entity-scoped memory accumulates across sessions
//! 2. Registry metadata → survives daemon restarts
//! 3. Org chart position → project/department scoping

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// A department — a named group of agents within a project.
///
/// Departments form a hierarchy (via `parent_id`) used for escalation chains.
/// Each department may have a manager agent and belongs to an optional project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Department {
    pub id: String,
    pub name: String,
    pub project: Option<String>,
    pub manager_id: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A persistent agent identity — one record = one agent ready to go.
///
/// Created from a template with YAML frontmatter:
/// ```text
/// ---
/// name: shadow
/// display_name: "Shadow — Your Dark Butler"
/// model: anthropic/claude-sonnet-4.6
/// capabilities: [spawn_agents, spawn_projects]
/// ---
///
/// You are Shadow, the user's personal assistant...
/// ```
///
/// Frontmatter → DB columns (searchable). Body → system_prompt field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentAgent {
    /// Stable UUID — used as entity_id for memory scoping.
    pub id: String,
    /// Human-readable name (NOT unique — multiple agents can share a name).
    /// The UUID is the true identity. Name is a display label.
    pub name: String,
    /// Display name shown in UI.
    pub display_name: Option<String>,
    /// Template file this agent was created from (e.g., "shadow", "analyst").
    /// Tracks origin. Multiple agents can share the same template.
    pub template: String,
    /// The full system prompt — the agent's identity, personality, role,
    /// instructions. Stored in DB. This IS the agent.
    pub system_prompt: String,
    /// Project scope. None = root (cross-project).
    pub project: Option<String>,
    /// Foreign key to departments table. None = unassigned.
    pub department_id: Option<String>,
    /// Preferred model.
    pub model: Option<String>,
    /// Capabilities beyond normal tools.
    /// "spawn_agents" = can create persistent agents (system leader).
    /// "spawn_projects" = can create projects (system leader).
    pub capabilities: Vec<String>,
    /// Agent status.
    pub status: AgentStatus,
    pub created_at: DateTime<Utc>,
    pub last_active: Option<DateTime<Utc>>,
    pub session_count: u32,
    pub total_tokens: u64,
    // --- Visual identity for TUI ---
    /// TUI color (CSS hex like "#FFD700" or named like "gold", "crimson").
    pub color: Option<String>,
    /// ASCII avatar/emoji shown in response headers and status bar.
    /// e.g., "⚕", "🔮", "⚔", or a kaomoji face like "(◕‿◕)".
    pub avatar: Option<String>,
    /// Emotional faces shown during different states.
    /// Keys: "greeting", "thinking", "working", "error", "complete", "idle"
    pub faces: Option<std::collections::HashMap<String, String>>,
}

/// Frontmatter parsed from a template file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTemplateFrontmatter {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub project: Option<String>,
    #[serde(default)]
    pub triggers: Vec<TemplateTrigger>,
    // --- Visual identity for TUI ---
    /// TUI color (CSS hex or named).
    pub color: Option<String>,
    /// ASCII avatar/emoji for response headers.
    pub avatar: Option<String>,
    /// Emotional faces by state: greeting, thinking, working, error, complete, idle.
    #[serde(default)]
    pub faces: std::collections::HashMap<String, String>,
}

/// A trigger definition within an agent template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateTrigger {
    pub name: String,
    /// Schedule expression: cron ("0 9 * * *") or interval ("every 1h").
    pub schedule: Option<String>,
    /// One-shot timestamp (ISO 8601).
    pub at: Option<String>,
    /// Event pattern name: "task_completed", "task_failed", "tool_call_completed".
    pub event: Option<String>,
    /// Event project filter (optional, for task_completed/task_failed).
    pub event_project: Option<String>,
    /// Event tool filter (optional, for tool_call_completed).
    pub event_tool: Option<String>,
    /// Event from_agent filter (optional, for dispatch_received/channel_message).
    pub event_from: Option<String>,
    /// Event kind filter (optional, for dispatch_received).
    pub event_kind: Option<String>,
    /// Event channel filter (optional, for channel_message).
    pub event_channel: Option<String>,
    /// Cooldown in seconds for event triggers (required for event type).
    pub cooldown_secs: Option<u64>,
    /// Skill to run when triggered.
    pub skill: String,
    /// Maximum budget per execution in USD.
    pub max_budget_usd: Option<f64>,
}

/// Parse a template with YAML frontmatter into (frontmatter, system_prompt body).
pub fn parse_agent_template(content: &str) -> (AgentTemplateFrontmatter, String) {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return (AgentTemplateFrontmatter::default(), content.to_string());
    }

    if let Some(end) = trimmed[3..].find("\n---") {
        let yaml_block = trimmed[3..3 + end].trim();
        let body = trimmed[3 + end + 4..].trim();

        match serde_json::from_value::<AgentTemplateFrontmatter>(parse_simple_yaml(yaml_block)) {
            Ok(fm) => (fm, body.to_string()),
            Err(_) => (AgentTemplateFrontmatter::default(), content.to_string()),
        }
    } else {
        (AgentTemplateFrontmatter::default(), content.to_string())
    }
}

/// Minimal YAML-like parser for frontmatter key: value pairs.
/// Supports flat key: value, inline arrays [a, b], and list-of-objects
/// (indented `- key: value` blocks under a parent key like `triggers:`).
fn parse_simple_yaml(text: &str) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().to_string();
            let val = val.trim();

            if val.is_empty() {
                // Peek ahead: "  - key:" → list-of-objects, "  key:" → nested map
                let next_line = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
                let is_list = next_line.starts_with("- ");

                if is_list {
                    let mut items: Vec<serde_json::Value> = Vec::new();
                    i += 1;
                    while i < lines.len() {
                        let sub = lines[i];
                        let trimmed = sub.trim();
                        if !sub.starts_with(' ') && !sub.starts_with('\t') && !trimmed.is_empty() {
                            break;
                        }
                        if trimmed.is_empty() {
                            i += 1;
                            continue;
                        }
                        if let Some(first_kv) = trimmed.strip_prefix("- ") {
                            let mut obj = serde_json::Map::new();
                            if let Some((k, v)) = first_kv.split_once(':') {
                                let v = v.trim().trim_matches('"');
                                insert_typed_value(&mut obj, k.trim(), v);
                            }
                            i += 1;
                            while i < lines.len() {
                                let inner = lines[i].trim();
                                if inner.is_empty() {
                                    i += 1;
                                    continue;
                                }
                                if inner.starts_with("- ")
                                    || (!lines[i].starts_with(' ') && !lines[i].starts_with('\t'))
                                {
                                    break;
                                }
                                if let Some((k, v)) = inner.split_once(':') {
                                    let v = v.trim().trim_matches('"');
                                    insert_typed_value(&mut obj, k.trim(), v);
                                }
                                i += 1;
                            }
                            items.push(serde_json::Value::Object(obj));
                        } else {
                            i += 1;
                        }
                    }
                    map.insert(key, serde_json::Value::Array(items));
                } else {
                    // Nested map (e.g., `faces:` followed by `  greeting: "..."`)
                    let mut nested = serde_json::Map::new();
                    i += 1;
                    while i < lines.len() {
                        let sub = lines[i];
                        let trimmed = sub.trim();
                        if !sub.starts_with(' ') && !sub.starts_with('\t') && !trimmed.is_empty() {
                            break;
                        }
                        if trimmed.is_empty() {
                            i += 1;
                            continue;
                        }
                        if let Some((k, v)) = trimmed.split_once(':') {
                            let v = v.trim().trim_matches('"');
                            insert_typed_value(&mut nested, k.trim(), v);
                        }
                        i += 1;
                    }
                    map.insert(key, serde_json::Value::Object(nested));
                }
                continue;
            }

            let val = val.trim_matches('"');
            if val.starts_with('[') && val.ends_with(']') {
                let items: Vec<serde_json::Value> = val[1..val.len() - 1]
                    .split(',')
                    .map(|s| serde_json::Value::String(s.trim().trim_matches('"').to_string()))
                    .collect();
                map.insert(key, serde_json::Value::Array(items));
            } else {
                map.insert(key, serde_json::Value::String(val.to_string()));
            }
        }
        i += 1;
    }
    serde_json::Value::Object(map)
}

/// Insert a value into a JSON map, trying to preserve numeric types.
fn insert_typed_value(map: &mut serde_json::Map<String, serde_json::Value>, key: &str, val: &str) {
    let key = key.to_string();
    if let Ok(n) = val.parse::<u64>() {
        map.insert(key, serde_json::json!(n));
    } else if let Ok(f) = val.parse::<f64>() {
        map.insert(key, serde_json::json!(f));
    } else {
        map.insert(key, serde_json::Value::String(val.to_string()));
    }
}

/// Lifecycle status of a persistent agent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    /// Active — available for sessions.
    Active,
    /// Paused — not available but retains memory.
    Paused,
    /// Retired — soft-deleted, memory preserved but agent won't be loaded.
    Retired,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Active => write!(f, "active"),
            AgentStatus::Paused => write!(f, "paused"),
            AgentStatus::Retired => write!(f, "retired"),
        }
    }
}

/// SQLite-backed registry for persistent agents.
pub struct AgentRegistry {
    db: Arc<Mutex<Connection>>,
}

impl AgentRegistry {
    /// Open or create the registry database.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join("agents.db");
        let conn = Connection::open(&db_path)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS agents (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 display_name TEXT,
                 template TEXT NOT NULL DEFAULT '',
                 system_prompt TEXT NOT NULL DEFAULT '',
                 project TEXT,
                 department TEXT,
                 parent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
                 model TEXT,
                 capabilities TEXT NOT NULL DEFAULT '[]',
                 status TEXT NOT NULL DEFAULT 'active',
                 created_at TEXT NOT NULL,
                 last_active TEXT,
                 session_count INTEGER NOT NULL DEFAULT 0,
                 total_tokens INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_agents_project ON agents(project);
             CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
             CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);
             CREATE INDEX IF NOT EXISTS idx_agents_parent ON agents(parent_id);
             CREATE TABLE IF NOT EXISTS triggers (
                 id TEXT PRIMARY KEY,
                 agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                 name TEXT NOT NULL,
                 trigger_type TEXT NOT NULL,
                 config TEXT NOT NULL,
                 skill TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 max_budget_usd REAL,
                 created_at TEXT NOT NULL,
                 last_fired TEXT,
                 fire_count INTEGER NOT NULL DEFAULT 0,
                 total_cost_usd REAL NOT NULL DEFAULT 0.0,
                 UNIQUE(agent_id, name)
             );
             CREATE INDEX IF NOT EXISTS idx_triggers_agent ON triggers(agent_id);
             CREATE INDEX IF NOT EXISTS idx_triggers_enabled ON triggers(enabled);
             CREATE TABLE IF NOT EXISTS departments (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 project TEXT,
                 manager_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
                 parent_id TEXT REFERENCES departments(id) ON DELETE SET NULL,
                 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
             );
             CREATE INDEX IF NOT EXISTS idx_departments_project ON departments(project);
             CREATE INDEX IF NOT EXISTS idx_departments_parent ON departments(parent_id);",
        )?;

        // Step 2: Add department_id column to agents table if missing.
        {
            let has_col = conn
                .prepare("PRAGMA table_info(agents)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .any(|col| col == "department_id");
            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE agents ADD COLUMN department_id TEXT REFERENCES departments(id) ON DELETE SET NULL;",
                )?;
            }
        }

        info!(path = %db_path.display(), "agent registry opened");
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Spawn a new persistent agent from a template string (frontmatter + prompt body).
    /// Also creates any triggers defined in the template frontmatter.
    pub async fn spawn_from_template(
        &self,
        template_content: &str,
        project_override: Option<&str>,
    ) -> Result<PersistentAgent> {
        let (fm, system_prompt) = parse_agent_template(template_content);
        let template_name = fm.name.clone().unwrap_or_else(|| "custom".to_string());
        let name = fm
            .name
            .unwrap_or_else(|| format!("agent-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let triggers = fm.triggers.clone();

        let mut agent = self
            .spawn(
                &name,
                fm.display_name.as_deref(),
                &template_name,
                &system_prompt,
                project_override.or(fm.project.as_deref()),
                fm.model.as_deref(),
                &fm.capabilities,
            )
            .await?;

        // Apply visual identity from template frontmatter.
        if fm.color.is_some() || fm.avatar.is_some() || !fm.faces.is_empty() {
            agent.color = fm.color;
            agent.avatar = fm.avatar;
            agent.faces = if fm.faces.is_empty() {
                None
            } else {
                Some(fm.faces)
            };
            // Persist visual identity to DB.
            let db = self.db.lock().await;
            let faces_json = agent
                .faces
                .as_ref()
                .map(|f| serde_json::to_string(f).unwrap_or_default());
            let _ = db.execute(
                "UPDATE agents SET color = ?1, avatar = ?2, faces = ?3 WHERE id = ?4",
                rusqlite::params![agent.color, agent.avatar, faces_json, agent.id],
            );
        }

        // Create triggers from template.
        if !triggers.is_empty() {
            let trigger_store = self.trigger_store();
            for t in &triggers {
                let trigger_type = template_trigger_to_type(t)?;
                trigger_store
                    .create(&crate::trigger::NewTrigger {
                        agent_id: agent.id.clone(),
                        name: t.name.clone(),
                        trigger_type,
                        skill: t.skill.clone(),
                        max_budget_usd: t.max_budget_usd,
                    })
                    .await?;
                info!(
                    agent = %agent.name,
                    trigger = %t.name,
                    skill = %t.skill,
                    "trigger created from template"
                );
            }
        }

        Ok(agent)
    }

    /// Spawn a new persistent agent directly.
    pub async fn spawn(
        &self,
        name: &str,
        display_name: Option<&str>,
        template: &str,
        system_prompt: &str,
        project: Option<&str>,
        model: Option<&str>,
        capabilities: &[String],
    ) -> Result<PersistentAgent> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let caps_json = serde_json::to_string(capabilities)?;

        let agent = PersistentAgent {
            id: id.clone(),
            name: name.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            template: template.to_string(),
            system_prompt: system_prompt.to_string(),
            project: project.map(|s| s.to_string()),
            department_id: None,
            model: model.map(|s| s.to_string()),
            capabilities: capabilities.to_vec(),
            status: AgentStatus::Active,
            created_at: now,
            last_active: None,
            session_count: 0,
            total_tokens: 0,
            color: None,
            avatar: None,
            faces: None,
        };

        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO agents (id, name, display_name, template, system_prompt, project, model, capabilities, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                agent.id,
                agent.name,
                agent.display_name,
                agent.template,
                agent.system_prompt,
                agent.project,
                agent.model,
                caps_json,
                agent.status.to_string(),
                agent.created_at.to_rfc3339(),
            ],
        )?;

        info!(id = %agent.id, name = %agent.name, "persistent agent spawned");
        Ok(agent)
    }

    /// List all agents, optionally filtered by project and/or status.
    pub async fn list(
        &self,
        project: Option<&str>,
        status: Option<AgentStatus>,
    ) -> Result<Vec<PersistentAgent>> {
        let db = self.db.lock().await;
        let mut sql = "SELECT * FROM agents WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(p) = project {
            sql.push_str(" AND project = ?");
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(s) = status {
            sql.push_str(" AND status = ?");
            params_vec.push(Box::new(s.to_string()));
        }
        sql.push_str(" ORDER BY created_at DESC");

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = db.prepare(&sql)?;
        let agents = stmt
            .query_map(params_refs.as_slice(), |row| Ok(row_to_agent(row)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(agents)
    }

    /// Get agents by name (multiple can share a name).
    /// Returns all matches sorted by created_at descending (newest first).
    pub async fn get_by_name(&self, name: &str) -> Result<Vec<PersistentAgent>> {
        let db = self.db.lock().await;
        let mut stmt =
            db.prepare("SELECT * FROM agents WHERE name = ?1 ORDER BY created_at DESC")?;
        let agents = stmt
            .query_map(params![name], |row| Ok(row_to_agent(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    /// Get the first active agent with this name (convenience for unambiguous lookups).
    pub async fn get_active_by_name(&self, name: &str) -> Result<Option<PersistentAgent>> {
        let db = self.db.lock().await;
        let agent = db
            .query_row(
                "SELECT * FROM agents WHERE name = ?1 AND status = 'active' ORDER BY created_at DESC LIMIT 1",
                params![name],
                |row| Ok(row_to_agent(row)),
            )
            .optional()?;
        Ok(agent)
    }

    /// Get a specific agent by UUID.
    pub async fn get(&self, id: &str) -> Result<Option<PersistentAgent>> {
        let db = self.db.lock().await;
        let agent = db
            .query_row("SELECT * FROM agents WHERE id = ?1", params![id], |row| {
                Ok(row_to_agent(row))
            })
            .optional()?;
        Ok(agent)
    }

    /// Record a session for this agent (increment count, update last_active, add tokens).
    pub async fn record_session(&self, id: &str, tokens: u64) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE agents SET
                session_count = session_count + 1,
                last_active = ?1,
                total_tokens = total_tokens + ?2
             WHERE id = ?3",
            params![Utc::now().to_rfc3339(), tokens as i64, id],
        )?;
        debug!(id = %id, tokens, "agent session recorded");
        Ok(())
    }

    /// Change agent status.
    pub async fn set_status(&self, name: &str, status: AgentStatus) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE agents SET status = ?1 WHERE name = ?2",
            params![status.to_string(), name],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{name}' not found");
        }
        info!(name = %name, status = %status, "agent status updated");
        Ok(())
    }

    /// Get a TriggerStore sharing this registry's database connection.
    pub fn trigger_store(&self) -> crate::trigger::TriggerStore {
        crate::trigger::TriggerStore::new(self.db.clone())
    }

    /// List all active agents, sorted by most recently active first.
    pub async fn list_active(&self) -> Result<Vec<PersistentAgent>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT * FROM agents WHERE status = 'active' \
             ORDER BY COALESCE(last_active, created_at) DESC",
        )?;
        let agents = stmt
            .query_map([], |row| Ok(row_to_agent(row)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(agents)
    }

    pub async fn default_for_project(
        &self,
        project: Option<&str>,
    ) -> Result<Option<PersistentAgent>> {
        let db = self.db.lock().await;

        // Try project-scoped first.
        if let Some(p) = project
            && let Some(agent) = db
                .query_row(
                    "SELECT * FROM agents WHERE project = ?1 AND status = 'active' ORDER BY created_at ASC LIMIT 1",
                    params![p],
                    |row| Ok(row_to_agent(row)),
                )
                .optional()?
            {
                return Ok(Some(agent));
            }

        // Fall back to root-scoped.
        let agent = db
            .query_row(
                "SELECT * FROM agents WHERE project IS NULL AND status = 'active' ORDER BY created_at ASC LIMIT 1",
                [],
                |row| Ok(row_to_agent(row)),
            )
            .optional()?;

        Ok(agent)
    }

    // -----------------------------------------------------------------------
    // Department operations
    // -----------------------------------------------------------------------

    /// Create a new department.
    pub async fn create_department(
        &self,
        name: &str,
        project: Option<&str>,
        manager_id: Option<&str>,
        parent_id: Option<&str>,
    ) -> Result<Department> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        let dept = Department {
            id: id.clone(),
            name: name.to_string(),
            project: project.map(|s| s.to_string()),
            manager_id: manager_id.map(|s| s.to_string()),
            parent_id: parent_id.map(|s| s.to_string()),
            created_at: now,
        };

        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO departments (id, name, project, manager_id, parent_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                dept.id,
                dept.name,
                dept.project,
                dept.manager_id,
                dept.parent_id,
                dept.created_at.to_rfc3339(),
            ],
        )?;

        info!(id = %dept.id, name = %dept.name, "department created");
        Ok(dept)
    }

    /// Get a department by UUID.
    pub async fn get_department(&self, id: &str) -> Result<Option<Department>> {
        let db = self.db.lock().await;
        let dept = db
            .query_row(
                "SELECT * FROM departments WHERE id = ?1",
                params![id],
                |row| Ok(row_to_department(row)),
            )
            .optional()?;
        Ok(dept)
    }

    /// Get a department by name (and optionally project).
    pub async fn get_department_by_name(
        &self,
        name: &str,
        project: Option<&str>,
    ) -> Result<Option<Department>> {
        let db = self.db.lock().await;
        let dept = match project {
            Some(p) => db
                .query_row(
                    "SELECT * FROM departments WHERE name = ?1 AND project = ?2 LIMIT 1",
                    params![name, p],
                    |row| Ok(row_to_department(row)),
                )
                .optional()?,
            None => db
                .query_row(
                    "SELECT * FROM departments WHERE name = ?1 AND project IS NULL LIMIT 1",
                    params![name],
                    |row| Ok(row_to_department(row)),
                )
                .optional()?,
        };
        Ok(dept)
    }

    /// List departments, optionally filtered by project.
    pub async fn list_departments(&self, project: Option<&str>) -> Result<Vec<Department>> {
        let db = self.db.lock().await;
        match project {
            Some(p) => {
                let mut stmt =
                    db.prepare("SELECT * FROM departments WHERE project = ?1 ORDER BY name ASC")?;
                let depts = stmt
                    .query_map(params![p], |row| Ok(row_to_department(row)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(depts)
            }
            None => {
                let mut stmt = db.prepare("SELECT * FROM departments ORDER BY name ASC")?;
                let depts = stmt
                    .query_map([], |row| Ok(row_to_department(row)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(depts)
            }
        }
    }

    /// Set (or clear) the manager of a department.
    pub async fn set_department_manager(
        &self,
        dept_id: &str,
        manager_id: Option<&str>,
    ) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE departments SET manager_id = ?1 WHERE id = ?2",
            params![manager_id, dept_id],
        )?;
        if updated == 0 {
            anyhow::bail!("department '{dept_id}' not found");
        }
        info!(dept_id = %dept_id, manager_id = ?manager_id, "department manager updated");
        Ok(())
    }

    /// Get all agents assigned to a department.
    pub async fn department_members(&self, dept_id: &str) -> Result<Vec<PersistentAgent>> {
        let db = self.db.lock().await;
        let mut stmt =
            db.prepare("SELECT * FROM agents WHERE department_id = ?1 ORDER BY name ASC")?;
        let agents = stmt
            .query_map(params![dept_id], |row| Ok(row_to_agent(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    /// Walk the parent_id chain from a department up to the root.
    /// Returns the chain starting from the given department.
    pub async fn department_chain(&self, dept_id: &str) -> Result<Vec<Department>> {
        let mut chain = Vec::new();
        let mut current_id = Some(dept_id.to_string());

        while let Some(id) = current_id {
            match self.get_department(&id).await? {
                Some(dept) => {
                    current_id = dept.parent_id.clone();
                    chain.push(dept);
                }
                None => break,
            }
        }

        Ok(chain)
    }

    /// Assign an agent to a department (or remove from department with None).
    pub async fn set_agent_department(&self, agent_id: &str, dept_id: Option<&str>) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE agents SET department_id = ?1 WHERE id = ?2",
            params![dept_id, agent_id],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{agent_id}' not found");
        }
        info!(agent_id = %agent_id, department_id = ?dept_id, "agent department updated");
        Ok(())
    }
}

/// Convert a template trigger definition to a TriggerType.
fn template_trigger_to_type(t: &TemplateTrigger) -> Result<crate::trigger::TriggerType> {
    if let Some(ref schedule) = t.schedule {
        return Ok(crate::trigger::TriggerType::Schedule {
            expr: schedule.clone(),
        });
    }
    if let Some(ref at_str) = t.at {
        let at = chrono::DateTime::parse_from_rfc3339(at_str)
            .map_err(|e| anyhow::anyhow!("invalid 'at' timestamp: {e}"))?
            .with_timezone(&Utc);
        return Ok(crate::trigger::TriggerType::Once { at });
    }
    if let Some(ref event) = t.event {
        let cooldown_secs = t.cooldown_secs.unwrap_or(300);
        if cooldown_secs < 60 {
            anyhow::bail!("cooldown_secs must be >= 60, got {cooldown_secs}");
        }
        let pattern = match event.as_str() {
            "task_completed" => crate::trigger::EventPattern::TaskCompleted {
                project: t.event_project.clone(),
            },
            "task_failed" => crate::trigger::EventPattern::TaskFailed {
                project: t.event_project.clone(),
            },
            "tool_call_completed" => crate::trigger::EventPattern::ToolCallCompleted {
                tool: t.event_tool.clone(),
            },
            "dispatch_received" => crate::trigger::EventPattern::DispatchReceived {
                from_agent: t.event_from.clone(),
                kind: t.event_kind.clone(),
            },
            "channel_message" => crate::trigger::EventPattern::ChannelMessage {
                channel_name: t.event_channel.clone(),
                from_agent: t.event_from.clone(),
            },
            other => anyhow::bail!("unknown event pattern: {other}"),
        };
        return Ok(crate::trigger::TriggerType::Event {
            pattern,
            cooldown_secs,
        });
    }
    anyhow::bail!(
        "trigger '{}' must have one of: schedule, at, or event",
        t.name
    )
}

fn row_to_department(row: &rusqlite::Row) -> Department {
    Department {
        id: row.get("id").unwrap_or_default(),
        name: row.get("name").unwrap_or_default(),
        project: row.get("project").ok(),
        manager_id: row.get("manager_id").ok(),
        parent_id: row.get("parent_id").ok(),
        created_at: row
            .get::<_, String>("created_at")
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_default(),
    }
}

fn row_to_agent(row: &rusqlite::Row) -> PersistentAgent {
    let status_str: String = row.get("status").unwrap_or_default();
    let status = match status_str.as_str() {
        "paused" => AgentStatus::Paused,
        "retired" => AgentStatus::Retired,
        _ => AgentStatus::Active,
    };

    let caps_str: String = row.get("capabilities").unwrap_or_else(|_| "[]".to_string());
    let capabilities: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();

    PersistentAgent {
        id: row.get("id").unwrap_or_default(),
        name: row.get("name").unwrap_or_default(),
        display_name: row.get("display_name").ok(),
        template: row.get("template").unwrap_or_default(),
        system_prompt: row.get("system_prompt").unwrap_or_default(),
        project: row.get("project").ok(),
        department_id: row.get("department_id").ok(),
        model: row.get("model").ok(),
        capabilities,
        status,
        created_at: row
            .get::<_, String>("created_at")
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_default(),
        last_active: row
            .get::<_, String>("last_active")
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)),
        session_count: row.get("session_count").unwrap_or(0),
        total_tokens: row.get::<_, i64>("total_tokens").unwrap_or(0) as u64,
        // Visual identity — read from DB if columns exist, fallback to None.
        color: row.get("color").ok(),
        avatar: row.get("avatar").ok(),
        faces: row
            .get::<_, String>("faces")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_registry() -> AgentRegistry {
        let dir = tempfile::tempdir().unwrap();
        AgentRegistry::open(dir.path()).unwrap()
    }

    #[tokio::test]
    async fn spawn_and_get() {
        let reg = test_registry().await;
        let agent = reg
            .spawn(
                "shadow",
                Some("Shadow"),
                "shadow",
                "You are Shadow.",
                None,
                Some("claude-sonnet-4.6"),
                &["spawn_agents".into()],
            )
            .await
            .unwrap();

        assert_eq!(agent.name, "shadow");
        assert_eq!(agent.system_prompt, "You are Shadow.");
        assert_eq!(agent.capabilities, vec!["spawn_agents"]);
        assert!(agent.project.is_none());
        assert_eq!(agent.status, AgentStatus::Active);
        assert_eq!(agent.session_count, 0);

        let fetched = reg.get_by_name("shadow").await.unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].id, agent.id);
    }

    #[tokio::test]
    async fn spawn_project_scoped() {
        let reg = test_registry().await;
        let agent = reg
            .spawn(
                "sigil-lead",
                None,
                "shadow",
                "Lead for sigil.",
                Some("sigil"),
                None,
                &[],
            )
            .await
            .unwrap();

        assert_eq!(agent.project.as_deref(), Some("sigil"));

        let list = reg.list(Some("sigil"), None).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "sigil-lead");

        let other = reg.list(Some("algostaking"), None).await.unwrap();
        assert!(other.is_empty());
    }

    #[tokio::test]
    async fn record_session_updates_stats() {
        let reg = test_registry().await;
        let agent = reg
            .spawn(
                "test-agent",
                None,
                "researcher",
                "Test agent.",
                None,
                None,
                &[],
            )
            .await
            .unwrap();

        reg.record_session(&agent.id, 5000).await.unwrap();
        reg.record_session(&agent.id, 3000).await.unwrap();

        let updated = reg.get(&agent.id).await.unwrap().unwrap();
        assert_eq!(updated.session_count, 2);
        assert_eq!(updated.total_tokens, 8000);
        assert!(updated.last_active.is_some());
    }

    #[tokio::test]
    async fn status_lifecycle() {
        let reg = test_registry().await;
        reg.spawn(
            "lifecycle",
            None,
            "shadow",
            "Lifecycle test.",
            None,
            None,
            &[],
        )
        .await
        .unwrap();

        reg.set_status("lifecycle", AgentStatus::Paused)
            .await
            .unwrap();
        let agents = reg.get_by_name("lifecycle").await.unwrap();
        assert_eq!(agents[0].status, AgentStatus::Paused);

        reg.set_status("lifecycle", AgentStatus::Retired)
            .await
            .unwrap();
        let agents = reg.get_by_name("lifecycle").await.unwrap();
        assert_eq!(agents[0].status, AgentStatus::Retired);

        // Active filter should not return retired agents.
        let active = reg.list(None, Some(AgentStatus::Active)).await.unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn default_for_project() {
        let reg = test_registry().await;
        reg.spawn(
            "root-shadow",
            None,
            "shadow",
            "Root agent.",
            None,
            None,
            &[],
        )
        .await
        .unwrap();
        reg.spawn(
            "sigil-lead",
            None,
            "shadow",
            "Sigil lead.",
            Some("sigil"),
            None,
            &[],
        )
        .await
        .unwrap();

        // Project-scoped takes priority.
        let default = reg
            .default_for_project(Some("sigil"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(default.name, "sigil-lead");

        // Unknown project falls back to root.
        let default = reg
            .default_for_project(Some("unknown"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(default.name, "root-shadow");

        // No project → root.
        let default = reg.default_for_project(None).await.unwrap().unwrap();
        assert_eq!(default.name, "root-shadow");
    }

    #[tokio::test]
    async fn duplicate_names_allowed() {
        let reg = test_registry().await;
        let agent1 = reg
            .spawn(
                "shadow",
                None,
                "shadow",
                "First shadow.",
                None,
                None,
                &[],
            )
            .await
            .unwrap();
        let agent2 = reg
            .spawn(
                "shadow",
                None,
                "shadow",
                "Second shadow.",
                None,
                None,
                &[],
            )
            .await
            .unwrap();
        // Same name, different UUIDs.
        assert_ne!(agent1.id, agent2.id);
        assert_eq!(agent1.name, agent2.name);
        // get_by_name returns both.
        let all = reg.get_by_name("shadow").await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn spawn_from_template_parses_frontmatter() {
        let reg = test_registry().await;
        let template = r#"---
name: shadow
display_name: "Shadow — Your Dark Butler"
model: anthropic/claude-sonnet-4.6
capabilities: [spawn_agents, spawn_projects]
---

You are Shadow, the user's personal assistant.
You learn everything about the user aggressively.
"#;
        let agent = reg.spawn_from_template(template, None).await.unwrap();
        assert_eq!(agent.name, "shadow");
        assert_eq!(
            agent.display_name.as_deref(),
            Some("Shadow — Your Dark Butler")
        );
        assert_eq!(agent.model.as_deref(), Some("anthropic/claude-sonnet-4.6"));
        assert_eq!(agent.capabilities, vec!["spawn_agents", "spawn_projects"]);
        assert!(agent.system_prompt.contains("personal assistant"));
        assert!(agent.project.is_none()); // Root scope
    }

    #[tokio::test]
    async fn spawn_from_template_creates_triggers() {
        let reg = test_registry().await;
        let template = r#"---
name: watcher
model: anthropic/claude-sonnet-4.6
capabilities: [manage_triggers]
triggers:
  - name: morning-brief
    schedule: "0 9 * * *"
    skill: morning-brief
    max_budget_usd: 0.50
  - name: failure-watch
    event: task_failed
    cooldown_secs: 300
    skill: failure-triage
    max_budget_usd: 1.00
---

You are a monitoring agent.
"#;
        let agent = reg.spawn_from_template(template, None).await.unwrap();
        assert_eq!(agent.name, "watcher");

        // Verify triggers were created.
        let trigger_store = reg.trigger_store();
        let triggers = trigger_store.list_for_agent(&agent.id).await.unwrap();
        assert_eq!(triggers.len(), 2);

        let brief = triggers.iter().find(|t| t.name == "morning-brief").unwrap();
        assert_eq!(brief.skill, "morning-brief");
        assert!(
            matches!(brief.trigger_type, crate::trigger::TriggerType::Schedule { ref expr } if expr == "0 9 * * *")
        );
        assert_eq!(brief.max_budget_usd, Some(0.50));

        let watch = triggers.iter().find(|t| t.name == "failure-watch").unwrap();
        assert_eq!(watch.skill, "failure-triage");
        assert!(matches!(
            watch.trigger_type,
            crate::trigger::TriggerType::Event {
                cooldown_secs: 300,
                ..
            }
        ));
    }

    // -----------------------------------------------------------------------
    // Department tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn create_department() {
        let reg = test_registry().await;
        let dept = reg
            .create_department("engineering", Some("sigil"), None, None)
            .await
            .unwrap();

        assert_eq!(dept.name, "engineering");
        assert_eq!(dept.project.as_deref(), Some("sigil"));
        assert!(dept.manager_id.is_none());
        assert!(dept.parent_id.is_none());
        assert!(!dept.id.is_empty());

        // Fetch by ID.
        let fetched = reg.get_department(&dept.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, dept.id);
        assert_eq!(fetched.name, "engineering");
    }

    #[tokio::test]
    async fn list_departments_by_project() {
        let reg = test_registry().await;
        reg.create_department("engineering", Some("sigil"), None, None)
            .await
            .unwrap();
        reg.create_department("design", Some("sigil"), None, None)
            .await
            .unwrap();
        reg.create_department("ops", Some("other"), None, None)
            .await
            .unwrap();
        reg.create_department("global", None, None, None)
            .await
            .unwrap();

        let sigil_depts = reg.list_departments(Some("sigil")).await.unwrap();
        assert_eq!(sigil_depts.len(), 2);
        let names: Vec<&str> = sigil_depts.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"engineering"));
        assert!(names.contains(&"design"));

        let other_depts = reg.list_departments(Some("other")).await.unwrap();
        assert_eq!(other_depts.len(), 1);
        assert_eq!(other_depts[0].name, "ops");

        // None returns all departments.
        let all = reg.list_departments(None).await.unwrap();
        assert_eq!(all.len(), 4);
    }

    #[tokio::test]
    async fn set_and_change_department_manager() {
        let reg = test_registry().await;
        let agent = reg
            .spawn("lead", None, "t", "Lead.", None, None, &[])
            .await
            .unwrap();
        let agent2 = reg
            .spawn("lead2", None, "t", "Lead2.", None, None, &[])
            .await
            .unwrap();

        let dept = reg
            .create_department("engineering", None, None, None)
            .await
            .unwrap();
        assert!(dept.manager_id.is_none());

        // Set manager.
        reg.set_department_manager(&dept.id, Some(&agent.id))
            .await
            .unwrap();
        let fetched = reg.get_department(&dept.id).await.unwrap().unwrap();
        assert_eq!(fetched.manager_id.as_deref(), Some(agent.id.as_str()));

        // Change manager.
        reg.set_department_manager(&dept.id, Some(&agent2.id))
            .await
            .unwrap();
        let fetched = reg.get_department(&dept.id).await.unwrap().unwrap();
        assert_eq!(fetched.manager_id.as_deref(), Some(agent2.id.as_str()));

        // Clear manager.
        reg.set_department_manager(&dept.id, None).await.unwrap();
        let fetched = reg.get_department(&dept.id).await.unwrap().unwrap();
        assert!(fetched.manager_id.is_none());
    }

    #[tokio::test]
    async fn department_members_returns_correct_agents() {
        let reg = test_registry().await;
        let dept = reg
            .create_department("engineering", Some("sigil"), None, None)
            .await
            .unwrap();
        let dept2 = reg
            .create_department("design", Some("sigil"), None, None)
            .await
            .unwrap();

        let a1 = reg
            .spawn(
                "eng1",
                None,
                "t",
                "Eng1.",
                Some("sigil"),
                None,
                &[],
            )
            .await
            .unwrap();
        let a2 = reg
            .spawn(
                "eng2",
                None,
                "t",
                "Eng2.",
                Some("sigil"),
                None,
                &[],
            )
            .await
            .unwrap();
        let a3 = reg
            .spawn(
                "designer",
                None,
                "t",
                "Des.",
                Some("sigil"),
                None,
                &[],
            )
            .await
            .unwrap();

        reg.set_agent_department(&a1.id, Some(&dept.id))
            .await
            .unwrap();
        reg.set_agent_department(&a2.id, Some(&dept.id))
            .await
            .unwrap();
        reg.set_agent_department(&a3.id, Some(&dept2.id))
            .await
            .unwrap();

        let members = reg.department_members(&dept.id).await.unwrap();
        assert_eq!(members.len(), 2);
        let names: Vec<&str> = members.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"eng1"));
        assert!(names.contains(&"eng2"));

        let design_members = reg.department_members(&dept2.id).await.unwrap();
        assert_eq!(design_members.len(), 1);
        assert_eq!(design_members[0].name, "designer");
    }

    #[tokio::test]
    async fn department_chain_walks_hierarchy() {
        let reg = test_registry().await;

        // Create hierarchy: company → engineering → backend
        let company = reg
            .create_department("company", None, None, None)
            .await
            .unwrap();
        let engineering = reg
            .create_department("engineering", None, None, Some(&company.id))
            .await
            .unwrap();
        let backend = reg
            .create_department("backend", None, None, Some(&engineering.id))
            .await
            .unwrap();

        // Chain from backend should be: backend → engineering → company
        let chain = reg.department_chain(&backend.id).await.unwrap();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].name, "backend");
        assert_eq!(chain[1].name, "engineering");
        assert_eq!(chain[2].name, "company");

        // Chain from engineering: engineering → company
        let chain = reg.department_chain(&engineering.id).await.unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].name, "engineering");
        assert_eq!(chain[1].name, "company");

        // Chain from company (root): just company
        let chain = reg.department_chain(&company.id).await.unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name, "company");
    }

    #[tokio::test]
    async fn set_agent_department_moves_between_departments() {
        let reg = test_registry().await;
        let dept_a = reg
            .create_department("alpha", None, None, None)
            .await
            .unwrap();
        let dept_b = reg
            .create_department("beta", None, None, None)
            .await
            .unwrap();

        let agent = reg
            .spawn("mover", None, "t", "Mover.", None, None, &[])
            .await
            .unwrap();

        // Initially no department.
        let fetched = reg.get(&agent.id).await.unwrap().unwrap();
        assert!(fetched.department_id.is_none());
        assert!(reg.department_members(&dept_a.id).await.unwrap().is_empty());

        // Assign to dept_a.
        reg.set_agent_department(&agent.id, Some(&dept_a.id))
            .await
            .unwrap();
        let members_a = reg.department_members(&dept_a.id).await.unwrap();
        assert_eq!(members_a.len(), 1);
        assert_eq!(members_a[0].id, agent.id);
        assert!(reg.department_members(&dept_b.id).await.unwrap().is_empty());

        // Move to dept_b.
        reg.set_agent_department(&agent.id, Some(&dept_b.id))
            .await
            .unwrap();
        assert!(reg.department_members(&dept_a.id).await.unwrap().is_empty());
        let members_b = reg.department_members(&dept_b.id).await.unwrap();
        assert_eq!(members_b.len(), 1);
        assert_eq!(members_b[0].id, agent.id);

        // Remove from department.
        reg.set_agent_department(&agent.id, None).await.unwrap();
        assert!(reg.department_members(&dept_b.id).await.unwrap().is_empty());
        let fetched = reg.get(&agent.id).await.unwrap().unwrap();
        assert!(fetched.department_id.is_none());
    }

    #[tokio::test]
    async fn get_department_by_name() {
        let reg = test_registry().await;
        reg.create_department("engineering", Some("sigil"), None, None)
            .await
            .unwrap();
        reg.create_department("engineering", Some("other"), None, None)
            .await
            .unwrap();

        let dept = reg
            .get_department_by_name("engineering", Some("sigil"))
            .await
            .unwrap();
        assert!(dept.is_some());
        assert_eq!(dept.unwrap().project.as_deref(), Some("sigil"));

        let dept = reg
            .get_department_by_name("engineering", Some("other"))
            .await
            .unwrap();
        assert!(dept.is_some());
        assert_eq!(dept.unwrap().project.as_deref(), Some("other"));

        let nope = reg
            .get_department_by_name("nonexistent", Some("sigil"))
            .await
            .unwrap();
        assert!(nope.is_none());
    }
}
