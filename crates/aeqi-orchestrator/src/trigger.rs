//! Trigger system — proactive agent automation via schedules and events.
//!
//! Every trigger is owned by a persistent agent (FK to agents table).
//! When a trigger fires, the daemon spawns a new async session for
//! the owning agent with the trigger's skill loaded.
//!
//! Trigger types:
//! - Schedule: cron expression ("0 9 * * *") or interval ("every 1h")
//! - Once: fire at a specific time, then auto-disable
//! - Event: pattern match on ExecutionEvent with mandatory cooldown

use anyhow::Result;
use chrono::{DateTime, Datelike, Timelike, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A persistent trigger owned by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    pub trigger_type: TriggerType,
    pub skill: String,
    pub enabled: bool,
    pub max_budget_usd: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub last_fired: Option<DateTime<Utc>>,
    pub fire_count: u32,
    pub total_cost_usd: f64,
    /// Prompt entries injected into tasks created by this trigger.
    #[serde(default)]
    pub prompts: Vec<aeqi_core::PromptEntry>,
}

/// Trigger classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TriggerType {
    /// Cron expression or interval (e.g., "every 1h").
    #[serde(rename = "schedule")]
    Schedule { expr: String },
    /// One-shot: fire once at a specific time, then auto-disable.
    #[serde(rename = "once")]
    Once { at: DateTime<Utc> },
    /// Event-driven: pattern match on ExecutionEvent with mandatory cooldown.
    #[serde(rename = "event")]
    Event {
        pattern: EventPattern,
        cooldown_secs: u64,
    },
    /// Webhook: externally triggered via HTTP POST.
    #[serde(rename = "webhook")]
    Webhook {
        /// Public ID used in the URL path (not the internal trigger UUID).
        public_id: String,
        /// Optional HMAC-SHA256 signing secret for payload verification.
        signing_secret: Option<String>,
    },
}

