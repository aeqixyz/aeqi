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
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// A persistent agent identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentAgent {
    /// Stable UUID — used as entity_id for memory scoping.
    pub id: String,
    /// Human-readable name (e.g., "rei", "ops-monitor").
    pub name: String,
    /// Display name (e.g., "Rei — The Living Sigil").
    pub display_name: Option<String>,
    /// The identity template this agent was created from (e.g., "rei", "researcher").
    pub template: String,
    /// Project scope. None = root (cross-project).
    pub project: Option<String>,
    /// Department scope within project. None = project-level.
    pub department: Option<String>,
    /// Preferred model for this agent.
    pub model: Option<String>,
    /// Agent status.
    pub status: AgentStatus,
    /// When the agent was created.
    pub created_at: DateTime<Utc>,
    /// When the agent last ran a session.
    pub last_active: Option<DateTime<Utc>>,
    /// Total sessions this agent has participated in.
    pub session_count: u32,
    /// Total tokens consumed across all sessions.
    pub total_tokens: u64,
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
             CREATE TABLE IF NOT EXISTS agents (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL UNIQUE,
                 display_name TEXT,
                 template TEXT NOT NULL,
                 project TEXT,
                 department TEXT,
                 model TEXT,
                 status TEXT NOT NULL DEFAULT 'active',
                 created_at TEXT NOT NULL,
                 last_active TEXT,
                 session_count INTEGER NOT NULL DEFAULT 0,
                 total_tokens INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_agents_project ON agents(project);
             CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);",
        )?;

        info!(path = %db_path.display(), "agent registry opened");
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Spawn a new persistent agent from a template.
    pub async fn spawn(
        &self,
        name: &str,
        template: &str,
        project: Option<&str>,
        department: Option<&str>,
        model: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<PersistentAgent> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        let agent = PersistentAgent {
            id: id.clone(),
            name: name.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            template: template.to_string(),
            project: project.map(|s| s.to_string()),
            department: department.map(|s| s.to_string()),
            model: model.map(|s| s.to_string()),
            status: AgentStatus::Active,
            created_at: now,
            last_active: None,
            session_count: 0,
            total_tokens: 0,
        };

        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO agents (id, name, display_name, template, project, department, model, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                agent.id,
                agent.name,
                agent.display_name,
                agent.template,
                agent.project,
                agent.department,
                agent.model,
                agent.status.to_string(),
                agent.created_at.to_rfc3339(),
            ],
        )?;

        info!(id = %agent.id, name = %agent.name, template = %template, "persistent agent spawned");
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
            .query_map(params_refs.as_slice(), |row| {
                Ok(row_to_agent(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(agents)
    }

    /// Get a specific agent by name.
    pub async fn get_by_name(&self, name: &str) -> Result<Option<PersistentAgent>> {
        let db = self.db.lock().await;
        let agent = db
            .query_row(
                "SELECT * FROM agents WHERE name = ?1",
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
            .query_row(
                "SELECT * FROM agents WHERE id = ?1",
                params![id],
                |row| Ok(row_to_agent(row)),
            )
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

    /// Get the default agent for a project (first active agent scoped to that project,
    /// or the first root-scoped active agent).
    pub async fn default_for_project(&self, project: Option<&str>) -> Result<Option<PersistentAgent>> {
        let db = self.db.lock().await;

        // Try project-scoped first.
        if let Some(p) = project {
            if let Some(agent) = db
                .query_row(
                    "SELECT * FROM agents WHERE project = ?1 AND status = 'active' ORDER BY created_at ASC LIMIT 1",
                    params![p],
                    |row| Ok(row_to_agent(row)),
                )
                .optional()?
            {
                return Ok(Some(agent));
            }
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
}

fn row_to_agent(row: &rusqlite::Row) -> PersistentAgent {
    let status_str: String = row.get("status").unwrap_or_default();
    let status = match status_str.as_str() {
        "paused" => AgentStatus::Paused,
        "retired" => AgentStatus::Retired,
        _ => AgentStatus::Active,
    };

    PersistentAgent {
        id: row.get("id").unwrap_or_default(),
        name: row.get("name").unwrap_or_default(),
        display_name: row.get("display_name").ok(),
        template: row.get("template").unwrap_or_default(),
        project: row.get("project").ok(),
        department: row.get("department").ok(),
        model: row.get("model").ok(),
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
            .spawn("rei", "rei", None, None, Some("claude-sonnet-4.6"), Some("Rei"))
            .await
            .unwrap();

        assert_eq!(agent.name, "rei");
        assert_eq!(agent.template, "rei");
        assert!(agent.project.is_none());
        assert_eq!(agent.status, AgentStatus::Active);
        assert_eq!(agent.session_count, 0);

        let fetched = reg.get_by_name("rei").await.unwrap().unwrap();
        assert_eq!(fetched.id, agent.id);
    }

    #[tokio::test]
    async fn spawn_project_scoped() {
        let reg = test_registry().await;
        let agent = reg
            .spawn("sigil-lead", "rei", Some("sigil"), None, None, None)
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
            .spawn("test-agent", "researcher", None, None, None, None)
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
        reg.spawn("lifecycle", "rei", None, None, None, None)
            .await
            .unwrap();

        reg.set_status("lifecycle", AgentStatus::Paused)
            .await
            .unwrap();
        let agent = reg.get_by_name("lifecycle").await.unwrap().unwrap();
        assert_eq!(agent.status, AgentStatus::Paused);

        reg.set_status("lifecycle", AgentStatus::Retired)
            .await
            .unwrap();
        let agent = reg.get_by_name("lifecycle").await.unwrap().unwrap();
        assert_eq!(agent.status, AgentStatus::Retired);

        // Active filter should not return retired agents.
        let active = reg.list(None, Some(AgentStatus::Active)).await.unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn default_for_project() {
        let reg = test_registry().await;
        reg.spawn("root-rei", "rei", None, None, None, None)
            .await
            .unwrap();
        reg.spawn("sigil-lead", "rei", Some("sigil"), None, None, None)
            .await
            .unwrap();

        // Project-scoped takes priority.
        let default = reg.default_for_project(Some("sigil")).await.unwrap().unwrap();
        assert_eq!(default.name, "sigil-lead");

        // Unknown project falls back to root.
        let default = reg.default_for_project(Some("unknown")).await.unwrap().unwrap();
        assert_eq!(default.name, "root-rei");

        // No project → root.
        let default = reg.default_for_project(None).await.unwrap().unwrap();
        assert_eq!(default.name, "root-rei");
    }

    #[tokio::test]
    async fn duplicate_name_rejected() {
        let reg = test_registry().await;
        reg.spawn("unique", "rei", None, None, None, None)
            .await
            .unwrap();
        let result = reg.spawn("unique", "rei", None, None, None, None).await;
        assert!(result.is_err());
    }
}
