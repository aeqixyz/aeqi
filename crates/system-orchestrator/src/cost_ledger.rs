//! Cost Ledger — Tracks spending per project/quest, enforces daily budgets.
//!
//! Records every worker execution cost, provides budget status queries,
//! and persists to JSONL for crash recovery.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

/// A single cost entry from a worker execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    #[serde(alias = "domain")]
    pub project: String,
    pub task_id: String,
    #[serde(alias = "spirit")]
    pub worker: String,
    pub cost_usd: f64,
    pub turns: u32,
    pub timestamp: DateTime<Utc>,
}

/// Cached sum with staleness tracking.
struct DailyCache {
    global_sum: f64,
    project_sums: HashMap<String, f64>,
    computed_at: DateTime<Utc>,
    entry_count: usize,
}

impl DailyCache {
    fn new() -> Self {
        Self {
            global_sum: 0.0,
            project_sums: HashMap::new(),
            computed_at: Utc::now(),
            entry_count: 0,
        }
    }

    fn is_stale(&self, actual_count: usize) -> bool {
        actual_count != self.entry_count
            || (Utc::now() - self.computed_at) > Duration::seconds(60)
    }
}

/// Tracks spending across projects and enforces budget caps.
pub struct CostLedger {
    entries: Mutex<Vec<CostEntry>>,
    /// Stored as AtomicU64 (f64 bits) so it can be updated via &self on config reload.
    daily_budget_usd: AtomicU64,
    persist_path: Option<PathBuf>,
    project_budgets: Mutex<HashMap<String, f64>>,
    cache: Mutex<DailyCache>,
}

