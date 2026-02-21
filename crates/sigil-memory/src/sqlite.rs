use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use sigil_core::traits::{Memory, MemoryCategory, MemoryEntry, MemoryQuery};
use std::path::Path;
use std::sync::Mutex;
use tracing::debug;

/// SQLite + FTS5 memory backend. Per-rig isolated databases.
pub struct SqliteMemory {
    conn: Mutex<Connection>,
    /// Temporal decay half-life in days.
    decay_halflife_days: f64,
}

impl SqliteMemory {
    /// Open or create a SQLite memory database.
    pub fn open(path: &Path, decay_halflife_days: f64) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open memory DB: {}", path.display()))?;

        // Enable WAL mode for concurrent reads.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        // Create tables.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                key TEXT NOT NULL,
                content TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'fact',
                session_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_memories_key ON memories(key);
            CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
            CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
            ",
        )?;

        // Create FTS5 virtual table if not exists.
        // Use IF NOT EXISTS via checking sqlite_master.
        let fts_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='memories_fts'",
            [],
            |row| row.get(0),
        )?;

        if !fts_exists {
            conn.execute_batch(
                "CREATE VIRTUAL TABLE memories_fts USING fts5(
                    key, content, content=memories, content_rowid=rowid
                );

                CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                    INSERT INTO memories_fts(rowid, key, content) VALUES (new.rowid, new.key, new.content);
                END;

                CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, key, content) VALUES('delete', old.rowid, old.key, old.content);
                END;

                CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, key, content) VALUES('delete', old.rowid, old.key, old.content);
                    INSERT INTO memories_fts(rowid, key, content) VALUES (new.rowid, new.key, new.content);
                END;",
            )?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
            decay_halflife_days,
        })
    }

    /// Compute temporal decay factor: e^(-ln(2)/halflife * age_days)
    fn decay_factor(&self, created_at: &DateTime<Utc>) -> f64 {
        let age_days = (Utc::now() - *created_at).num_seconds() as f64 / 86400.0;
        if age_days <= 0.0 {
            return 1.0;
        }
        let lambda = (2.0_f64).ln() / self.decay_halflife_days;
        (-lambda * age_days).exp()
    }
}

#[async_trait]
impl Memory for SqliteMemory {
    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let cat = serde_json::to_string(&category)?.trim_matches('"').to_string();

        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        conn.execute(
            "INSERT INTO memories (id, key, content, category, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, key, content, cat, now],
        )?;

        debug!(id = %id, key = %key, "memory stored");
        Ok(id)
    }

    async fn search(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        // Use FTS5 for keyword search with BM25 ranking.
        let sql = "SELECT m.id, m.key, m.content, m.category, m.created_at, m.session_id,
                          bm25(memories_fts) as rank
                   FROM memories_fts f
                   JOIN memories m ON m.rowid = f.rowid
                   WHERE memories_fts MATCH ?1
                   ORDER BY rank
                   LIMIT ?2";

        // FTS5 query: escape special characters.
        let fts_query = query
            .text
            .split_whitespace()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(" OR ");

        let mut stmt = conn.prepare(sql)?;
        let entries = stmt
            .query_map(rusqlite::params![fts_query, query.top_k as i64], |row| {
                let id: String = row.get(0)?;
                let key: String = row.get(1)?;
                let content: String = row.get(2)?;
                let cat_str: String = row.get(3)?;
                let created_str: String = row.get(4)?;
                let session_id: Option<String> = row.get(5)?;
                let bm25_score: f64 = row.get(6)?;

                Ok((id, key, content, cat_str, created_str, session_id, bm25_score))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, key, content, cat_str, created_str, session_id, bm25_score)| {
                let category = match cat_str.as_str() {
                    "fact" => MemoryCategory::Fact,
                    "procedure" => MemoryCategory::Procedure,
                    "preference" => MemoryCategory::Preference,
                    "context" => MemoryCategory::Context,
                    "evergreen" => MemoryCategory::Evergreen,
                    _ => MemoryCategory::Fact,
                };

                // Filter by category if specified.
                if let Some(ref q_cat) = query.category {
                    if &category != q_cat {
                        return None;
                    }
                }

                // Filter by session if specified.
                if let Some(ref q_session) = query.session_id {
                    if session_id.as_deref() != Some(q_session.as_str()) {
                        return None;
                    }
                }

                let created_at = DateTime::parse_from_rfc3339(&created_str)
                    .ok()?
                    .with_timezone(&Utc);

                // Apply temporal decay (evergreen memories don't decay).
                let decay = if category == MemoryCategory::Evergreen {
                    1.0
                } else {
                    self.decay_factor(&created_at)
                };

                // BM25 returns negative scores (more negative = more relevant).
                let raw_score = -bm25_score;
                let score = raw_score * decay;

                Some(MemoryEntry {
                    id,
                    key,
                    content,
                    category,
                    created_at,
                    session_id,
                    score,
                })
            })
            .collect::<Vec<_>>();

        // Re-sort by decay-adjusted score.
        let mut entries = entries;
        entries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(entries)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        conn.execute("DELETE FROM memories WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    fn name(&self) -> &str {
        "sqlite"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_search() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test_memory.db");
        let mem = SqliteMemory::open(&db_path, 30.0).unwrap();

        // Store some memories.
        mem.store("login-flow", "The login uses JWT tokens with 24h expiry", MemoryCategory::Fact)
            .await
            .unwrap();
        mem.store("deploy-process", "Deploy by merging to dev branch, auto-deploys", MemoryCategory::Procedure)
            .await
            .unwrap();
        mem.store("db-config", "PostgreSQL on port 5432 with TimescaleDB", MemoryCategory::Fact)
            .await
            .unwrap();

        // Search.
        let results = mem
            .search(&MemoryQuery::new("login JWT", 10))
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert!(results[0].content.contains("JWT"));

        let results = mem
            .search(&MemoryQuery::new("deploy", 10))
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert!(results[0].content.contains("deploy"));
    }
}
