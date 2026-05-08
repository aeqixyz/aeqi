//! SQLite event sink for aeqi-indexer.
//!
//! Schema (v1):
//! - `events` — one row per decoded Anchor event. Idempotent on
//!   (signature, program, event_type) so replays + reorgs don't double-count.
//! - `cursor` — last processed slot per program. Resumed at startup so the
//!   indexer can re-attach to a public RPC and skip-ahead-or-replay
//!   accordingly.
//!
//! Schema is forward-compat: new event types only need their decoder; the
//! generic blob column holds the raw borsh payload for clients that want
//! to decode lazily.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    program       TEXT NOT NULL,
    event_type    TEXT NOT NULL,
    slot          INTEGER NOT NULL,
    signature     TEXT NOT NULL,
    payload_b64   TEXT NOT NULL,
    created_at    INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(signature, program, event_type)
);

CREATE INDEX IF NOT EXISTS events_program_slot_idx ON events(program, slot);
CREATE INDEX IF NOT EXISTS events_signature_idx ON events(signature);

CREATE TABLE IF NOT EXISTS cursor (
    program       TEXT PRIMARY KEY,
    last_slot     INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
"#;

/// Connection wrapped in Mutex so `Arc<Sink>` is `Send + Sync` for the
/// per-program tokio::spawn'd subscription tasks. Lock scope is tight
/// (one SQL statement per acquire) so contention is minimal.
pub struct Sink {
    conn: Mutex<Connection>,
}

impl Sink {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(&path)
            .with_context(|| format!("opening sqlite at {:?}", path.as_ref()))?;
        conn.execute_batch(SCHEMA).context("applying sqlite schema")?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            "#,
        )
        .context("applying sqlite pragmas")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn record_event(
        &self,
        program: &str,
        event_type: &str,
        slot: u64,
        signature: &str,
        payload_b64: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            r#"INSERT OR IGNORE INTO events(program, event_type, slot, signature, payload_b64)
               VALUES (?1, ?2, ?3, ?4, ?5)"#,
            params![program, event_type, slot as i64, signature, payload_b64],
        )?;
        Ok(changed > 0)
    }

    pub fn bump_cursor(&self, program: &str, slot: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO cursor(program, last_slot) VALUES (?1, ?2)
               ON CONFLICT(program) DO UPDATE SET
                 last_slot = MAX(cursor.last_slot, excluded.last_slot),
                 updated_at = strftime('%s', 'now')"#,
            params![program, slot as i64],
        )?;
        Ok(())
    }

    pub fn cursor(&self, program: &str) -> Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT last_slot FROM cursor WHERE program = ?1")?;
        let row: Option<i64> = stmt.query_row(params![program], |r| r.get(0)).ok();
        Ok(row.map(|v| v as u64))
    }

    pub fn event_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_and_idempotency() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sink = Sink::open(&path).unwrap();

        let inserted = sink
            .record_event("aeqi_trust", "TrustInitialized", 100, "sigA", "b64A")
            .unwrap();
        assert!(inserted);

        // Replay — same tuple should be a no-op.
        let inserted_again = sink
            .record_event("aeqi_trust", "TrustInitialized", 100, "sigA", "b64A")
            .unwrap();
        assert!(!inserted_again);

        // Different event_type with same sig is allowed (one tx can emit
        // multiple events).
        let other = sink
            .record_event("aeqi_trust", "ModuleRegistered", 100, "sigA", "b64B")
            .unwrap();
        assert!(other);

        assert_eq!(sink.event_count().unwrap(), 2);
    }

    #[test]
    fn cursor_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sink = Sink::open(&path).unwrap();
        assert_eq!(sink.cursor("aeqi_trust").unwrap(), None);

        sink.bump_cursor("aeqi_trust", 100).unwrap();
        assert_eq!(sink.cursor("aeqi_trust").unwrap(), Some(100));

        // Cursor only moves forward
        sink.bump_cursor("aeqi_trust", 50).unwrap();
        assert_eq!(sink.cursor("aeqi_trust").unwrap(), Some(100));

        sink.bump_cursor("aeqi_trust", 200).unwrap();
        assert_eq!(sink.cursor("aeqi_trust").unwrap(), Some(200));
    }
}
