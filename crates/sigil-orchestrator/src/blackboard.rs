//! Inter-Agent Blackboard — shared coordination space between workers.
//!
//! The blackboard provides a shared knowledge and coordination surface where
//! agents can post discoveries, claim resources, signal state changes, and
//! record decisions. Entries are ephemeral (TTL-based) and scoped per-project,
//! with optional cross-project queries.
//!
//! ## Entry Types (by key prefix convention)
//!
//! - `claim:{resource}` — exclusive resource lock (auto-expires, releasable)
//! - `signal:{event}` — broadcast state change (build-broken, deploy-hold, etc.)
//! - `finding:{topic}` — investigation result for sibling agents
//! - `decision:{topic}` — architectural/implementation decision with rationale

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

/// Result of a claim attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimResult {
    /// Claim acquired successfully.
    Acquired,
    /// Claim already held by another agent.
    Held { holder: String, content: String },
    /// Claim was already held by the same agent (renewed).
    Renewed,
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

/// Describes which scoped key prefixes an agent is allowed to see.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentVisibility {
    /// The agent's own UUID — grants access to `agent:{uuid}:*` entries.
    pub agent_id: Option<String>,
    /// The project the agent belongs to — grants access to `project:{name}:*` entries.
    pub project: Option<String>,
    /// The department the agent belongs to — grants access to `dept:{name}:*` entries.
    pub department: Option<String>,
}

/// SQLite-backed inter-agent blackboard.
pub struct Blackboard {
    conn: Mutex<Connection>,
    transient_ttl_hours: u64,
    durable_ttl_days: u64,
    claim_ttl_hours: u64,
}

impl Blackboard {
    pub fn open(
        path: &Path,
        transient_ttl_hours: u64,
        durable_ttl_days: u64,
        claim_ttl_hours: u64,
    ) -> Result<Self> {
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
            claim_ttl_hours,
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

    /// Atomically claim a resource. Returns `Held` if another agent owns it.
    ///
    /// Claims use the key prefix `claim:` automatically — callers pass the
    /// bare resource name (e.g. `"src/api/auth.rs"`).
    pub fn claim(
        &self,
        resource: &str,
        agent: &str,
        project: &str,
        content: &str,
    ) -> Result<ClaimResult> {
        let claim_key = format!("claim:{resource}");
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;

        // Check for existing live claim
        let existing: Option<(String, String)> = conn
            .prepare(
                "SELECT agent, content FROM sigil_blackboard
                 WHERE project = ?1 AND key = ?2 AND expires_at > ?3
                 ORDER BY created_at DESC LIMIT 1",
            )?
            .query_row(
                rusqlite::params![project, &claim_key, &now_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        match existing {
            Some((holder, held_content)) if holder != agent => {
                Ok(ClaimResult::Held {
                    holder,
                    content: held_content,
                })
            }
            Some(_) => {
                // Same agent — renew the claim
                let expires_at =
                    now + chrono::Duration::hours(self.claim_ttl_hours as i64);
                let id = uuid::Uuid::new_v4().to_string();
                let tags_json = serde_json::to_string(&vec!["claim"])?;

                conn.execute(
                    "DELETE FROM sigil_blackboard WHERE project = ?1 AND key = ?2",
                    rusqlite::params![project, &claim_key],
                )?;
                conn.execute(
                    "INSERT INTO sigil_blackboard (id, key, content, agent, project, tags_json, durability, created_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        id, &claim_key, content, agent, project,
                        tags_json, "transient", now_str, expires_at.to_rfc3339(),
                    ],
                )?;
                Ok(ClaimResult::Renewed)
            }
            None => {
                // No claim — acquire it
                let expires_at =
                    now + chrono::Duration::hours(self.claim_ttl_hours as i64);
                let id = uuid::Uuid::new_v4().to_string();
                let tags_json = serde_json::to_string(&vec!["claim"])?;

                conn.execute(
                    "INSERT INTO sigil_blackboard (id, key, content, agent, project, tags_json, durability, created_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        id, &claim_key, content, agent, project,
                        tags_json, "transient", now_str, expires_at.to_rfc3339(),
                    ],
                )?;
                Ok(ClaimResult::Acquired)
            }
        }
    }

    /// Release a claim. Only the holding agent (or any agent if `force` is true) can release.
    pub fn release(
        &self,
        resource: &str,
        agent: &str,
        project: &str,
        force: bool,
    ) -> Result<bool> {
        let claim_key = format!("claim:{resource}");
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;

        let deleted = if force {
            conn.execute(
                "DELETE FROM sigil_blackboard WHERE project = ?1 AND key = ?2",
                rusqlite::params![project, &claim_key],
            )?
        } else {
            conn.execute(
                "DELETE FROM sigil_blackboard WHERE project = ?1 AND key = ?2 AND agent = ?3",
                rusqlite::params![project, &claim_key, agent],
            )?
        };

        Ok(deleted > 0)
    }

    /// Delete an entry by project and key.
    pub fn delete_by_key(&self, project: &str, key: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let deleted = conn.execute(
            "DELETE FROM sigil_blackboard WHERE project = ?1 AND key = ?2",
            rusqlite::params![project, key],
        )?;
        Ok(deleted > 0)
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
            return self.list_project_internal(&conn, project, limit, &now, None);
        }

        // Query all non-expired entries for the project, then filter by tag in Rust
        // (SQLite JSON queries are limited).
        let entries = self.list_project_internal(&conn, project, 1000, &now, None)?;
        let mut matched: Vec<BlackboardEntry> = entries
            .into_iter()
            .filter(|e| e.tags.iter().any(|t| tags.contains(t)))
            .collect();
        matched.truncate(limit as usize);
        Ok(matched)
    }

