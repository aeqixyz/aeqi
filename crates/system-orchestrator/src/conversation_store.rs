//! Persistent Conversation Store — SQLite-backed conversation history
//! that survives daemon restarts.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// A single conversation message.
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub chat_id: i64,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// Persistent conversation store backed by SQLite.
pub struct ConversationStore {
    db: Arc<Mutex<rusqlite::Connection>>,
    /// Max messages per chat before auto-summarization kicks in.
    pub max_messages_per_chat: usize,
}

impl ConversationStore {
    /// Open or create a conversation store at the given path.
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
                 summarized INTEGER DEFAULT 0
             );

             CREATE INDEX IF NOT EXISTS idx_conv_chat ON conversations(chat_id);
             CREATE INDEX IF NOT EXISTS idx_conv_ts ON conversations(timestamp);

             CREATE TABLE IF NOT EXISTS conversation_summaries (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 chat_id INTEGER NOT NULL,
                 summary TEXT NOT NULL,
                 covers_until TEXT NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_summ_chat ON conversation_summaries(chat_id);",
        )
        .context("failed to initialize conversation schema")?;

        debug!(path = %path.display(), "conversation store opened");

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            max_messages_per_chat: 30,
        })
    }

    /// Record a message in a conversation.
    pub async fn record(&self, chat_id: i64, role: &str, content: &str) -> Result<()> {
        let db = self.db.lock().await;
        let now = Utc::now().to_rfc3339();
        db.execute(
            "INSERT INTO conversations (chat_id, role, content, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![chat_id, role, content, now],
        )
        .context("failed to insert conversation message")?;
        Ok(())
    }

    /// Get recent messages for a chat (most recent `limit` messages, with optional offset for pagination).
    pub async fn recent(&self, chat_id: i64, limit: usize) -> Result<Vec<ConversationMessage>> {
        self.recent_with_offset(chat_id, limit, 0).await
    }

    /// Get messages for a chat with offset-based pagination.
    /// Offset 0 = most recent, offset N = skip N newest messages.
    pub async fn recent_with_offset(&self, chat_id: i64, limit: usize, offset: usize) -> Result<Vec<ConversationMessage>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT chat_id, role, content, timestamp FROM conversations \
             WHERE chat_id = ?1 AND summarized = 0 \
             ORDER BY id DESC LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt
            .query_map(params![chat_id, limit as i64, offset as i64], |row| {
                Ok(ConversationMessage {
                    chat_id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    timestamp: row
                        .get::<_, String>(3)
                        .map(|s| {
                            DateTime::parse_from_rfc3339(&s)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now())
                        })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Reverse to chronological order.
        let mut messages = rows;
        messages.reverse();
        Ok(messages)
    }

    /// Get conversation context formatted as a string (for injection into quest descriptions).
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

    /// Count unsummarized messages for a chat.
    pub async fn message_count(&self, chat_id: i64) -> Result<usize> {
        let db = self.db.lock().await;
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM conversations WHERE chat_id = ?1 AND summarized = 0",
            params![chat_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Store a summary and mark older messages as summarized.
    pub async fn save_summary(&self, chat_id: i64, summary: &str, keep_recent: usize) -> Result<()> {
        let db = self.db.lock().await;
        let now = Utc::now().to_rfc3339();

        db.execute(
            "INSERT INTO conversation_summaries (chat_id, summary, covers_until) VALUES (?1, ?2, ?3)",
            params![chat_id, summary, now],
        )?;

        // Mark all but the most recent `keep_recent` as summarized.
        db.execute(
            "UPDATE conversations SET summarized = 1 WHERE chat_id = ?1 AND summarized = 0 \
             AND id NOT IN (SELECT id FROM conversations WHERE chat_id = ?1 AND summarized = 0 ORDER BY id DESC LIMIT ?2)",
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_record_and_recent() {
        let dir = TempDir::new().unwrap();
        let store = ConversationStore::open(&dir.path().join("conv.db")).unwrap();

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
        let store = ConversationStore::open(&dir.path().join("conv.db")).unwrap();

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
        let store = ConversationStore::open(&dir.path().join("conv.db")).unwrap();

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
        let store = ConversationStore::open(&dir.path().join("conv.db")).unwrap();

        let ctx = store.context_string(999, 10).await.unwrap();
        assert!(ctx.is_empty());
    }

    #[tokio::test]
    async fn test_save_summary() {
        let dir = TempDir::new().unwrap();
        let store = ConversationStore::open(&dir.path().join("conv.db")).unwrap();

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
        let store = ConversationStore::open(&dir.path().join("conv.db")).unwrap();

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
        let store = ConversationStore::open(&dir.path().join("conv.db")).unwrap();

        store.record(1, "User", "chat1").await.unwrap();
        store.record(2, "User", "chat2").await.unwrap();

        let msgs1 = store.recent(1, 10).await.unwrap();
        let msgs2 = store.recent(2, 10).await.unwrap();

        assert_eq!(msgs1.len(), 1);
        assert_eq!(msgs1[0].content, "chat1");
        assert_eq!(msgs2.len(), 1);
        assert_eq!(msgs2[0].content, "chat2");
    }
}
