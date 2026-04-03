//! Persistent Session Store — SQLite-backed conversation history
//! that survives daemon restarts.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// A single session message.
#[derive(Debug, Clone)]
pub struct SessionMessage {
    pub chat_id: i64,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub source: Option<String>,
}

/// A single typed thread event in a chat timeline.
#[derive(Debug, Clone)]
pub struct ThreadEvent {
    pub id: i64,
    pub chat_id: i64,
    pub event_type: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub source: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Persistent session store backed by SQLite.
pub struct SessionStore {
    db: Arc<Mutex<rusqlite::Connection>>,
    /// Max messages per chat before auto-summarization kicks in.
    pub max_messages_per_chat: usize,
}

impl SessionStore {
    /// Open or create a session store at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir: {}", parent.display()))?;
        }

        let conn = rusqlite::Connection::open(path)
            .with_context(|| format!("failed to open conversation db: {}", path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;

             CREATE TABLE IF NOT EXISTS conversations (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 chat_id INTEGER NOT NULL,
                 role TEXT NOT NULL,
                 content TEXT NOT NULL,
                 timestamp TEXT NOT NULL,
                 summarized INTEGER DEFAULT 0,
                 source TEXT DEFAULT NULL,
                 event_type TEXT NOT NULL DEFAULT 'message',
                 metadata TEXT DEFAULT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_conv_chat ON conversations(chat_id);
             CREATE INDEX IF NOT EXISTS idx_conv_ts ON conversations(timestamp);

             CREATE TABLE IF NOT EXISTS conversation_summaries (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 chat_id INTEGER NOT NULL,
                 summary TEXT NOT NULL,
                 covers_until TEXT NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_summ_chat ON conversation_summaries(chat_id);

             -- NOTE: `channels` conceptually represents sessions; not renamed to avoid data migration risk.
             CREATE TABLE IF NOT EXISTS channels (
                 chat_id INTEGER PRIMARY KEY,
                 channel_type TEXT NOT NULL,
                 name TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );",
        )
        .context("failed to initialize conversation schema")?;

        // FTS5 virtual table for full-text search across transcripts.
        let _ = conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                 content,
                 content=conversations,
                 content_rowid=id
             );
             -- Triggers to keep FTS5 in sync with base table.
             CREATE TRIGGER IF NOT EXISTS conversations_ai AFTER INSERT ON conversations BEGIN
                 INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
             END;
             CREATE TRIGGER IF NOT EXISTS conversations_ad AFTER DELETE ON conversations BEGIN
                 INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.id, old.content);
             END;
             CREATE TRIGGER IF NOT EXISTS conversations_au AFTER UPDATE ON conversations BEGIN
                 INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.id, old.content);
                 INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
             END;",
        );

        // Migrations (idempotent).
        let _ =
            conn.execute_batch("ALTER TABLE conversations ADD COLUMN source TEXT DEFAULT NULL;");
        let _ = conn.execute_batch(
            "ALTER TABLE conversations ADD COLUMN event_type TEXT DEFAULT 'message';",
        );
        let _ =
            conn.execute_batch("ALTER TABLE conversations ADD COLUMN metadata TEXT DEFAULT NULL;");

        debug!(path = %path.display(), "session store opened");

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            max_messages_per_chat: 30,
        })
    }

    /// Record a message in a conversation.
    pub async fn record(&self, chat_id: i64, role: &str, content: &str) -> Result<()> {
        self.record_with_source(chat_id, role, content, None).await
    }

    /// Record a message with source tag (e.g. "telegram", "web").
    pub async fn record_with_source(
        &self,
        chat_id: i64,
        role: &str,
        content: &str,
        source: Option<&str>,
    ) -> Result<()> {
        self.record_event(chat_id, "message", role, content, source, None)
            .await
    }

    /// Record a typed event in a conversation timeline.
    pub async fn record_event(
        &self,
        chat_id: i64,
        event_type: &str,
        role: &str,
        content: &str,
        source: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<()> {
        let db = self.db.lock().await;
        let now = Utc::now().to_rfc3339();
        let metadata_text = metadata.map(serde_json::Value::to_string);
        db.execute(
            "INSERT INTO conversations (chat_id, role, content, timestamp, source, event_type, metadata) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![chat_id, role, content, now, source, event_type, metadata_text],
        )
        .context("failed to insert conversation message")?;
        Ok(())
    }

    /// Get recent messages for a chat (most recent `limit` messages, with optional offset for pagination).
    pub async fn recent(&self, chat_id: i64, limit: usize) -> Result<Vec<SessionMessage>> {
        self.recent_with_offset(chat_id, limit, 0).await
    }

    /// Get messages for a chat with offset-based pagination.
    /// Offset 0 = most recent, offset N = skip N newest messages.
    pub async fn recent_with_offset(
        &self,
        chat_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SessionMessage>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT chat_id, role, content, timestamp, source FROM conversations \
             WHERE chat_id = ?1 AND summarized = 0 AND event_type = 'message' \
             ORDER BY id DESC LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt
            .query_map(params![chat_id, limit as i64, offset as i64], |row| {
                Ok(SessionMessage {
                    chat_id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    timestamp: row.get::<_, String>(3).map(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now())
                    })?,
                    source: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Reverse to chronological order.
        let mut messages = rows;
        messages.reverse();
        Ok(messages)
    }

    /// Get conversation context formatted as a string (for injection into task descriptions).
    pub async fn context_string(&self, chat_id: i64, limit: usize) -> Result<String> {
        let messages = self.recent(chat_id, limit).await?;
        if messages.is_empty() {
            return Ok(String::new());
        }

        // Prepend any summary if available.
        let db = self.db.lock().await;
        let summary: Option<String> = db
            .query_row(
                "SELECT summary FROM conversation_summaries WHERE chat_id = ?1 ORDER BY id DESC LIMIT 1",
                params![chat_id],
                |row| row.get(0),
            )
            .ok();
        drop(db);

        let mut ctx = String::from("## Conversation History\n\n");

        if let Some(ref s) = summary {
            ctx.push_str(&format!("*Earlier context:* {s}\n\n"));
        }

        for msg in &messages {
            ctx.push_str(&format!("**{}**: {}\n\n", msg.role, msg.content));
        }

        Ok(ctx)
    }

    /// Full-text search across all transcript channels.
    /// Returns matching messages with their channel and timestamp.
    pub async fn search_transcripts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionMessage>> {
        let db = self.db.lock().await;

        // Search via FTS5 on transcript channels only.
        let mut stmt = db.prepare(
            "SELECT c.chat_id, c.role, c.content, c.timestamp, c.source
             FROM conversations c
             JOIN channels ch ON c.chat_id = ch.chat_id
             WHERE ch.channel_type = 'transcript'
               AND c.rowid IN (
                   SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?1
               )
             ORDER BY c.timestamp DESC
             LIMIT ?2",
        )?;

        let messages = stmt
            .query_map(params![query, limit as i64], |row| {
                Ok(SessionMessage {
                    chat_id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    timestamp: row
                        .get::<_, String>(3)
                        .ok()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    source: row.get(4).ok(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    /// Count unsummarized messages for a chat.
    pub async fn message_count(&self, chat_id: i64) -> Result<usize> {
        let db = self.db.lock().await;
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM conversations WHERE chat_id = ?1 AND summarized = 0 AND event_type = 'message'",
            params![chat_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Store a summary and mark older messages as summarized.
    pub async fn save_summary(
        &self,
        chat_id: i64,
        summary: &str,
        keep_recent: usize,
    ) -> Result<()> {
        let db = self.db.lock().await;
        let now = Utc::now().to_rfc3339();

        db.execute(
            "INSERT INTO conversation_summaries (chat_id, summary, covers_until) VALUES (?1, ?2, ?3)",
            params![chat_id, summary, now],
        )?;

        // Mark all but the most recent `keep_recent` as summarized.
        db.execute(
            "UPDATE conversations SET summarized = 1 WHERE chat_id = ?1 AND summarized = 0 AND event_type = 'message' \
             AND id NOT IN (SELECT id FROM conversations WHERE chat_id = ?1 AND summarized = 0 AND event_type = 'message' ORDER BY id DESC LIMIT ?2)",
            params![chat_id, keep_recent as i64],
        )?;

        debug!(chat_id, "conversation summary saved");
        Ok(())
    }

    /// Evict conversations older than the given duration.
    pub async fn evict_older_than(&self, hours: i64) -> Result<usize> {
        let cutoff = (Utc::now() - chrono::TimeDelta::hours(hours)).to_rfc3339();
        let db = self.db.lock().await;

        let deleted: usize = db.execute(
            "DELETE FROM conversations WHERE timestamp < ?1",
            params![cutoff],
        )?;

        if deleted > 0 {
            debug!(deleted, hours, "evicted old conversation messages");
        }

        Ok(deleted)
    }

    /// Get typed timeline events for a chat.
    pub async fn timeline(&self, chat_id: i64, limit: usize) -> Result<Vec<ThreadEvent>> {
        self.timeline_with_offset(chat_id, limit, 0).await
    }

    /// Get timeline events for a chat with offset-based pagination.
    pub async fn timeline_with_offset(
        &self,
        chat_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ThreadEvent>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, chat_id, event_type, role, content, timestamp, source, metadata \
             FROM conversations \
             WHERE chat_id = ?1 AND summarized = 0 \
             ORDER BY id DESC LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt
            .query_map(params![chat_id, limit as i64, offset as i64], |row| {
                let metadata_text: Option<String> = row.get(7)?;
                Ok(ThreadEvent {
                    id: row.get(0)?,
                    chat_id: row.get(1)?,
                    event_type: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    timestamp: row.get::<_, String>(5).map(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now())
                    })?,
                    source: row.get(6)?,
                    metadata: metadata_text
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut events = rows;
        events.reverse();
        Ok(events)
    }

    /// Get transcript for a specific task.
    pub async fn task_transcript(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionMessage>> {
        let channel_name = format!("transcript:task:{}", task_id);
        let chat_id = named_channel_chat_id(&channel_name);
        self.recent(chat_id, limit).await
    }

    // ── Channel methods ──

    /// Ensure a channel exists, creating it if needed.
    pub async fn ensure_channel(&self, chat_id: i64, channel_type: &str, name: &str) -> Result<()> {
        let db = self.db.lock().await;
        let now = Utc::now().to_rfc3339();
        db.execute(
            "INSERT INTO channels (chat_id, channel_type, name, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(chat_id) DO NOTHING",
            params![chat_id, channel_type, name, now],
        )?;
        Ok(())
    }

    /// List all channels with their last message.
    pub async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT ch.chat_id, ch.channel_type, ch.name, ch.created_at,
                    (SELECT content FROM conversations WHERE chat_id = ch.chat_id AND event_type = 'message' ORDER BY id DESC LIMIT 1),
                    (SELECT timestamp FROM conversations WHERE chat_id = ch.chat_id AND event_type = 'message' ORDER BY id DESC LIMIT 1)
             FROM channels ch
             ORDER BY ch.created_at",
        )?;
        let results = stmt
            .query_map([], |row| {
                Ok(ChannelInfo {
                    chat_id: row.get(0)?,
                    channel_type: row.get(1)?,
                    name: row.get(2)?,
                    created_at: row.get(3)?,
                    last_message: row.get(4)?,
                    last_message_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }
}

/// Mask to keep chat IDs within JS MAX_SAFE_INTEGER (2^53 - 1).
/// Bottom 4 bits reserved for channel-type tag.
const JS_SAFE_MASK: u64 = 0x1F_FFFF_FFFF_FFF0;

fn hashed_chat_id(key: &str, tag: u64) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash & JS_SAFE_MASK | tag) as i64
}

/// Deterministic chat ID for a project-wide channel.
pub fn company_chat_id(project_name: &str) -> i64 {
    hashed_chat_id(&format!("project:{project_name}"), 1)
}

/// Deterministic chat ID for a named shared channel.
pub fn named_channel_chat_id(channel_name: &str) -> i64 {
    hashed_chat_id(&format!("channel:{channel_name}"), 2)
}

/// Deterministic chat ID for a department channel within a company.
pub fn department_chat_id(project_name: &str, department: &str) -> i64 {
    hashed_chat_id(&format!("dept:{project_name}:{department}"), 4)
}

/// Deterministic chat ID for the agency-wide group chat.
pub fn agency_chat_id() -> i64 {
    hashed_chat_id("agency:global", 3)
}

/// Channel metadata returned by `list_channels`.
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    pub chat_id: i64,
    pub channel_type: String,
    pub name: String,
    pub created_at: String,
    pub last_message: Option<String>,
    pub last_message_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_record_and_recent() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store.record(123, "User", "hello").await.unwrap();
        store.record(123, "Assistant", "hi there").await.unwrap();
        store.record(123, "User", "how are you?").await.unwrap();

        let msgs = store.recent(123, 10).await.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "User");
        assert_eq!(msgs[0].content, "hello");
        assert_eq!(msgs[2].content, "how are you?");
    }

    #[tokio::test]
    async fn test_recent_limit() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        for i in 0..10 {
            store.record(1, "User", &format!("msg {i}")).await.unwrap();
        }

        let msgs = store.recent(1, 3).await.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "msg 7");
        assert_eq!(msgs[2].content, "msg 9");
    }

    #[tokio::test]
    async fn test_context_string() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store.record(42, "User", "hello").await.unwrap();
        store.record(42, "Assistant", "world").await.unwrap();

        let ctx = store.context_string(42, 10).await.unwrap();
        assert!(ctx.contains("Conversation History"));
        assert!(ctx.contains("**User**: hello"));
        assert!(ctx.contains("**Assistant**: world"));
    }

    #[tokio::test]
    async fn test_context_string_empty() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        let ctx = store.context_string(999, 10).await.unwrap();
        assert!(ctx.is_empty());
    }

    #[tokio::test]
    async fn test_save_summary() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        for i in 0..10 {
            store.record(1, "User", &format!("msg {i}")).await.unwrap();
        }

        store
            .save_summary(1, "User said messages 0-7", 2)
            .await
            .unwrap();

        // Only 2 recent messages should remain unsummarized.
        let msgs = store.recent(1, 100).await.unwrap();
        assert_eq!(msgs.len(), 2);

        // Summary should appear in context.
        let ctx = store.context_string(1, 100).await.unwrap();
        assert!(ctx.contains("User said messages 0-7"));
    }

    #[tokio::test]
    async fn test_message_count() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store.record(1, "User", "a").await.unwrap();
        store.record(1, "User", "b").await.unwrap();
        store.record(2, "User", "c").await.unwrap();

        assert_eq!(store.message_count(1).await.unwrap(), 2);
        assert_eq!(store.message_count(2).await.unwrap(), 1);
        assert_eq!(store.message_count(999).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_chat_isolation() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store.record(1, "User", "chat1").await.unwrap();
        store.record(2, "User", "chat2").await.unwrap();

        let msgs1 = store.recent(1, 10).await.unwrap();
        let msgs2 = store.recent(2, 10).await.unwrap();

        assert_eq!(msgs1.len(), 1);
        assert_eq!(msgs1[0].content, "chat1");
        assert_eq!(msgs2.len(), 1);
        assert_eq!(msgs2[0].content, "chat2");
    }

    #[tokio::test]
    async fn test_timeline_records_typed_events() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store.record(7, "User", "hello").await.unwrap();
        store
            .record_event(
                7,
                "task_created",
                "system",
                "Task sg-001 created.",
                Some("web"),
                Some(&serde_json::json!({"task_id": "sg-001"})),
            )
            .await
            .unwrap();

        let events = store.timeline(7, 10).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "message");
        assert_eq!(events[1].event_type, "task_created");
        assert_eq!(
            events[1]
                .metadata
                .as_ref()
                .and_then(|m| m.get("task_id"))
                .and_then(|v| v.as_str()),
            Some("sg-001")
        );

        let messages = store.recent(7, 10).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "hello");
    }

    // ── Channel tests ──

    #[tokio::test]
    async fn test_ensure_channel_and_list() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store
            .ensure_channel(100, "company", "myproject")
            .await
            .unwrap();
        store.ensure_channel(200, "dm", "akira").await.unwrap();

        let channels = store.list_channels().await.unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].channel_type, "company");
        assert_eq!(channels[0].name, "myproject");
        assert_eq!(channels[1].channel_type, "dm");
        assert_eq!(channels[1].name, "akira");
    }

    #[tokio::test]
    async fn test_ensure_channel_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store.ensure_channel(100, "company", "proj").await.unwrap();
        store.ensure_channel(100, "company", "proj").await.unwrap();

        let channels = store.list_channels().await.unwrap();
        assert_eq!(channels.len(), 1);
    }

    #[tokio::test]
    async fn test_channel_with_messages() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::open(&dir.path().join("conv.db")).unwrap();

        store.ensure_channel(100, "company", "proj").await.unwrap();
        store.record(100, "User", "hello company").await.unwrap();
        store.record(100, "CTO", "hi there").await.unwrap();

        let channels = store.list_channels().await.unwrap();
        assert_eq!(channels[0].last_message.as_deref(), Some("hi there"));
        assert!(channels[0].last_message_at.is_some());

        let msgs = store.recent(100, 10).await.unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_deterministic_chat_ids_use_distinct_tags() {
        let project = company_chat_id("alpha");
        let department = department_chat_id("alpha", "backend");
        let named = named_channel_chat_id("ops");
        let agency = agency_chat_id();

        assert_ne!(project, department);
        assert_ne!(project, named);
        assert_ne!(project, agency);
        assert_ne!(department, named);
        assert_ne!(department, agency);
        assert_ne!(named, agency);
    }
}