    /// Query entries filtered by the caller's visibility scope.
    ///
    /// Key prefix rules:
    /// - `system:*` → always visible
    /// - `project:{name}:*` → visible if `visibility.project` matches `{name}`
    /// - `dept:{name}:*` → visible if `visibility.department` matches `{name}`
    /// - `agent:{uuid}:*` → visible only if `visibility.agent_id` matches `{uuid}`
    /// - `session:*` → always visible (session entries are already scoped by task)
    /// - No recognised prefix → always visible (backwards compatible)
    pub fn query_scoped(
        &self,
        project: &str,
        visibility: &AgentVisibility,
        tags: &[String],
        limit: usize,
    ) -> Result<Vec<BlackboardEntry>> {
        // Fetch unfiltered entries (use a generous internal limit).
        let raw = self.query(project, tags, 1000)?;

        let mut filtered: Vec<BlackboardEntry> = raw
            .into_iter()
            .filter(|e| entry_visible(&e.key, visibility))
            .collect();

        filtered.truncate(limit);
        Ok(filtered)
    }

    /// Query entries created after `since` for efficient polling.
    pub fn query_since(
        &self,
        project: &str,
        tags: &[String],
        since: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<BlackboardEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();

        let entries = self.list_project_internal(&conn, project, 1000, &now, Some(since))?;
        if tags.is_empty() {
            let mut result = entries;
            result.truncate(limit as usize);
            return Ok(result);
        }

        let mut matched: Vec<BlackboardEntry> = entries
            .into_iter()
            .filter(|e| e.tags.iter().any(|t| tags.contains(t)))
            .collect();
        matched.truncate(limit as usize);
        Ok(matched)
    }

    /// Query entries across all projects (for cross-service coordination).
    pub fn query_cross_project(
        &self,
        tags: &[String],
        since: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<BlackboardEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();

        let entries = if let Some(since_dt) = since {
            let since_str = since_dt.to_rfc3339();
            let mut stmt = conn.prepare(
                "SELECT id, key, content, agent, project, tags_json, durability, created_at, expires_at
                 FROM sigil_blackboard WHERE expires_at > ?1 AND created_at > ?2
                 ORDER BY created_at DESC LIMIT ?3",
            )?;
            stmt.query_map(rusqlite::params![now, since_str, 1000u32], Self::row_to_entry)?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>()
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, key, content, agent, project, tags_json, durability, created_at, expires_at
                 FROM sigil_blackboard WHERE expires_at > ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?;
            stmt.query_map(rusqlite::params![now, 1000u32], Self::row_to_entry)?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>()
        };

        if tags.is_empty() {
            let mut result = entries;
            result.truncate(limit as usize);
            return Ok(result);
        }

        let mut matched: Vec<BlackboardEntry> = entries
            .into_iter()
            .filter(|e| e.tags.iter().any(|t| tags.contains(t)))
            .collect();
        matched.truncate(limit as usize);
        Ok(matched)
    }

