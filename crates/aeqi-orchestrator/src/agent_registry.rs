//! Agent Registry — the unified agent tree.
//!
//! Everything is an agent. A "company" is an agent. A "department" is an agent.
//! A "worker" is an agent. The root agent (parent_id IS NULL) is the user's
//! workspace — the single point of contact. Structure is emergent, not typed.
//!
//! The agent tree IS the process tree:
//! - Spawn = create a child agent
//! - Delegate = parent→child or sibling→sibling message passing
//! - Memory = walk parent_id chain (self → parent → grandparent → root)
//! - Identity = per-agent (system_prompt, model, capabilities)
//!
//! Persistent agents are NOT running processes — they are identities that get
//! loaded into fresh sessions on demand. Their "persistence" comes from:
//! 1. Stable UUID → entity-scoped memory accumulates across sessions
//! 2. Registry metadata → survives daemon restarts
//! 3. Tree position → parent_id chain for memory scoping and delegation

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// A persistent agent identity — one record = one node in the agent tree.
///
/// Created from a template with YAML frontmatter:
/// ```text
/// ---
/// name: shadow
/// display_name: "Shadow — Your Dark Butler"
/// model: anthropic/claude-sonnet-4.6
/// capabilities: [spawn_agents]
/// ---
///
/// You are Shadow, the user's personal assistant...
/// ```
///
/// Frontmatter → DB columns (searchable). Body → system_prompt field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Stable UUID — the true identity. Used for memory scoping, delegation, everything.
    pub id: String,
    /// Human-readable name (NOT unique — multiple agents can share a name).
    pub name: String,
    /// Display name shown in UI.
    pub display_name: Option<String>,
    /// Template file this agent was created from (e.g., "shadow", "analyst").
    pub template: String,
    /// The full system prompt — the agent's identity, personality, role, instructions.
    pub system_prompt: String,
    /// Parent agent UUID. None = root agent (the user's workspace).
    pub parent_id: Option<String>,
    /// Preferred model. None = inherit from parent.
    pub model: Option<String>,
    /// Capabilities beyond normal tools.
    /// "spawn_agents" = can create child agents.
    pub capabilities: Vec<String>,
    /// Agent status.
    pub status: AgentStatus,
    pub created_at: DateTime<Utc>,
    pub last_active: Option<DateTime<Utc>>,
    pub session_count: u32,
    pub total_tokens: u64,
    // --- Visual identity ---
    pub color: Option<String>,
    pub avatar: Option<String>,
    /// Emotional faces shown during different states.
    pub faces: Option<std::collections::HashMap<String, String>>,
    /// Maximum concurrent workers for this agent (default: 1).
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    /// Current session ID.
    #[serde(default)]
    pub session_id: Option<String>,
    // --- Operational fields (the "agent OS" columns) ---
    /// Working directory for this agent (repo path). None = inherit from parent.
    #[serde(default)]
    pub workdir: Option<String>,
    /// Budget in USD. None = inherit from parent's budget_policies.
    #[serde(default)]
    pub budget_usd: Option<f64>,
    /// Execution mode: "agent" (native loop) or "claude_code" (CLI delegation).
    #[serde(default)]
    pub execution_mode: Option<String>,
    /// Quest ID prefix (e.g., "cto" for the CTO agent). None = derived from name.
    #[serde(default, alias = "task_prefix")]
    pub quest_prefix: Option<String>,
    /// Worker timeout in seconds. None = inherit from parent or use global default.
    #[serde(default)]
    pub worker_timeout_secs: Option<u64>,
    /// Ordered prompt entries — replaces system_prompt, primers, skills.
    #[serde(default)]
    pub prompts: Vec<aeqi_core::PromptEntry>,
}

fn default_max_concurrent() -> u32 {
    1
}

/// Frontmatter parsed from a template file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTemplateFrontmatter {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Parent agent name — resolved to parent_id at spawn time.
    pub parent: Option<String>,
    #[serde(default)]
    pub triggers: Vec<TemplateTrigger>,
    // --- Visual identity ---
    pub color: Option<String>,
    pub avatar: Option<String>,
    #[serde(default)]
    pub faces: std::collections::HashMap<String, String>,
}

/// A trigger definition within an agent template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateTrigger {
    pub name: String,
    pub schedule: Option<String>,
    pub at: Option<String>,
    pub event: Option<String>,
    pub event_project: Option<String>,
    pub event_tool: Option<String>,
    pub event_from: Option<String>,
    pub event_to: Option<String>,
    pub event_kind: Option<String>,
    pub event_channel: Option<String>,
    pub cooldown_secs: Option<u64>,
    pub skill: String,
    pub max_budget_usd: Option<f64>,
}

/// Parse a template with YAML frontmatter into (frontmatter, system_prompt body).
pub fn parse_agent_template(content: &str) -> (AgentTemplateFrontmatter, String) {
    match aeqi_core::frontmatter::load_frontmatter::<AgentTemplateFrontmatter>(content) {
        Ok((fm, body)) => (fm, body),
        Err(_) => (AgentTemplateFrontmatter::default(), content.to_string()),
    }
}

/// Lifecycle status of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Active,
    Paused,
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