impl CostLedger {
    pub fn new(daily_budget_usd: f64) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            daily_budget_usd: AtomicU64::new(daily_budget_usd.to_bits()),
            persist_path: None,
            project_budgets: Mutex::new(HashMap::new()),
            cache: Mutex::new(DailyCache::new()),
        }
    }

    pub fn with_persistence(daily_budget_usd: f64, path: PathBuf) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            daily_budget_usd: AtomicU64::new(daily_budget_usd.to_bits()),
            persist_path: Some(path),
            project_budgets: Mutex::new(HashMap::new()),
            cache: Mutex::new(DailyCache::new()),
        }
    }

    fn daily_budget(&self) -> f64 {
        f64::from_bits(self.daily_budget_usd.load(Ordering::Relaxed))
    }

    /// Rebuild the daily cache from entries.
    fn rebuild_cache(entries: &[CostEntry]) -> DailyCache {
        let since = Utc::now() - Duration::hours(24);
        let mut global_sum = 0.0;
        let mut project_sums: HashMap<String, f64> = HashMap::new();
        for e in entries {
            if e.timestamp > since {
                global_sum += e.cost_usd;
                *project_sums.entry(e.project.clone()).or_default() += e.cost_usd;
            }
        }
        DailyCache {
            global_sum,
            project_sums,
            computed_at: Utc::now(),
            entry_count: entries.len(),
        }
    }

    /// Get cached daily sums, rebuilding if stale.
    fn cached_sums(&self, entries: &[CostEntry]) -> (f64, HashMap<String, f64>) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.is_stale(entries.len()) {
            *cache = Self::rebuild_cache(entries);
        }
        (cache.global_sum, cache.project_sums.clone())
    }

    /// Invalidate the cache (call after prune or load).
    fn invalidate_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.entry_count = 0; // Force stale on next read.
        }
    }

    /// Record a cost entry. Warns if daily budget or project budget exceeded.
    pub fn record(&self, entry: CostEntry) -> Result<()> {
        let project_name = entry.project.clone();
        let cost = entry.cost_usd;
        let mut entries = self.entries.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        info!(
            project = %entry.project,
            task = %entry.task_id,
            worker = %entry.worker,
            cost = entry.cost_usd,
            turns = entry.turns,
            "cost recorded"
        );

        entries.push(entry);

        // Incrementally update cache.
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.global_sum += cost;
            *cache.project_sums.entry(project_name.clone()).or_default() += cost;
            cache.entry_count = entries.len();
        }

        // Check global budget.
        let (global_spent, project_sums) = self.cached_sums(&entries);
        if global_spent > self.daily_budget() {
            warn!(
                spent = global_spent,
                budget = self.daily_budget(),
                overage = global_spent - self.daily_budget(),
                "DAILY BUDGET EXCEEDED"
            );
        }

        // Check project-specific budget.
        let budgets = self.project_budgets.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&project_budget) = budgets.get(&project_name) {
            let project_spent = project_sums.get(&project_name).copied().unwrap_or(0.0);
            if project_spent > project_budget {
                warn!(
                    project = %project_name,
                    spent = project_spent,
                    budget = project_budget,
                    overage = project_spent - project_budget,
                    "PROJECT BUDGET EXCEEDED"
                );
            }
        }

        Ok(())
    }

    /// Check budget status: (spent_today, budget, remaining). O(1) when cache is warm.
    pub fn budget_status(&self) -> (f64, f64, f64) {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let (spent, _) = self.cached_sums(&entries);
        let remaining = (self.daily_budget() - spent).max(0.0);
        (spent, self.daily_budget(), remaining)
    }

    /// Total spend for a project in the last 24 hours. O(1) when cache is warm.
    pub fn project_spend(&self, project: &str, hours: u32) -> f64 {
        if hours == 24 {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            let (_, project_sums) = self.cached_sums(&entries);
            return project_sums.get(project).copied().unwrap_or(0.0);
        }
        // Non-24h queries fall back to scan.
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let since = Utc::now() - Duration::hours(hours as i64);
        entries
            .iter()
            .filter(|e| e.project == project && e.timestamp > since)
            .map(|e| e.cost_usd)
            .sum()
    }

    /// Total spend for a quest across all attempts.
    pub fn quest_spend(&self, task_id: &str) -> f64 {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries
            .iter()
            .filter(|e| e.task_id == task_id)
            .map(|e| e.cost_usd)
            .sum()
    }

    /// Check if we can afford a new execution (budget not exhausted).
    pub fn can_afford(&self) -> bool {
        let (spent, budget, _) = self.budget_status();
        spent < budget
    }

    /// Update the global daily budget cap (e.g. on config reload).
    pub fn set_daily_budget(&self, budget_usd: f64) {
        self.daily_budget_usd.store(budget_usd.to_bits(), Ordering::Relaxed);
        info!(budget_usd, "global daily budget updated");
    }

    /// Set the daily budget cap for a specific project.
    pub fn set_project_budget(&self, project: &str, budget_usd: f64) {
        let mut budgets = self.project_budgets.lock().unwrap_or_else(|e| e.into_inner());
        budgets.insert(project.to_string(), budget_usd);
        info!(project = %project, budget_usd, "project budget set");
    }

    /// Check if a project can afford a new execution.
    /// Returns false if EITHER the global daily budget OR the project-specific cap is exceeded.
    /// If no project budget is set, falls back to the global budget check only.
    pub fn can_afford_project(&self, project: &str) -> bool {
        // Global check first.
        if !self.can_afford() {
            return false;
        }

        // Project-specific check.
        let budgets = self.project_budgets.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&project_budget) = budgets.get(project) {
            let spent = self.project_spend(project, 24);
            if spent >= project_budget {
                return false;
            }
        }

        true
    }

    /// Get per-project budget status: (spent_today, budget, remaining).
    /// If no project budget is set, returns (spent_today, global_budget, global_remaining).
    pub fn project_budget_status(&self, project: &str) -> (f64, f64, f64) {
        let spent = self.project_spend(project, 24);
        let budgets = self.project_budgets.lock().unwrap_or_else(|e| e.into_inner());
        let budget = budgets.get(project).copied().unwrap_or(self.daily_budget());
        let remaining = (budget - spent).max(0.0);
        (spent, budget, remaining)
    }

    /// Get all per-project budget statuses as a map. O(1) when cache is warm.
    pub fn all_project_budget_statuses(&self) -> HashMap<String, (f64, f64, f64)> {
        let budgets = self.project_budgets.lock().unwrap_or_else(|e| e.into_inner());
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let (_, project_sums) = self.cached_sums(&entries);

        let mut all_projects: HashSet<String> = budgets.keys().cloned().collect();
        all_projects.extend(project_sums.keys().cloned());

        let mut result = HashMap::new();
        for project in all_projects {
            let spent = project_sums.get(&project).copied().unwrap_or(0.0);
            let budget = budgets.get(&project).copied().unwrap_or(self.daily_budget());
            let remaining = (budget - spent).max(0.0);
            result.insert(project, (spent, budget, remaining));
        }

        result
    }

    /// Save entries to JSONL file.
    pub fn save(&self) -> Result<()> {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let entries = self.entries.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut content = String::new();
        for entry in entries.iter() {
            content.push_str(&serde_json::to_string(entry)?);
            content.push('\n');
        }

        std::fs::write(path, &content)
            .with_context(|| format!("failed to write cost ledger: {}", path.display()))?;

        Ok(())
    }

    /// Load entries from JSONL file.
    pub fn load(&self) -> Result<usize> {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return Ok(0),
        };

        if !path.exists() {
            return Ok(0);
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read cost ledger: {}", path.display()))?;

        let mut entries = self.entries.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut count = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<CostEntry>(line) {
                Ok(entry) => {
                    entries.push(entry);
                    count += 1;
                }
                Err(e) => {
                    warn!(error = %e, "skipping malformed cost entry");
                }
            }
        }

        self.invalidate_cache();
        Ok(count)
    }

    /// Per-project totals for the last 24 hours. O(1) when cache is warm.
    pub fn daily_report(&self) -> HashMap<String, f64> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let (_, project_sums) = self.cached_sums(&entries);
        project_sums
    }

    /// Prune entries older than 7 days to prevent unbounded growth.
    pub fn prune_old(&self) {
        let cutoff = Utc::now() - Duration::days(7);
        if let Ok(mut entries) = self.entries.lock() {
            let before = entries.len();
            entries.retain(|e| e.timestamp > cutoff);
            let pruned = before - entries.len();
            if pruned > 0 {
                info!(pruned, "pruned old cost entries");
                self.invalidate_cache();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_query() {
        let ledger = CostLedger::new(100.0);

        ledger
            .record(CostEntry {
                project: "algostaking".into(),
                task_id: "as-001".into(),
                worker: "as-worker-1".into(),
                cost_usd: 0.50,
                turns: 5,
                timestamp: Utc::now(),
            })
            .unwrap();

        ledger
            .record(CostEntry {
                project: "riftdecks-shop".into(),
                task_id: "rd-001".into(),
                worker: "rd-worker-1".into(),
                cost_usd: 0.30,
                turns: 3,
                timestamp: Utc::now(),
            })
            .unwrap();

        let (spent, budget, remaining) = ledger.budget_status();
        assert!((spent - 0.80).abs() < 0.01);
        assert!((budget - 100.0).abs() < 0.01);
        assert!(remaining > 99.0);

        assert!((ledger.project_spend("algostaking", 24) - 0.50).abs() < 0.01);
        assert!((ledger.quest_spend("as-001") - 0.50).abs() < 0.01);
        assert!(ledger.can_afford());
    }

    #[test]
    fn test_daily_report() {
        let ledger = CostLedger::new(100.0);

        for i in 0..5 {
            ledger
                .record(CostEntry {
                    project: "algostaking".into(),
                    task_id: format!("as-{i:03}"),
                    worker: format!("as-worker-{i}"),
                    cost_usd: 1.0,
                    turns: 5,
                    timestamp: Utc::now(),
                })
                .unwrap();
        }

        let report = ledger.daily_report();
        assert!((report["algostaking"] - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_persistence_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("costs.jsonl");

        let ledger = CostLedger::with_persistence(100.0, path.clone());
        ledger
            .record(CostEntry {
                project: "test".into(),
                task_id: "t-001".into(),
                worker: "w1".into(),
                cost_usd: 1.23,
                turns: 4,
                timestamp: Utc::now(),
            })
            .unwrap();
        ledger.save().unwrap();

        let ledger2 = CostLedger::with_persistence(100.0, path);
        let count = ledger2.load().unwrap();
        assert_eq!(count, 1);
        assert!((ledger2.quest_spend("t-001") - 1.23).abs() < 0.01);
    }

    #[test]
    fn test_prune_old() {
        let ledger = CostLedger::new(100.0);

        // Add an old entry.
        {
            let mut entries = ledger.entries.lock().unwrap();
            entries.push(CostEntry {
                project: "test".into(),
                task_id: "old".into(),
                worker: "w".into(),
                cost_usd: 1.0,
                turns: 1,
                timestamp: Utc::now() - Duration::days(10),
            });
        }

        // Add a recent entry.
        ledger
            .record(CostEntry {
                project: "test".into(),
                task_id: "new".into(),
                worker: "w".into(),
                cost_usd: 2.0,
                turns: 2,
                timestamp: Utc::now(),
            })
            .unwrap();

        ledger.prune_old();
        assert!((ledger.quest_spend("old")).abs() < 0.01); // pruned
        assert!((ledger.quest_spend("new") - 2.0).abs() < 0.01); // kept
    }

    #[test]
    fn test_project_budget_blocks_overspend() {
        let ledger = CostLedger::new(100.0); // Global: $100/day
        ledger.set_project_budget("algostaking", 2.0); // Project: $2/day

        // Spend $1.50 in algostaking — should still be under project cap.
        ledger.record(CostEntry {
            project: "algostaking".into(),
            task_id: "as-001".into(),
            worker: "w1".into(),
            cost_usd: 1.50,
            turns: 5,
            timestamp: Utc::now(),
        }).unwrap();

        assert!(ledger.can_afford_project("algostaking"));

        // Spend another $1.00 — now over the $2 project cap.
        ledger.record(CostEntry {
            project: "algostaking".into(),
            task_id: "as-002".into(),
            worker: "w2".into(),
            cost_usd: 1.00,
            turns: 3,
            timestamp: Utc::now(),
        }).unwrap();

        assert!(!ledger.can_afford_project("algostaking"));
        // Global budget is still fine.
        assert!(ledger.can_afford());
    }

    #[test]
    fn test_project_budget_does_not_affect_other_projects() {
        let ledger = CostLedger::new(100.0);
        ledger.set_project_budget("algostaking", 1.0);

        // Exhaust algostaking's budget.
        ledger.record(CostEntry {
            project: "algostaking".into(),
            task_id: "as-001".into(),
            worker: "w1".into(),
            cost_usd: 2.0,
            turns: 5,
            timestamp: Utc::now(),
        }).unwrap();

        // algostaking is blocked.
        assert!(!ledger.can_afford_project("algostaking"));
        // riftdecks (no project budget) is still fine.
        assert!(ledger.can_afford_project("riftdecks-shop"));
    }

    #[test]
    fn test_project_without_budget_uses_global() {
        let ledger = CostLedger::new(5.0); // Global: $5/day
        // No project budget set for "riftdecks-shop".

        ledger.record(CostEntry {
            project: "riftdecks-shop".into(),
            task_id: "rd-001".into(),
            worker: "w1".into(),
            cost_usd: 3.0,
            turns: 5,
            timestamp: Utc::now(),
        }).unwrap();

        // Under global budget — still affordable.
        assert!(ledger.can_afford_project("riftdecks-shop"));

        // Exceed global budget.
        ledger.record(CostEntry {
            project: "riftdecks-shop".into(),
            task_id: "rd-002".into(),
            worker: "w2".into(),
            cost_usd: 3.0,
            turns: 5,
            timestamp: Utc::now(),
        }).unwrap();

        // Global budget exceeded — no project can afford.
        assert!(!ledger.can_afford_project("riftdecks-shop"));
        assert!(!ledger.can_afford_project("algostaking"));
    }

    #[test]
    fn test_project_budget_status() {
        let ledger = CostLedger::new(100.0);
        ledger.set_project_budget("algostaking", 10.0);

        ledger.record(CostEntry {
            project: "algostaking".into(),
            task_id: "as-001".into(),
            worker: "w1".into(),
            cost_usd: 3.50,
            turns: 5,
            timestamp: Utc::now(),
        }).unwrap();

        let (spent, budget, remaining) = ledger.project_budget_status("algostaking");
        assert!((spent - 3.50).abs() < 0.01);
        assert!((budget - 10.0).abs() < 0.01);
        assert!((remaining - 6.50).abs() < 0.01);

        // Project without a budget returns global budget.
        let (spent, budget, _remaining) = ledger.project_budget_status("riftdecks-shop");
        assert!((spent).abs() < 0.01);
        assert!((budget - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_all_project_budget_statuses() {
        let ledger = CostLedger::new(100.0);
        ledger.set_project_budget("algostaking", 10.0);
        ledger.set_project_budget("riftdecks-shop", 5.0);

        ledger.record(CostEntry {
            project: "algostaking".into(),
            task_id: "as-001".into(),
            worker: "w1".into(),
            cost_usd: 2.0,
            turns: 5,
            timestamp: Utc::now(),
        }).unwrap();

        ledger.record(CostEntry {
            project: "sigil".into(),
            task_id: "sg-001".into(),
            worker: "w1".into(),
            cost_usd: 1.0,
            turns: 3,
            timestamp: Utc::now(),
        }).unwrap();

        let statuses = ledger.all_project_budget_statuses();

        // algostaking: has budget + spending.
        let (spent, budget, remaining) = statuses["algostaking"];
        assert!((spent - 2.0).abs() < 0.01);
        assert!((budget - 10.0).abs() < 0.01);
        assert!((remaining - 8.0).abs() < 0.01);

        // riftdecks: has budget, no spending.
        let (spent, budget, remaining) = statuses["riftdecks-shop"];
        assert!((spent).abs() < 0.01);
        assert!((budget - 5.0).abs() < 0.01);
        assert!((remaining - 5.0).abs() < 0.01);

        // sigil: no project budget set, but has spending — uses global budget.
        let (spent, budget, _remaining) = statuses["sigil"];
        assert!((spent - 1.0).abs() < 0.01);
        assert!((budget - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_global_budget_blocks_even_with_project_headroom() {
        let ledger = CostLedger::new(5.0); // Tight global budget
        ledger.set_project_budget("algostaking", 50.0); // Generous project budget

        // Spend enough to exhaust global but not project.
        ledger.record(CostEntry {
            project: "algostaking".into(),
            task_id: "as-001".into(),
            worker: "w1".into(),
            cost_usd: 6.0,
            turns: 10,
            timestamp: Utc::now(),
        }).unwrap();

        // Project has headroom ($6 of $50) but global is exceeded ($6 of $5).
        assert!(!ledger.can_afford_project("algostaking"));
        assert!(!ledger.can_afford());
    }
}
