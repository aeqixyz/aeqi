//! Notes & Directives — persistent will that reshapes Sigil.
//!
//! Notes are the second surface of Layer 0. Chat is ephemeral conversation;
//! notes are persistent will. Each note is scanned for directives — imperative
//! lines that become trackable goals linked to tasks.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tracing::debug;

/// Monotonically increasing counter to ensure unique IDs within a process.
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A persistent note scoped to a channel (project or department).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub channel: String,
    pub content: String,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Lifecycle status of a directive extracted from a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveStatus {
    Pending,
    Active,
    Done,
    Failed,
}

impl fmt::Display for DirectiveStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Done => write!(f, "done"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl DirectiveStatus {
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "done" => Self::Done,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// A directive extracted from a note, persisted and tracked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directive {
    pub id: String,
    pub note_id: String,
    pub line_number: u32,
    pub content: String,
    pub status: DirectiveStatus,
    pub task_id: Option<String>,
    pub matched_task_id: Option<String>,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Directive detection
// ---------------------------------------------------------------------------

/// The pattern type that triggered directive detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    Imperative,
    Checkbox,
    Goal,
}

/// A directive detected from raw note content (not yet persisted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedDirective {
    pub line_number: u32,
    pub content: String,
    pub pattern_type: PatternType,
}

/// Imperative verbs that signal a directive when they start a line.
const IMPERATIVE_VERBS: &[&str] = &[
    "build",
    "fix",
    "deploy",
    "create",
    "launch",
    "redesign",
    "research",
    "migrate",
    "implement",
    "refactor",
    "optimize",
    "set up",
    "configure",
    "add",
    "remove",
    "update",
    "ship",
    "test",
    "review",
    "investigate",
];

/// Goal keywords that signal a directive when they appear anywhere in a line.
const GOAL_MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "jan",
    "feb",
    "mar",
    "apr",
    "jun",
    "jul",
    "aug",
    "sep",
    "oct",
    "nov",
    "dec",
    "q1",
    "q2",
    "q3",
    "q4",
];

/// Scans note content line-by-line for directives.
pub struct DirectiveDetector;

impl DirectiveDetector {
    /// Detect directives from raw markdown content.
    pub fn detect(content: &str) -> Vec<DetectedDirective> {
        let mut results = Vec::new();

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // Skip empty lines, headings, and very short lines.
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.len() < 5 {
                continue;
            }

            // 1. Checkbox pattern: "[ ] something" or "- [ ] something"
            let after_prefix = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .unwrap_or(trimmed);
            if after_prefix.starts_with("[ ] ") || after_prefix.starts_with("[] ") {
                let text = after_prefix
                    .strip_prefix("[ ] ")
                    .or_else(|| after_prefix.strip_prefix("[] "))
                    .unwrap_or(after_prefix)
                    .to_string();
                if text.len() >= 3 {
                    results.push(DetectedDirective {
                        line_number: (idx + 1) as u32,
                        content: text,
                        pattern_type: PatternType::Checkbox,
                    });
                    continue;
                }
            }

            // Strip list markers for imperative detection.
            let stripped = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| {
                    // Numbered list: "1. something", "2) something"
                    let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
                    rest.strip_prefix(". ")
                        .or_else(|| rest.strip_prefix(") "))
                })
                .unwrap_or(trimmed);

            let lower = stripped.to_lowercase();

            // 2. Imperative verb at start of line.
            let is_imperative = IMPERATIVE_VERBS
                .iter()
                .any(|verb| lower.starts_with(verb) && lower[verb.len()..].starts_with(' '));

            if is_imperative {
                results.push(DetectedDirective {
                    line_number: (idx + 1) as u32,
                    content: stripped.to_string(),
                    pattern_type: PatternType::Imperative,
                });
                continue;
            }

            // 3. Goal pattern: contains "by <month/date>", "achieve", "reach X users/customers".
            let is_goal = Self::is_goal_pattern(&lower);
            if is_goal {
                results.push(DetectedDirective {
                    line_number: (idx + 1) as u32,
                    content: stripped.to_string(),
                    pattern_type: PatternType::Goal,
                });
            }
        }

        results
    }

    /// Check if a lowercased line matches a goal pattern.
    fn is_goal_pattern(lower: &str) -> bool {
        // "by <month>"
        for month in GOAL_MONTHS {
            let pattern = format!("by {month}");
            if lower.contains(&pattern) {
                return true;
            }
        }

        // "achieve" or "reach X users/customers"
        if lower.contains("achieve") {
            return true;
        }

        if lower.contains("reach") && (lower.contains("users") || lower.contains("customers")) {
            return true;
        }

        false
    }
}