/// A company record — business identity stored in the companies table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyRecord {
    pub name: String,
    pub display_name: Option<String>,
    pub prefix: String,
    pub tagline: Option<String>,
    pub logo_url: Option<String>,
    pub primer: Option<String>,
    pub repo: Option<String>,
    pub model: Option<String>,
    pub max_workers: u32,
    pub execution_mode: String,
    pub worker_timeout_secs: u64,
    pub worktree_root: Option<String>,
    pub max_turns: Option<u32>,
    pub max_budget_usd: Option<f64>,
    pub max_cost_per_day_usd: Option<f64>,
    pub source: String,
    pub agent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// SQLite-backed registry — the single source of truth for the agent tree.
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
                 parent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
                 model TEXT,
                 capabilities TEXT NOT NULL DEFAULT '[]',
                 status TEXT NOT NULL DEFAULT 'active',
                 created_at TEXT NOT NULL,
                 last_active TEXT,
                 session_count INTEGER NOT NULL DEFAULT 0,
                 total_tokens INTEGER NOT NULL DEFAULT 0,
                 color TEXT,
                 avatar TEXT,
                 faces TEXT,
                 max_concurrent INTEGER NOT NULL DEFAULT 1,
                 session_id TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_agents_parent ON agents(parent_id);
             CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
             CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);

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

             CREATE TABLE IF NOT EXISTS budget_policies (
                 id TEXT PRIMARY KEY,
                 agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                 window TEXT NOT NULL,
                 amount_usd REAL NOT NULL,
                 warn_pct REAL DEFAULT 0.8,
                 hard_stop INTEGER DEFAULT 1,
                 paused INTEGER DEFAULT 0,
                 created_at TEXT NOT NULL
             );

             CREATE TABLE IF NOT EXISTS approvals (
                 id TEXT PRIMARY KEY,
                 agent_id TEXT NOT NULL,
                 task_id TEXT,
                 request_type TEXT NOT NULL,
                 payload_json TEXT NOT NULL,
                 status TEXT NOT NULL DEFAULT 'pending',
                 decided_by TEXT,
                 decision_note TEXT,
                 created_at TEXT NOT NULL,
                 decided_at TEXT
             );",
        )?;

        // Idempotent migration: add agent OS columns.
        let agent_columns = [
            ("workdir", "TEXT"),
            ("budget_usd", "REAL"),
            ("execution_mode", "TEXT"),
            ("task_prefix", "TEXT"),
            ("worker_timeout_secs", "INTEGER"),
            ("prompts", "TEXT DEFAULT '[]'"),
        ];
        for (col, typ) in &agent_columns {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(agents)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .any(|c| c == *col);
            if !has_col {
                conn.execute_batch(&format!("ALTER TABLE agents ADD COLUMN {col} {typ};"))?;
            }
        }

        // Create unified tasks table (replaces per-project JSONL TaskBoards).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS quests (
                 id TEXT PRIMARY KEY,
                 subject TEXT NOT NULL,
                 description TEXT NOT NULL DEFAULT '',
                 status TEXT NOT NULL DEFAULT 'pending',
                 priority TEXT NOT NULL DEFAULT 'normal',
                 assignee TEXT,
                 agent_id TEXT REFERENCES agents(id),
                 skill TEXT,
                 labels TEXT NOT NULL DEFAULT '[]',
                 retry_count INTEGER NOT NULL DEFAULT 0,
                 checkpoints TEXT NOT NULL DEFAULT '[]',
                 metadata TEXT NOT NULL DEFAULT '{}',
                 depends_on TEXT NOT NULL DEFAULT '[]',
                 blocks TEXT NOT NULL DEFAULT '[]',
                 acceptance_criteria TEXT,
                 locked_by TEXT,
                 locked_at TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT,
                 closed_at TEXT,
                 closed_reason TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_quests_status ON quests(status);
             CREATE INDEX IF NOT EXISTS idx_quests_agent ON quests(agent_id);
             CREATE INDEX IF NOT EXISTS idx_quests_assignee ON quests(assignee);
             CREATE INDEX IF NOT EXISTS idx_quests_created ON quests(created_at);

             CREATE TABLE IF NOT EXISTS quest_sequences (
                 prefix TEXT PRIMARY KEY,
                 next_seq INTEGER NOT NULL DEFAULT 1
             );",
        )?;

        // Idempotent migration: add prompts column to tasks table.
        let has_task_prompts: bool = conn
            .prepare("PRAGMA table_info(quests)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|col| col == "prompts");
        if !has_task_prompts {
            conn.execute_batch("ALTER TABLE quests ADD COLUMN prompts TEXT DEFAULT '[]';")?;
        }

        // Idempotent migration: add public_id column for webhook triggers.
        let has_public_id: bool = conn
            .prepare("PRAGMA table_info(triggers)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|col| col == "public_id");
        if !has_public_id {
            conn.execute_batch(
                "ALTER TABLE triggers ADD COLUMN public_id TEXT;
                 CREATE INDEX IF NOT EXISTS idx_triggers_public_id ON triggers(public_id);",
            )?;
            let mut stmt =
                conn.prepare("SELECT id, config FROM triggers WHERE trigger_type = 'webhook'")?;
            let rows: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            for (id, config_json) in rows {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&config_json)
                    && let Some(pid) = v.get("public_id").and_then(|p| p.as_str())
                {
                    conn.execute(
                        "UPDATE triggers SET public_id = ?1 WHERE id = ?2",
                        rusqlite::params![pid, id],
                    )?;
                }
            }
        } else {
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_triggers_public_id ON triggers(public_id);",
            )?;
        }

        // Idempotent migration: add prompts column to triggers table.
        let has_trigger_prompts: bool = conn
            .prepare("PRAGMA table_info(triggers)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|col| col == "prompts");
        if !has_trigger_prompts {
            conn.execute_batch("ALTER TABLE triggers ADD COLUMN prompts TEXT DEFAULT '[]';")?;
        }

        // Create companies table (identity + config, separate from agent tree).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS companies (
                 name TEXT PRIMARY KEY,
                 display_name TEXT,
                 prefix TEXT NOT NULL,
                 tagline TEXT,
                 logo_url TEXT,
                 primer TEXT,
                 repo TEXT,
                 model TEXT,
                 max_workers INTEGER NOT NULL DEFAULT 2,
                 execution_mode TEXT NOT NULL DEFAULT 'agent',
                 worker_timeout_secs INTEGER NOT NULL DEFAULT 1800,
                 worktree_root TEXT,
                 max_turns INTEGER DEFAULT 25,
                 max_budget_usd REAL,
                 max_cost_per_day_usd REAL,
                 source TEXT NOT NULL DEFAULT 'api',
                 agent_id TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );",
        )?;

        // Create events table (unified event store).
        crate::event_store::EventStore::create_tables(&conn)?;

        // Create session/conversation tables (unified session store).
        crate::session_store::SessionStore::create_tables(&conn)?;

        // Backfill: populate prompts from system_prompt for existing agents.
        conn.execute_batch(
            "UPDATE agents SET prompts = json_array(json_object(
                'content', system_prompt,
                'position', 'system',
                'scope', 'self'
             ))
             WHERE (prompts IS NULL OR prompts = '[]') AND system_prompt != '';",
        )?;

        info!(path = %db_path.display(), "agent registry opened");
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    // -----------------------------------------------------------------------
    // Core CRUD
    // -----------------------------------------------------------------------

    /// Spawn a new agent from a template string (frontmatter + prompt body).
    pub async fn spawn_from_template(
        &self,
        template_content: &str,
        parent_id: Option<&str>,
    ) -> Result<Agent> {
        let (fm, system_prompt) = parse_agent_template(template_content);
        let template_name = fm.name.clone().unwrap_or_else(|| "custom".to_string());
        let name = fm
            .name
            .unwrap_or_else(|| format!("agent-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let triggers = fm.triggers.clone();

        // Resolve parent: explicit parent_id > frontmatter parent name > None (root).
        let resolved_parent = if parent_id.is_some() {
            parent_id.map(|s| s.to_string())
        } else if let Some(ref parent_name) = fm.parent {
            self.get_active_by_name(parent_name).await?.map(|a| a.id)
        } else {
            None
        };

        let mut agent = self
            .spawn(
                &name,
                fm.display_name.as_deref(),
                &template_name,
                &system_prompt,
                resolved_parent.as_deref(),
                fm.model.as_deref(),
                &fm.capabilities,
            )
            .await?;

        // Apply visual identity from template.
        if fm.color.is_some() || fm.avatar.is_some() || !fm.faces.is_empty() {
            agent.color = fm.color;
            agent.avatar = fm.avatar;
            agent.faces = if fm.faces.is_empty() {
                None
            } else {
                Some(fm.faces)
            };
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

    /// Spawn a new agent directly.
    pub async fn spawn(
        &self,
        name: &str,
        display_name: Option<&str>,
        template: &str,
        system_prompt: &str,
        parent_id: Option<&str>,
        model: Option<&str>,
        capabilities: &[String],
    ) -> Result<Agent> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let caps_json = serde_json::to_string(capabilities)?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let agent = Agent {
            id: id.clone(),
            name: name.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            template: template.to_string(),
            system_prompt: system_prompt.to_string(),
            parent_id: parent_id.map(|s| s.to_string()),
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
            max_concurrent: 1,
            session_id: Some(session_id.clone()),
            workdir: None,
            budget_usd: None,
            execution_mode: None,
            quest_prefix: None,
            worker_timeout_secs: None,
            prompts: if system_prompt.is_empty() {
                Vec::new()
            } else {
                vec![aeqi_core::PromptEntry::system(system_prompt)]
            },
        };

        let db = self.db.lock().await;
        let prompts_json =
            serde_json::to_string(&agent.prompts).unwrap_or_else(|_| "[]".to_string());
        db.execute(
            "INSERT INTO agents (id, name, display_name, template, system_prompt, parent_id, model, capabilities, status, created_at, session_id, prompts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                agent.id,
                agent.name,
                agent.display_name,
                agent.template,
                agent.system_prompt,
                agent.parent_id,
                agent.model,
                caps_json,
                agent.status.to_string(),
                agent.created_at.to_rfc3339(),
                session_id,
                prompts_json,
            ],
        )?;

        info!(id = %agent.id, name = %agent.name, parent_id = ?parent_id, "agent spawned");
        Ok(agent)
    }

    /// Get a specific agent by UUID.
    pub async fn get(&self, id: &str) -> Result<Option<Agent>> {
        let db = self.db.lock().await;
        let agent = db
            .query_row("SELECT * FROM agents WHERE id = ?1", params![id], |row| {
                Ok(row_to_agent(row))
            })
            .optional()?;
        Ok(agent)
    }

    /// Get agents by name (multiple can share a name).
    pub async fn get_by_name(&self, name: &str) -> Result<Vec<Agent>> {
        let db = self.db.lock().await;
        let mut stmt =
            db.prepare("SELECT * FROM agents WHERE name = ?1 ORDER BY created_at DESC")?;
        let agents = stmt
            .query_map(params![name], |row| Ok(row_to_agent(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    /// Get the first active agent with this name.
    pub async fn get_active_by_name(&self, name: &str) -> Result<Option<Agent>> {
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

    /// Resolve an agent by hint — tries name first, then UUID.
    pub async fn resolve_by_hint(&self, hint: &str) -> Result<Option<Agent>> {
        if let Some(agent) = self.get_active_by_name(hint).await? {
            return Ok(Some(agent));
        }
        self.get(hint).await
    }

    /// List all agents, optionally filtered by parent and/or status.
    pub async fn list(
        &self,
        parent_id: Option<Option<&str>>,
        status: Option<AgentStatus>,
    ) -> Result<Vec<Agent>> {
        let db = self.db.lock().await;
        let mut sql = "SELECT * FROM agents WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(pid) = parent_id {
            match pid {
                Some(id) => {
                    sql.push_str(" AND parent_id = ?");
                    params_vec.push(Box::new(id.to_string()));
                }
                None => {
                    sql.push_str(" AND parent_id IS NULL");
                }
            }
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

    /// List all active agents.
    pub async fn list_active(&self) -> Result<Vec<Agent>> {
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

    // -----------------------------------------------------------------------
    // Tree operations
    // -----------------------------------------------------------------------

    /// Get the root agent (parent_id IS NULL). Every workspace has exactly one.
    pub async fn get_root(&self) -> Result<Option<Agent>> {
        let db = self.db.lock().await;
        let agent = db
            .query_row(
                "SELECT * FROM agents WHERE parent_id IS NULL AND status = 'active' ORDER BY created_at ASC LIMIT 1",
                [],
                |row| Ok(row_to_agent(row)),
            )
            .optional()?;
        Ok(agent)
    }

    /// Get direct children of an agent.
    pub async fn get_children(&self, agent_id: &str) -> Result<Vec<Agent>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT * FROM agents WHERE parent_id = ?1 AND status = 'active' ORDER BY name ASC",
        )?;
        let agents = stmt
            .query_map(params![agent_id], |row| Ok(row_to_agent(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    /// Walk the parent_id chain from an agent up to root.
    /// Returns the chain starting from the given agent (inclusive).
    /// Includes cycle detection.
    pub async fn get_ancestors(&self, agent_id: &str) -> Result<Vec<Agent>> {
        let mut chain = Vec::new();
        let mut current_id = Some(agent_id.to_string());
        let mut visited = std::collections::HashSet::new();

        while let Some(id) = current_id {
            if !visited.insert(id.clone()) {
                tracing::warn!(agent_id = %id, "cycle detected in agent hierarchy");
                break;
            }
            match self.get(&id).await? {
                Some(agent) => {
                    current_id = agent.parent_id.clone();
                    chain.push(agent);
                }
                None => break,
            }
        }

        Ok(chain)
    }

    /// Get the ancestor IDs for an agent (for memory scoping).
    /// Returns [self_id, parent_id, grandparent_id, ..., root_id].
    pub async fn get_ancestor_ids(&self, agent_id: &str) -> Result<Vec<String>> {
        let db = self.db.lock().await;
        let mut ids = Vec::new();
        let mut current_id = Some(agent_id.to_string());
        let mut visited = std::collections::HashSet::new();

        while let Some(id) = current_id {
            if !visited.insert(id.clone()) {
                break;
            }
            ids.push(id.clone());
            current_id = db
                .query_row(
                    "SELECT parent_id FROM agents WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
        }

        Ok(ids)
    }

    /// Get the full subtree rooted at an agent (recursive CTE).
    pub async fn get_subtree(&self, agent_id: &str) -> Result<Vec<Agent>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "WITH RECURSIVE subtree AS (
                 SELECT * FROM agents WHERE id = ?1
                 UNION ALL
                 SELECT a.* FROM agents a
                 JOIN subtree s ON a.parent_id = s.id
             )
             SELECT * FROM subtree ORDER BY created_at ASC",
        )?;
        let agents = stmt
            .query_map(params![agent_id], |row| Ok(row_to_agent(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    /// Move an agent to a new parent (reparent).
    pub async fn move_agent(&self, agent_id: &str, new_parent_id: Option<&str>) -> Result<()> {
        // Prevent cycles: new parent must not be a descendant of agent_id.
        if let Some(pid) = new_parent_id {
            let subtree = self.get_subtree(agent_id).await?;
            if subtree.iter().any(|a| a.id == pid) {
                anyhow::bail!("cannot move agent under its own subtree (would create cycle)");
            }
        }

        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE agents SET parent_id = ?1 WHERE id = ?2",
            params![new_parent_id, agent_id],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{agent_id}' not found");
        }
        info!(agent_id = %agent_id, new_parent_id = ?new_parent_id, "agent reparented");
        Ok(())
    }

    /// Find the default agent to talk to — the root, or a named child.
    pub async fn default_agent(&self, hint: Option<&str>) -> Result<Option<Agent>> {
        if let Some(h) = hint
            && let Some(agent) = self.resolve_by_hint(h).await?
        {
            return Ok(Some(agent));
        }
        self.get_root().await
    }

    // -----------------------------------------------------------------------
    // Stats & lifecycle
    // -----------------------------------------------------------------------

    /// Record a session for this agent.
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
    pub async fn set_status(&self, id: &str, status: AgentStatus) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE agents SET status = ?1 WHERE id = ?2",
            params![status.to_string(), id],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{id}' not found");
        }
        info!(id = %id, status = %status, "agent status updated");
        Ok(())
    }

    /// Update the prompts array for an agent.
    pub async fn update_prompts(&self, id: &str, prompts_json: &str) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE agents SET prompts = ?1 WHERE id = ?2",
            params![prompts_json, id],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{id}' not found");
        }
        debug!(id = %id, "agent prompts updated");
        Ok(())
    }

    /// Update the model for an agent.
    pub async fn update_model(&self, id: &str, model: &str) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE agents SET model = ?1 WHERE id = ?2",
            params![model, id],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{id}' not found");
        }
        info!(id = %id, model = %model, "agent model updated");
        Ok(())
    }

    /// Update the system prompt for an agent.
    pub async fn update_system_prompt(&self, id: &str, system_prompt: &str) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE agents SET system_prompt = ?1 WHERE id = ?2",
            params![system_prompt, id],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{id}' not found");
        }
        Ok(())
    }

    /// Update operational fields on an agent.
    pub async fn update_agent_ops(
        &self,
        id: &str,
        workdir: Option<&str>,
        budget_usd: Option<f64>,
        execution_mode: Option<&str>,
        quest_prefix: Option<&str>,
        worker_timeout_secs: Option<u64>,
    ) -> Result<()> {
        let db = self.db.lock().await;
        // DB column is still `task_prefix` for backward compat; Rust field is `quest_prefix`.
        let updated = db.execute(
            "UPDATE agents SET workdir = ?1, budget_usd = ?2, execution_mode = ?3, task_prefix = ?4, worker_timeout_secs = ?5 WHERE id = ?6",
            params![
                workdir,
                budget_usd,
                execution_mode,
                quest_prefix,
                worker_timeout_secs.map(|v| v as i64),
                id,
            ],
        )?;
        if updated == 0 {
            anyhow::bail!("agent '{id}' not found");
        }
        info!(id = %id, "agent operational fields updated");
        Ok(())
    }

    /// Resolve workdir for an agent — walks ancestor chain to find first non-None.
    pub async fn resolve_workdir(&self, agent_id: &str) -> Result<Option<String>> {
        let ancestors = self.get_ancestors(agent_id).await?;
        for a in &ancestors {
            if let Some(ref wd) = a.workdir {
                return Ok(Some(wd.clone()));
            }
        }
        Ok(None)
    }

    /// Resolve execution mode for an agent — walks ancestor chain.
    pub async fn resolve_execution_mode(&self, agent_id: &str) -> Result<String> {
        let ancestors = self.get_ancestors(agent_id).await?;
        for a in &ancestors {
            if let Some(ref mode) = a.execution_mode {
                return Ok(mode.clone());
            }
        }
        Ok("agent".to_string())
    }

    /// Resolve worker timeout for an agent — walks ancestor chain.
    pub async fn resolve_worker_timeout(&self, agent_id: &str) -> Result<u64> {
        let ancestors = self.get_ancestors(agent_id).await?;
        for a in &ancestors {
            if let Some(timeout) = a.worker_timeout_secs {
                return Ok(timeout);
            }
        }
        Ok(3600) // Default: 1 hour.
    }

    /// Resolve model for an agent — walks ancestor chain, falls back to default.
    pub async fn resolve_model(&self, agent_id: &str, default_model: &str) -> String {
        if let Ok(ancestors) = self.get_ancestors(agent_id).await {
            for a in &ancestors {
                if let Some(ref model) = a.model {
                    return model.clone();
                }
            }
        }
        default_model.to_string()
    }

    /// Get max_concurrent for an agent by ID. Returns 1 if not found.
    pub async fn get_max_concurrent(&self, id: &str) -> Result<u32> {
        let db = self.db.lock().await;
        let max_concurrent: Option<u32> = db
            .query_row(
                "SELECT max_concurrent FROM agents WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(max_concurrent.unwrap_or(1))
    }

    /// Get a TriggerStore sharing this registry's database connection.
    pub fn trigger_store(&self) -> crate::trigger::TriggerStore {
        crate::trigger::TriggerStore::new(self.db.clone())
    }

    // -----------------------------------------------------------------------
    // Budget policy operations
    // -----------------------------------------------------------------------

    pub async fn list_budget_policies(&self) -> Result<Vec<serde_json::Value>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, agent_id, window, amount_usd, warn_pct, hard_stop, paused, created_at \
             FROM budget_policies ORDER BY created_at DESC",
        )?;
        let policies = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "agent_id": row.get::<_, String>(1)?,
                    "window": row.get::<_, String>(2)?,
                    "amount_usd": row.get::<_, f64>(3)?,
                    "warn_pct": row.get::<_, f64>(4)?,
                    "hard_stop": row.get::<_, i32>(5)? != 0,
                    "paused": row.get::<_, i32>(6)? != 0,
                    "created_at": row.get::<_, String>(7)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(policies)
    }

    pub async fn create_budget_policy(
        &self,
        agent_id: &str,
        window: &str,
        amount_usd: f64,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO budget_policies (id, agent_id, window, amount_usd, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, agent_id, window, amount_usd, now],
        )?;
        info!(id = %id, agent_id = %agent_id, "budget policy created");
        Ok(id)
    }

    /// Check budget for an agent — walks ancestor chain to find the tightest policy.
    pub async fn check_budget(&self, agent_id: &str) -> Result<Option<f64>> {
        let ancestor_ids = self.get_ancestor_ids(agent_id).await?;
        let db = self.db.lock().await;
        let mut tightest: Option<f64> = None;
        for id in &ancestor_ids {
            let amount: Option<f64> = db
                .query_row(
                    "SELECT amount_usd FROM budget_policies \
                     WHERE agent_id = ?1 AND paused = 0 \
                     ORDER BY amount_usd ASC LIMIT 1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(a) = amount {
                tightest = Some(tightest.map_or(a, |t: f64| t.min(a)));
            }
        }
        Ok(tightest)
    }

    pub async fn set_budget_paused(&self, policy_id: &str, paused: bool) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE budget_policies SET paused = ?1 WHERE id = ?2",
            params![paused as i32, policy_id],
        )?;
        if updated == 0 {
            anyhow::bail!("budget policy '{policy_id}' not found");
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Approval queue
    // -----------------------------------------------------------------------

    pub async fn create_approval(
        &self,
        agent_id: &str,
        task_id: Option<&str>,
        request_type: &str,
        payload: &serde_json::Value,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let payload_json = serde_json::to_string(payload)?;
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO approvals (id, agent_id, task_id, request_type, payload_json, status, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
            params![id, agent_id, task_id, request_type, payload_json, now],
        )?;
        info!(id = %id, agent_id = %agent_id, request_type = %request_type, "approval request created");
        Ok(id)
    }

    pub async fn list_approvals(&self, status: Option<&str>) -> Result<Vec<serde_json::Value>> {
        let db = self.db.lock().await;
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
            Some(s) => (
                "SELECT id, agent_id, task_id, request_type, payload_json, status, decided_by, decision_note, created_at, decided_at \
                 FROM approvals WHERE status = ?1 ORDER BY created_at DESC"
                    .to_string(),
                vec![Box::new(s.to_string())],
            ),
            None => (
                "SELECT id, agent_id, task_id, request_type, payload_json, status, decided_by, decision_note, created_at, decided_at \
                 FROM approvals ORDER BY created_at DESC"
                    .to_string(),
                vec![],
            ),
        };
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = db.prepare(&sql)?;
        let approvals = stmt
            .query_map(params_refs.as_slice(), |row| {
                let payload_str: String = row.get(4)?;
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "agent_id": row.get::<_, String>(1)?,
                    "task_id": row.get::<_, Option<String>>(2)?,
                    "request_type": row.get::<_, String>(3)?,
                    "payload": payload,
                    "status": row.get::<_, String>(5)?,
                    "decided_by": row.get::<_, Option<String>>(6)?,
                    "decision_note": row.get::<_, Option<String>>(7)?,
                    "created_at": row.get::<_, String>(8)?,
                    "decided_at": row.get::<_, Option<String>>(9)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(approvals)
    }

    pub async fn resolve_approval(
        &self,
        approval_id: &str,
        status: &str,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE approvals SET status = ?1, decided_by = ?2, decision_note = ?3, decided_at = ?4 \
             WHERE id = ?5",
            params![status, decided_by, note, now, approval_id],
        )?;
        if updated == 0 {
            anyhow::bail!("approval '{approval_id}' not found");
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Unified task store (SQLite-backed, replaces per-project JSONL)
    // -----------------------------------------------------------------------

    /// Create a task assigned to an agent.
    pub async fn create_task(
        &self,
        agent_id: &str,
        subject: &str,
        description: &str,
        skill: Option<&str>,
        labels: &[String],
    ) -> Result<aeqi_quests::Quest> {
        // Resolve quest prefix: agent's quest_prefix, or first 2 chars of name, or "t".
        let agent = self
            .get(agent_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("agent not found: {agent_id}"))?;
        let prefix = agent.quest_prefix.unwrap_or_else(|| {
            let name = &agent.name;
            if name.len() >= 2 {
                name[..2].to_lowercase()
            } else {
                "t".to_string()
            }
        });

        let db = self.db.lock().await;

        // Get and increment sequence for this prefix.
        db.execute(
            "INSERT OR IGNORE INTO quest_sequences (prefix, next_seq) VALUES (?1, 1)",
            params![prefix],
        )?;
        let seq: u32 = db.query_row(
            "UPDATE quest_sequences SET next_seq = next_seq + 1 WHERE prefix = ?1 RETURNING next_seq - 1",
            params![prefix],
            |row| row.get(0),
        )?;

        let task_id = format!("{prefix}-{seq:03}");
        let now = chrono::Utc::now();
        let labels_json = serde_json::to_string(labels)?;

        let task = aeqi_quests::Quest {
            id: aeqi_quests::QuestId(task_id.clone()),
            name: subject.to_string(),
            description: description.to_string(),
            status: aeqi_quests::QuestStatus::Pending,
            priority: aeqi_quests::quest::Priority::Normal,
            assignee: Some(agent.name.clone()),
            agent_id: Some(agent_id.to_string()),
            depends_on: Vec::new(),
            blocks: Vec::new(),
            skill: skill.map(|s| s.to_string()),
            labels: labels.to_vec(),
            retry_count: 0,
            checkpoints: Vec::new(),
            metadata: serde_json::Value::Null,
            created_at: now,
            updated_at: None,
            closed_at: None,
            closed_reason: None,
            acceptance_criteria: None,
            locked_by: None,
            locked_at: None,
        };

        db.execute(
            "INSERT INTO quests (id, subject, description, status, priority, assignee, agent_id, skill, labels, created_at)
             VALUES (?1, ?2, ?3, 'pending', 'normal', ?4, ?5, ?6, ?7, ?8)",
            params![
                task_id,
                subject,
                description,
                agent.name,
                agent_id,
                skill,
                labels_json,
                now.to_rfc3339(),
            ],
        )?;

        info!(task = %task_id, agent = %agent.name, subject = %subject, "task created");
        Ok(task)
    }

    /// Get all pending tasks that are ready to run (no unmet dependencies).
    pub async fn ready_tasks(&self) -> Result<Vec<aeqi_quests::Quest>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT * FROM quests WHERE status = 'pending' ORDER BY
             CASE priority
                WHEN 'critical' THEN 0
                WHEN 'high' THEN 1
                WHEN 'normal' THEN 2
                WHEN 'low' THEN 3
                ELSE 4
             END,
             created_at ASC",
        )?;
        let tasks: Vec<aeqi_quests::Quest> = stmt
            .query_map([], |row| Ok(row_to_task(row)))?
            .filter_map(|r| r.ok())
            .collect();

        // Filter out tasks with unmet dependencies.
        let ready: Vec<aeqi_quests::Quest> = tasks
            .into_iter()
            .filter(|t| {
                if t.depends_on.is_empty() {
                    return true;
                }
                // Check all deps are done (synchronous — we have the db lock).
                t.depends_on.iter().all(|dep_id| {
                    db.query_row(
                        "SELECT status FROM quests WHERE id = ?1",
                        params![dep_id.0],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                    .as_deref()
                        == Some("done")
                })
            })
            .collect();

        Ok(ready)
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Result<Option<aeqi_quests::Quest>> {
        let db = self.db.lock().await;
        let task = db
            .query_row(
                "SELECT * FROM quests WHERE id = ?1",
                params![task_id],
                |row| Ok(row_to_task(row)),
            )
            .optional()?;
        Ok(task)
    }

    /// Update a task's status.
    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: aeqi_quests::QuestStatus,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let status_str = status.to_string();
        let db = self.db.lock().await;

        let closed_at = if matches!(
            status,
            aeqi_quests::QuestStatus::Done | aeqi_quests::QuestStatus::Cancelled
        ) {
            Some(now.clone())
        } else {
            None
        };

        db.execute(
            "UPDATE quests SET status = ?1, updated_at = ?2, closed_at = COALESCE(?3, closed_at) WHERE id = ?4",
            params![status_str, now, closed_at, task_id],
        )?;
        Ok(())
    }

    /// Update a task using a closure (mirrors QuestBoard API).
    pub async fn update_task<F: FnOnce(&mut aeqi_quests::Quest)>(
        &self,
        task_id: &str,
        f: F,
    ) -> Result<aeqi_quests::Quest> {
        let db = self.db.lock().await;
        let mut task = db
            .query_row(
                "SELECT * FROM quests WHERE id = ?1",
                params![task_id],
                |row| Ok(row_to_task(row)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?;

        f(&mut task);

        let now = chrono::Utc::now().to_rfc3339();
        let labels_json = serde_json::to_string(&task.labels).unwrap_or_default();
        let checkpoints_json = serde_json::to_string(&task.checkpoints).unwrap_or_default();
        let deps_json = serde_json::to_string(&task.depends_on).unwrap_or_default();
        let blocks_json = serde_json::to_string(&task.blocks).unwrap_or_default();
        let metadata_json = serde_json::to_string(&task.metadata).unwrap_or_default();

        db.execute(
            "UPDATE quests SET
                subject = ?1, description = ?2, status = ?3, priority = ?4,
                assignee = ?5, agent_id = ?6, skill = ?7, labels = ?8,
                retry_count = ?9, checkpoints = ?10, metadata = ?11,
                depends_on = ?12, blocks = ?13, acceptance_criteria = ?14,
                locked_by = ?15, locked_at = ?16, updated_at = ?17,
                closed_at = ?18, closed_reason = ?19
             WHERE id = ?20",
            params![
                task.name,
                task.description,
                task.status.to_string(),
                task.priority.to_string(),
                task.assignee,
                task.agent_id,
                task.skill,
                labels_json,
                task.retry_count,
                checkpoints_json,
                metadata_json,
                deps_json,
                blocks_json,
                task.acceptance_criteria,
                task.locked_by,
                task.locked_at.map(|d| d.to_rfc3339()),
                now,
                task.closed_at.map(|d| d.to_rfc3339()),
                task.closed_reason,
                task.id.0,
            ],
        )?;

        Ok(task)
    }

    /// List all tasks, optionally filtered by status and/or agent.
    pub async fn list_tasks(
        &self,
        status: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<aeqi_quests::Quest>> {
        let db = self.db.lock().await;
        let mut sql = "SELECT * FROM quests WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(s) = status {
            sql.push_str(" AND status = ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(a) = agent_id {
            sql.push_str(" AND agent_id = ?");
            params_vec.push(Box::new(a.to_string()));
        }
        sql.push_str(" ORDER BY created_at DESC");

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = db.prepare(&sql)?;
        let tasks = stmt
            .query_map(params_refs.as_slice(), |row| Ok(row_to_task(row)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    /// Expose the database connection for shared use (e.g., by Scheduler).
    pub fn db(&self) -> Arc<Mutex<Connection>> {
        self.db.clone()
    }

    // -- Company CRUD ---------------------------------------------------------

    /// Upsert a company from TOML config (overwrites existing TOML-sourced entries).
    pub async fn upsert_company_from_toml(&self, record: &CompanyRecord) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO companies (name, display_name, prefix, tagline, logo_url, primer, repo, model,
                max_workers, execution_mode, worker_timeout_secs, worktree_root, max_turns,
                max_budget_usd, max_cost_per_day_usd, source, agent_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'toml',?16,?17,?17)
             ON CONFLICT(name) DO UPDATE SET
                prefix=?3, primer=?6, repo=?7, model=?8, max_workers=?9,
                execution_mode=?10, worker_timeout_secs=?11, worktree_root=?12,
                max_turns=?13, max_budget_usd=?14, max_cost_per_day_usd=?15,
                agent_id=?16, updated_at=?17
             WHERE source='toml'",
            rusqlite::params![
                record.name,
                record.display_name,
                record.prefix,
                record.tagline,
                record.logo_url,
                record.primer,
                record.repo,
                record.model,
                record.max_workers,
                record.execution_mode,
                record.worker_timeout_secs,
                record.worktree_root,
                record.max_turns,
                record.max_budget_usd,
                record.max_cost_per_day_usd,
                record.agent_id,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Create a company via API (fails if name already exists).
    pub async fn create_company(&self, record: &CompanyRecord) -> Result<()> {
        let db = self.db.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "INSERT INTO companies (name, display_name, prefix, tagline, logo_url, primer, repo, model,
                max_workers, execution_mode, worker_timeout_secs, source, agent_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'api',?12,?13,?13)",
            rusqlite::params![
                record.name, record.display_name, record.prefix, record.tagline,
                record.logo_url, record.primer, record.repo, record.model,
                record.max_workers, record.execution_mode, record.worker_timeout_secs,
                record.agent_id, now,
            ],
        )?;
        Ok(())
    }

    /// Get a company by name.
    pub async fn get_company(&self, name: &str) -> Result<Option<CompanyRecord>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT name, display_name, prefix, tagline, logo_url, primer, repo, model,
                    max_workers, execution_mode, worker_timeout_secs, worktree_root, max_turns,
                    max_budget_usd, max_cost_per_day_usd, source, agent_id, created_at, updated_at
             FROM companies WHERE name = ?1",
        )?;
        let result = stmt.query_row(rusqlite::params![name], row_to_company).ok();
        Ok(result)
    }

    /// List all companies.
    pub async fn list_companies(&self) -> Result<Vec<CompanyRecord>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT name, display_name, prefix, tagline, logo_url, primer, repo, model,
                    max_workers, execution_mode, worker_timeout_secs, worktree_root, max_turns,
                    max_budget_usd, max_cost_per_day_usd, source, agent_id, created_at, updated_at
             FROM companies ORDER BY created_at",
        )?;
        let companies = stmt
            .query_map([], row_to_company)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(companies)
    }

    /// List companies created via API (not TOML).
    pub async fn list_api_companies(&self) -> Result<Vec<CompanyRecord>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT name, display_name, prefix, tagline, logo_url, primer, repo, model,
                    max_workers, execution_mode, worker_timeout_secs, worktree_root, max_turns,
                    max_budget_usd, max_cost_per_day_usd, source, agent_id, created_at, updated_at
             FROM companies WHERE source = 'api' ORDER BY created_at",
        )?;
        let companies = stmt
            .query_map([], row_to_company)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(companies)
    }

    /// Update the agent_id link for a company.
    pub async fn update_company_agent_id(&self, name: &str, agent_id: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE companies SET agent_id = ?1, updated_at = ?2 WHERE name = ?3",
            rusqlite::params![agent_id, chrono::Utc::now().to_rfc3339(), name],
        )?;
        Ok(())
    }
}

