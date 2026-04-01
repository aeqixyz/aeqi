use anyhow::Result;
use chrono::{Local, Utc};
use serde::Serialize;
use sigil_tasks::{Priority, Task, TaskStatus};
use std::io::Write;
use std::path::PathBuf;

use crate::helpers::{
    daemon_ipc_request, format_project_org_hint, load_config_with_agents, open_tasks_for_project,
};

#[derive(Debug, Clone, Default, Serialize)]
struct DispatchHealthSnapshot {
    unread: u64,
    awaiting_ack: u64,
    retrying_delivery: u64,
    overdue_ack: u64,
    dead_letters: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
struct DaemonMonitor {
    online: bool,
    ready: Option<bool>,
    leader_agent: Option<String>,
    registered_owner_count: Option<u64>,
    configured_projects: Option<u64>,
    configured_advisors: Option<u64>,
    max_workers: Option<u64>,
    cost_today_usd: Option<f64>,
    daily_budget_usd: Option<f64>,
    budget_remaining_usd: Option<f64>,
    dispatch_health: DispatchHealthSnapshot,
    warnings: Vec<String>,
    blocking_reasons: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectMonitor {
    name: String,
    repo: String,
    repo_present: bool,
    runtime_provider: String,
    model: String,
    org_hint: String,
    open_tasks: usize,
    ready_tasks: usize,
    blocked_tasks: usize,
    in_progress_tasks: usize,
    critical_ready_tasks: usize,
    budget_blocked_tasks: usize,
    stalled: bool,
    top_ready_tasks: Vec<String>,
    top_blocked_tasks: Vec<String>,
    task_store_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct MonitorReport {
    generated_at: String,
    daemon: DaemonMonitor,
    projects: Vec<ProjectMonitor>,
    interventions: Vec<String>,
}

pub(crate) async fn cmd_monitor(
    config_path: &Option<PathBuf>,
    project_filter: Option<&str>,
    watch: bool,
    interval_secs: u64,
    json: bool,
) -> Result<()> {
    if watch && json {
        anyhow::bail!("`sigil monitor --json` does not support `--watch`");
    }

    loop {
        let report = build_monitor_report(config_path, project_filter).await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }

        render_monitor_report(&report);

        if !watch {
            return Ok(());
        }

        std::io::stdout().flush().ok();
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs.max(1))).await;
        print!("\x1B[2J\x1B[H");
    }
}

async fn build_monitor_report(
    config_path: &Option<PathBuf>,
    project_filter: Option<&str>,
) -> Result<MonitorReport> {
    let (config, _) = load_config_with_agents(config_path)?;

    let projects_cfg: Vec<_> = if let Some(name) = project_filter {
        let projects: Vec<_> = config
            .projects
            .iter()
            .filter(|project| project.name == name)
            .collect();
        if projects.is_empty() {
            anyhow::bail!("project not found: {name}");
        }
        projects
    } else {
        config.projects.iter().collect()
    };

    let daemon = load_daemon_monitor(config_path).await;
    let mut projects = Vec::new();
    for project in projects_cfg {
        let runtime = config.runtime_for_project(&project.name);
        projects.push(build_project_monitor(
            &config,
            &project.name,
            &project.repo,
            &runtime.provider.to_string(),
            &config.model_for_project(&project.name),
        ));
    }

    let interventions = build_interventions(&daemon, &projects);

    Ok(MonitorReport {
        generated_at: Utc::now().to_rfc3339(),
        daemon,
        projects,
        interventions,
    })
}