/// Event patterns that can fire triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum EventPattern {
    #[serde(rename = "quest_completed")]
    QuestCompleted {
        #[serde(skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    #[serde(rename = "quest_failed")]
    QuestFailed {
        #[serde(skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    #[serde(rename = "tool_call_completed")]
    ToolCallCompleted {
        #[serde(skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
    },
    /// Fire when a memory entry matching the key pattern is stored.
    #[serde(rename = "memory_stored")]
    MemoryStored {
        #[serde(skip_serializing_if = "Option::is_none")]
        key_pattern: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// Fire when a note entry matching the key pattern is posted.
    #[serde(rename = "note_posted")]
    NotePosted {
        #[serde(skip_serializing_if = "Option::is_none")]
        key_pattern: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// Fire when project cost exceeds a threshold (USD).
    #[serde(rename = "budget_exceeded")]
    BudgetExceeded { project: String, threshold_usd: f64 },
    /// Fire when a persistent agent has been idle for N seconds.
    #[serde(rename = "agent_idle")]
    AgentIdle {
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
        idle_secs: u64,
    },
    /// Fire when a dispatch is received matching criteria.
    #[serde(rename = "dispatch_received")]
    DispatchReceived {
        #[serde(skip_serializing_if = "Option::is_none")]
        from_agent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        to_agent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    /// Fire when a message is posted to a conversation channel.
    #[serde(rename = "channel_message")]
    ChannelMessage {
        #[serde(skip_serializing_if = "Option::is_none")]
        channel_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_agent: Option<String>,
    },
}

/// Verify HMAC-SHA256 signature for webhook payloads.
///
/// Supports signatures with or without the `sha256=` prefix (GitHub-style).
pub fn verify_webhook_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let sig = signature.strip_prefix("sha256=").unwrap_or(signature);
    let Ok(sig_bytes) = hex::decode(sig) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    mac.verify_slice(&sig_bytes).is_ok()
}

/// Generate a URL-friendly public ID for webhooks (12 hex chars).
pub fn generate_webhook_public_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 6] = rng.random();
    hex::encode(bytes)
}

/// Input for creating a new trigger.
pub struct NewTrigger {
    pub agent_id: String,
    pub name: String,
    pub trigger_type: TriggerType,
    pub skill: String,
    pub max_budget_usd: Option<f64>,
}

// ---------------------------------------------------------------------------
// Trigger type helpers
// ---------------------------------------------------------------------------

impl TriggerType {
    pub fn type_str(&self) -> &str {
        match self {
            TriggerType::Schedule { .. } => "schedule",
            TriggerType::Once { .. } => "once",
            TriggerType::Event { .. } => "event",
            TriggerType::Webhook { .. } => "webhook",
        }
    }

    /// Serialize the type-specific config to JSON.
    pub fn config_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialize from type string + config JSON.
    pub fn from_db(type_str: &str, config_json: &str) -> Option<Self> {
        match type_str {
            "schedule" => {
                let v: serde_json::Value = serde_json::from_str(config_json).ok()?;
                let expr = v.get("expr")?.as_str()?.to_string();
                Some(TriggerType::Schedule { expr })
            }
            "once" => {
                let v: serde_json::Value = serde_json::from_str(config_json).ok()?;
                let at_str = v.get("at")?.as_str()?;
                let at = DateTime::parse_from_rfc3339(at_str)
                    .ok()?
                    .with_timezone(&Utc);
                Some(TriggerType::Once { at })
            }
            "event" => {
                let v: serde_json::Value = serde_json::from_str(config_json).ok()?;
                let pattern: EventPattern =
                    serde_json::from_value(v.get("pattern")?.clone()).ok()?;
                let cooldown_secs = v.get("cooldown_secs")?.as_u64()?;
                Some(TriggerType::Event {
                    pattern,
                    cooldown_secs,
                })
            }
            "webhook" => {
                let v: serde_json::Value = serde_json::from_str(config_json).ok()?;
                let public_id = v.get("public_id")?.as_str()?.to_string();
                let signing_secret = v
                    .get("signing_secret")
                    .and_then(|s| s.as_str())
                    .map(String::from);
                Some(TriggerType::Webhook {
                    public_id,
                    signing_secret,
                })
            }
            _ => None,
        }
    }
}

impl Trigger {
    /// Check if this trigger's schedule is currently due.
    pub fn is_due(&self) -> bool {
        if !self.enabled {
            return false;
        }
        match &self.trigger_type {
            TriggerType::Schedule { expr } => is_schedule_due(expr, self.last_fired.as_ref()),
            TriggerType::Once { at } => self.last_fired.is_none() && Utc::now() >= *at,
            TriggerType::Event { .. } => false, // Events are checked separately
            TriggerType::Webhook { .. } => false, // Webhooks are externally triggered
        }
    }
}

/// Check if a schedule expression (cron or interval) is due.
pub fn is_schedule_due(expr: &str, last_fired: Option<&DateTime<Utc>>) -> bool {
    let now = Utc::now();

    // Try interval first ("every 30m", "every 1h", "every 2d")
    if let Some(duration) = parse_interval(expr) {
        return match last_fired {
            None => true, // Never fired → immediately due
            Some(last) => (now - *last).num_seconds() >= duration.num_seconds(),
        };
    }

    // Try cron expression
    if let Some(matcher) = parse_simple_cron(expr) {
        if !matcher.matches(&now) {
            return false;
        }
        // Don't fire if we already ran this minute.
        if let Some(last) = last_fired
            && last.minute() == now.minute()
            && last.hour() == now.hour()
            && last.day() == now.day()
        {
            return false;
        }
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Cron matching (moved from schedule.rs)
// ---------------------------------------------------------------------------

struct CronMatcher {
    minute: CronField,
    hour: CronField,
    day: CronField,
    month: CronField,
    weekday: CronField,
}

enum CronField {
    Any,
    Value(u32),
    Step(u32),
}

impl CronMatcher {
    fn matches(&self, dt: &DateTime<Utc>) -> bool {
        self.minute.matches(dt.minute())
            && self.hour.matches(dt.hour())
            && self.day.matches(dt.day())
            && self.month.matches(dt.month())
            && self.weekday.matches(dt.weekday().num_days_from_sunday())
    }
}

impl CronField {
    fn matches(&self, value: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Value(v) => *v == value,
            CronField::Step(s) => *s > 0 && value.is_multiple_of(*s),
        }
    }
}

fn parse_simple_cron(expr: &str) -> Option<CronMatcher> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }

    fn parse_field(s: &str) -> CronField {
        if s == "*" {
            CronField::Any
        } else if let Some(step) = s.strip_prefix("*/") {
            step.parse().map(CronField::Step).unwrap_or(CronField::Any)
        } else {
            s.parse().map(CronField::Value).unwrap_or(CronField::Any)
        }
    }

    Some(CronMatcher {
        minute: parse_field(parts[0]),
        hour: parse_field(parts[1]),
        day: parse_field(parts[2]),
        month: parse_field(parts[3]),
        weekday: parse_field(parts[4]),
    })
}