fn row_to_company(row: &rusqlite::Row) -> rusqlite::Result<CompanyRecord> {
    Ok(CompanyRecord {
        name: row.get(0)?,
        display_name: row.get(1)?,
        prefix: row.get(2)?,
        tagline: row.get(3)?,
        logo_url: row.get(4)?,
        primer: row.get(5)?,
        repo: row.get(6)?,
        model: row.get(7)?,
        max_workers: row.get::<_, u32>(8).unwrap_or(2),
        execution_mode: row
            .get::<_, String>(9)
            .unwrap_or_else(|_| "agent".to_string()),
        worker_timeout_secs: row.get::<_, u64>(10).unwrap_or(1800),
        worktree_root: row.get(11)?,
        max_turns: row.get(12)?,
        max_budget_usd: row.get(13)?,
        max_cost_per_day_usd: row.get(14)?,
        source: row
            .get::<_, String>(15)
            .unwrap_or_else(|_| "api".to_string()),
        agent_id: row.get(16)?,
        created_at: row.get::<_, String>(17).unwrap_or_default(),
        updated_at: row.get::<_, String>(18).unwrap_or_default(),
    })
}

/// Convert a SQLite row to a Task.
fn row_to_task(row: &rusqlite::Row) -> aeqi_quests::Quest {
    let labels_str: String = row.get("labels").unwrap_or_else(|_| "[]".to_string());
    let checkpoints_str: String = row.get("checkpoints").unwrap_or_else(|_| "[]".to_string());
    let deps_str: String = row.get("depends_on").unwrap_or_else(|_| "[]".to_string());
    let blocks_str: String = row.get("blocks").unwrap_or_else(|_| "[]".to_string());
    let metadata_str: String = row.get("metadata").unwrap_or_else(|_| "{}".to_string());

    let status_str: String = row.get("status").unwrap_or_else(|_| "pending".to_string());
    let status = match status_str.as_str() {
        "in_progress" => aeqi_quests::QuestStatus::InProgress,
        "done" => aeqi_quests::QuestStatus::Done,
        "blocked" => aeqi_quests::QuestStatus::Blocked,
        "cancelled" => aeqi_quests::QuestStatus::Cancelled,
        _ => aeqi_quests::QuestStatus::Pending,
    };

    let priority_str: String = row.get("priority").unwrap_or_else(|_| "normal".to_string());
    let priority = match priority_str.as_str() {
        "low" => aeqi_quests::quest::Priority::Low,
        "high" => aeqi_quests::quest::Priority::High,
        "critical" => aeqi_quests::quest::Priority::Critical,
        _ => aeqi_quests::quest::Priority::Normal,
    };

    aeqi_quests::Quest {
        id: aeqi_quests::QuestId(row.get("id").unwrap_or_default()),
        name: row.get("subject").unwrap_or_default(),
        description: row.get("description").unwrap_or_default(),
        status,
        priority,
        assignee: row.get("assignee").ok(),
        agent_id: row.get("agent_id").ok(),
        depends_on: serde_json::from_str(&deps_str).unwrap_or_default(),
        blocks: serde_json::from_str(&blocks_str).unwrap_or_default(),
        skill: row.get("skill").ok(),
        labels: serde_json::from_str(&labels_str).unwrap_or_default(),
        retry_count: row.get::<_, u32>("retry_count").unwrap_or(0),
        checkpoints: serde_json::from_str(&checkpoints_str).unwrap_or_default(),
        metadata: serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null),
        created_at: row
            .get::<_, String>("created_at")
            .ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_default(),
        updated_at: row
            .get::<_, String>("updated_at")
            .ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc)),
        closed_at: row
            .get::<_, String>("closed_at")
            .ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc)),
        closed_reason: row.get("closed_reason").ok(),
        acceptance_criteria: row.get("acceptance_criteria").ok(),
        locked_by: row.get("locked_by").ok(),
        locked_at: row
            .get::<_, String>("locked_at")
            .ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc)),
    }
}

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
            "quest_completed" => crate::trigger::EventPattern::QuestCompleted {
                project: t.event_project.clone(),
            },
            "quest_failed" => crate::trigger::EventPattern::QuestFailed {
                project: t.event_project.clone(),
            },
            "tool_call_completed" => crate::trigger::EventPattern::ToolCallCompleted {
                tool: t.event_tool.clone(),
            },
            "dispatch_received" => crate::trigger::EventPattern::DispatchReceived {
                from_agent: t.event_from.clone(),
                to_agent: t.event_to.clone(),
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

fn row_to_agent(row: &rusqlite::Row) -> Agent {
    let status_str: String = row.get("status").unwrap_or_default();
    let status = match status_str.as_str() {
        "paused" => AgentStatus::Paused,
        "retired" => AgentStatus::Retired,
        _ => AgentStatus::Active,
    };

    let caps_str: String = row.get("capabilities").unwrap_or_else(|_| "[]".to_string());
    let capabilities: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();

    let prompts: Vec<aeqi_core::PromptEntry> = row
        .get::<_, String>("prompts")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // Derive system_prompt from prompts[0] (canonical source).
    // Fall back to the DB column for agents that predate the prompts[] migration.
    let system_prompt = prompts
        .first()
        .map(|p| p.content.clone())
        .unwrap_or_else(|| row.get("system_prompt").unwrap_or_default());

    Agent {
        id: row.get("id").unwrap_or_default(),
        name: row.get("name").unwrap_or_default(),
        display_name: row.get("display_name").ok(),
        template: row.get("template").unwrap_or_default(),
        system_prompt,
        parent_id: row.get("parent_id").ok(),
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
        color: row.get("color").ok(),
        avatar: row.get("avatar").ok(),
        faces: row
            .get::<_, String>("faces")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok()),
        max_concurrent: row.get::<_, u32>("max_concurrent").unwrap_or(1),
        session_id: row.get("session_id").ok(),
        workdir: row.get("workdir").ok(),
        budget_usd: row.get("budget_usd").ok(),
        execution_mode: row.get("execution_mode").ok(),
        quest_prefix: row.get("task_prefix").ok(), // DB column is still `task_prefix`
        worker_timeout_secs: row
            .get::<_, i64>("worker_timeout_secs")
            .ok()
            .map(|v| v as u64),
        prompts,
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
        assert!(agent.parent_id.is_none()); // Root agent
        assert_eq!(agent.status, AgentStatus::Active);

        let fetched = reg.get(&agent.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, agent.id);
    }

    #[tokio::test]
    async fn parent_child_relationship() {
        let reg = test_registry().await;
        let root = reg
            .spawn("assistant", None, "root", "Root agent.", None, None, &[])
            .await
            .unwrap();
        let child = reg
            .spawn(
                "engineering",
                None,
                "team",
                "Engineering team.",
                Some(&root.id),
                None,
                &[],
            )
            .await
            .unwrap();
        let grandchild = reg
            .spawn(
                "backend",
                None,
                "team",
                "Backend team.",
                Some(&child.id),
                None,
                &[],
            )
            .await
            .unwrap();

        // Children
        let children = reg.get_children(&root.id).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "engineering");

        // Ancestors
        let ancestors = reg.get_ancestors(&grandchild.id).await.unwrap();
        assert_eq!(ancestors.len(), 3);
        assert_eq!(ancestors[0].name, "backend");
        assert_eq!(ancestors[1].name, "engineering");
        assert_eq!(ancestors[2].name, "assistant");

        // Ancestor IDs
        let ids = reg.get_ancestor_ids(&grandchild.id).await.unwrap();
        assert_eq!(
            ids,
            vec![grandchild.id.clone(), child.id.clone(), root.id.clone()]
        );
    }

    #[tokio::test]
    async fn get_root() {
        let reg = test_registry().await;
        let root = reg
            .spawn("assistant", None, "root", "Root.", None, None, &[])
            .await
            .unwrap();
        let _child = reg
            .spawn("worker", None, "t", "Worker.", Some(&root.id), None, &[])
            .await
            .unwrap();

        let found = reg.get_root().await.unwrap().unwrap();
        assert_eq!(found.id, root.id);
    }

    #[tokio::test]
    async fn subtree() {
        let reg = test_registry().await;
        let root = reg
            .spawn("root", None, "t", "R.", None, None, &[])
            .await
            .unwrap();
        let a = reg
            .spawn("a", None, "t", "A.", Some(&root.id), None, &[])
            .await
            .unwrap();
        let _b = reg
            .spawn("b", None, "t", "B.", Some(&root.id), None, &[])
            .await
            .unwrap();
        let _c = reg
            .spawn("c", None, "t", "C.", Some(&a.id), None, &[])
            .await
            .unwrap();

        let tree = reg.get_subtree(&root.id).await.unwrap();
        assert_eq!(tree.len(), 4); // root + a + b + c
    }

    #[tokio::test]
    async fn move_agent_prevents_cycles() {
        let reg = test_registry().await;
        let root = reg
            .spawn("root", None, "t", "R.", None, None, &[])
            .await
            .unwrap();
        let child = reg
            .spawn("child", None, "t", "C.", Some(&root.id), None, &[])
            .await
            .unwrap();

        // Moving root under child would create a cycle.
        let result = reg.move_agent(&root.id, Some(&child.id)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn record_session_updates_stats() {
        let reg = test_registry().await;
        let agent = reg
            .spawn("test", None, "t", "T.", None, None, &[])
            .await
            .unwrap();

        reg.record_session(&agent.id, 5000).await.unwrap();
        reg.record_session(&agent.id, 3000).await.unwrap();

        let updated = reg.get(&agent.id).await.unwrap().unwrap();
        assert_eq!(updated.session_count, 2);
        assert_eq!(updated.total_tokens, 8000);
    }

    #[tokio::test]
    async fn spawn_from_template_parses_frontmatter() {
        let reg = test_registry().await;
        let template = r#"---
name: shadow
display_name: "Shadow — Your Dark Butler"
model: anthropic/claude-sonnet-4.6
capabilities: [spawn_agents]
---

You are Shadow, the user's personal assistant."#;
        let agent = reg.spawn_from_template(template, None).await.unwrap();
        assert_eq!(agent.name, "shadow");
        assert_eq!(
            agent.display_name.as_deref(),
            Some("Shadow — Your Dark Butler")
        );
        assert!(agent.parent_id.is_none()); // Root
    }

    #[tokio::test]
    async fn spawn_from_template_with_parent() {
        let reg = test_registry().await;
        let root = reg
            .spawn("root", None, "t", "R.", None, None, &[])
            .await
            .unwrap();
        let template = r#"---
name: worker
model: anthropic/claude-sonnet-4.6
---

You are a worker agent."#;
        let agent = reg
            .spawn_from_template(template, Some(&root.id))
            .await
            .unwrap();
        assert_eq!(agent.parent_id.as_deref(), Some(root.id.as_str()));
    }

    #[tokio::test]
    async fn budget_walks_ancestor_chain() {
        let reg = test_registry().await;
        let root = reg
            .spawn("root", None, "t", "R.", None, None, &[])
            .await
            .unwrap();
        let child = reg
            .spawn("child", None, "t", "C.", Some(&root.id), None, &[])
            .await
            .unwrap();

        // Set budget on root.
        reg.create_budget_policy(&root.id, "daily", 10.0)
            .await
            .unwrap();

        // Child inherits it.
        let budget = reg.check_budget(&child.id).await.unwrap();
        assert_eq!(budget, Some(10.0));

        // Tighter budget on child takes precedence.
        reg.create_budget_policy(&child.id, "daily", 5.0)
            .await
            .unwrap();
        let budget = reg.check_budget(&child.id).await.unwrap();
        assert_eq!(budget, Some(5.0));
    }
}
