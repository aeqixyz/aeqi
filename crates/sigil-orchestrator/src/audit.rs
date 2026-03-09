//! Decision audit trail — structured recording of all orchestration decisions.
//!
//! Every routing, assignment, escalation, and retry decision is recorded with
//! reasoning and metadata. Provides the event stream for watchdog triggers (Phase 8)
//! and accountability for multi-agent orchestration.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// Classification of orchestration decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    TaskAssigned,
    TaskCompleted,
    TaskBlocked,
    TaskFailed,
    TaskEscalated,
    TaskRetried,
    TaskCancelled,
    WorkerSpawned,
    WorkerTimedOut,
    BudgetBlocked,
    RouteDecision,
    PreflightRejected,
    FailureAnalyzed,
    BlackboardPost,
    WatchdogFired,
    MissionDecomposed,
    DependencyInferred,
}

impl std::fmt::Display for DecisionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{self:?}"));
        write!(f, "{s}")
    }
}

/// A single recorded decision event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub project: String,
    pub task_id: Option<String>,
    pub decision_type: DecisionType,
    pub agent: Option<String>,
    pub reasoning: String,
    pub metadata: serde_json::Value,
}

impl AuditEvent {
    pub fn new(
        project: impl Into<String>,
        decision_type: DecisionType,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            project: project.into(),
            task_id: None,
            decision_type,
            agent: None,
            reasoning: reasoning.into(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_task(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// SQLite-backed audit log for decision events.
pub struct AuditLog {
    conn: Mutex<Connection>,
}

impl AuditLog {
    /// Open or create an audit log at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create audit dir: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open audit DB: {}", path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;

             CREATE TABLE IF NOT EXISTS sigil_decisions (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp TEXT NOT NULL,
                 project TEXT NOT NULL,
                 task_id TEXT,
                 decision_type TEXT NOT NULL,
                 agent TEXT,
                 reasoning TEXT NOT NULL,
                 metadata_json TEXT NOT NULL DEFAULT '{}'
             );

             CREATE INDEX IF NOT EXISTS idx_decisions_task
                 ON sigil_decisions(task_id);
             CREATE INDEX IF NOT EXISTS idx_decisions_project
                 ON sigil_decisions(project);
             CREATE INDEX IF NOT EXISTS idx_decisions_timestamp
                 ON sigil_decisions(timestamp);",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Record a decision event.
    pub fn record(&self, event: &AuditEvent) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let metadata_json = serde_json::to_string(&event.metadata).unwrap_or_default();
        let decision_type_str = event.decision_type.to_string();

        conn.execute(
            "INSERT INTO sigil_decisions (timestamp, project, task_id, decision_type, agent, reasoning, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                event.timestamp.to_rfc3339(),
                event.project,
                event.task_id,
                decision_type_str,
                event.agent,
                event.reasoning,
                metadata_json,
            ],
        )?;
        Ok(())
    }

    /// Query events for a specific task.
    pub fn query_by_task(&self, task_id: &str) -> Result<Vec<AuditEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        Self::query_events(
            &conn,
            "SELECT timestamp, project, task_id, decision_type, agent, reasoning, metadata_json
             FROM sigil_decisions WHERE task_id = ?1 ORDER BY timestamp ASC",
            rusqlite::params![task_id],
        )
    }

    /// Query events for a specific project.
    pub fn query_by_project(&self, project: &str) -> Result<Vec<AuditEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        Self::query_events(
            &conn,
            "SELECT timestamp, project, task_id, decision_type, agent, reasoning, metadata_json
             FROM sigil_decisions WHERE project = ?1 ORDER BY timestamp ASC",
            rusqlite::params![project],
        )
    }

    /// Query the N most recent events.
    pub fn query_recent(&self, limit: u32) -> Result<Vec<AuditEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        Self::query_events(
            &conn,
            "SELECT timestamp, project, task_id, decision_type, agent, reasoning, metadata_json
             FROM sigil_decisions ORDER BY timestamp DESC LIMIT ?1",
            rusqlite::params![limit],
        )
    }

    fn query_events(
        conn: &Connection,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<AuditEvent>> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, |row| {
            let ts_str: String = row.get(0)?;
            let project: String = row.get(1)?;
            let task_id: Option<String> = row.get(2)?;
            let decision_type_str: String = row.get(3)?;
            let agent: Option<String> = row.get(4)?;
            let reasoning: String = row.get(5)?;
            let metadata_json: String = row.get(6)?;
            Ok((
                ts_str,
                project,
                task_id,
                decision_type_str,
                agent,
                reasoning,
                metadata_json,
            ))
        })?;

        let mut events = Vec::new();
        for row in rows {
            let (ts_str, project, task_id, decision_type_str, agent, reasoning, metadata_json) =
                row?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let decision_type: DecisionType =
                serde_json::from_value(serde_json::Value::String(decision_type_str.clone()))
                    .unwrap_or(DecisionType::RouteDecision);
            let metadata: serde_json::Value =
                serde_json::from_str(&metadata_json).unwrap_or(serde_json::Value::Null);

            events.push(AuditEvent {
                timestamp,
                project,
                task_id,
                decision_type,
                agent,
                reasoning,
                metadata,
            });
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_audit() -> (AuditLog, TempDir) {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::open(&dir.path().join("audit.db")).unwrap();
        (log, dir)
    }

    #[test]
    fn test_audit_record_and_query_by_task() {
        let (log, _dir) = temp_audit();

        log.record(
            &AuditEvent::new(
                "proj-a",
                DecisionType::TaskAssigned,
                "best agent for domain",
            )
            .with_task("t-001")
            .with_agent("worker-1"),
        )
        .unwrap();

        log.record(
            &AuditEvent::new("proj-a", DecisionType::WorkerTimedOut, "exceeded 1800s")
                .with_task("t-001")
                .with_agent("worker-1"),
        )
        .unwrap();

        log.record(
            &AuditEvent::new("proj-a", DecisionType::TaskAssigned, "reassigned")
                .with_task("t-002")
                .with_agent("worker-2"),
        )
        .unwrap();

        let events = log.query_by_task("t-001").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].decision_type, DecisionType::TaskAssigned);
        assert_eq!(events[1].decision_type, DecisionType::WorkerTimedOut);
    }

    #[test]
    fn test_audit_query_by_project() {
        let (log, _dir) = temp_audit();

        log.record(
            &AuditEvent::new("proj-a", DecisionType::TaskAssigned, "assigned").with_task("t-001"),
        )
        .unwrap();
        log.record(&AuditEvent::new(
            "proj-b",
            DecisionType::BudgetBlocked,
            "over budget",
        ))
        .unwrap();
        log.record(
            &AuditEvent::new("proj-a", DecisionType::TaskEscalated, "escalated").with_task("t-001"),
        )
        .unwrap();

        let events = log.query_by_project("proj-a").unwrap();
        assert_eq!(events.len(), 2);

        let events_b = log.query_by_project("proj-b").unwrap();
        assert_eq!(events_b.len(), 1);
        assert_eq!(events_b[0].decision_type, DecisionType::BudgetBlocked);
    }

    #[test]
    fn test_audit_query_recent() {
        let (log, _dir) = temp_audit();

        for i in 0..10 {
            log.record(
                &AuditEvent::new("proj", DecisionType::TaskAssigned, format!("event {i}"))
                    .with_task(format!("t-{i:03}")),
            )
            .unwrap();
        }

        let events = log.query_recent(3).unwrap();
        assert_eq!(events.len(), 3);
        // Most recent first
        assert!(events[0].reasoning.contains("event 9"));
    }
}
