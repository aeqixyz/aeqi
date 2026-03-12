use anyhow::Context;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sigil_orchestrator::schedule::CronSchedule;
use sigil_orchestrator::{ScheduleStore, ScheduledJob};
use std::path::PathBuf;

use crate::cli::CronAction;
use crate::helpers::load_config;

pub(crate) async fn cmd_cron(config_path: &Option<PathBuf>, action: CronAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let cron_path = config.data_dir().join("fate.json");

    match action {
        CronAction::Add {
            name,
            schedule,
            at,
            project,
            prompt,
            isolated,
        } => {
            config
                .project(&project)
                .context(format!("project not found: {project}"))?;

            let cron_schedule = if let Some(at_str) = at {
                let dt = at_str
                    .parse::<DateTime<Utc>>()
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(&at_str, "%Y-%m-%dT%H:%M:%S")
                            .map(|ndt| ndt.and_utc())
                    })
                    .context(format!(
                        "invalid datetime: {at_str} (use ISO 8601, e.g. 2026-02-22T15:00:00Z)"
                    ))?;
                CronSchedule::Once { at: dt }
            } else if let Some(expr) = schedule {
                CronSchedule::Cron { expr }
            } else {
                anyhow::bail!("specify --schedule \"0 9 * * *\" or --at \"2026-02-22T15:00:00Z\"");
            };

            let job = ScheduledJob {
                name: name.clone(),
                schedule: cron_schedule,
                project,
                prompt,
                isolated,
                created_at: Utc::now(),
                last_run: None,
            };

            let mut store = ScheduleStore::open(&cron_path)?;
            store.add(job)?;
            println!("Cron job '{name}' added.");
        }

        CronAction::List => {
            let store = ScheduleStore::open(&cron_path)?;
            if store.jobs.is_empty() {
                println!("No cron jobs.");
            } else {
                for job in &store.jobs {
                    let sched = match &job.schedule {
                        CronSchedule::Cron { expr } => format!("cron: {expr}"),
                        CronSchedule::Once { at } => format!("once: {at}"),
                    };
                    let last = job
                        .last_run
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "never".to_string());
                    let iso = if job.isolated { " [isolated]" } else { "" };
                    println!(
                        "  {} — project={} {} last_run={}{}",
                        job.name, job.project, sched, last, iso
                    );
                }
            }
        }

        CronAction::Remove { name } => {
            let mut store = ScheduleStore::open(&cron_path)?;
            store.remove(&name)?;
            println!("Cron job '{name}' removed.");
        }
    }
    Ok(())
}
