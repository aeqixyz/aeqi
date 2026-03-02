//! Lightweight Prometheus-compatible metrics for Realm.
//!
//! Zero external dependencies — atomic counters, gauges, and histograms
//! serialized to Prometheus text exposition format.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub struct Counter {
    value: AtomicU64,
    name: String,
    help: String,
    labels: Vec<(String, String)>,
}

impl Counter {
    pub fn new(name: &str, help: &str) -> Self {
        Self {
            value: AtomicU64::new(0),
            name: name.to_string(),
            help: help.to_string(),
            labels: Vec::new(),
        }
    }

    pub fn with_labels(name: &str, help: &str, labels: Vec<(String, String)>) -> Self {
        Self {
            value: AtomicU64::new(0),
            name: name.to_string(),
            help: help.to_string(),
            labels,
        }
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_by(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Gauge: value can go up and down. Stored as fixed-point (×1000).
pub struct Gauge {
    value: AtomicU64,
    name: String,
    help: String,
    labels: Vec<(String, String)>,
}

impl Gauge {
    pub fn new(name: &str, help: &str) -> Self {
        Self {
            value: AtomicU64::new(0),
            name: name.to_string(),
            help: help.to_string(),
            labels: Vec::new(),
        }
    }

    pub fn with_labels(name: &str, help: &str, labels: Vec<(String, String)>) -> Self {
        Self {
            value: AtomicU64::new(0),
            name: name.to_string(),
            help: help.to_string(),
            labels,
        }
    }

    pub fn set(&self, v: f64) {
        self.value.store((v * 1000.0) as u64, Ordering::Relaxed);
    }

    pub fn get(&self) -> f64 {
        self.value.load(Ordering::Relaxed) as f64 / 1000.0
    }
}

/// Histogram with pre-defined buckets.
pub struct Histogram {
    buckets: Vec<(f64, AtomicU64)>,
    sum: AtomicU64,
    count: AtomicU64,
    name: String,
    help: String,
    labels: Vec<(String, String)>,
}

impl Histogram {
    pub fn new(name: &str, help: &str, bucket_bounds: &[f64]) -> Self {
        let buckets = bucket_bounds
            .iter()
            .map(|&b| (b, AtomicU64::new(0)))
            .chain(std::iter::once((f64::INFINITY, AtomicU64::new(0))))
            .collect();
        Self {
            buckets,
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
            name: name.to_string(),
            help: help.to_string(),
            labels: Vec::new(),
        }
    }

    pub fn observe(&self, value: f64) {
        for (bound, count) in &self.buckets {
            if value <= *bound {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.sum
            .fetch_add((value * 1000.0) as u64, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

fn format_labels(labels: &[(String, String)]) -> String {
    if labels.is_empty() {
        String::new()
    } else {
        let pairs: Vec<String> = labels.iter().map(|(k, v)| format!("{k}=\"{v}\"")).collect();
        format!("{{{}}}", pairs.join(","))
    }
}

/// Global metrics registry.
pub struct SystemMetrics {
    pub tasks_completed: Counter,
    pub tasks_failed: Counter,
    pub tasks_blocked: Counter,
    pub workers_spawned: Counter,
    pub workers_timed_out: Counter,
    pub whispers_sent: Counter,
    pub escalations_total: Counter,
    pub patrol_cycles: Counter,

    pub workers_active: Gauge,
    pub tasks_pending: Gauge,
    pub whisper_queue_depth: Gauge,
    pub daily_cost_usd: Gauge,

    pub worker_duration_seconds: Histogram,
    pub task_cost_usd: Histogram,
    pub patrol_cycle_seconds: Histogram,

    project_counters: Mutex<HashMap<String, ProjectMetrics>>,
}

pub struct ProjectMetrics {
    pub tasks_completed: Counter,
    pub tasks_failed: Counter,
    pub workers_active: Gauge,
    pub cost_usd_total: Gauge,
}

impl ProjectMetrics {
    fn new(project: &str) -> Self {
        let pl = vec![("project".to_string(), project.to_string())];
        Self {
            tasks_completed: Counter::with_labels(
                "realm_project_tasks_completed_total",
                "Quests completed per project",
                pl.clone(),
            ),
            tasks_failed: Counter::with_labels(
                "realm_project_tasks_failed_total",
                "Quests failed per project",
                pl.clone(),
            ),
            workers_active: Gauge::with_labels(
                "realm_project_workers_active",
                "Active spirits per project",
                pl.clone(),
            ),
            cost_usd_total: Gauge::with_labels(
                "realm_project_cost_usd_total",
                "Total cost per project",
                pl,
            ),
        }
    }
}

impl SystemMetrics {
    pub fn new() -> Self {
        Self {
            tasks_completed: Counter::new(
                "system_tasks_completed_total",
                "Total quests completed",
            ),
            tasks_failed: Counter::new("system_tasks_failed_total", "Total quests failed"),
            tasks_blocked: Counter::new("system_tasks_blocked_total", "Total quests blocked"),
            workers_spawned: Counter::new("realm_workers_spawned_total", "Total workers spawned"),
            workers_timed_out: Counter::new(
                "realm_workers_timed_out_total",
                "Total workers timed out",
            ),
            whispers_sent: Counter::new("realm_whispers_sent_total", "Total whispers sent"),
            escalations_total: Counter::new("realm_escalations_total", "Total escalations"),
            patrol_cycles: Counter::new("realm_patrol_cycles_total", "Total patrol cycles"),

            workers_active: Gauge::new("realm_workers_active", "Currently active workers"),
            tasks_pending: Gauge::new("system_tasks_pending", "Currently pending quests"),
            whisper_queue_depth: Gauge::new("realm_whisper_queue_depth", "Message queue depth"),
            daily_cost_usd: Gauge::new("realm_daily_cost_usd", "Cost today in USD"),

            worker_duration_seconds: Histogram::new(
                "realm_worker_duration_seconds",
                "Worker execution duration",
                &[10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0],
            ),
            task_cost_usd: Histogram::new(
                "realm_task_cost_usd",
                "Cost per quest execution",
                &[0.01, 0.05, 0.10, 0.25, 0.50, 1.0, 2.0, 5.0, 10.0],
            ),
            patrol_cycle_seconds: Histogram::new(
                "realm_patrol_cycle_seconds",
                "Patrol cycle duration",
                &[0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0],
            ),

            project_counters: Mutex::new(HashMap::new()),
        }
    }

    /// Ensure per-project metrics exist and return mutable access.
    pub fn ensure_project(&self, name: &str) {
        let mut map = self.project_counters.lock().unwrap();
        if !map.contains_key(name) {
            map.insert(name.to_string(), ProjectMetrics::new(name));
        }
    }

    /// Render all metrics in Prometheus text exposition format.
    pub fn render(&self) -> String {
        let mut out = String::new();

        render_counter(&mut out, &self.tasks_completed);
        render_counter(&mut out, &self.tasks_failed);
        render_counter(&mut out, &self.tasks_blocked);
        render_counter(&mut out, &self.workers_spawned);
        render_counter(&mut out, &self.workers_timed_out);
        render_counter(&mut out, &self.whispers_sent);
        render_counter(&mut out, &self.escalations_total);
        render_counter(&mut out, &self.patrol_cycles);

        render_gauge(&mut out, &self.workers_active);
        render_gauge(&mut out, &self.tasks_pending);
        render_gauge(&mut out, &self.whisper_queue_depth);
        render_gauge(&mut out, &self.daily_cost_usd);

        render_histogram(&mut out, &self.worker_duration_seconds);
        render_histogram(&mut out, &self.task_cost_usd);
        render_histogram(&mut out, &self.patrol_cycle_seconds);

        // Per-project metrics.
        if let Ok(map) = self.project_counters.lock() {
            for dm in map.values() {
                render_counter(&mut out, &dm.tasks_completed);
                render_counter(&mut out, &dm.tasks_failed);
                render_gauge(&mut out, &dm.workers_active);
                render_gauge(&mut out, &dm.cost_usd_total);
            }
        }

        out
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}

fn render_counter(out: &mut String, c: &Counter) {
    out.push_str(&format!("# HELP {} {}\n", c.name, c.help));
    out.push_str(&format!("# TYPE {} counter\n", c.name));
    out.push_str(&format!(
        "{}{} {}\n",
        c.name,
        format_labels(&c.labels),
        c.get()
    ));
}

fn render_gauge(out: &mut String, g: &Gauge) {
    out.push_str(&format!("# HELP {} {}\n", g.name, g.help));
    out.push_str(&format!("# TYPE {} gauge\n", g.name));
    out.push_str(&format!(
        "{}{} {:.3}\n",
        g.name,
        format_labels(&g.labels),
        g.get()
    ));
}

fn render_histogram(out: &mut String, h: &Histogram) {
    out.push_str(&format!("# HELP {} {}\n", h.name, h.help));
    out.push_str(&format!("# TYPE {} histogram\n", h.name));
    let labels = format_labels(&h.labels);
    for (bound, count) in &h.buckets {
        let le = if bound.is_infinite() {
            "+Inf".to_string()
        } else {
            format!("{bound}")
        };
        let label_str = if labels.is_empty() {
            format!("{{le=\"{le}\"}}")
        } else {
            let inner = &labels[1..labels.len() - 1];
            format!("{{{inner},le=\"{le}\"}}")
        };
        out.push_str(&format!(
            "{}_bucket{} {}\n",
            h.name,
            label_str,
            count.load(Ordering::Relaxed)
        ));
    }
    out.push_str(&format!(
        "{}_sum{} {:.3}\n",
        h.name,
        labels,
        h.sum.load(Ordering::Relaxed) as f64 / 1000.0
    ));
    out.push_str(&format!(
        "{}_count{} {}\n",
        h.name,
        labels,
        h.count.load(Ordering::Relaxed)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let c = Counter::new("test", "help");
        assert_eq!(c.get(), 0);
        c.inc();
        c.inc();
        assert_eq!(c.get(), 2);
        c.inc_by(5);
        assert_eq!(c.get(), 7);
    }

    #[test]
    fn test_gauge() {
        let g = Gauge::new("test", "help");
        g.set(3.14);
        assert!((g.get() - 3.14).abs() < 0.01);
    }

    #[test]
    fn test_histogram() {
        let h = Histogram::new("test", "help", &[1.0, 5.0, 10.0]);
        h.observe(0.5);
        h.observe(3.0);
        h.observe(7.0);
        h.observe(15.0);
        assert_eq!(h.count.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn test_render() {
        let m = SystemMetrics::new();
        m.tasks_completed.inc();
        m.workers_active.set(3.0);
        m.worker_duration_seconds.observe(45.0);
        let output = m.render();
        assert!(output.contains("system_tasks_completed_total 1"));
        assert!(output.contains("realm_workers_active 3.000"));
        assert!(output.contains("realm_worker_duration_seconds_count 1"));
    }
}
