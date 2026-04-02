use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DispatchKind {
    TaskDone {
        task_id: String,
        summary: String,
    },
    TaskBlocked {
        task_id: String,
        question: String,
        context: String,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },

    PatrolReport {
        project: String,
        active: usize,
        pending: usize,
    },
    WorkerCrashed {
        project: String,
        worker: String,
        error: String,
    },
    Escalation {
        project: String,
        task_id: String,
        subject: String,
        description: String,
        attempts: u32,
    },

    Resolution {
        task_id: String,
        answer: String,
    },

    HumanEscalation {
        project: String,
        task_id: String,
        subject: String,
        summary: String,
    },

    /// A delegation request from one agent to another.
    DelegateRequest {
        prompt: String,
        /// How the response should be routed: "origin", "perpetual", "async", "department", "none".
        response_mode: String,
        /// Whether to also create a tracked task for this delegation.
        create_task: bool,
        /// Optional skill hint for the target agent.
        skill: Option<String>,
        /// Dispatch ID this is replying to (for chained delegations).
        reply_to: Option<String>,
    },
    /// A response to a previous DelegateRequest.
    DelegateResponse {
        /// The dispatch ID of the original DelegateRequest.
        reply_to: String,
        /// Copied from the request for routing purposes.
        response_mode: String,
        /// The response content.
        content: String,
    },
}

impl DispatchKind {
    pub fn requires_ack_by_default(&self) -> bool {
        matches!(
            self,
            Self::TaskDone { .. }
                | Self::TaskBlocked { .. }
                | Self::TaskFailed { .. }
                | Self::WorkerCrashed { .. }
                | Self::Resolution { .. }
                | Self::Escalation { .. }
                | Self::DelegateRequest { .. }
        )
    }

