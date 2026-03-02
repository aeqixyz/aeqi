use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DispatchKind {
    QuestDone { task_id: String, summary: String },
    QuestBlocked { task_id: String, question: String, context: String },
    QuestFailed { task_id: String, error: String },

    PatrolReport { #[serde(alias = "domain")] project: String, active: usize, pending: usize },
    WorkerCrashed { #[serde(alias = "domain")] project: String, worker: String, error: String },
    Escalation { #[serde(alias = "domain")] project: String, task_id: String, subject: String, description: String, attempts: u32 },

    PulseAlert { #[serde(alias = "domain")] project: String, issues: String },

    Resolution { task_id: String, answer: String },

    QuestProposal { #[serde(alias = "domain")] project: String, prefix: String, subject: String, description: String, confidence: f32, reasoning: String },

    AgentAdvice { agent: String, topic: String, advice: String, cost_usd: f64 },

    CouncilTopic { topic_id: String, message: String, familiars: Vec<String> },
    ChamberResponse { topic_id: String, familiar: String, response: String },
    ChamberSynthesis { topic_id: String, synthesis: String },
}

impl DispatchKind {
    pub fn subject_tag(&self) -> &'static str {
        match self {
            Self::QuestDone { .. } => "DONE",
            Self::QuestBlocked { .. } => "BLOCKED",
            Self::QuestFailed { .. } => "FAILED",
            Self::PatrolReport { .. } => "PATROL",
            Self::WorkerCrashed { .. } => "WORKER_CRASHED",
            Self::Escalation { .. } => "ESCALATE",
            Self::PulseAlert { .. } => "HEARTBEAT_ALERT",
            Self::Resolution { .. } => "RESOLVED",
            Self::QuestProposal { .. } => "QUEST_PROPOSAL",
            Self::AgentAdvice { .. } => "AGENT_ADVICE",
            Self::CouncilTopic { .. } => "CHAMBER_TOPIC",
            Self::ChamberResponse { .. } => "CHAMBER_RESPONSE",
            Self::ChamberSynthesis { .. } => "CHAMBER_SYNTHESIS",
        }
    }

    pub fn body_text(&self) -> String {
        match self {
            Self::QuestDone { task_id, summary } =>
                format!("Completed quest {task_id}: {summary}"),
            Self::QuestBlocked { task_id, question, context } =>
                format!("Task {task_id} blocked: {question}\n\nFull context:\n{context}"),
            Self::QuestFailed { task_id, error } =>
                format!("Failed quest {task_id}: {error}"),
            Self::PatrolReport { project, active, pending } =>
                format!("Project {project}: {active} active spirits, {pending} pending quests"),
            Self::WorkerCrashed { project, worker, error } =>
                format!("Worker {worker} crashed in {project}: {error}"),
            Self::Escalation { project, task_id, subject, description, attempts } =>
                format!(
                    "Project {project} needs help resolving a blocker.\n\n\
                     Task: {task_id} — {subject}\n\n\
                     Full description:\n{description}\n\n\
                     Blocked after {attempts} resolution attempt(s).",
                ),
            Self::PulseAlert { project, issues } =>
                format!("Project {project} pulse detected issues:\n{issues}"),
            Self::Resolution { task_id, answer } =>
                format!("Resolution for quest {task_id}: {answer}"),
            Self::QuestProposal { project, prefix, subject, confidence, reasoning, .. } =>
                format!("Gap proposal for {project} ({prefix}): \"{subject}\" (confidence: {:.0}%) — {reasoning}", confidence * 100.0),
            Self::AgentAdvice { agent, topic, advice, cost_usd } =>
                format!("[{agent}] on \"{topic}\" (${cost_usd:.3}): {advice}"),
            Self::CouncilTopic { topic_id, message, familiars } =>
                format!("Council {topic_id}: \"{message}\" — summoning: {}", familiars.join(", ")),
            Self::ChamberResponse { topic_id, familiar, response } =>
                format!("Council {topic_id} [{familiar}]: {response}"),
            Self::ChamberSynthesis { topic_id, synthesis } =>
                format!("Council {topic_id} synthesis: {synthesis}"),
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
}

impl Dispatch {
    pub fn new_typed(from: &str, to: &str, kind: DispatchKind) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            timestamp: Utc::now(),
            read: false,
        }
    }
}

enum BusBackend {
    Memory {
        queues: tokio::sync::Mutex<std::collections::HashMap<String, std::collections::VecDeque<Dispatch>>>,
    },
    Sqlite {
        conn: Mutex<Connection>,
    },
}

pub struct DispatchBus {
    backend: BusBackend,
    ttl_secs: u64,
    max_queue_per_recipient: usize,
}

impl DispatchBus {
    pub fn new() -> Self {
        Self {
            backend: BusBackend::Memory {
                queues: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            },
            ttl_secs: 3600,
            max_queue_per_recipient: 1000,
        }
    }

    pub fn with_persistence(path: PathBuf) -> Self {
        let db_path = path.with_extension("db");
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match Self::open_sqlite(&db_path) {
            Ok(conn) => {
                debug!(path = %db_path.display(), "whisper bus using SQLite WAL");
                Self {
                    backend: BusBackend::Sqlite { conn: Mutex::new(conn) },
                    ttl_secs: 3600,
                    max_queue_per_recipient: 1000,
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to open SQLite whisper bus, falling back to memory");
                Self::new()
            }
        }
    }

    fn open_sqlite(path: &std::path::Path) -> Result<Connection> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open whisper DB: {}", path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;

             CREATE TABLE IF NOT EXISTS whispers (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 from_agent TEXT NOT NULL,
                 to_agent TEXT NOT NULL,
                 kind_json TEXT NOT NULL,
                 timestamp TEXT NOT NULL,
                 is_read INTEGER NOT NULL DEFAULT 0
             );

             CREATE INDEX IF NOT EXISTS idx_whispers_recipient
                 ON whispers(to_agent, is_read);
             CREATE INDEX IF NOT EXISTS idx_whispers_timestamp
                 ON whispers(timestamp);"
        )?;

        Ok(conn)
    }

    pub fn set_ttl(&mut self, secs: u64) {
        self.ttl_secs = secs;
    }

    pub async fn send(&self, mail: Dispatch) {
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

                let cutoff = (Utc::now() - chrono::Duration::seconds(self.ttl_secs as i64)).to_rfc3339();
                let _ = conn.execute("DELETE FROM whispers WHERE timestamp < ?1", rusqlite::params![cutoff]);

                let count: u32 = conn.query_row(
                    "SELECT COUNT(*) FROM whispers WHERE to_agent = ?1",
                    rusqlite::params![mail.to],
                    |row| row.get(0),
                ).unwrap_or(0);

                if count as usize >= self.max_queue_per_recipient {
                    let excess = count as usize - self.max_queue_per_recipient + 1;
                    let _ = conn.execute(
                        "DELETE FROM whispers WHERE id IN (
                            SELECT id FROM whispers WHERE to_agent = ?1
                            ORDER BY timestamp ASC LIMIT ?2
                        )",
                        rusqlite::params![mail.to, excess],
                    );
                }

                let kind_json = match serde_json::to_string(&mail.kind) {
                    Ok(j) => j,
                    Err(e) => { warn!(error = %e, "failed to serialize whisper kind"); return; }
                };

                let _ = conn.execute(
                    "INSERT INTO whispers (from_agent, to_agent, kind_json, timestamp, is_read)
                     VALUES (?1, ?2, ?3, ?4, 0)",
                    rusqlite::params![
                        mail.from,
                        mail.to,
                        kind_json,
                        mail.timestamp.to_rfc3339(),
                    ],
                );
            }
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
                let Ok(conn) = conn.lock() else { return Vec::new() };
                let mut result = Vec::new();

                let mut stmt = match conn.prepare(
                    "SELECT id, from_agent, to_agent, kind_json, timestamp
                     FROM whispers WHERE to_agent = ?1 AND is_read = 0
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
                    Ok((id, from, to, kind_json, ts_str))
                });

                if let Ok(rows) = rows {
                    for row in rows.flatten() {
                        let (id, from, to, kind_json, ts_str) = row;
                        let Ok(kind) = serde_json::from_str::<DispatchKind>(&kind_json) else { continue };
                        let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());

                        result.push(Dispatch { from, to, kind, timestamp, read: true });
                        ids_to_mark.push(id);
                    }
                }

                for id in ids_to_mark {
                    let _ = conn.execute("UPDATE whispers SET is_read = 1 WHERE id = ?1", rusqlite::params![id]);
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
                let Ok(conn) = conn.lock() else { return Vec::new() };
                Self::query_whispers(&conn, "SELECT from_agent, to_agent, kind_json, timestamp, is_read FROM whispers ORDER BY timestamp ASC", &[])
            }
        }
    }

    pub async fn unread_count(&self, recipient: &str) -> usize {
        match &self.backend {
            BusBackend::Memory { queues } => {
                let queues = queues.lock().await;
                queues.get(recipient)
                    .map(|q| q.iter().filter(|m| !m.read).count())
                    .unwrap_or(0)
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return 0 };
                conn.query_row(
                    "SELECT COUNT(*) FROM whispers WHERE to_agent = ?1 AND is_read = 0",
                    rusqlite::params![recipient],
                    |row| row.get::<_, u32>(0),
                ).unwrap_or(0) as usize
            }
        }
    }

    pub fn pending_count(&self) -> usize {
        match &self.backend {
            BusBackend::Memory { queues } => {
                queues.try_lock()
                    .map(|queues| {
                        queues.values()
                            .flat_map(|q| q.iter())
                            .filter(|m| !m.read)
                            .count()
                    })
                    .unwrap_or(0)
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return 0 };
                conn.query_row(
                    "SELECT COUNT(*) FROM whispers WHERE is_read = 0",
                    [],
                    |row| row.get::<_, u32>(0),
                ).unwrap_or(0) as usize
            }
        }
    }

    pub fn drain(&self) -> Vec<Dispatch> {
        match &self.backend {
            BusBackend::Memory { queues } => {
                queues.try_lock()
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
                    .unwrap_or_default()
            }
            BusBackend::Sqlite { conn } => {
                let Ok(conn) = conn.lock() else { return Vec::new() };
                let result = Self::query_whispers(
                    &conn,
                    "SELECT from_agent, to_agent, kind_json, timestamp, is_read FROM whispers WHERE is_read = 0 ORDER BY timestamp ASC",
                    &[],
                );
                let _ = conn.execute("UPDATE whispers SET is_read = 1 WHERE is_read = 0", []);
                result
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
                let count: u32 = conn.query_row(
                    "SELECT COUNT(*) FROM whispers WHERE is_read = 0",
                    [],
                    |row| row.get(0),
                ).unwrap_or(0);
                if count > 0 {
                    debug!(count, "whisper bus has persisted unread messages");
                }
                Ok(count as usize)
            }
            BusBackend::Memory { .. } => Ok(0),
        }
    }

    fn query_whispers(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Vec<Dispatch> {
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
            Ok((from, to, kind_json, ts_str, is_read))
        });

        if let Ok(rows) = rows {
            for row in rows.flatten() {
                let (from, to, kind_json, ts_str, read) = row;
                let Ok(kind) = serde_json::from_str::<DispatchKind>(&kind_json) else { continue };
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                result.push(Dispatch { from, to, kind, timestamp, read });
            }
        }

        result
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
        bus.send(Dispatch::new_typed("a", "b", DispatchKind::QuestDone {
            task_id: "q1".into(),
            summary: "done".into(),
        })).await;

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 1);

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 0);
    }

    #[tokio::test]
    async fn test_indexed_recipient() {
        let bus = DispatchBus::new();

        bus.send(Dispatch::new_typed("a", "b", DispatchKind::QuestDone {
            task_id: "q1".into(), summary: "done".into(),
        })).await;
        bus.send(Dispatch::new_typed("a", "c", DispatchKind::QuestFailed {
            task_id: "q2".into(), error: "err".into(),
        })).await;

        assert_eq!(bus.read("b").await.len(), 1);
        assert_eq!(bus.read("c").await.len(), 1);
        assert_eq!(bus.read("d").await.len(), 0);
    }

    #[tokio::test]
    async fn test_ttl_expiry() {
        let mut bus = DispatchBus::new();
        bus.set_ttl(1);

        bus.send(Dispatch::new_typed("a", "b", DispatchKind::QuestDone {
            task_id: "q1".into(), summary: "done".into(),
        })).await;
        assert_eq!(bus.read("b").await.len(), 1);

        if let BusBackend::Memory { ref queues } = bus.backend {
            let mut queues = queues.lock().await;
            let q = queues.entry("b".to_string()).or_default();
            q.push_back(Dispatch {
                from: "a".into(),
                to: "b".into(),
                kind: DispatchKind::QuestDone { task_id: "old".into(), summary: "old".into() },
                timestamp: Utc::now() - chrono::Duration::seconds(10),
                read: false,
            });
        }

        bus.send(Dispatch::new_typed("a", "b", DispatchKind::QuestDone {
            task_id: "new".into(), summary: "new".into(),
        })).await;

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 1);
        match &msgs[0].kind {
            DispatchKind::QuestDone { task_id, .. } => assert_eq!(task_id, "new"),
            _ => panic!("unexpected kind"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("whispers.jsonl");

        let bus = DispatchBus::with_persistence(path.clone());
        bus.send(Dispatch::new_typed("a", "b", DispatchKind::QuestDone {
            task_id: "q1".into(), summary: "done".into(),
        })).await;

        let bus2 = DispatchBus::with_persistence(path);
        let count = bus2.load().await.unwrap();
        assert_eq!(count, 1);

        let msgs = bus2.read("b").await;
        assert_eq!(msgs.len(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_drain() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("whispers.jsonl");

        let bus = DispatchBus::with_persistence(path);
        bus.send(Dispatch::new_typed("a", "b", DispatchKind::QuestDone {
            task_id: "q1".into(), summary: "done".into(),
        })).await;
        bus.send(Dispatch::new_typed("a", "c", DispatchKind::QuestFailed {
            task_id: "q2".into(), error: "err".into(),
        })).await;

        assert_eq!(bus.pending_count(), 2);
        let drained = bus.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(bus.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_sqlite_max_queue_depth() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("whispers.jsonl");

        let mut bus = DispatchBus::with_persistence(path);
        bus.max_queue_per_recipient = 3;

        for i in 0..5 {
            bus.send(Dispatch::new_typed("a", "b", DispatchKind::QuestDone {
                task_id: format!("q{i}"), summary: format!("msg{i}"),
            })).await;
        }

        let msgs = bus.read("b").await;
        assert_eq!(msgs.len(), 3);
    }
}