async fn load_daemon_monitor(config_path: &Option<PathBuf>) -> DaemonMonitor {
    let request = serde_json::json!({ "cmd": "readiness" });
    let response = match daemon_ipc_request(config_path, &request).await {
        Ok(response) => response,
        Err(error) => {
            return DaemonMonitor {
                error: Some(error.to_string()),
                ..DaemonMonitor::default()
            };
        }
    };

    if !response
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return DaemonMonitor {
            error: response
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .or_else(|| Some("daemon returned an unknown readiness error".to_string())),
            ..DaemonMonitor::default()
        };
    }

    DaemonMonitor {
        online: true,
        ready: response.get("ready").and_then(serde_json::Value::as_bool),
        leader_agent: response
            .get("leader_agent")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        registered_owner_count: json_u64(&response, "registered_owner_count"),
        configured_projects: json_u64(&response, "configured_projects"),
        configured_advisors: json_u64(&response, "configured_advisors"),
        max_workers: json_u64(&response, "max_workers"),
        cost_today_usd: response
            .get("cost_today_usd")
            .and_then(serde_json::Value::as_f64),
        daily_budget_usd: response
            .get("daily_budget_usd")
            .and_then(serde_json::Value::as_f64),
        budget_remaining_usd: response
            .get("budget_remaining_usd")
            .and_then(serde_json::Value::as_f64),
        dispatch_health: DispatchHealthSnapshot {
            unread: json_u64_nested(&response, &["dispatch_health", "unread"]).unwrap_or(0),
            awaiting_ack: json_u64_nested(&response, &["dispatch_health", "awaiting_ack"])
                .unwrap_or(0),
            retrying_delivery: json_u64_nested(
                &response,
                &["dispatch_health", "retrying_delivery"],
            )
            .unwrap_or(0),
            overdue_ack: json_u64_nested(&response, &["dispatch_health", "overdue_ack"])
                .unwrap_or(0),
            dead_letters: json_u64_nested(&response, &["dispatch_health", "dead_letters"])
                .unwrap_or(0),
        },
        warnings: string_array(&response, "warnings"),
        blocking_reasons: string_array(&response, "blocking_reasons"),
        error: None,
    }
}

fn build_project_monitor(
    config: &sigil_core::SigilConfig,
    project_name: &str,
    repo: &str,
    runtime_provider: &str,
    model: &str,
) -> ProjectMonitor {
    let repo_present = PathBuf::from(repo).exists();
    let org_hint = format_project_org_hint(config, project_name)
        .trim()
        .to_string();

    let store = match open_tasks_for_project(project_name) {
        Ok(store) => store,
        Err(error) => {
            return ProjectMonitor {
                name: project_name.to_string(),
                repo: repo.to_string(),
                repo_present,
                runtime_provider: runtime_provider.to_string(),
                model: model.to_string(),
                org_hint,
                open_tasks: 0,
                ready_tasks: 0,
                blocked_tasks: 0,
                in_progress_tasks: 0,
                critical_ready_tasks: 0,
                budget_blocked_tasks: 0,
                stalled: false,
                top_ready_tasks: Vec::new(),
                top_blocked_tasks: Vec::new(),
                task_store_error: Some(error.to_string()),
            };
        }
    };

    let all_tasks = store.all();
    let ready_tasks = store.ready();
    let open_tasks: Vec<_> = all_tasks
        .iter()
        .copied()
        .filter(|task| !task.is_closed())
        .collect();
    let blocked_tasks = sort_tasks(
        open_tasks
            .iter()
            .copied()
            .filter(|task| task.status == TaskStatus::Blocked)
            .collect(),
    );
    let in_progress_tasks = open_tasks
        .iter()
        .filter(|task| task.status == TaskStatus::InProgress)
        .count();
    let critical_ready_tasks = ready_tasks
        .iter()
        .filter(|task| task.priority == Priority::Critical)
        .count();
    let budget_blocked_tasks = open_tasks
        .iter()
        .filter(|task| {
            task.status == TaskStatus::Blocked
                && task
                    .labels
                    .iter()
                    .any(|label| label.eq_ignore_ascii_case("budget-blocked"))
        })
        .count();

    ProjectMonitor {
        name: project_name.to_string(),
        repo: repo.to_string(),
        repo_present,
        runtime_provider: runtime_provider.to_string(),
        model: model.to_string(),
        org_hint,
        open_tasks: open_tasks.len(),
        ready_tasks: ready_tasks.len(),
        blocked_tasks: blocked_tasks.len(),
        in_progress_tasks,
        critical_ready_tasks,
        budget_blocked_tasks,
        stalled: !open_tasks.is_empty() && ready_tasks.is_empty() && in_progress_tasks == 0,
        top_ready_tasks: ready_tasks
            .iter()
            .take(3)
            .map(|task| task_brief(task))
            .collect(),
        top_blocked_tasks: blocked_tasks
            .iter()
            .take(3)
            .map(|task| task_brief(task))
            .collect(),
        task_store_error: None,
    }
}