    /// Check if a resource is claimed (for hook enforcement).
    pub fn check_claim(&self, resource: &str, project: &str) -> Result<Option<(String, String)>> {
        let claim_key = format!("claim:{resource}");
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();

        let result: Option<(String, String)> = conn
            .prepare(
                "SELECT agent, content FROM sigil_blackboard
                 WHERE project = ?1 AND key = ?2 AND expires_at > ?3
                 ORDER BY created_at DESC LIMIT 1",
            )?
            .query_row(
                rusqlite::params![project, &claim_key, &now],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        Ok(result)
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
        self.list_project_internal(&conn, project, limit, &now, None)
    }

    fn list_project_internal(
        &self,
        conn: &Connection,
        project: &str,
        limit: u32,
        now: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<BlackboardEntry>> {
        let entries = if let Some(since_dt) = since {
            let since_str = since_dt.to_rfc3339();
            let mut stmt = conn.prepare(
                "SELECT id, key, content, agent, project, tags_json, durability, created_at, expires_at
                 FROM sigil_blackboard WHERE project = ?1 AND expires_at > ?2 AND created_at > ?3
                 ORDER BY created_at DESC LIMIT ?4",
            )?;
            stmt.query_map(
                rusqlite::params![project, now, since_str, limit],
                Self::row_to_entry,
            )?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, key, content, agent, project, tags_json, durability, created_at, expires_at
                 FROM sigil_blackboard WHERE project = ?1 AND expires_at > ?2
                 ORDER BY created_at DESC LIMIT ?3",
            )?;
            stmt.query_map(rusqlite::params![project, now, limit], Self::row_to_entry)?
                .filter_map(|r| r.ok())
                .collect()
        };

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

/// Check whether a blackboard key is visible to the given agent scope.
fn entry_visible(key: &str, vis: &AgentVisibility) -> bool {
    if key.starts_with("system:") || key.starts_with("session:") {
        return true;
    }
    if let Some(rest) = key.strip_prefix("project:") {
        return match vis.project.as_deref() {
            Some(p) => rest.starts_with(&format!("{p}:")),
            None => false,
        };
    }
    if let Some(rest) = key.strip_prefix("dept:") {
        return match vis.department.as_deref() {
            Some(d) => rest.starts_with(&format!("{d}:")),
            None => false,
        };
    }
    if let Some(rest) = key.strip_prefix("agent:") {
        return match vis.agent_id.as_deref() {
            Some(id) => rest.starts_with(&format!("{id}:")),
            None => false,
        };
    }
    // No recognised scope prefix — backwards-compatible, always visible.
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_bb() -> (Blackboard, TempDir) {
        let dir = TempDir::new().unwrap();
        let bb = Blackboard::open(&dir.path().join("bb.db"), 24, 7, 2).unwrap();
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

        bb.post("k1", "c1", "a", "p", &[], EntryDurability::Transient)
            .unwrap();

        // Manually expire it
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
        let transient = serde_json::to_value(EntryDurability::Transient).unwrap();
        assert_eq!(transient, serde_json::json!("transient"));
        let durable = serde_json::to_value(EntryDurability::Durable).unwrap();
        assert_eq!(durable, serde_json::json!("durable"));
    }

    #[test]
    fn test_claim_acquire() {
        let (bb, _dir) = temp_bb();

        let result = bb.claim("src/main.rs", "worker-1", "proj", "refactoring main").unwrap();
        assert_eq!(result, ClaimResult::Acquired);

        // Verify the claim entry exists
        let entry = bb.get_by_key("proj", "claim:src/main.rs").unwrap();
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.agent, "worker-1");
        assert_eq!(entry.content, "refactoring main");
        assert!(entry.tags.contains(&"claim".to_string()));
    }

    #[test]
    fn test_claim_contention() {
        let (bb, _dir) = temp_bb();

        // First agent claims
        let r1 = bb.claim("src/api.rs", "worker-1", "proj", "adding endpoint").unwrap();
        assert_eq!(r1, ClaimResult::Acquired);

        // Second agent tries to claim same resource
        let r2 = bb.claim("src/api.rs", "worker-2", "proj", "fixing bug").unwrap();
        assert_eq!(
            r2,
            ClaimResult::Held {
                holder: "worker-1".to_string(),
                content: "adding endpoint".to_string(),
            }
        );
    }

    #[test]
    fn test_claim_renew() {
        let (bb, _dir) = temp_bb();

        let r1 = bb.claim("src/lib.rs", "worker-1", "proj", "initial work").unwrap();
        assert_eq!(r1, ClaimResult::Acquired);

        // Same agent re-claims (renew)
        let r2 = bb.claim("src/lib.rs", "worker-1", "proj", "still working").unwrap();
        assert_eq!(r2, ClaimResult::Renewed);

        // Content should be updated
        let entry = bb.get_by_key("proj", "claim:src/lib.rs").unwrap().unwrap();
        assert_eq!(entry.content, "still working");
    }

    #[test]
    fn test_release() {
        let (bb, _dir) = temp_bb();

        bb.claim("src/mod.rs", "worker-1", "proj", "editing").unwrap();

        // Wrong agent can't release
        let released = bb.release("src/mod.rs", "worker-2", "proj", false).unwrap();
        assert!(!released);

        // Right agent can release
        let released = bb.release("src/mod.rs", "worker-1", "proj", false).unwrap();
        assert!(released);

        // Claim is gone
        let entry = bb.get_by_key("proj", "claim:src/mod.rs").unwrap();
        assert!(entry.is_none());
    }

    #[test]
    fn test_release_force() {
        let (bb, _dir) = temp_bb();

        bb.claim("src/mod.rs", "worker-1", "proj", "editing").unwrap();

        // Force release by different agent
        let released = bb.release("src/mod.rs", "worker-2", "proj", true).unwrap();
        assert!(released);

        let entry = bb.get_by_key("proj", "claim:src/mod.rs").unwrap();
        assert!(entry.is_none());
    }

    #[test]
    fn test_delete_by_key() {
        let (bb, _dir) = temp_bb();

        bb.post("signal:build-broken", "compile error in handler.rs", "ci", "proj", &["signal".to_string()], EntryDurability::Transient).unwrap();

        let deleted = bb.delete_by_key("proj", "signal:build-broken").unwrap();
        assert!(deleted);

        let entry = bb.get_by_key("proj", "signal:build-broken").unwrap();
        assert!(entry.is_none());

        // Delete non-existent key
        let deleted = bb.delete_by_key("proj", "nope").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_query_since() {
        let (bb, _dir) = temp_bb();

        let before = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(10));

        bb.post("k1", "old", "a", "p", &[], EntryDurability::Transient).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        let mid = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(10));

        bb.post("k2", "new", "a", "p", &[], EntryDurability::Transient).unwrap();

        // Query since before — should get both
        let all = bb.query_since("p", &[], before, 10).unwrap();
        assert_eq!(all.len(), 2);

        // Query since mid — should get only k2
        let recent = bb.query_since("p", &[], mid, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].key, "k2");
    }

