use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// A scheduled cron job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub name: String,
    pub schedule: CronSchedule,
    pub project: String,
    pub prompt: String,
    /// If true, spawn an isolated worker. If false, enqueue for next heartbeat.
    #[serde(default)]
    pub isolated: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
}

/// Cron schedule: either a cron expression or a one-shot timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CronSchedule {
    #[serde(rename = "cron")]
    Cron { expr: String },
    #[serde(rename = "once")]
    Once { at: DateTime<Utc> },
}

impl CronSchedule {
    /// Check if this schedule should fire now (given last run time).
    pub fn is_due(&self, last_run: Option<&DateTime<Utc>>) -> bool {
        let now = Utc::now();
        match self {
            CronSchedule::Once { at } => last_run.is_none() && now >= *at,
            CronSchedule::Cron { expr } => {
                match parse_simple_cron(expr) {
                    Some(matcher) => {
                        // Check if current minute matches and we haven't run this minute.
                        let matches = matcher.matches(&now);
                        if !matches {
                            return false;
                        }
                        // Don't fire if we already ran this minute.
                        if let Some(last) = last_run
                            && last.minute() == now.minute()
                            && last.hour() == now.hour()
                            && last.day() == now.day()
                        {
                            return false;
                        }
                        true
                    }
                    None => false,
                }
            }
        }
    }
}

/// Simple cron matcher (minute hour day month weekday).
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
            CronField::Step(s) => value.is_multiple_of(*s),
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

/// Persistent cron job store.
pub struct ScheduleStore {
    path: PathBuf,
    pub jobs: Vec<ScheduledJob>,
}

impl ScheduleStore {
    /// Open or create the cron store.
    pub fn open(path: &Path) -> Result<Self> {
        let mut store = Self {
            path: path.to_path_buf(),
            jobs: Vec::new(),
        };

        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read cron store: {}", path.display()))?;
            store.jobs = serde_json::from_str(&content).unwrap_or_default();
        }

        Ok(store)
    }

    /// Save the store to disk.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.jobs)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    /// Add a cron job.
    pub fn add(&mut self, job: ScheduledJob) -> Result<()> {
        if self.jobs.iter().any(|j| j.name == job.name) {
            anyhow::bail!("cron job '{}' already exists", job.name);
        }
        info!(name = %job.name, project = %job.project, "cron job added");
        self.jobs.push(job);
        self.save()
    }

    /// Remove a cron job by name.
    pub fn remove(&mut self, name: &str) -> Result<()> {
        let before = self.jobs.len();
        self.jobs.retain(|j| j.name != name);
        if self.jobs.len() == before {
            anyhow::bail!("cron job '{name}' not found");
        }
        self.save()
    }

    /// Get all jobs that are currently due.
    pub fn due_jobs(&self) -> Vec<&ScheduledJob> {
        self.jobs
            .iter()
            .filter(|j| j.schedule.is_due(j.last_run.as_ref()))
            .collect()
    }

    /// Mark a job as having just run.
    pub fn mark_run(&mut self, name: &str) -> Result<()> {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.name == name) {
            job.last_run = Some(Utc::now());
            self.save()?;
        }
        Ok(())
    }

    /// Remove completed one-shot jobs.
    pub fn cleanup_oneshots(&mut self) -> Result<()> {
        let before = self.jobs.len();
        self.jobs.retain(|j| {
            if let CronSchedule::Once { .. } = &j.schedule {
                j.last_run.is_none()
            } else {
                true
            }
        });
        if self.jobs.len() != before {
            self.save()?;
        }
        Ok(())
    }
}