// ---------------------------------------------------------------------------
// SQLite-backed note storage
// ---------------------------------------------------------------------------

/// Persistent storage for notes and their directives.
pub struct NoteStore {
    conn: Mutex<Connection>,
}

impl NoteStore {
    /// Open or create a NoteStore at the given SQLite path.
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create notes dir: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open notes DB: {}", path.display()))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create tables if they don't already exist.
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;

             CREATE TABLE IF NOT EXISTS notes (
                 id TEXT PRIMARY KEY,
                 channel TEXT NOT NULL,
                 content TEXT NOT NULL,
                 version INTEGER DEFAULT 1,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );

             CREATE UNIQUE INDEX IF NOT EXISTS idx_notes_channel
                 ON notes(channel);

             CREATE TABLE IF NOT EXISTS note_directives (
                 id TEXT PRIMARY KEY,
                 note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
                 line_number INTEGER NOT NULL,
                 content TEXT NOT NULL,
                 status TEXT DEFAULT 'pending',
                 task_id TEXT,
                 matched_task_id TEXT,
                 confidence REAL DEFAULT 0.0,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_directives_note
                 ON note_directives(note_id);",
        )
        .context("failed to initialize notes schema")?;

        Ok(())
    }

    /// Generate a unique ID with a prefix.
    fn gen_id(prefix: &str) -> String {
        let ts = Utc::now().timestamp_millis();
        let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{ts}-{seq}")
    }

    /// Save or update a note for a channel.
    ///
    /// If a note already exists for the channel, update its content and bump the version.
    /// Otherwise, create a new note.
    pub fn save_note(&self, channel: &str, content: &str) -> Result<Note> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // Check if a note already exists for this channel.
        let existing: Option<(String, u32)> = conn
            .query_row(
                "SELECT id, version FROM notes WHERE channel = ?1",
                params![channel],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (id, version) = if let Some((existing_id, existing_version)) = existing {
            let new_version = existing_version + 1;
            conn.execute(
                "UPDATE notes SET content = ?1, version = ?2, updated_at = ?3 WHERE id = ?4",
                params![content, new_version, &now_str, &existing_id],
            )
            .context("failed to update note")?;
            (existing_id, new_version)
        } else {
            let id = Self::gen_id("note");
            conn.execute(
                "INSERT INTO notes (id, channel, content, version, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?5)",
                params![&id, channel, content, &now_str, &now_str],
            )
            .context("failed to insert note")?;
            (id, 1)
        };

        Ok(Note {
            id,
            channel: channel.to_string(),
            content: content.to_string(),
            version,
            created_at: now,
            updated_at: now,
        })
    }

    /// Get the note for a channel, if it exists.
    pub fn get_note(&self, channel: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let result = conn
            .query_row(
                "SELECT id, channel, content, version, created_at, updated_at FROM notes WHERE channel = ?1",
                params![channel],
                |row| {
                    let created_str: String = row.get(4)?;
                    let updated_str: String = row.get(5)?;
                    Ok(Note {
                        id: row.get(0)?,
                        channel: row.get(1)?,
                        content: row.get(2)?,
                        version: row.get(3)?,
                        created_at: DateTime::parse_from_rfc3339(&created_str)
                            .unwrap_or_default()
                            .with_timezone(&Utc),
                        updated_at: DateTime::parse_from_rfc3339(&updated_str)
                            .unwrap_or_default()
                            .with_timezone(&Utc),
                    })
                },
            )
            .ok();

        Ok(result)
    }

    /// List all notes.
    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, channel, content, version, created_at, updated_at FROM notes ORDER BY updated_at DESC",
            )
            .context("failed to prepare list notes query")?;

        let notes = stmt
            .query_map([], |row| {
                let created_str: String = row.get(4)?;
                let updated_str: String = row.get(5)?;
                Ok(Note {
                    id: row.get(0)?,
                    channel: row.get(1)?,
                    content: row.get(2)?,
                    version: row.get(3)?,
                    created_at: DateTime::parse_from_rfc3339(&created_str)
                        .unwrap_or_default()
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&updated_str)
                        .unwrap_or_default()
                        .with_timezone(&Utc),
                })
            })
            .context("failed to query notes")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to collect notes")?;

        Ok(notes)
    }

    /// Delete a note by ID. Returns true if a row was deleted.
    pub fn delete_note(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        // Delete directives first (in case FK cascade isn't enforced).
        conn.execute(
            "DELETE FROM note_directives WHERE note_id = ?1",
            params![id],
        )
        .ok();
        let deleted = conn
            .execute("DELETE FROM notes WHERE id = ?1", params![id])
            .context("failed to delete note")?;
        Ok(deleted > 0)
    }

    /// Replace all directives for a note with newly detected ones.
    pub fn save_directives(
        &self,
        note_id: &str,
        directives: Vec<DetectedDirective>,
    ) -> Result<Vec<Directive>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // Clear old directives for this note.
        conn.execute(
            "DELETE FROM note_directives WHERE note_id = ?1",
            params![note_id],
        )
        .context("failed to clear old directives")?;

        let mut result = Vec::with_capacity(directives.len());
        for d in directives {
            let id = Self::gen_id("dir");
            conn.execute(
                "INSERT INTO note_directives (id, note_id, line_number, content, status, confidence, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'pending', 0.0, ?5, ?6)",
                params![&id, note_id, d.line_number, &d.content, &now_str, &now_str],
            )
            .context("failed to insert directive")?;

            result.push(Directive {
                id,
                note_id: note_id.to_string(),
                line_number: d.line_number,
                content: d.content,
                status: DirectiveStatus::Pending,
                task_id: None,
                matched_task_id: None,
                confidence: 0.0,
                created_at: now,
                updated_at: now,
            });
        }

        debug!(note_id, count = result.len(), "saved directives");
        Ok(result)
    }

    /// Get all directives for a note.
    pub fn get_directives(&self, note_id: &str) -> Result<Vec<Directive>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, line_number, content, status, task_id, matched_task_id, confidence, created_at, updated_at
                 FROM note_directives WHERE note_id = ?1 ORDER BY line_number",
            )
            .context("failed to prepare get directives query")?;

        let directives = stmt
            .query_map(params![note_id], |row| {
                let status_str: String = row.get(4)?;
                let task_id: Option<String> = row.get(5)?;
                let matched_task_id: Option<String> = row.get(6)?;
                let created_str: String = row.get(8)?;
                let updated_str: String = row.get(9)?;
                Ok(Directive {
                    id: row.get(0)?,
                    note_id: row.get(1)?,
                    line_number: row.get(2)?,
                    content: row.get(3)?,
                    status: DirectiveStatus::from_str_lossy(&status_str),
                    task_id,
                    matched_task_id,
                    confidence: row.get(7)?,
                    created_at: DateTime::parse_from_rfc3339(&created_str)
                        .unwrap_or_default()
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&updated_str)
                        .unwrap_or_default()
                        .with_timezone(&Utc),
                })
            })
            .context("failed to query directives")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to collect directives")?;

        Ok(directives)
    }

    /// Get all pending directives across all notes, ordered by creation time.
    pub fn get_pending_directives(&self) -> Result<Vec<Directive>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, line_number, content, status, task_id, matched_task_id, confidence, created_at, updated_at
                 FROM note_directives WHERE status = 'pending' ORDER BY created_at",
            )
            .context("failed to prepare get pending directives query")?;

        let directives = stmt
            .query_map([], |row| {
                let status_str: String = row.get(4)?;
                let task_id: Option<String> = row.get(5)?;
                let matched_task_id: Option<String> = row.get(6)?;
                let created_str: String = row.get(8)?;
                let updated_str: String = row.get(9)?;
                Ok(Directive {
                    id: row.get(0)?,
                    note_id: row.get(1)?,
                    line_number: row.get(2)?,
                    content: row.get(3)?,
                    status: DirectiveStatus::from_str_lossy(&status_str),
                    task_id,
                    matched_task_id,
                    confidence: row.get(7)?,
                    created_at: DateTime::parse_from_rfc3339(&created_str)
                        .unwrap_or_default()
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&updated_str)
                        .unwrap_or_default()
                        .with_timezone(&Utc),
                })
            })
            .context("failed to query pending directives")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to collect pending directives")?;

        Ok(directives)
    }

    /// Update a directive's status and optionally link a task.
    /// Returns true if the directive was found and updated.
    pub fn update_directive_status(
        &self,
        directive_id: &str,
        status: DirectiveStatus,
        task_id: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let now_str = Utc::now().to_rfc3339();

        let updated = conn
            .execute(
                "UPDATE note_directives SET status = ?1, task_id = ?2, updated_at = ?3 WHERE id = ?4",
                params![status.to_string(), task_id, &now_str, directive_id],
            )
            .context("failed to update directive status")?;

        Ok(updated > 0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- DirectiveDetector tests ---

    #[test]
    fn test_imperative_detection() {
        let content = "build the new auth service\nfix trading bot overshoot\ndeploy to production";
        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].pattern_type, PatternType::Imperative);
        assert_eq!(results[0].content, "build the new auth service");
        assert_eq!(results[0].line_number, 1);
        assert_eq!(results[1].pattern_type, PatternType::Imperative);
        assert_eq!(results[1].content, "fix trading bot overshoot");
        assert_eq!(results[1].line_number, 2);
        assert_eq!(results[2].pattern_type, PatternType::Imperative);
        assert_eq!(results[2].content, "deploy to production");
        assert_eq!(results[2].line_number, 3);
    }

    #[test]
    fn test_imperative_with_list_markers() {
        let content = "- build the auth service\n* fix the bug\n1. deploy to staging";
        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|d| d.pattern_type == PatternType::Imperative));
        assert_eq!(results[0].content, "build the auth service");
        assert_eq!(results[1].content, "fix the bug");
        assert_eq!(results[2].content, "deploy to staging");
    }

    #[test]
    fn test_checkbox_detection() {
        let content = "[ ] launch pricing page\n- [ ] set up monitoring\n* [ ] CDN for assets";
        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|d| d.pattern_type == PatternType::Checkbox));
        assert_eq!(results[0].content, "launch pricing page");
        assert_eq!(results[1].content, "set up monitoring");
        assert_eq!(results[2].content, "CDN for assets");
    }

    #[test]
    fn test_goal_detection() {
        let content = "100 users by April\nachieve profitable trading\nreach 1000 customers by Q2";
        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|d| d.pattern_type == PatternType::Goal));
    }

    #[test]
    fn test_goal_by_month() {
        let content = "launch v2 by march\nship feature by december";
        let _results = DirectiveDetector::detect(content);
        // "launch" is imperative, "ship" is imperative — both match imperative first
        // So test with non-imperative lines
        let content2 = "100 users by march\nprofitable trading by q4";
        let results2 = DirectiveDetector::detect(content2);
        assert_eq!(results2.len(), 2);
        assert!(results2.iter().all(|d| d.pattern_type == PatternType::Goal));
    }

    #[test]
    fn test_skip_headings() {
        let content = "# Q2 Goals\n## Infrastructure\nbuild the auth service";
        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "build the auth service");
    }

    #[test]
    fn test_skip_short_lines() {
        let content = "yes\nno\nok\nbuild the auth service";
        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "build the auth service");
    }

    #[test]
    fn test_skip_empty_lines() {
        let content = "\n\n\nbuild the auth service\n\n";
        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_mixed_content() {
        let content = "\
# Q2 Goals

build the pricing page
[ ] set up monitoring
100 users by April

## Notes
This is just a regular paragraph that shouldn't match.
Short.

- fix the trading bot
- [ ] CDN for static assets
reach 500 customers by Q3";

        let results = DirectiveDetector::detect(content);
        assert_eq!(results.len(), 6);

        // build the pricing page — Imperative
        assert_eq!(results[0].pattern_type, PatternType::Imperative);
        assert_eq!(results[0].content, "build the pricing page");

        // [ ] set up monitoring — Checkbox
        assert_eq!(results[1].pattern_type, PatternType::Checkbox);
        assert_eq!(results[1].content, "set up monitoring");

        // 100 users by April — Goal
        assert_eq!(results[2].pattern_type, PatternType::Goal);

        // fix the trading bot — Imperative
        assert_eq!(results[3].pattern_type, PatternType::Imperative);
        assert_eq!(results[3].content, "fix the trading bot");

        // [ ] CDN for static assets — Checkbox
        assert_eq!(results[4].pattern_type, PatternType::Checkbox);

        // reach 500 customers by Q3 — Goal
        assert_eq!(results[5].pattern_type, PatternType::Goal);
    }

    // --- NoteStore tests ---

    fn temp_store() -> NoteStore {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_notes.db");
        // Leak the tempdir so it lives long enough.
        let path = db_path.clone();
        std::mem::forget(dir);
        NoteStore::new(&path).unwrap()
    }

    #[test]
    fn test_save_and_get_note() {
        let store = temp_store();
        let note = store.save_note("sigil", "build great things").unwrap();
        assert_eq!(note.channel, "sigil");
        assert_eq!(note.content, "build great things");
        assert_eq!(note.version, 1);

        let fetched = store.get_note("sigil").unwrap().unwrap();
        assert_eq!(fetched.id, note.id);
        assert_eq!(fetched.content, "build great things");
        assert_eq!(fetched.version, 1);
    }

    #[test]
    fn test_version_bumping() {
        let store = temp_store();
        let note1 = store.save_note("sigil", "v1 content").unwrap();
        assert_eq!(note1.version, 1);

        let note2 = store.save_note("sigil", "v2 content").unwrap();
        assert_eq!(note2.version, 2);
        assert_eq!(note2.id, note1.id); // Same note, updated.

        let note3 = store.save_note("sigil", "v3 content").unwrap();
        assert_eq!(note3.version, 3);

        let fetched = store.get_note("sigil").unwrap().unwrap();
        assert_eq!(fetched.content, "v3 content");
        assert_eq!(fetched.version, 3);
    }

    #[test]
    fn test_list_notes() {
        let store = temp_store();
        store.save_note("sigil", "sigil notes").unwrap();
        store.save_note("algostaking", "trading notes").unwrap();

        let notes = store.list_notes().unwrap();
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn test_delete_note() {
        let store = temp_store();
        let note = store.save_note("sigil", "to be deleted").unwrap();
        assert!(store.delete_note(&note.id).unwrap());
        assert!(store.get_note("sigil").unwrap().is_none());
        // Double delete returns false.
        assert!(!store.delete_note(&note.id).unwrap());
    }

    #[test]
    fn test_get_nonexistent_note() {
        let store = temp_store();
        assert!(store.get_note("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_directive_save_and_retrieval() {
        let store = temp_store();
        let note = store
            .save_note("sigil", "build auth\nfix trading bot")
            .unwrap();

        let detected = vec![
            DetectedDirective {
                line_number: 1,
                content: "build auth".to_string(),
                pattern_type: PatternType::Imperative,
            },
            DetectedDirective {
                line_number: 2,
                content: "fix trading bot".to_string(),
                pattern_type: PatternType::Imperative,
            },
        ];

        let saved = store.save_directives(&note.id, detected).unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].status, DirectiveStatus::Pending);
        assert_eq!(saved[0].content, "build auth");
        assert_eq!(saved[1].content, "fix trading bot");

        let fetched = store.get_directives(&note.id).unwrap();
        assert_eq!(fetched.len(), 2);
        assert_eq!(fetched[0].line_number, 1);
        assert_eq!(fetched[1].line_number, 2);
    }

    #[test]
    fn test_directive_replacement_on_resave() {
        let store = temp_store();
        let note = store.save_note("sigil", "build auth").unwrap();

        let detected1 = vec![DetectedDirective {
            line_number: 1,
            content: "build auth".to_string(),
            pattern_type: PatternType::Imperative,
        }];
        store.save_directives(&note.id, detected1).unwrap();

        // Re-save with different directives.
        let detected2 = vec![
            DetectedDirective {
                line_number: 1,
                content: "build auth v2".to_string(),
                pattern_type: PatternType::Imperative,
            },
            DetectedDirective {
                line_number: 2,
                content: "deploy to prod".to_string(),
                pattern_type: PatternType::Imperative,
            },
        ];
        let saved = store.save_directives(&note.id, detected2).unwrap();
        assert_eq!(saved.len(), 2);

        let fetched = store.get_directives(&note.id).unwrap();
        assert_eq!(fetched.len(), 2);
        assert_eq!(fetched[0].content, "build auth v2");
    }

    #[test]
    fn test_directive_status_update() {
        let store = temp_store();
        let note = store.save_note("sigil", "build auth").unwrap();

        let detected = vec![DetectedDirective {
            line_number: 1,
            content: "build auth".to_string(),
            pattern_type: PatternType::Imperative,
        }];
        let saved = store.save_directives(&note.id, detected).unwrap();

        let updated = store
            .update_directive_status(&saved[0].id, DirectiveStatus::Active, Some("sg-1234"))
            .unwrap();
        assert!(updated);

        let fetched = store.get_directives(&note.id).unwrap();
        assert_eq!(fetched[0].status, DirectiveStatus::Active);
        assert_eq!(fetched[0].task_id.as_deref(), Some("sg-1234"));
    }

    #[test]
    fn test_directive_status_update_nonexistent() {
        let store = temp_store();
        let updated = store
            .update_directive_status("nonexistent", DirectiveStatus::Done, None)
            .unwrap();
        assert!(!updated);
    }

    #[test]
    fn test_full_flow() {
        let store = temp_store();

        // 1. Save a note with mixed content.
        let content = "\
# Q2 Goals
build the pricing page
[ ] set up monitoring
100 users by April";

        let note = store.save_note("sigil/engineering", content).unwrap();
        assert_eq!(note.version, 1);

        // 2. Detect directives.
        let detected = DirectiveDetector::detect(content);
        assert_eq!(detected.len(), 3);

        // 3. Save directives.
        let directives = store.save_directives(&note.id, detected).unwrap();
        assert_eq!(directives.len(), 3);

        // 4. Update one directive to active with a linked task.
        store
            .update_directive_status(&directives[0].id, DirectiveStatus::Active, Some("sg-1001"))
            .unwrap();

        // 5. Mark another as done.
        store
            .update_directive_status(&directives[1].id, DirectiveStatus::Done, None)
            .unwrap();

        // 6. Verify final state.
        let final_directives = store.get_directives(&note.id).unwrap();
        assert_eq!(final_directives[0].status, DirectiveStatus::Active);
        assert_eq!(final_directives[0].task_id.as_deref(), Some("sg-1001"));
        assert_eq!(final_directives[1].status, DirectiveStatus::Done);
        assert_eq!(final_directives[2].status, DirectiveStatus::Pending);

        // 7. Update the note — version bumps.
        let updated_content = "\
# Q2 Goals
build the pricing page v2
[ ] set up monitoring
200 users by April";

        let note2 = store
            .save_note("sigil/engineering", updated_content)
            .unwrap();
        assert_eq!(note2.version, 2);
        assert_eq!(note2.id, note.id);

        // 8. Re-detect and re-save directives (old ones replaced).
        let detected2 = DirectiveDetector::detect(updated_content);
        let directives2 = store.save_directives(&note.id, detected2).unwrap();
        assert_eq!(directives2.len(), 3);
        assert_eq!(directives2[0].content, "build the pricing page v2");
        // All reset to pending after re-detection.
        assert!(directives2
            .iter()
            .all(|d| d.status == DirectiveStatus::Pending));
    }

    #[test]
    fn test_delete_note_cascades_directives() {
        let store = temp_store();
        let note = store.save_note("sigil", "build auth").unwrap();
        let detected = vec![DetectedDirective {
            line_number: 1,
            content: "build auth".to_string(),
            pattern_type: PatternType::Imperative,
        }];
        store.save_directives(&note.id, detected).unwrap();

        store.delete_note(&note.id).unwrap();
        let directives = store.get_directives(&note.id).unwrap();
        assert!(directives.is_empty());
    }

    #[test]
    fn test_get_pending_directives_returns_only_pending() {
        let store = temp_store();

        // Create two notes with directives.
        let note1 = store.save_note("sigil", "build auth\nfix bug").unwrap();
        let detected1 = vec![
            DetectedDirective {
                line_number: 1,
                content: "build auth".to_string(),
                pattern_type: PatternType::Imperative,
            },
            DetectedDirective {
                line_number: 2,
                content: "fix bug".to_string(),
                pattern_type: PatternType::Imperative,
            },
        ];
        let saved1 = store.save_directives(&note1.id, detected1).unwrap();

        let note2 = store
            .save_note("algostaking", "deploy to prod")
            .unwrap();
        let detected2 = vec![DetectedDirective {
            line_number: 1,
            content: "deploy to prod".to_string(),
            pattern_type: PatternType::Imperative,
        }];
        let saved2 = store.save_directives(&note2.id, detected2).unwrap();

        // All three should be pending initially.
        let pending = store.get_pending_directives().unwrap();
        assert_eq!(pending.len(), 3);

        // Mark one as active, one as done.
        store
            .update_directive_status(&saved1[0].id, DirectiveStatus::Active, Some("sg-100"))
            .unwrap();
        store
            .update_directive_status(&saved2[0].id, DirectiveStatus::Done, None)
            .unwrap();

        // Only the remaining pending directive should be returned.
        let pending = store.get_pending_directives().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].content, "fix bug");
        assert_eq!(pending[0].status, DirectiveStatus::Pending);
    }
}
