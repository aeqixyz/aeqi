//! Agent Expertise Ledger — empirical performance tracking for smart routing.
//!
//! Records task outcomes per agent per domain, then ranks agents using Wilson
//! score lower-bound intervals. An agent with 3/3 successes ranks lower than
//! one with 50/55 (less confident). Enables emergent specialization.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// Outcome kind for a completed task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutcomeKind {
    Done,
    Failed,
    Handoff,
    Blocked,
}

/// A single performance record for an agent on a task domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertiseRecord {
    pub agent_name: String,
    pub task_domain: String,
    pub outcome: TaskOutcomeKind,
    pub cost_usd: f64,
    pub duration_secs: f64,
    pub turns: u32,
    pub timestamp: DateTime<Utc>,
}

/// Aggregated score for an agent in a domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentScore {
    pub agent_name: String,
    pub success_rate: f64,
    pub avg_cost: f64,
    pub total_tasks: u32,
    pub confidence: f64,
}

/// SQLite-backed expertise ledger.
pub struct ExpertiseLedger {
    conn: Mutex<Connection>,
}

impl ExpertiseLedger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create expertise dir: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open expertise DB: {}", path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;

             CREATE TABLE IF NOT EXISTS sigil_expertise (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 agent_name TEXT NOT NULL,
                 task_domain TEXT NOT NULL,
                 outcome TEXT NOT NULL,
                 cost_usd REAL NOT NULL,
                 duration_secs REAL NOT NULL,
                 turns INTEGER NOT NULL,
                 timestamp TEXT NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_expertise_agent_domain
                 ON sigil_expertise(agent_name, task_domain);
             CREATE INDEX IF NOT EXISTS idx_expertise_domain
                 ON sigil_expertise(task_domain);",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Record a task outcome for an agent.
    pub fn record(&self, entry: &ExpertiseRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let outcome_str = serde_json::to_value(entry.outcome)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        conn.execute(
            "INSERT INTO sigil_expertise (agent_name, task_domain, outcome, cost_usd, duration_secs, turns, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.agent_name,
                entry.task_domain,
                outcome_str,
                entry.cost_usd,
                entry.duration_secs,
                entry.turns,
                entry.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Rank agents for a given domain using Wilson score lower bound.
    /// Returns agents sorted by confidence-weighted success rate (best first).
    pub fn rank_for_domain(&self, domain: &str) -> Result<Vec<AgentScore>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;

        let mut stmt = conn.prepare(
            "SELECT agent_name,
                    COUNT(*) as total,
                    SUM(CASE WHEN outcome = 'done' THEN 1 ELSE 0 END) as successes,
                    AVG(cost_usd) as avg_cost
             FROM sigil_expertise
             WHERE task_domain = ?1
             GROUP BY agent_name",
        )?;

        let mut scores: Vec<AgentScore> = stmt
            .query_map(rusqlite::params![domain], |row| {
                let agent_name: String = row.get(0)?;
                let total: u32 = row.get(1)?;
                let successes: u32 = row.get(2)?;
                let avg_cost: f64 = row.get(3)?;
                Ok((agent_name, total, successes, avg_cost))
            })?
            .filter_map(|r| r.ok())
            .map(|(agent_name, total, successes, avg_cost)| {
                let success_rate = if total > 0 {
                    successes as f64 / total as f64
                } else {
                    0.0
                };
                let confidence = wilson_lower_bound(successes, total);
                AgentScore {
                    agent_name,
                    success_rate,
                    avg_cost,
                    total_tasks: total,
                    confidence,
                }
            })
            .collect();

        // Sort by Wilson confidence (highest first), break ties by avg cost (lowest first).
        scores.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.avg_cost
                        .partial_cmp(&b.avg_cost)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        Ok(scores)
    }

    /// Check if an agent should be deprioritized for a domain (>60% failure rate, >3 attempts).
    pub fn is_deprioritized(&self, agent: &str, domain: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;

        let (total, failures): (u32, u32) = conn
            .query_row(
                "SELECT COUNT(*), SUM(CASE WHEN outcome != 'done' THEN 1 ELSE 0 END)
             FROM sigil_expertise WHERE agent_name = ?1 AND task_domain = ?2",
                rusqlite::params![agent, domain],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0));

        Ok(total > 3 && (failures as f64 / total as f64) > 0.6)
    }

    /// Extract a domain string from task labels and subject.
    /// Uses the first label if available, otherwise extracts a keyword from the subject.
    pub fn extract_domain(labels: &[String], subject: &str) -> String {
        // Check for domain-specific labels first.
        for label in labels {
            if !label.starts_with("escalation:")
                && label != "escalated"
                && label != "escalated-system"
                && label != "escalated-human"
            {
                return label.to_lowercase();
            }
        }
        // Fall back to first meaningful word from subject.
        let stop_words = [
            "fix",
            "add",
            "update",
            "implement",
            "create",
            "remove",
            "the",
            "a",
            "an",
        ];
        for word in subject.split_whitespace() {
            let lower = word.to_lowercase();
            let clean: String = lower.chars().filter(|c| c.is_alphanumeric()).collect();
            if clean.len() > 2 && !stop_words.contains(&clean.as_str()) {
                return clean;
            }
        }
        "general".to_string()
    }
}

