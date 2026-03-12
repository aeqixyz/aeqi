//! Inter-Agent Blackboard — shared per-project knowledge between workers.
//!
//! Workers are isolated during execution. The blackboard creates a shared
//! knowledge space where workers can post discoveries (file locations, API
//! details, gotchas) that sibling workers can query during parallel execution.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// How long a blackboard entry persists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryDurability {
    /// Short-lived discovery (default 24h).
    #[default]
    Transient,
    /// Important finding (default 7d).
    Durable,
}

/// A single blackboard entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackboardEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub agent: String,
    pub project: String,
    pub tags: Vec<String>,
    pub durability: EntryDurability,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// SQLite-backed inter-agent blackboard.
pub struct Blackboard {
    conn: Mutex<Connection>,
    transient_ttl_hours: u64,
    durable_ttl_days: u64,
}

impl Blackboard {
    pub fn open(path: &Path, transient_ttl_hours: u64, durable_ttl_days: u64) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create blackboard dir: {}", parent.display())
            })?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open blackboard DB: {}", path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;

             CREATE TABLE IF NOT EXISTS sigil_blackboard (
                 id TEXT PRIMARY KEY,
                 key TEXT NOT NULL,
                 content TEXT NOT NULL,
                 agent TEXT NOT NULL,
                 project TEXT NOT NULL,
                 tags_json TEXT NOT NULL DEFAULT '[]',
                 durability TEXT NOT NULL DEFAULT 'transient',
                 created_at TEXT NOT NULL,
                 expires_at TEXT NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_bb_project
                 ON sigil_blackboard(project);
             CREATE INDEX IF NOT EXISTS idx_bb_key
                 ON sigil_blackboard(project, key);
             CREATE INDEX IF NOT EXISTS idx_bb_expires
                 ON sigil_blackboard(expires_at);",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
            transient_ttl_hours,
            durable_ttl_days,
        })
    }

    /// Post a new entry to the blackboard.
    pub fn post(
        &self,
        key: &str,
        content: &str,
        agent: &str,
        project: &str,
        tags: &[String],
        durability: EntryDurability,
    ) -> Result<BlackboardEntry> {
        let now = Utc::now();
        let expires_at = match durability {
            EntryDurability::Transient => {
                now + chrono::Duration::hours(self.transient_ttl_hours as i64)
            }
            EntryDurability::Durable => now + chrono::Duration::days(self.durable_ttl_days as i64),
        };
        let id = uuid::Uuid::new_v4().to_string();

        let entry = BlackboardEntry {
            id: id.clone(),
            key: key.to_string(),
            content: content.to_string(),
            agent: agent.to_string(),
            project: project.to_string(),
            tags: tags.to_vec(),
            durability,
            created_at: now,
            expires_at,
        };

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let tags_json = serde_json::to_string(&entry.tags)?;
        let dur_str = serde_json::to_value(entry.durability)?
            .as_str()
            .unwrap_or("transient")
            .to_string();

        conn.execute(
            "INSERT OR REPLACE INTO sigil_blackboard (id, key, content, agent, project, tags_json, durability, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                entry.id, entry.key, entry.content, entry.agent, entry.project,
                tags_json, dur_str, entry.created_at.to_rfc3339(), entry.expires_at.to_rfc3339(),
            ],
        )?;

        Ok(entry)
    }

    /// Query entries by project and tags (any tag matches).
    pub fn query(
        &self,
        project: &str,
        tags: &[String],
        limit: u32,
    ) -> Result<Vec<BlackboardEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();

        if tags.is_empty() {
            return self.list_project_internal(&conn, project, limit, &now);
        }

        // Query all non-expired entries for the project, then filter by tag in Rust
        // (SQLite JSON queries are limited).
        let entries = self.list_project_internal(&conn, project, 1000, &now)?;
        let mut matched: Vec<BlackboardEntry> = entries
            .into_iter()
            .filter(|e| e.tags.iter().any(|t| tags.contains(t)))
            .collect();
        matched.truncate(limit as usize);
        Ok(matched)
    }

    /// Get a specific entry by project and key.
    pub fn get_by_key(&self, project: &str, key: &str) -> Result<Option<BlackboardEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();

        let mut stmt = conn.prepare(
            "SELECT id, key, content, agent, project, tags_json, durability, created_at, expires_at
             FROM sigil_blackboard WHERE project = ?1 AND key = ?2 AND expires_at > ?3
             ORDER BY created_at DESC LIMIT 1",
        )?;

        let mut rows = stmt.query_map(rusqlite::params![project, key, now], Self::row_to_entry)?;
        match rows.next() {
            Some(Ok(entry)) => Ok(Some(entry)),
            _ => Ok(None),
        }
    }

    /// Remove expired entries.
    pub fn prune_expired(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();
        let count = conn.execute(
            "DELETE FROM sigil_blackboard WHERE expires_at <= ?1",
            rusqlite::params![now],
        )?;
        Ok(count as u64)
    }

    /// List all non-expired entries for a project.
    pub fn list_project(&self, project: &str, limit: u32) -> Result<Vec<BlackboardEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();
        self.list_project_internal(&conn, project, limit, &now)
    }

    fn list_project_internal(
        &self,
        conn: &Connection,
        project: &str,
        limit: u32,
        now: &str,
    ) -> Result<Vec<BlackboardEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, key, content, agent, project, tags_json, durability, created_at, expires_at
             FROM sigil_blackboard WHERE project = ?1 AND expires_at > ?2
             ORDER BY created_at DESC LIMIT ?3",
        )?;

        let entries: Vec<BlackboardEntry> = stmt
            .query_map(rusqlite::params![project, now, limit], Self::row_to_entry)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<BlackboardEntry> {
        let id: String = row.get(0)?;
        let key: String = row.get(1)?;
        let content: String = row.get(2)?;
        let agent: String = row.get(3)?;
        let project: String = row.get(4)?;
        let tags_json: String = row.get(5)?;
        let dur_str: String = row.get(6)?;
        let created_str: String = row.get(7)?;
        let expires_str: String = row.get(8)?;

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let durability: EntryDurability =
            serde_json::from_value(serde_json::Value::String(dur_str)).unwrap_or_default();
        let created_at = DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let expires_at = DateTime::parse_from_rfc3339(&expires_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(BlackboardEntry {
            id,
            key,
            content,
            agent,
            project,
            tags,
            durability,
            created_at,
            expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_bb() -> (Blackboard, TempDir) {
        let dir = TempDir::new().unwrap();
        let bb = Blackboard::open(&dir.path().join("bb.db"), 24, 7).unwrap();
        (bb, dir)
    }

    #[test]
    fn test_post_and_query() {
        let (bb, _dir) = temp_bb();
        bb.post(
            "api-endpoint",
            "POST /api/v2/users",
            "worker-1",
            "proj-a",
            &["api".to_string()],
            EntryDurability::Transient,
        )
        .unwrap();
        bb.post(
            "db-schema",
            "users table has uuid PK",
            "worker-2",
            "proj-a",
            &["database".to_string()],
            EntryDurability::Durable,
        )
        .unwrap();

        let all = bb.list_project("proj-a", 10).unwrap();
        assert_eq!(all.len(), 2);

        let api = bb.query("proj-a", &["api".to_string()], 10).unwrap();
        assert_eq!(api.len(), 1);
        assert_eq!(api[0].key, "api-endpoint");
    }

    #[test]
    fn test_tag_relevance() {
        let (bb, _dir) = temp_bb();
        bb.post(
            "k1",
            "content1",
            "a",
            "p",
            &["rust".to_string(), "backend".to_string()],
            EntryDurability::Transient,
        )
        .unwrap();
        bb.post(
            "k2",
            "content2",
            "a",
            "p",
            &["python".to_string()],
            EntryDurability::Transient,
        )
        .unwrap();

        let rust = bb.query("p", &["rust".to_string()], 10).unwrap();
        assert_eq!(rust.len(), 1);
        assert_eq!(rust[0].key, "k1");

        let backend = bb.query("p", &["backend".to_string()], 10).unwrap();
        assert_eq!(backend.len(), 1);
    }

    #[test]
    fn test_prune_expired() {
        let (bb, _dir) = temp_bb();

        // Post with 0-hour TTL (already expired in practice, but let's create manually)
        bb.post("k1", "c1", "a", "p", &[], EntryDurability::Transient)
            .unwrap();

        // Manually expire it by updating the DB
        {
            let conn = bb.conn.lock().unwrap();
            let expired = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
            conn.execute(
                "UPDATE sigil_blackboard SET expires_at = ?1",
                rusqlite::params![expired],
            )
            .unwrap();
        }

        let pruned = bb.prune_expired().unwrap();
        assert_eq!(pruned, 1);

        let remaining = bb.list_project("p", 10).unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn test_get_by_key() {
        let (bb, _dir) = temp_bb();
        bb.post(
            "config-path",
            "/etc/app.toml",
            "w1",
            "proj",
            &[],
            EntryDurability::Durable,
        )
        .unwrap();

        let entry = bb.get_by_key("proj", "config-path").unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "/etc/app.toml");

        let missing = bb.get_by_key("proj", "nonexistent").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_blackboard_tool_spec() {
        // Verify the durability enum serializes correctly
        let transient = serde_json::to_value(EntryDurability::Transient).unwrap();
        assert_eq!(transient, serde_json::json!("transient"));
        let durable = serde_json::to_value(EntryDurability::Durable).unwrap();
        assert_eq!(durable, serde_json::json!("durable"));
    }
}