fn sort_tasks(mut tasks: Vec<&Task>) -> Vec<&Task> {
    tasks.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
    tasks
}

fn task_brief(task: &Task) -> String {
    format!("{} [{}] {}", task.id, task.priority, task.subject)
}

fn build_interventions(daemon: &DaemonMonitor, projects: &[ProjectMonitor]) -> Vec<String> {
    let mut interventions = Vec::new();

    if !daemon.online {
        interventions.push(
            "Daemon is offline, so patrols, watchdogs, and chat ingress are inactive. Start it with `sigil daemon start` or `sigil daemon install --start`.".to_string(),
        );
    }

    for reason in &daemon.blocking_reasons {
        if reason.contains("budget exhausted") {
            interventions.push(
                "Daily budget is exhausted. Raise `[security].max_cost_per_day_usd` or wait for the budget window to reset before expecting new autonomous work.".to_string(),
            );
        } else if reason.contains("skipped") {
            interventions.push(
                "Some configured owners were skipped because their directories are missing. Fix those paths and rerun `sigil doctor --strict`.".to_string(),
            );
        } else if reason.contains("zero worker capacity") {
            interventions.push(
                "Registered owners expose zero worker capacity. Increase `max_workers` on the affected project or advisor before relying on background execution.".to_string(),
            );
        } else if reason.contains("no projects or advisor agents") {
            interventions.push(
                "No runnable owners are configured. Add a project or advisor, then rerun `sigil setup` or `sigil doctor --strict`.".to_string(),
            );
        }
    }

    if daemon.dispatch_health.dead_letters > 0 {
        interventions.push(format!(
            "{} dispatch(es) are in dead-letter state. Inspect the backlog with `sigil daemon query dispatches` and clear the failing route.",
            daemon.dispatch_health.dead_letters
        ));
    }
    if daemon.dispatch_health.overdue_ack > 0 {
        interventions.push(format!(
            "{} dispatch(es) are overdue for acknowledgment. Review `sigil daemon query dispatches` before they silently stall escalations.",
            daemon.dispatch_health.overdue_ack
        ));
    }

    let mut project_actions = Vec::new();
    for project in projects {
        if !project.repo_present {
            project_actions.push(format!(
                "{} points at a missing repo path ({}). Fix the repo path before trusting autonomous execution there.",
                project.name, project.repo
            ));
        }
        if let Some(error) = &project.task_store_error {
            project_actions.push(format!(
                "{} task board could not be opened ({error}). Fix the project directory before expecting patrols or monitor detail.",
                project.name
            ));
        }
        if project.budget_blocked_tasks > 0 {
            project_actions.push(format!(
                "{} has {} budget-blocked task(s). Lower task burn, switch runtime, or raise project/day budgets.",
                project.name, project.budget_blocked_tasks
            ));
        }
        if project.stalled && project.blocked_tasks > 0 {
            let focus = project
                .top_blocked_tasks
                .first()
                .cloned()
                .unwrap_or_else(|| "blocked work".to_string());
            project_actions.push(format!(
                "{} is stalled with blocked work and no active execution. Start with `{focus}` and inspect `sigil audit --project {}`.",
                project.name, project.name
            ));
        } else if project.critical_ready_tasks > 0 {
            project_actions.push(format!(
                "{} has {} critical ready task(s). Pull them into execution with `sigil ready --project {}` or let the daemon patrol pick them up.",
                project.name, project.critical_ready_tasks, project.name
            ));
        } else if project.ready_tasks > 0 && project.in_progress_tasks == 0 {
            project_actions.push(format!(
                "{} has {} ready task(s) but no active work. That is idle capacity or a stopped daemon.",
                project.name, project.ready_tasks
            ));
        }
    }

    project_actions.sort();
    interventions.extend(project_actions);

    interventions.sort();
    interventions.dedup();
    interventions.truncate(8);

    if interventions.is_empty() {
        interventions.push(
            "No immediate interventions detected. Keep `sigil monitor --watch` open while the daemon runs to spot drift early.".to_string(),
        );
    }

    interventions
}