/// Wilson score lower bound for a binomial proportion.
/// Provides a conservative estimate of the true success rate.
/// z = 1.96 for 95% confidence interval.
fn wilson_lower_bound(successes: u32, total: u32) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let n = total as f64;
    let p = successes as f64 / n;
    let z = 1.96_f64;
    let z2 = z * z;

    let numerator = p + z2 / (2.0 * n) - z * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt();
    let denominator = 1.0 + z2 / n;

    (numerator / denominator).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_ledger() -> (ExpertiseLedger, TempDir) {
        let dir = TempDir::new().unwrap();
        let ledger = ExpertiseLedger::open(&dir.path().join("expertise.db")).unwrap();
        (ledger, dir)
    }

    fn record(ledger: &ExpertiseLedger, agent: &str, domain: &str, outcome: TaskOutcomeKind) {
        ledger
            .record(&ExpertiseRecord {
                agent_name: agent.to_string(),
                task_domain: domain.to_string(),
                outcome,
                cost_usd: 0.01,
                duration_secs: 60.0,
                turns: 5,
                timestamp: Utc::now(),
            })
            .unwrap();
    }

    #[test]
    fn test_record_and_rank() {
        let (ledger, _dir) = temp_ledger();

        // Agent A: 10 done + 2 failed = 83% success
        for _ in 0..10 {
            record(&ledger, "agent-a", "rust", TaskOutcomeKind::Done);
        }
        for _ in 0..2 {
            record(&ledger, "agent-a", "rust", TaskOutcomeKind::Failed);
        }

        // Agent B: 5 done + 5 failed = 50% success
        for _ in 0..5 {
            record(&ledger, "agent-b", "rust", TaskOutcomeKind::Done);
        }
        for _ in 0..5 {
            record(&ledger, "agent-b", "rust", TaskOutcomeKind::Failed);
        }

        let scores = ledger.rank_for_domain("rust").unwrap();
        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].agent_name, "agent-a");
        assert!(scores[0].confidence > scores[1].confidence);
    }

    #[test]
    fn test_new_domain_uniform() {
        let (ledger, _dir) = temp_ledger();
        let scores = ledger.rank_for_domain("unknown").unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_deprioritization() {
        let (ledger, _dir) = temp_ledger();

        // 4 tasks, 3 failed (75% failure rate, > threshold of 60%)
        record(&ledger, "bad-agent", "python", TaskOutcomeKind::Done);
        for _ in 0..3 {
            record(&ledger, "bad-agent", "python", TaskOutcomeKind::Failed);
        }

        assert!(ledger.is_deprioritized("bad-agent", "python").unwrap());
        assert!(!ledger.is_deprioritized("bad-agent", "rust").unwrap());
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            ExpertiseLedger::extract_domain(&["database".to_string()], "Fix login"),
            "database"
        );
        assert_eq!(
            ExpertiseLedger::extract_domain(&[], "Fix authentication bug"),
            "authentication"
        );
        assert_eq!(ExpertiseLedger::extract_domain(&[], "Add the a"), "general");
    }

    #[test]
    fn test_wilson_confidence() {
        // 3/3 should rank lower than 50/55 despite higher success rate
        let small = wilson_lower_bound(3, 3);
        let large = wilson_lower_bound(50, 55);
        assert!(
            large > small,
            "50/55 ({large}) should rank higher than 3/3 ({small})"
        );
    }
}