    pub fn subject_tag(&self) -> &'static str {
        match self {
            Self::TaskDone { .. } => "DONE",
            Self::TaskBlocked { .. } => "BLOCKED",
            Self::TaskFailed { .. } => "FAILED",
            Self::PatrolReport { .. } => "PATROL",
            Self::WorkerCrashed { .. } => "WORKER_CRASHED",
            Self::Escalation { .. } => "ESCALATE",
            Self::Resolution { .. } => "RESOLVED",
            Self::HumanEscalation { .. } => "HUMAN_ESCALATION",
            Self::DelegateRequest { .. } => "DELEGATE_REQUEST",
            Self::DelegateResponse { .. } => "DELEGATE_RESPONSE",
        }
    }

    pub fn body_text(&self) -> String {
        match self {
            Self::TaskDone { task_id, summary } => format!("Completed task {task_id}: {summary}"),
            Self::TaskBlocked {
                task_id,
                question,
                context,
            } => format!("Task {task_id} blocked: {question}\n\nFull context:\n{context}"),
            Self::TaskFailed { task_id, error } => format!("Failed task {task_id}: {error}"),
            Self::PatrolReport {
                project,
                active,
                pending,
            } => format!("Project {project}: {active} active workers, {pending} pending tasks"),
            Self::WorkerCrashed {
                project,
                worker,
                error,
            } => format!("Worker {worker} crashed in {project}: {error}"),
            Self::Escalation {
                project,
                task_id,
                subject,
                description,
                attempts,
            } => format!(
                "Project {project} needs help resolving a blocker.\n\n\
                     Task: {task_id} — {subject}\n\n\
                     Full description:\n{description}\n\n\
                     Blocked after {attempts} resolution attempt(s).",
            ),
            Self::Resolution { task_id, answer } => {
                format!("Resolution for task {task_id}: {answer}")
            }
            Self::HumanEscalation {
                project,
                task_id,
                subject,
                summary,
            } => format!(
                "BLOCKED: {project}/{task_id} — {subject}\n\n{summary}\n\n\
                     This task has exhausted all automated resolution attempts and requires human input.",
            ),
            Self::DelegateRequest {
                prompt,
                response_mode,
                create_task,
                skill,
                reply_to,
            } => {
                let mut text = format!(
                    "Delegation request (response_mode: {response_mode}, create_task: {create_task})"
                );
                if let Some(s) = skill {
                    text.push_str(&format!(", skill: {s}"));
                }
                if let Some(rt) = reply_to {
                    text.push_str(&format!(", reply_to: {rt}"));
                }
                text.push_str(&format!("\n\n{prompt}"));
                text
            }
            Self::DelegateResponse {
                reply_to,
                response_mode,
                content,
            } => format!(
                "Delegation response (reply_to: {reply_to}, mode: {response_mode})\n\n{content}"
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispatch {
    pub from: String,
    pub to: String,
    pub kind: DispatchKind,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
    /// Unique dispatch ID for acknowledgment tracking.
    #[serde(default = "default_dispatch_id")]
    pub id: String,
    /// Whether this dispatch requires explicit acknowledgment.
    #[serde(default)]
    pub requires_ack: bool,
    /// Number of retry attempts so far.
    #[serde(default)]
    pub retry_count: u32,
    /// Maximum retries before dead-lettering.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// When the dispatch was first sent (for total latency tracking).
    #[serde(default = "Utc::now")]
    pub first_sent_at: DateTime<Utc>,
}

/// Snapshot of control-plane delivery state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchHealth {
    /// Messages currently unread by their recipient.
    pub unread: usize,
    /// Ack-required messages that were delivered but not yet acknowledged.
    pub awaiting_ack: usize,
    /// Ack-required messages that are back in the unread queue after a retry.
    pub retrying_delivery: usize,
    /// Awaiting-ack messages older than the patrol retry threshold.
    pub overdue_ack: usize,
    /// Messages that exhausted retries and are now in dead-letter state.
    pub dead_letters: usize,
}

fn default_dispatch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_max_retries() -> u32 {
    3
}

impl Dispatch {
    pub fn new_typed(from: &str, to: &str, kind: DispatchKind) -> Self {
        let now = Utc::now();
        let requires_ack = kind.requires_ack_by_default();
        Self {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            timestamp: now,
            read: false,
            id: default_dispatch_id(),
            requires_ack,
            retry_count: 0,
            max_retries: 3,
            first_sent_at: now,
        }
    }

    /// Mark this dispatch as requiring acknowledgment.
    pub fn with_ack_required(mut self) -> Self {
        self.requires_ack = true;
        self
    }
}

enum BusBackend {
    Memory {
        queues: tokio::sync::Mutex<
            std::collections::HashMap<String, std::collections::VecDeque<Dispatch>>,
        >,
    },
    Sqlite {
        conn: Mutex<Connection>,
    },
}

pub struct DispatchBus {
    backend: BusBackend,
    ttl_secs: u64,
    max_queue_per_recipient: usize,
    event_broadcaster: Option<Arc<crate::execution_events::EventBroadcaster>>,
}

impl DispatchBus {
    pub fn new() -> Self {
        Self {
            backend: BusBackend::Memory {
                queues: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            },
            ttl_secs: 3600,
            max_queue_per_recipient: 1000,
            event_broadcaster: None,
        }
    }

    /// Set the event broadcaster for emitting DispatchReceived events.
    pub fn set_event_broadcaster(
        &mut self,
        broadcaster: Arc<crate::execution_events::EventBroadcaster>,
    ) {
        self.event_broadcaster = Some(broadcaster);
    }

    pub fn with_persistence(path: PathBuf) -> Self {
        let db_path = path.with_extension("db");
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match Self::open_sqlite(&db_path) {
            Ok(conn) => {
                debug!(path = %db_path.display(), "dispatch bus using SQLite WAL");
                Self {
                    backend: BusBackend::Sqlite {
                        conn: Mutex::new(conn),
                    },
                    ttl_secs: 3600,
                    max_queue_per_recipient: 1000,
                    event_broadcaster: None,
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to open SQLite dispatch bus, falling back to memory");
                Self::new()
            }
        }
    }

    fn open_sqlite(path: &std::path::Path) -> Result<Connection> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open dispatch DB: {}", path.display()))?;

        // Migrate legacy table name (SQLite doesn't support ALTER TABLE IF EXISTS).
        let has_whispers: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='whispers'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if has_whispers {
            conn.execute_batch("ALTER TABLE whispers RENAME TO dispatches;")?;
        }

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;

             CREATE TABLE IF NOT EXISTS dispatches (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 from_agent TEXT NOT NULL,
                 to_agent TEXT NOT NULL,
                 kind_json TEXT NOT NULL,
                 timestamp TEXT NOT NULL,
                 is_read INTEGER NOT NULL DEFAULT 0,
                 dispatch_id TEXT NOT NULL DEFAULT '',
                 requires_ack INTEGER NOT NULL DEFAULT 0,
                 retry_count INTEGER NOT NULL DEFAULT 0,
                 max_retries INTEGER NOT NULL DEFAULT 3,
                 first_sent_at TEXT NOT NULL DEFAULT ''
             );

             CREATE INDEX IF NOT EXISTS idx_dispatches_recipient
                 ON dispatches(to_agent, is_read);
             CREATE INDEX IF NOT EXISTS idx_dispatches_timestamp
                 ON dispatches(timestamp);",
        )?;

        // Schema migration: add delivery guarantee columns if missing.
        let has_dispatch_id: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('dispatches') WHERE name='dispatch_id'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !has_dispatch_id {
            let _ = conn.execute_batch(
                "ALTER TABLE dispatches ADD COLUMN dispatch_id TEXT DEFAULT '';
                 ALTER TABLE dispatches ADD COLUMN requires_ack INTEGER DEFAULT 0;
                 ALTER TABLE dispatches ADD COLUMN retry_count INTEGER DEFAULT 0;
                 ALTER TABLE dispatches ADD COLUMN max_retries INTEGER DEFAULT 3;
                 ALTER TABLE dispatches ADD COLUMN first_sent_at TEXT DEFAULT '';",
            );
        }

        Ok(conn)
    }

    pub fn set_ttl(&mut self, secs: u64) {
        self.ttl_secs = secs;
    }

    pub async fn send(&self, mail: Dispatch) {
        // Capture event data before mail is consumed by backend.
        let event_from = mail.from.clone();
        let event_to = mail.to.clone();
        let event_kind = mail.kind.subject_tag().to_string();

        match &self.backend {
            BusBackend::Memory { queues } => {
                let recipient = mail.to.clone();
                let mut queues = queues.lock().await;
                let queue = queues.entry(recipient).or_default();

                let cutoff = Utc::now() - chrono::Duration::seconds(self.ttl_secs as i64);
                queue.retain(|m| m.timestamp > cutoff);
                while queue.len() >= self.max_queue_per_recipient {
                    queue.pop_front();
                }
                queue.push_back(mail);
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return };

                let cutoff =
                    (Utc::now() - chrono::Duration::seconds(self.ttl_secs as i64)).to_rfc3339();
                let _ = conn.execute(
                    "DELETE FROM dispatches WHERE timestamp < ?1",
                    rusqlite::params![cutoff],
                );

                let count: u32 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM dispatches WHERE to_agent = ?1",
                        rusqlite::params![mail.to],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                if count as usize >= self.max_queue_per_recipient {
                    let excess = count as usize - self.max_queue_per_recipient + 1;
                    let _ = conn.execute(
                        "DELETE FROM dispatches WHERE id IN (
                            SELECT id FROM dispatches WHERE to_agent = ?1
                            ORDER BY timestamp ASC LIMIT ?2
                        )",
                        rusqlite::params![mail.to, excess],
                    );
                }

                let kind_json = match serde_json::to_string(&mail.kind) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "failed to serialize dispatch kind");
                        return;
                    }
                };

                let _ = conn.execute(
                    "INSERT INTO dispatches (
                        from_agent, to_agent, kind_json, timestamp, is_read,
                        dispatch_id, requires_ack, retry_count, max_retries, first_sent_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        mail.from,
                        mail.to,
                        kind_json,
                        mail.timestamp.to_rfc3339(),
                        mail.read,
                        mail.id,
                        mail.requires_ack,
                        mail.retry_count,
                        mail.max_retries,
                        mail.first_sent_at.to_rfc3339(),
                    ],
                );
            }
        }

        // Emit DispatchReceived event for trigger system.
        if let Some(ref broadcaster) = self.event_broadcaster {
            broadcaster.publish(crate::execution_events::ExecutionEvent::DispatchReceived {
                from_agent: event_from,
                to_agent: event_to,
                kind: event_kind,
            });
        }
    }

    pub async fn read(&self, recipient: &str) -> Vec<Dispatch> {
        match &self.backend {
            BusBackend::Memory { queues } => {
                let mut queues = queues.lock().await;
                let mut result = Vec::new();
                if let Some(queue) = queues.get_mut(recipient) {
                    for msg in queue.iter_mut() {
                        if !msg.read {
                            msg.read = true;
                            result.push(msg.clone());
                        }
                    }
                }
                result
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else {
                    return Vec::new();
                };
                let mut result = Vec::new();

                let mut stmt = match conn.prepare(
                    "SELECT id, from_agent, to_agent, kind_json, timestamp,
                            is_read, dispatch_id, requires_ack, retry_count, max_retries, first_sent_at
                     FROM dispatches WHERE to_agent = ?1 AND is_read = 0
                     ORDER BY timestamp ASC"
                ) {
                    Ok(s) => s,
                    Err(_) => return result,
                };

                let mut ids_to_mark = Vec::new();
                let rows = stmt.query_map(rusqlite::params![recipient], |row| {
                    let id: i64 = row.get(0)?;
                    let from: String = row.get(1)?;
                    let to: String = row.get(2)?;
                    let kind_json: String = row.get(3)?;
                    let ts_str: String = row.get(4)?;
                    let is_read: bool = row.get(5)?;
                    let dispatch_id: String = row.get(6)?;
                    let requires_ack: bool = row.get(7)?;
                    let retry_count: u32 = row.get(8)?;
                    let max_retries: u32 = row.get(9)?;
                    let first_sent_at: String = row.get(10)?;
                    Ok((
                        id,
                        from,
                        to,
                        kind_json,
                        ts_str,
                        is_read,
                        dispatch_id,
                        requires_ack,
                        retry_count,
                        max_retries,
                        first_sent_at,
                    ))
                });

                if let Ok(rows) = rows {
                    for row in rows.flatten() {
                        let (
                            id,
                            from,
                            to,
                            kind_json,
                            ts_str,
                            _is_read,
                            dispatch_id,
                            requires_ack,
                            retry_count,
                            max_retries,
                            first_sent_at,
                        ) = row;
                        let Ok(kind) = serde_json::from_str::<DispatchKind>(&kind_json) else {
                            continue;
                        };
                        let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                        let first_sent_at = DateTime::parse_from_rfc3339(&first_sent_at)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or(timestamp);

                        result.push(Dispatch {
                            from,
                            to,
                            kind,
                            timestamp,
                            read: true,
                            id: if dispatch_id.is_empty() {
                                default_dispatch_id()
                            } else {
                                dispatch_id
                            },
                            requires_ack,
                            retry_count,
                            max_retries,
                            first_sent_at,
                        });
                        ids_to_mark.push(id);
                    }
                }

                for id in ids_to_mark {
                    let _ = conn.execute(
                        "UPDATE dispatches SET is_read = 1 WHERE id = ?1",
                        rusqlite::params![id],
                    );
                }

                result
            }
        }
    }

    pub async fn all(&self) -> Vec<Dispatch> {
        match &self.backend {
            BusBackend::Memory { queues } => {
                let queues = queues.lock().await;
                queues.values().flat_map(|q| q.iter().cloned()).collect()
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else {
                    return Vec::new();
                };
                Self::query_dispatches(
                    &conn,
                    "SELECT from_agent, to_agent, kind_json, timestamp, is_read,
                            dispatch_id, requires_ack, retry_count, max_retries, first_sent_at
                     FROM dispatches ORDER BY timestamp ASC",
                    &[],
                )
            }
        }
    }

    pub async fn unread_count(&self, recipient: &str) -> usize {
        match &self.backend {
            BusBackend::Memory { queues } => {
                let queues = queues.lock().await;
                queues
                    .get(recipient)
                    .map(|q| q.iter().filter(|m| !m.read).count())
                    .unwrap_or(0)
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return 0 };
                conn.query_row(
                    "SELECT COUNT(*) FROM dispatches WHERE to_agent = ?1 AND is_read = 0",
                    rusqlite::params![recipient],
                    |row| row.get::<_, u32>(0),
                )
                .unwrap_or(0) as usize
            }
        }
    }

    pub fn pending_count(&self) -> usize {
        match &self.backend {
            BusBackend::Memory { queues } => queues
                .try_lock()
                .map(|queues| {
                    queues
                        .values()
                        .flat_map(|q| q.iter())
                        .filter(|m| !m.read)
                        .count()
                })
                .unwrap_or(0),
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return 0 };
                conn.query_row(
                    "SELECT COUNT(*) FROM dispatches WHERE is_read = 0",
                    [],
                    |row| row.get::<_, u32>(0),
                )
                .unwrap_or(0) as usize
            }
        }
    }

    /// Summarize current control-plane delivery health.
    pub async fn health(&self, overdue_age_secs: u64) -> DispatchHealth {
        let overdue_cutoff = Utc::now() - chrono::Duration::seconds(overdue_age_secs as i64);
        let dispatches = self.all().await;
        Self::summarize_health(&dispatches, overdue_cutoff)
    }

    pub fn drain(&self) -> Vec<Dispatch> {
        match &self.backend {
            BusBackend::Memory { queues } => queues
                .try_lock()
                .map(|mut queues| {
                    let mut result = Vec::new();
                    for queue in queues.values_mut() {
                        for msg in queue.iter_mut() {
                            if !msg.read {
                                msg.read = true;
                                result.push(msg.clone());
                            }
                        }
                    }
                    result
                })
                .unwrap_or_default(),
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else {
                    return Vec::new();
                };
                let result = Self::query_dispatches(
                    &conn,
                    "SELECT from_agent, to_agent, kind_json, timestamp, is_read,
                            dispatch_id, requires_ack, retry_count, max_retries, first_sent_at
                     FROM dispatches WHERE is_read = 0 ORDER BY timestamp ASC",
                    &[],
                );
                let _ = conn.execute("UPDATE dispatches SET is_read = 1 WHERE is_read = 0", []);
                result
            }
        }
    }

    /// Acknowledge a dispatch by ID, preventing future retries.
    pub async fn acknowledge(&self, dispatch_id: &str) {
        match &self.backend {
            BusBackend::Memory { queues } => {
                let mut queues = queues.lock().await;
                for queue in queues.values_mut() {
                    for msg in queue.iter_mut() {
                        if msg.id == dispatch_id {
                            msg.read = true;
                            msg.requires_ack = false;
                            return;
                        }
                    }
                }
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return };
                let _ = conn.execute(
                    "UPDATE dispatches SET is_read = 1, requires_ack = 0 WHERE dispatch_id = ?1",
                    rusqlite::params![dispatch_id],
                );
            }
        }
    }

    /// Return unacknowledged dispatches older than `max_age_secs` that haven't
    /// exceeded their retry limit. Increments retry_count on each returned dispatch.
    pub async fn retry_unacked(&self, max_age_secs: u64) -> Vec<Dispatch> {
        let cutoff = Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        match &self.backend {
            BusBackend::Memory { queues } => {
                let mut queues = queues.lock().await;
                let mut retries = Vec::new();
                for queue in queues.values_mut() {
                    for msg in queue.iter_mut() {
                        if msg.requires_ack
                            && msg.read
                            && msg.timestamp < cutoff
                            && msg.retry_count < msg.max_retries
                        {
                            msg.retry_count += 1;
                            msg.timestamp = Utc::now();
                            msg.read = false;
                            retries.push(msg.clone());
                        }
                    }
                }
                retries
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else {
                    return Vec::new();
                };
                let cutoff_str = cutoff.to_rfc3339();
                // Increment retry_count and return matching dispatches.
                let _ = conn.execute(
                    "UPDATE dispatches SET retry_count = retry_count + 1, timestamp = ?1, is_read = 0
                     WHERE is_read = 1 AND requires_ack = 1
                     AND retry_count < max_retries AND timestamp < ?2",
                    rusqlite::params![Utc::now().to_rfc3339(), cutoff_str],
                );
                Self::query_dispatches(
                    &conn,
                    "SELECT from_agent, to_agent, kind_json, timestamp, is_read,
                            dispatch_id, requires_ack, retry_count, max_retries, first_sent_at
                     FROM dispatches WHERE requires_ack = 1 AND is_read = 0
                     AND retry_count > 0 AND retry_count <= max_retries
                     ORDER BY timestamp ASC",
                    &[],
                )
            }
        }
    }

    /// Return dispatches that have exceeded their max retry count (dead letters).
    pub async fn dead_letters(&self) -> Vec<Dispatch> {
        match &self.backend {
            BusBackend::Memory { queues } => {
                let queues = queues.lock().await;
                let mut dead = Vec::new();
                for queue in queues.values() {
                    for msg in queue.iter() {
                        if msg.requires_ack && msg.retry_count >= msg.max_retries {
                            dead.push(msg.clone());
                        }
                    }
                }
                dead
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else {
                    return Vec::new();
                };
                Self::query_dispatches(
                    &conn,
                    "SELECT from_agent, to_agent, kind_json, timestamp, is_read,
                            dispatch_id, requires_ack, retry_count, max_retries, first_sent_at
                     FROM dispatches WHERE requires_ack = 1
                     AND retry_count >= max_retries
                     ORDER BY timestamp ASC",
                    &[],
                )
            }
        }
    }

    pub async fn save(&self) -> Result<()> {
        Ok(())
    }

    pub async fn load(&self) -> Result<usize> {
        match &self.backend {
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return Ok(0) };
                let count: u32 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM dispatches WHERE is_read = 0",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                if count > 0 {
                    debug!(count, "dispatch bus has persisted unread messages");
                }
                Ok(count as usize)
            }
            BusBackend::Memory { .. } => Ok(0),
        }
    }

    fn query_dispatches(
        conn: &Connection,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Vec<Dispatch> {
        let mut result = Vec::new();
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return result,
        };

        let rows = stmt.query_map(params, |row| {
            let from: String = row.get(0)?;
            let to: String = row.get(1)?;
            let kind_json: String = row.get(2)?;
            let ts_str: String = row.get(3)?;
            let is_read: bool = row.get(4)?;
            let dispatch_id: String = row.get(5)?;
            let requires_ack: bool = row.get(6)?;
            let retry_count: u32 = row.get(7)?;
            let max_retries: u32 = row.get(8)?;
            let first_sent_at: String = row.get(9)?;
            Ok((
                from,
                to,
                kind_json,
                ts_str,
                is_read,
                dispatch_id,
                requires_ack,
                retry_count,
                max_retries,
                first_sent_at,
            ))
        });

        if let Ok(rows) = rows {
            for row in rows.flatten() {
                let (
                    from,
                    to,
                    kind_json,
                    ts_str,
                    read,
                    dispatch_id,
                    requires_ack,
                    retry_count,
                    max_retries,
                    first_sent_at,
                ) = row;
                let Ok(kind) = serde_json::from_str::<DispatchKind>(&kind_json) else {
                    continue;
                };
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let first_sent_at = DateTime::parse_from_rfc3339(&first_sent_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(timestamp);
                result.push(Dispatch {
                    from,
                    to,
                    kind,
                    timestamp,
                    read,
                    id: if dispatch_id.is_empty() {
                        default_dispatch_id()
                    } else {
                        dispatch_id
                    },
                    requires_ack,
                    retry_count,
                    max_retries,
                    first_sent_at,
                });
            }
        }

        result
    }

    fn summarize_health(dispatches: &[Dispatch], overdue_cutoff: DateTime<Utc>) -> DispatchHealth {
        let mut health = DispatchHealth::default();

        for dispatch in dispatches {
            if !dispatch.read {
                health.unread += 1;
            }

            if !dispatch.requires_ack {
                continue;
            }

            if dispatch.retry_count >= dispatch.max_retries {
                health.dead_letters += 1;
                continue;
            }

            if dispatch.read {
                health.awaiting_ack += 1;
                if dispatch.timestamp < overdue_cutoff {
                    health.overdue_ack += 1;
                }
            } else if dispatch.retry_count > 0 {
                health.retrying_delivery += 1;
            }
        }

        health
    }
}