fn render_monitor_report(report: &MonitorReport) {
    let generated = chrono::DateTime::parse_from_rfc3339(&report.generated_at)
        .map(|ts| ts.with_timezone(&Local))
        .unwrap_or_else(|_| Local::now());
    println!("Sigil Monitor");
    println!("Generated: {}", generated.format("%Y-%m-%d %H:%M:%S %Z"));
    println!(
        "Mode: {}",
        if report.daemon.online {
            "live daemon + local task state"
        } else {
            "local task state only"
        }
    );

    println!("\nControl Plane");
    if report.daemon.online {
        println!(
            "  readiness: {}",
            if report.daemon.ready == Some(true) {
                "READY"
            } else {
                "BLOCKED"
            }
        );
        if let Some(leader) = &report.daemon.leader_agent {
            println!("  leader: {leader}");
        }
        if let Some(count) = report.daemon.registered_owner_count {
            let configured_projects = report.daemon.configured_projects.unwrap_or(0);
            let configured_advisors = report.daemon.configured_advisors.unwrap_or(0);
            println!(
                "  owners: {} registered ({} projects, {} advisors configured)",
                count, configured_projects, configured_advisors
            );
        }
        if let Some(max_workers) = report.daemon.max_workers {
            println!("  worker capacity: {} max", max_workers);
        }
        if let (Some(spent), Some(budget), Some(remaining)) = (
            report.daemon.cost_today_usd,
            report.daemon.daily_budget_usd,
            report.daemon.budget_remaining_usd,
        ) {
            let pct = if budget > 0.0 {
                (spent / budget * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            println!(
                "  budget: ${spent:.2} / ${budget:.2} used ({pct:.0}%), ${remaining:.2} remaining"
            );
        }
        println!(
            "  dispatches: unread={} awaiting_ack={} retrying={} overdue={} dead_letters={}",
            report.daemon.dispatch_health.unread,
            report.daemon.dispatch_health.awaiting_ack,
            report.daemon.dispatch_health.retrying_delivery,
            report.daemon.dispatch_health.overdue_ack,
            report.daemon.dispatch_health.dead_letters
        );
    } else {
        println!("  readiness: unavailable");
        if let Some(error) = &report.daemon.error {
            println!("  daemon: {error}");
        }
    }

    if !report.daemon.blocking_reasons.is_empty() {
        println!("\nBlocking");
        for reason in &report.daemon.blocking_reasons {
            println!("  - {reason}");
        }
    }
    if !report.daemon.warnings.is_empty() {
        println!("\nWarnings");
        for warning in &report.daemon.warnings {
            println!("  - {warning}");
        }
    }

    println!("\nProjects");
    if report.projects.is_empty() {
        println!("  (no projects selected)");
    } else {
        for project in &report.projects {
            let repo_state = if project.repo_present {
                "ok"
            } else {
                "missing"
            };
            let org_suffix = if project.org_hint.is_empty() {
                String::new()
            } else {
                format!(" {}", project.org_hint)
            };
            println!(
                "  {:<16} open={:<3} ready={:<3} blocked={:<3} active={:<3} critical={:<3} repo={} runtime={} model={}{}",
                project.name,
                project.open_tasks,
                project.ready_tasks,
                project.blocked_tasks,
                project.in_progress_tasks,
                project.critical_ready_tasks,
                repo_state,
                project.runtime_provider,
                project.model,
                org_suffix,
            );
            if let Some(error) = &project.task_store_error {
                println!("    task-store-error: {error}");
                continue;
            }
            if project.stalled {
                println!("    state: stalled");
            }
            if !project.top_ready_tasks.is_empty() {
                println!("    ready: {}", project.top_ready_tasks.join(" | "));
            }
            if !project.top_blocked_tasks.is_empty() {
                println!("    blocked: {}", project.top_blocked_tasks.join(" | "));
            }
        }
    }

    println!("\nInterventions");
    for (index, action) in report.interventions.iter().enumerate() {
        println!("  {}. {}", index + 1, action);
    }
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(serde_json::Value::as_u64)
}

fn json_u64_nested(value: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
}

fn string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DaemonMonitor, DispatchHealthSnapshot, ProjectMonitor, build_interventions};

    #[test]
    fn monitor_interventions_prioritize_control_plane_failures() {
        let daemon = DaemonMonitor {
            online: false,
            blocking_reasons: vec![
                "daily budget exhausted ($50.00 spent of $50.00)".to_string(),
                "1 configured project(s) were skipped because their directories were missing"
                    .to_string(),
            ],
            dispatch_health: DispatchHealthSnapshot {
                dead_letters: 1,
                overdue_ack: 2,
                ..DispatchHealthSnapshot::default()
            },
            ..DaemonMonitor::default()
        };
        let projects = vec![ProjectMonitor {
            name: "alpha".to_string(),
            repo: "/tmp/alpha".to_string(),
            repo_present: true,
            runtime_provider: "openrouter".to_string(),
            model: "x".to_string(),
            org_hint: String::new(),
            open_tasks: 3,
            ready_tasks: 0,
            blocked_tasks: 2,
            in_progress_tasks: 0,
            critical_ready_tasks: 0,
            budget_blocked_tasks: 1,
            stalled: true,
            top_ready_tasks: Vec::new(),
            top_blocked_tasks: vec!["aa-001 [high] unblock deploy".to_string()],
            task_store_error: None,
        }];

        let interventions = build_interventions(&daemon, &projects);

        assert!(
            interventions
                .iter()
                .any(|item| item.contains("Daemon is offline"))
        );
        assert!(
            interventions
                .iter()
                .any(|item| item.contains("Daily budget is exhausted"))
        );
        assert!(
            interventions
                .iter()
                .any(|item| item.contains("dead-letter"))
        );
        assert!(
            interventions
                .iter()
                .any(|item| item.contains("alpha is stalled"))
        );
    }

    #[test]
    fn monitor_interventions_highlight_critical_ready_backlog() {
        let daemon = DaemonMonitor {
            online: true,
            ..DaemonMonitor::default()
        };
        let projects = vec![ProjectMonitor {
            name: "beta".to_string(),
            repo: "/tmp/beta".to_string(),
            repo_present: true,
            runtime_provider: "anthropic".to_string(),
            model: "claude".to_string(),
            org_hint: String::new(),
            open_tasks: 4,
            ready_tasks: 2,
            blocked_tasks: 0,
            in_progress_tasks: 0,
            critical_ready_tasks: 1,
            budget_blocked_tasks: 0,
            stalled: false,
            top_ready_tasks: vec!["bb-001 [critical] ship release".to_string()],
            top_blocked_tasks: Vec::new(),
            task_store_error: None,
        }];

        let interventions = build_interventions(&daemon, &projects);

        assert_eq!(interventions.len(), 1);
        assert!(interventions[0].contains("critical ready task"));
        assert!(interventions[0].contains("sigil ready --project beta"));
    }

    #[test]
    fn monitor_interventions_fallback_when_clear() {
        let interventions = build_interventions(
            &DaemonMonitor {
                online: true,
                ..DaemonMonitor::default()
            },
            &[],
        );
        assert_eq!(interventions.len(), 1);
        assert!(interventions[0].contains("sigil monitor --watch"));
    }
}