// ---------------------------------------------------------------------------
// Interval parsing ("every 30m", "every 1h", "every 2d")
// ---------------------------------------------------------------------------

fn parse_interval(expr: &str) -> Option<chrono::Duration> {
    let expr = expr.trim().to_lowercase();
    let body = expr.strip_prefix("every ")?.trim();

    // Parse number + unit
    let (num_str, unit) = if let Some(n) = body.strip_suffix('m') {
        (n, 'm')
    } else if let Some(n) = body.strip_suffix('h') {
        (n, 'h')
    } else if let Some(n) = body.strip_suffix('d') {
        (n, 'd')
    } else if let Some(n) = body.strip_suffix('s') {
        (n, 's')
    } else {
        return None;
    };

    let num: i64 = num_str.trim().parse().ok()?;
    if num <= 0 {
        return None;
    }

    match unit {
        's' => Some(chrono::Duration::seconds(num)),
        'm' => Some(chrono::Duration::minutes(num)),
        'h' => Some(chrono::Duration::hours(num)),
        'd' => Some(chrono::Duration::days(num)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Event matching
// ---------------------------------------------------------------------------

impl EventPattern {
    /// Check if an execution event matches this pattern.
    ///
    /// Note: QuestCompleted/QuestFailed events don't carry a project field,
    /// so project filters match against the task_id prefix convention
    /// (task IDs are formatted as "project:counter").
    pub fn matches_event(&self, event: &crate::execution_events::ExecutionEvent) -> bool {
        use crate::execution_events::ExecutionEvent;
        match (self, event) {
            (
                EventPattern::QuestCompleted { project },
                ExecutionEvent::QuestCompleted { task_id, .. },
            ) => match project {
                Some(p) => task_id.starts_with(&format!("{p}:")),
                None => true,
            },
            (
                EventPattern::QuestFailed { project },
                ExecutionEvent::QuestFailed { task_id, .. },
            ) => match project {
                Some(p) => task_id.starts_with(&format!("{p}:")),
                None => true,
            },
            (
                EventPattern::ToolCallCompleted { tool },
                ExecutionEvent::ToolCallCompleted { tool_name, .. },
            ) => match tool {
                Some(t) => tool_name == t,
                None => true,
            },
            (
                EventPattern::MemoryStored {
                    key_pattern,
                    scope: pattern_scope,
                },
                ExecutionEvent::MemoryStored { key, scope, .. },
            ) => {
                let key_match = key_pattern
                    .as_ref()
                    .is_none_or(|p| key.contains(p.as_str()));
                let scope_match = pattern_scope
                    .as_ref()
                    .is_none_or(|s| scope.eq_ignore_ascii_case(s));
                key_match && scope_match
            }
            (
                EventPattern::NotePosted {
                    key_pattern,
                    project: pattern_project,
                },
                ExecutionEvent::NotePosted { key, project, .. },
            ) => {
                let key_match = key_pattern
                    .as_ref()
                    .is_none_or(|p| key.contains(p.as_str()));
                let proj_match = pattern_project.as_ref().is_none_or(|p| project == p);
                key_match && proj_match
            }
            (
                EventPattern::BudgetExceeded {
                    project: pattern_project,
                    threshold_usd,
                },
                ExecutionEvent::BudgetExceeded {
                    project,
                    current_usd,
                    ..
                },
            ) => project == pattern_project && current_usd >= threshold_usd,
            (
                EventPattern::AgentIdle {
                    agent_id: pattern_agent,
                    idle_secs: threshold,
                },
                ExecutionEvent::AgentIdle {
                    agent_id,
                    idle_secs,
                },
            ) => {
                let agent_match = pattern_agent.as_ref().is_none_or(|p| agent_id == p);
                agent_match && idle_secs >= threshold
            }
            (
                EventPattern::DispatchReceived {
                    from_agent: pattern_from,
                    to_agent: pattern_to,
                    kind: pattern_kind,
                },
                ExecutionEvent::DispatchReceived {
                    from_agent,
                    to_agent,
                    kind,
                },
            ) => {
                let from_match = pattern_from.as_ref().is_none_or(|p| from_agent == p);
                let to_match = pattern_to.as_ref().is_none_or(|p| to_agent == p);
                let kind_match = pattern_kind.as_ref().is_none_or(|k| kind == k);
                from_match && to_match && kind_match
            }
            (
                EventPattern::ChannelMessage {
                    channel_name: pattern_channel,
                    from_agent: pattern_from,
                },
                ExecutionEvent::ChannelMessage {
                    channel_name,
                    from_agent,
                    ..
                },
            ) => {
                let channel_match = pattern_channel.as_ref().is_none_or(|c| channel_name == c);
                let from_match = pattern_from.as_ref().is_none_or(|f| from_agent == f);
                channel_match && from_match
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// TriggerStore — SQLite-backed
// ---------------------------------------------------------------------------

/// SQLite-backed store for triggers. Shares the agents.db connection.
pub struct TriggerStore {
    db: Arc<Mutex<Connection>>,
}

impl TriggerStore {
    /// Create a new TriggerStore sharing a database connection.
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Create a new trigger.
    pub async fn create(&self, t: &NewTrigger) -> Result<Trigger> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let type_str = t.trigger_type.type_str().to_string();
        let config = t.trigger_type.config_json();

        let trigger = Trigger {
            id: id.clone(),
            agent_id: t.agent_id.clone(),
            name: t.name.clone(),
            trigger_type: t.trigger_type.clone(),
            skill: t.skill.clone(),
            enabled: true,
            max_budget_usd: t.max_budget_usd,
            created_at: now,
            last_fired: None,
            fire_count: 0,
            total_cost_usd: 0.0,
            prompts: Vec::new(),
        };

        // Extract public_id for webhook triggers so it lives in a dedicated column.
        let public_id_col: Option<String> = match &trigger.trigger_type {
            TriggerType::Webhook { public_id, .. } => Some(public_id.clone()),
            _ => None,
        };

        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO triggers (id, agent_id, name, trigger_type, config, skill, enabled, max_budget_usd, created_at, public_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9)",
            params![
                trigger.id,
                trigger.agent_id,
                trigger.name,
                type_str,
                config,
                trigger.skill,
                trigger.max_budget_usd,
                trigger.created_at.to_rfc3339(),
                public_id_col,
            ],
        )?;

        info!(id = %trigger.id, name = %trigger.name, agent = %trigger.agent_id, "trigger created");
        Ok(trigger)
    }

    /// Get a trigger by ID.
    pub async fn get(&self, id: &str) -> Result<Option<Trigger>> {
        let db = self.db.lock().await;
        let trigger = db
            .query_row("SELECT * FROM triggers WHERE id = ?1", params![id], |row| {
                Ok(row_to_trigger(row))
            })
            .optional()?;
        Ok(trigger)
    }

    /// Find a webhook trigger by its public_id.
    ///
    /// Uses the indexed `public_id` column for O(1) lookup instead of
    /// loading all triggers and filtering in Rust.
    pub async fn find_by_public_id(&self, public_id: &str) -> Result<Option<Trigger>> {
        let db = self.db.lock().await;
        let trigger = db
            .query_row(
                "SELECT * FROM triggers WHERE public_id = ?1 AND enabled = 1",
                params![public_id],
                |row| Ok(row_to_trigger(row)),
            )
            .optional()?;
        Ok(trigger)
    }

    /// List all triggers for a specific agent.
    pub async fn list_for_agent(&self, agent_id: &str) -> Result<Vec<Trigger>> {
        let db = self.db.lock().await;
        let mut stmt =
            db.prepare("SELECT * FROM triggers WHERE agent_id = ?1 ORDER BY created_at ASC")?;
        let triggers = stmt
            .query_map(params![agent_id], |row| Ok(row_to_trigger(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(triggers)
    }

    /// List all enabled triggers.
    pub async fn list_all_enabled(&self) -> Result<Vec<Trigger>> {
        let db = self.db.lock().await;
        let mut stmt =
            db.prepare("SELECT * FROM triggers WHERE enabled = 1 ORDER BY created_at ASC")?;
        let triggers = stmt
            .query_map([], |row| Ok(row_to_trigger(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(triggers)
    }

    /// List all triggers (enabled and disabled).
    pub async fn list_all(&self) -> Result<Vec<Trigger>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare("SELECT * FROM triggers ORDER BY created_at ASC")?;
        let triggers = stmt
            .query_map([], |row| Ok(row_to_trigger(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(triggers)
    }

    /// Get all enabled schedule/once triggers that are currently due.
    pub async fn due_schedule_triggers(&self) -> Result<Vec<Trigger>> {
        let all = self.list_all_enabled().await?;
        Ok(all
            .into_iter()
            .filter(|t| {
                matches!(
                    t.trigger_type,
                    TriggerType::Schedule { .. } | TriggerType::Once { .. }
                )
            })
            .filter(|t| t.is_due())
            .collect())
    }

    /// Get all enabled event triggers.
    pub async fn list_event_triggers(&self) -> Result<Vec<Trigger>> {
        let all = self.list_all_enabled().await?;
        Ok(all
            .into_iter()
            .filter(|t| matches!(t.trigger_type, TriggerType::Event { .. }))
            .collect())
    }

    /// Enable or disable a trigger.
    pub async fn update_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let db = self.db.lock().await;
        let updated = db.execute(
            "UPDATE triggers SET enabled = ?1 WHERE id = ?2",
            params![enabled as i32, id],
        )?;
        if updated == 0 {
            anyhow::bail!("trigger '{id}' not found");
        }
        info!(id = %id, enabled, "trigger enabled state changed");
        Ok(())
    }

    /// Delete a trigger.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let db = self.db.lock().await;
        let deleted = db.execute("DELETE FROM triggers WHERE id = ?1", params![id])?;
        if deleted == 0 {
            anyhow::bail!("trigger '{id}' not found");
        }
        info!(id = %id, "trigger deleted");
        Ok(())
    }

    /// Delete all triggers for an agent.
    pub async fn delete_for_agent(&self, agent_id: &str) -> Result<u32> {
        let db = self.db.lock().await;
        let deleted = db.execute(
            "DELETE FROM triggers WHERE agent_id = ?1",
            params![agent_id],
        )?;
        debug!(agent_id = %agent_id, count = deleted, "triggers deleted for agent");
        Ok(deleted as u32)
    }

    /// Advance the trigger's last_fired BEFORE execution (at-most-once semantics).
    /// If the agent crashes mid-execution, the trigger won't re-fire on restart.
    /// Inspired by Hermes Agent's advance-before-execute cron pattern.
    pub async fn advance_before_execute(&self, id: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE triggers SET last_fired = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), id],
        )?;
        debug!(id = %id, "trigger advanced before execution (at-most-once)");
        Ok(())
    }

    /// Record a trigger fire: increment count, update last_fired, add cost.
    pub async fn record_fire(&self, id: &str, cost_usd: f64) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE triggers SET
                fire_count = fire_count + 1,
                last_fired = ?1,
                total_cost_usd = total_cost_usd + ?2
             WHERE id = ?3",
            params![Utc::now().to_rfc3339(), cost_usd, id],
        )?;
        debug!(id = %id, cost_usd, "trigger fire recorded");
        Ok(())
    }

    /// Count of enabled triggers.
    pub async fn count_enabled(&self) -> Result<u32> {
        let db = self.db.lock().await;
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM triggers WHERE enabled = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }
}

// ---------------------------------------------------------------------------
// Row deserialization
// ---------------------------------------------------------------------------

fn row_to_trigger(row: &rusqlite::Row) -> Trigger {
    let type_str: String = row.get("trigger_type").unwrap_or_default();
    let config_json: String = row.get("config").unwrap_or_else(|_| "{}".to_string());
    let trigger_type =
        TriggerType::from_db(&type_str, &config_json).unwrap_or(TriggerType::Schedule {
            expr: "* * * * *".to_string(),
        });

    Trigger {
        id: row.get("id").unwrap_or_default(),
        agent_id: row.get("agent_id").unwrap_or_default(),
        name: row.get("name").unwrap_or_default(),
        trigger_type,
        skill: row.get("skill").unwrap_or_default(),
        enabled: row.get::<_, i32>("enabled").unwrap_or(1) != 0,
        max_budget_usd: row.get("max_budget_usd").ok(),
        created_at: row
            .get::<_, String>("created_at")
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_default(),
        last_fired: row
            .get::<_, String>("last_fired")
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)),
        fire_count: row.get::<_, i32>("fire_count").unwrap_or(0) as u32,
        total_cost_usd: row.get("total_cost_usd").unwrap_or(0.0),
        prompts: row
            .get::<_, String>("prompts")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE agents (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 display_name TEXT,
                 template TEXT NOT NULL DEFAULT '',
                 system_prompt TEXT NOT NULL DEFAULT '',
                 project TEXT,
                 department TEXT,
                 model TEXT,
                 capabilities TEXT NOT NULL DEFAULT '[]',
                 status TEXT NOT NULL DEFAULT 'active',
                 created_at TEXT NOT NULL,
                 last_active TEXT,
                 session_count INTEGER NOT NULL DEFAULT 0,
                 total_tokens INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE triggers (
                 id TEXT PRIMARY KEY,
                 agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                 name TEXT NOT NULL,
                 trigger_type TEXT NOT NULL,
                 config TEXT NOT NULL,
                 skill TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 max_budget_usd REAL,
                 created_at TEXT NOT NULL,
                 last_fired TEXT,
                 fire_count INTEGER NOT NULL DEFAULT 0,
                 total_cost_usd REAL NOT NULL DEFAULT 0.0,
                 public_id TEXT,
                 UNIQUE(agent_id, name)
             );
             CREATE INDEX IF NOT EXISTS idx_triggers_public_id ON triggers(public_id);",
        )
        .unwrap();

        // Insert a test agent.
        conn.execute(
            "INSERT INTO agents (id, name, template, system_prompt, created_at)
             VALUES ('agent-1', 'shadow', 'shadow', 'You are Shadow.', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        Arc::new(Mutex::new(conn))
    }

    #[tokio::test]
    async fn create_and_get() {
        let db = test_db();
        let store = TriggerStore::new(db);

        let trigger = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "morning-brief".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 9 * * *".into(),
                },
                skill: "morning-brief".into(),
                max_budget_usd: Some(0.50),
            })
            .await
            .unwrap();

        assert_eq!(trigger.name, "morning-brief");
        assert_eq!(trigger.agent_id, "agent-1");
        assert_eq!(trigger.skill, "morning-brief");
        assert!(trigger.enabled);
        assert_eq!(trigger.fire_count, 0);

        let fetched = store.get(&trigger.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, trigger.id);
        assert_eq!(fetched.name, "morning-brief");
        assert!(
            matches!(fetched.trigger_type, TriggerType::Schedule { expr } if expr == "0 9 * * *")
        );
    }

    #[tokio::test]
    async fn unique_name_per_agent() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "daily".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 9 * * *".into(),
                },
                skill: "check".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        // Same name, same agent → error
        let result = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "daily".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 10 * * *".into(),
                },
                skill: "other".into(),
                max_budget_usd: None,
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_for_agent() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "t1".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 9 * * *".into(),
                },
                skill: "s1".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();
        store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "t2".into(),
                trigger_type: TriggerType::Once { at: Utc::now() },
                skill: "s2".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        let triggers = store.list_for_agent("agent-1").await.unwrap();
        assert_eq!(triggers.len(), 2);

        let triggers = store.list_for_agent("nonexistent").await.unwrap();
        assert!(triggers.is_empty());
    }

    #[tokio::test]
    async fn enable_disable() {
        let db = test_db();
        let store = TriggerStore::new(db);

        let trigger = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "test".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 9 * * *".into(),
                },
                skill: "s".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        store.update_enabled(&trigger.id, false).await.unwrap();
        let fetched = store.get(&trigger.id).await.unwrap().unwrap();
        assert!(!fetched.enabled);

        store.update_enabled(&trigger.id, true).await.unwrap();
        let fetched = store.get(&trigger.id).await.unwrap().unwrap();
        assert!(fetched.enabled);
    }

    #[tokio::test]
    async fn delete_trigger() {
        let db = test_db();
        let store = TriggerStore::new(db);

        let trigger = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "ephemeral".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "* * * * *".into(),
                },
                skill: "s".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        store.delete(&trigger.id).await.unwrap();
        assert!(store.get(&trigger.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn cascade_delete_on_agent_removal() {
        let db = test_db();
        let store = TriggerStore::new(db.clone());

        store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "t1".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 9 * * *".into(),
                },
                skill: "s".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();
        store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "t2".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 10 * * *".into(),
                },
                skill: "s".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        assert_eq!(store.list_for_agent("agent-1").await.unwrap().len(), 2);

        // Delete the agent → triggers should cascade.
        {
            let conn = db.lock().await;
            conn.execute("DELETE FROM agents WHERE id = 'agent-1'", [])
                .unwrap();
        }

        assert!(store.list_for_agent("agent-1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn record_fire() {
        let db = test_db();
        let store = TriggerStore::new(db);

        let trigger = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "counter".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "* * * * *".into(),
                },
                skill: "s".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        store.record_fire(&trigger.id, 0.05).await.unwrap();
        store.record_fire(&trigger.id, 0.10).await.unwrap();

        let fetched = store.get(&trigger.id).await.unwrap().unwrap();
        assert_eq!(fetched.fire_count, 2);
        assert!((fetched.total_cost_usd - 0.15).abs() < 0.001);
        assert!(fetched.last_fired.is_some());
    }

    #[tokio::test]
    async fn event_trigger_type_roundtrip() {
        let db = test_db();
        let store = TriggerStore::new(db);

        let trigger = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "failure-watch".into(),
                trigger_type: TriggerType::Event {
                    pattern: EventPattern::QuestFailed {
                        project: Some("aeqi".into()),
                    },
                    cooldown_secs: 300,
                },
                skill: "failure-triage".into(),
                max_budget_usd: Some(1.0),
            })
            .await
            .unwrap();

        let fetched = store.get(&trigger.id).await.unwrap().unwrap();
        match &fetched.trigger_type {
            TriggerType::Event {
                pattern,
                cooldown_secs,
            } => {
                assert_eq!(*cooldown_secs, 300);
                match pattern {
                    EventPattern::QuestFailed { project } => {
                        assert_eq!(project.as_deref(), Some("aeqi"));
                    }
                    _ => panic!("expected QuestFailed pattern"),
                }
            }
            _ => panic!("expected Event trigger type"),
        }
    }

    #[tokio::test]
    async fn once_trigger_type_roundtrip() {
        let db = test_db();
        let store = TriggerStore::new(db);

        let target = Utc::now() + chrono::Duration::hours(1);
        let trigger = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "one-shot".into(),
                trigger_type: TriggerType::Once { at: target },
                skill: "deploy".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        let fetched = store.get(&trigger.id).await.unwrap().unwrap();
        match &fetched.trigger_type {
            TriggerType::Once { at } => {
                assert!((at.timestamp() - target.timestamp()).abs() < 2);
            }
            _ => panic!("expected Once trigger type"),
        }
    }

    #[tokio::test]
    async fn count_enabled() {
        let db = test_db();
        let store = TriggerStore::new(db);

        store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "a".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 9 * * *".into(),
                },
                skill: "s".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();
        let t2 = store
            .create(&NewTrigger {
                agent_id: "agent-1".into(),
                name: "b".into(),
                trigger_type: TriggerType::Schedule {
                    expr: "0 10 * * *".into(),
                },
                skill: "s".into(),
                max_budget_usd: None,
            })
            .await
            .unwrap();

        assert_eq!(store.count_enabled().await.unwrap(), 2);

        store.update_enabled(&t2.id, false).await.unwrap();
        assert_eq!(store.count_enabled().await.unwrap(), 1);
    }

    // -----------------------------------------------------------------------
    // Schedule parsing tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_interval_minutes() {
        let d = parse_interval("every 30m").unwrap();
        assert_eq!(d.num_minutes(), 30);
    }

    #[test]
    fn parse_interval_hours() {
        let d = parse_interval("every 1h").unwrap();
        assert_eq!(d.num_hours(), 1);
    }

    #[test]
    fn parse_interval_days() {
        let d = parse_interval("every 2d").unwrap();
        assert_eq!(d.num_days(), 2);
    }

    #[test]
    fn parse_interval_seconds() {
        let d = parse_interval("every 90s").unwrap();
        assert_eq!(d.num_seconds(), 90);
    }

    #[test]
    fn parse_interval_invalid() {
        assert!(parse_interval("every").is_none());
        assert!(parse_interval("every 0m").is_none());
        assert!(parse_interval("every -1h").is_none());
        assert!(parse_interval("30m").is_none()); // Missing "every" prefix
        assert!(parse_interval("every abc").is_none());
    }

    #[test]
    fn cron_matcher_all_stars() {
        let m = parse_simple_cron("* * * * *").unwrap();
        assert!(m.matches(&Utc::now()));
    }

    #[test]
    fn cron_matcher_specific_values() {
        let m = parse_simple_cron("30 9 * * *").unwrap();
        // 9:30 should match
        let dt = chrono::NaiveDate::from_ymd_opt(2026, 3, 31)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap()
            .and_utc();
        assert!(m.matches(&dt));

        // 9:31 should not
        let dt2 = chrono::NaiveDate::from_ymd_opt(2026, 3, 31)
            .unwrap()
            .and_hms_opt(9, 31, 0)
            .unwrap()
            .and_utc();
        assert!(!m.matches(&dt2));
    }

    #[test]
    fn cron_matcher_step() {
        let m = parse_simple_cron("*/15 * * * *").unwrap();
        let dt0 = chrono::NaiveDate::from_ymd_opt(2026, 3, 31)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_utc();
        let dt15 = chrono::NaiveDate::from_ymd_opt(2026, 3, 31)
            .unwrap()
            .and_hms_opt(10, 15, 0)
            .unwrap()
            .and_utc();
        let dt7 = chrono::NaiveDate::from_ymd_opt(2026, 3, 31)
            .unwrap()
            .and_hms_opt(10, 7, 0)
            .unwrap()
            .and_utc();
        assert!(m.matches(&dt0));
        assert!(m.matches(&dt15));
        assert!(!m.matches(&dt7));
    }

    #[test]
    fn cron_invalid_field_count() {
        assert!(parse_simple_cron("* * *").is_none());
        assert!(parse_simple_cron("* * * * * *").is_none());
    }

    #[test]
    fn schedule_due_never_fired() {
        // "every 1h" with no last_fired → immediately due
        assert!(is_schedule_due("every 1h", None));
    }

    #[test]
    fn schedule_due_interval_elapsed() {
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1) - chrono::Duration::seconds(1);
        assert!(is_schedule_due("every 1h", Some(&one_hour_ago)));
    }

    #[test]
    fn schedule_not_due_interval_recent() {
        let five_min_ago = Utc::now() - chrono::Duration::minutes(5);
        assert!(!is_schedule_due("every 1h", Some(&five_min_ago)));
    }

    // -----------------------------------------------------------------------
    // TriggerType serialization roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn trigger_type_schedule_roundtrip() {
        let tt = TriggerType::Schedule {
            expr: "0 9 * * *".into(),
        };
        let json = tt.config_json();
        let restored = TriggerType::from_db("schedule", &json).unwrap();
        match restored {
            TriggerType::Schedule { expr } => assert_eq!(expr, "0 9 * * *"),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn trigger_type_event_roundtrip() {
        let tt = TriggerType::Event {
            pattern: EventPattern::QuestCompleted {
                project: Some("aeqi".into()),
            },
            cooldown_secs: 300,
        };
        let json = tt.config_json();
        let restored = TriggerType::from_db("event", &json).unwrap();
        match restored {
            TriggerType::Event {
                pattern,
                cooldown_secs,
            } => {
                assert_eq!(cooldown_secs, 300);
                match pattern {
                    EventPattern::QuestCompleted { project } => {
                        assert_eq!(project.as_deref(), Some("aeqi"))
                    }
                    _ => panic!("wrong pattern"),
                }
            }
            _ => panic!("wrong type"),
        }
    }
}