impl Default for DispatchBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_send_and_read() {
        let bus = DispatchBus::new();
        bus.send(Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::TaskDone {
                task_id: "q1".into(),
                summary: "done".into(),
            },
        ))
        .await;

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 1);

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 0);
    }

    #[tokio::test]
    async fn test_indexed_recipient() {
        let bus = DispatchBus::new();

        bus.send(Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::TaskDone {
                task_id: "q1".into(),
                summary: "done".into(),
            },
        ))
        .await;
        bus.send(Dispatch::new_typed(
            "a",
            "c",
            DispatchKind::TaskFailed {
                task_id: "q2".into(),
                error: "err".into(),
            },
        ))
        .await;

        assert_eq!(bus.read("b").await.len(), 1);
        assert_eq!(bus.read("c").await.len(), 1);
        assert_eq!(bus.read("d").await.len(), 0);
    }

    #[tokio::test]
    async fn test_ttl_expiry() {
        let mut bus = DispatchBus::new();
        bus.set_ttl(1);

        bus.send(Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::TaskDone {
                task_id: "q1".into(),
                summary: "done".into(),
            },
        ))
        .await;
        assert_eq!(bus.read("b").await.len(), 1);

        if let BusBackend::Memory { ref queues } = bus.backend {
            let mut queues = queues.lock().await;
            let q = queues.entry("b".to_string()).or_default();
            let old_ts = Utc::now() - chrono::Duration::seconds(10);
            q.push_back(Dispatch {
                from: "a".into(),
                to: "b".into(),
                kind: DispatchKind::TaskDone {
                    task_id: "old".into(),
                    summary: "old".into(),
                },
                timestamp: old_ts,
                read: false,
                id: default_dispatch_id(),
                requires_ack: false,
                retry_count: 0,
                max_retries: 3,
                first_sent_at: old_ts,
            });
        }

        bus.send(Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::TaskDone {
                task_id: "new".into(),
                summary: "new".into(),
            },
        ))
        .await;

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 1);
        match &msgs[0].kind {
            DispatchKind::TaskDone { task_id, .. } => assert_eq!(task_id, "new"),
            _ => panic!("unexpected kind"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("dispatches.jsonl");

        let bus = DispatchBus::with_persistence(path.clone());
        bus.send(Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::TaskDone {
                task_id: "q1".into(),
                summary: "done".into(),
            },
        ))
        .await;

        let bus2 = DispatchBus::with_persistence(path);
        let count = bus2.load().await.unwrap();
        assert_eq!(count, 1);

        let msgs = bus2.read("b").await;
        assert_eq!(msgs.len(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_drain() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("dispatches.jsonl");

        let bus = DispatchBus::with_persistence(path);
        bus.send(Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::TaskDone {
                task_id: "q1".into(),
                summary: "done".into(),
            },
        ))
        .await;
        bus.send(Dispatch::new_typed(
            "a",
            "c",
            DispatchKind::TaskFailed {
                task_id: "q2".into(),
                error: "err".into(),
            },
        ))
        .await;

        assert_eq!(bus.pending_count(), 2);
        let drained = bus.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(bus.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_sqlite_max_queue_depth() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("dispatches.jsonl");

        let mut bus = DispatchBus::with_persistence(path);
        bus.max_queue_per_recipient = 3;

        for i in 0..5 {
            bus.send(Dispatch::new_typed(
                "a",
                "b",
                DispatchKind::TaskDone {
                    task_id: format!("q{i}"),
                    summary: format!("msg{i}"),
                },
            ))
            .await;
        }

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 3);
    }

    #[tokio::test]
    async fn test_ack_required_dispatch() {
        let bus = DispatchBus::new();
        let dispatch = Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::Escalation {
                project: "test".into(),
                task_id: "t1".into(),
                subject: "blocked".into(),
                description: "help".into(),
                attempts: 1,
            },
        )
        .with_ack_required();
        let dispatch_id = dispatch.id.clone();
        assert!(dispatch.requires_ack);
        bus.send(dispatch).await;

        let delivered = bus.read("b").await;
        assert_eq!(delivered.len(), 1);

        let retries = bus.retry_unacked(0).await;
        assert_eq!(retries.len(), 1);
        assert_eq!(retries[0].retry_count, 1);

        // After ack: should not be retried.
        bus.acknowledge(&dispatch_id).await;
        let retries = bus.retry_unacked(0).await;
        assert_eq!(retries.len(), 0);
    }

    #[tokio::test]
    async fn test_dead_letter_after_max_retries() {
        let bus = DispatchBus::new();
        let mut dispatch = Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::Escalation {
                project: "test".into(),
                task_id: "t1".into(),
                subject: "blocked".into(),
                description: "help".into(),
                attempts: 1,
            },
        )
        .with_ack_required();
        dispatch.max_retries = 2;
        bus.send(dispatch).await;
        let delivered = bus.read("b").await;
        assert_eq!(delivered.len(), 1);

        // Retry twice to exhaust max_retries.
        let _ = bus.retry_unacked(0).await; // retry_count → 1
        let retried = bus.read("b").await;
        assert_eq!(retried.len(), 1);
        let _ = bus.retry_unacked(0).await; // retry_count → 2

        // Should now be dead-lettered.
        let dead = bus.dead_letters().await;
        assert_eq!(dead.len(), 1);

        // Retry should return nothing (exceeded max).
        let retries = bus.retry_unacked(0).await;
        assert_eq!(retries.len(), 0);
    }

    #[tokio::test]
    async fn test_ack_prevents_retry() {
        let bus = DispatchBus::new();
        let dispatch = Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::Resolution {
                task_id: "t1".into(),
                answer: "yes".into(),
            },
        )
        .with_ack_required();
        let id = dispatch.id.clone();
        bus.send(dispatch).await;
        let delivered = bus.read("b").await;
        assert_eq!(delivered.len(), 1);

        bus.acknowledge(&id).await;

        let retries = bus.retry_unacked(0).await;
        assert!(retries.is_empty());

        let dead = bus.dead_letters().await;
        assert!(dead.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_health_tracks_delivery_states() {
        let bus = DispatchBus::new();
        let old_ts = Utc::now() - chrono::Duration::seconds(120);

        bus.send(Dispatch {
            from: "a".into(),
            to: "leader".into(),
            kind: DispatchKind::PatrolReport {
                project: "demo".into(),
                active: 2,
                pending: 3,
            },
            timestamp: Utc::now(),
            read: false,
            id: default_dispatch_id(),
            requires_ack: false,
            retry_count: 0,
            max_retries: 3,
            first_sent_at: Utc::now(),
        })
        .await;

        bus.send(Dispatch {
            from: "a".into(),
            to: "leader".into(),
            kind: DispatchKind::Resolution {
                task_id: "t-overdue".into(),
                answer: "answer".into(),
            },
            timestamp: old_ts,
            read: true,
            id: default_dispatch_id(),
            requires_ack: true,
            retry_count: 0,
            max_retries: 3,
            first_sent_at: old_ts,
        })
        .await;
        bus.send(Dispatch {
            from: "a".into(),
            to: "leader".into(),
            kind: DispatchKind::Escalation {
                project: "demo".into(),
                task_id: "t-retry".into(),
                subject: "blocked".into(),
                description: "help".into(),
                attempts: 1,
            },
            timestamp: Utc::now(),
            read: false,
            id: default_dispatch_id(),
            requires_ack: true,
            retry_count: 1,
            max_retries: 3,
            first_sent_at: old_ts,
        })
        .await;
        bus.send(Dispatch {
            from: "a".into(),
            to: "leader".into(),
            kind: DispatchKind::TaskFailed {
                task_id: "t-dead".into(),
                error: "boom".into(),
            },
            timestamp: Utc::now(),
            read: false,
            id: default_dispatch_id(),
            requires_ack: true,
            retry_count: 2,
            max_retries: 2,
            first_sent_at: old_ts,
        })
        .await;

        let health = bus.health(60).await;
        assert_eq!(health.unread, 3);
        assert_eq!(health.awaiting_ack, 1);
        assert_eq!(health.retrying_delivery, 1);
        assert_eq!(health.overdue_ack, 1);
        assert_eq!(health.dead_letters, 1);
    }

    #[tokio::test]
    async fn test_sqlite_dispatch_health_tracks_delivery_states() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("dispatches.jsonl");
        let bus = DispatchBus::with_persistence(path);

        let old_ts = Utc::now() - chrono::Duration::seconds(120);
        bus.send(Dispatch {
            from: "a".into(),
            to: "leader".into(),
            kind: DispatchKind::Resolution {
                task_id: "t-overdue".into(),
                answer: "answer".into(),
            },
            timestamp: old_ts,
            read: true,
            id: default_dispatch_id(),
            requires_ack: true,
            retry_count: 0,
            max_retries: 2,
            first_sent_at: old_ts,
        })
        .await;
        bus.send(Dispatch {
            from: "a".into(),
            to: "leader".into(),
            kind: DispatchKind::Escalation {
                project: "demo".into(),
                task_id: "t-retry".into(),
                subject: "blocked".into(),
                description: "help".into(),
                attempts: 1,
            },
            timestamp: Utc::now(),
            read: false,
            id: default_dispatch_id(),
            requires_ack: true,
            retry_count: 1,
            max_retries: 3,
            first_sent_at: old_ts,
        })
        .await;
        bus.send(Dispatch {
            from: "a".into(),
            to: "leader".into(),
            kind: DispatchKind::TaskFailed {
                task_id: "t-dead".into(),
                error: "boom".into(),
            },
            timestamp: Utc::now(),
            read: false,
            id: default_dispatch_id(),
            requires_ack: true,
            retry_count: 2,
            max_retries: 2,
            first_sent_at: old_ts,
        })
        .await;

        let health = bus.health(60).await;
        assert_eq!(health.unread, 2);
        assert_eq!(health.awaiting_ack, 1);
        assert_eq!(health.retrying_delivery, 1);
        assert_eq!(health.overdue_ack, 1);
        assert_eq!(health.dead_letters, 1);
    }

    #[tokio::test]
    async fn test_sqlite_ack_metadata_survives_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("dispatches.jsonl");

        let bus = DispatchBus::with_persistence(path);
        let dispatch = Dispatch::new_typed(
            "a",
            "b",
            DispatchKind::Resolution {
                task_id: "t1".into(),
                answer: "yes".into(),
            },
        )
        .with_ack_required();
        let id = dispatch.id.clone();
        bus.send(dispatch).await;

        let delivered = bus.read("b").await;
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].id, id);
        assert!(delivered[0].requires_ack);

        let retries = bus.retry_unacked(0).await;
        assert_eq!(retries.len(), 1);
        assert_eq!(retries[0].id, id);
    }

    #[test]
    fn test_critical_dispatches_require_ack_by_default() {
        assert!(
            Dispatch::new_typed(
                "a",
                "leader",
                DispatchKind::TaskDone {
                    task_id: "t1".into(),
                    summary: "done".into(),
                },
            )
            .requires_ack
        );
        assert!(
            Dispatch::new_typed(
                "a",
                "leader",
                DispatchKind::Resolution {
                    task_id: "t1".into(),
                    answer: "yes".into(),
                },
            )
            .requires_ack
        );
        assert!(
            !Dispatch::new_typed(
                "a",
                "leader",
                DispatchKind::PatrolReport {
                    project: "demo".into(),
                    active: 1,
                    pending: 2,
                },
            )
            .requires_ack
        );
    }
}