    #[test]
    fn test_cross_project_query() {
        let (bb, _dir) = temp_bb();

        bb.post("signal:api-changed", "POST /users now returns 201", "w1", "backend",
            &["signal".to_string()], EntryDurability::Durable).unwrap();
        bb.post("finding:css-bug", "overflow in sidebar", "w2", "frontend",
            &["finding".to_string()], EntryDurability::Transient).unwrap();

        // Cross-project query — gets both
        let all = bb.query_cross_project(&[], None, 10).unwrap();
        assert_eq!(all.len(), 2);

        // Filter by tag
        let signals = bb.query_cross_project(&["signal".to_string()], None, 10).unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].project, "backend");
    }

    #[test]
    fn test_check_claim() {
        let (bb, _dir) = temp_bb();

        // No claim
        let result = bb.check_claim("src/main.rs", "proj").unwrap();
        assert!(result.is_none());

        // After claiming
        bb.claim("src/main.rs", "worker-1", "proj", "working on it").unwrap();
        let result = bb.check_claim("src/main.rs", "proj").unwrap();
        assert!(result.is_some());
        let (agent, content) = result.unwrap();
        assert_eq!(agent, "worker-1");
        assert_eq!(content, "working on it");
    }

    #[test]
    fn test_claim_expired_allows_new_claim() {
        let (bb, _dir) = temp_bb();

        bb.claim("src/main.rs", "worker-1", "proj", "working").unwrap();

        // Manually expire the claim
        {
            let conn = bb.conn.lock().unwrap();
            let expired = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
            conn.execute(
                "UPDATE sigil_blackboard SET expires_at = ?1 WHERE key = 'claim:src/main.rs'",
                rusqlite::params![expired],
            )
            .unwrap();
        }

        // Different agent can now claim
        let result = bb.claim("src/main.rs", "worker-2", "proj", "taking over").unwrap();
        assert_eq!(result, ClaimResult::Acquired);
    }

    // ── Scoped visibility tests ──────────────────────────────────────

    #[test]
    fn test_entry_visible_helper() {
        let vis = AgentVisibility {
            agent_id: Some("a-123".into()),
            project: Some("alpha".into()),
            department: Some("eng".into()),
        };

        // system: always visible
        assert!(super::entry_visible("system:config", &vis));

        // session: always visible
        assert!(super::entry_visible("session:abc:step1", &vis));

        // project: matching
        assert!(super::entry_visible("project:alpha:schema", &vis));
        // project: non-matching
        assert!(!super::entry_visible("project:beta:schema", &vis));

        // dept: matching
        assert!(super::entry_visible("dept:eng:oncall", &vis));
        // dept: non-matching
        assert!(!super::entry_visible("dept:sales:pipeline", &vis));

        // agent: matching
        assert!(super::entry_visible("agent:a-123:scratch", &vis));
        // agent: non-matching
        assert!(!super::entry_visible("agent:b-456:scratch", &vis));

        // No prefix — backwards compatible
        assert!(super::entry_visible("finding:something", &vis));
        assert!(super::entry_visible("plain-key", &vis));

        // Empty visibility — only system/session/unprefixed visible
        let empty = AgentVisibility::default();
        assert!(super::entry_visible("system:x", &empty));
        assert!(super::entry_visible("session:x", &empty));
        assert!(super::entry_visible("plain-key", &empty));
        assert!(!super::entry_visible("project:alpha:x", &empty));
        assert!(!super::entry_visible("dept:eng:x", &empty));
        assert!(!super::entry_visible("agent:a-123:x", &empty));
    }

    #[test]
    fn test_query_scoped() {
        let (bb, _dir) = temp_bb();
        let proj = "proj";

        // Post entries with various scoped keys
        bb.post("system:config", "global cfg", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("project:proj:schema", "table def", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("project:other:schema", "other table", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("dept:eng:oncall", "alice", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("dept:sales:quota", "100", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("agent:a1:scratch", "private", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("agent:a2:scratch", "other private", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("session:s1:step", "step data", "w", proj, &[], EntryDurability::Transient).unwrap();
        bb.post("plain-finding", "no prefix", "w", proj, &[], EntryDurability::Transient).unwrap();

        // Agent in project "proj", dept "eng", id "a1"
        let vis = AgentVisibility {
            agent_id: Some("a1".into()),
            project: Some("proj".into()),
            department: Some("eng".into()),
        };

        let results = bb.query_scoped(proj, &vis, &[], 100).unwrap();
        let keys: Vec<&str> = results.iter().map(|e| e.key.as_str()).collect();

        // Should see: system:config, project:proj:schema, dept:eng:oncall,
        //             agent:a1:scratch, session:s1:step, plain-finding
        assert!(keys.contains(&"system:config"));
        assert!(keys.contains(&"project:proj:schema"));
        assert!(keys.contains(&"dept:eng:oncall"));
        assert!(keys.contains(&"agent:a1:scratch"));
        assert!(keys.contains(&"session:s1:step"));
        assert!(keys.contains(&"plain-finding"));

        // Should NOT see: project:other:schema, dept:sales:quota, agent:a2:scratch
        assert!(!keys.contains(&"project:other:schema"));
        assert!(!keys.contains(&"dept:sales:quota"));
        assert!(!keys.contains(&"agent:a2:scratch"));

        assert_eq!(results.len(), 6);

        // Test limit
        let limited = bb.query_scoped(proj, &vis, &[], 3).unwrap();
        assert_eq!(limited.len(), 3);
    }
}
