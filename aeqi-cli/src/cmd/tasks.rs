use aeqi_orchestrator::OperationStore;
use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::helpers::{load_config, open_tasks_for_project, project_name_for_prefix};

pub(crate) async fn cmd_assign(
    config_path: &Option<PathBuf>,
    subject: &str,
    project_name: &str,
    description: &str,
    priority: Option<&str>,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let prefix = if let Some(pcfg) = config.project(project_name) {
        pcfg.prefix.clone()
    } else if let Some(acfg) = config.agent(project_name) {
        acfg.prefix.clone()
    } else {
        anyhow::bail!("project or agent not found: {project_name}");
    };

    let mut store = open_tasks_for_project(project_name)?;
    let mut task = store.create_with_agent(&prefix, subject, None)?;

    if !description.is_empty() || priority.is_some() {
        task = store.update(&task.id.0, |b| {
            if !description.is_empty() {
                b.description = description.to_string();
            }
            if let Some(p) = priority {
                b.priority = match p {
                    "low" => aeqi_tasks::Priority::Low,
                    "high" => aeqi_tasks::Priority::High,
                    "critical" => aeqi_tasks::Priority::Critical,
                    _ => aeqi_tasks::Priority::Normal,
                };
            }
        })?;
    }

    println!("Created {} [{}] {}", task.id, task.priority, task.subject);
    Ok(())
}

pub(crate) async fn cmd_ready(
    config_path: &Option<PathBuf>,
    project_name: Option<&str>,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let projects: Vec<&str> = if let Some(name) = project_name {
        vec![name]
    } else {
        config.projects.iter().map(|r| r.name.as_str()).collect()
    };

    let mut found = false;
    for name in projects {
        if let Ok(store) = open_tasks_for_project(name) {
            let ready = store.ready();
            for task in ready {
                found = true;
                println!(
                    "{} [{}] {} — {}",
                    task.id,
                    task.priority,
                    task.subject,
                    if task.description.is_empty() {
                        "(no description)"
                    } else {
                        &task.description
                    }
                );
            }
        }
    }

    if !found {
        println!("No ready work.");
    }
    Ok(())
}

pub(crate) async fn cmd_tasks(
    config_path: &Option<PathBuf>,
    project_name: Option<&str>,
    show_all: bool,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let projects: Vec<&str> = if let Some(name) = project_name {
        vec![name]
    } else {
        config.projects.iter().map(|r| r.name.as_str()).collect()
    };

    for name in projects {
        if let Ok(store) = open_tasks_for_project(name) {
            let tasks = store.all();
            let tasks: Vec<_> = if show_all {
                tasks
            } else {
                tasks.into_iter().filter(|b| !b.is_closed()).collect()
            };

            if tasks.is_empty() {
                continue;
            }

            println!("=== {} ===", name);
            for task in tasks {
                let assignee = task.assignee.as_deref().unwrap_or("-");
                let deps = if task.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (needs: {})",
                        task.depends_on
                            .iter()
                            .map(|d| d.0.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let checkpoints = if task.checkpoints.is_empty() {
                    String::new()
                } else {
                    format!(" checkpoints={}", task.checkpoints.len())
                };
                println!(
                    "  {} [{}] {} — {} assignee={}{}{}",
                    task.id, task.status, task.priority, task.subject, assignee, deps, checkpoints
                );
            }
        }
    }
    Ok(())
}

pub(crate) async fn cmd_close(config_path: &Option<PathBuf>, id: &str, reason: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = id.split('-').next().unwrap_or("");
    let project_name = project_name_for_prefix(&config, prefix)
        .context(format!("no project with prefix '{prefix}'"))?;

    let mut store = open_tasks_for_project(&project_name)?;
    let task = store.close(id, reason)?;
    println!("Closed {} — {}", task.id, task.subject);
    Ok(())
}

pub(crate) async fn cmd_hook(
    config_path: &Option<PathBuf>,
    worker: &str,
    task_id: &str,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = task_id.split('-').next().unwrap_or("");
    let project_name = project_name_for_prefix(&config, prefix)
        .context(format!("no project with prefix '{prefix}'"))?;

    let mut store = open_tasks_for_project(&project_name)?;
    let task = store.update(task_id, |b| {
        b.status = aeqi_tasks::TaskStatus::InProgress;
        b.assignee = Some(worker.to_string());
    })?;

    println!("Hooked {} to {} — {}", worker, task.id, task.subject);
    Ok(())
}

pub(crate) async fn cmd_done(
    config_path: &Option<PathBuf>,
    task_id: &str,
    reason: &str,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = task_id.split('-').next().unwrap_or("");
    let project_name = project_name_for_prefix(&config, prefix)
        .context(format!("no project with prefix '{prefix}'"))?;

    let mut store = open_tasks_for_project(&project_name)?;
    let task = store.close(task_id, reason)?;
    println!("Done {} — {}", task.id, task.subject);

    // Also update any operations tracking this task.
    let ops_path = config.data_dir().join("operations.json");
    if ops_path.exists() {
        let mut op_store = OperationStore::open(&ops_path)?;
        let completed = op_store.mark_task_closed(&task.id)?;
        for c_id in &completed {
            println!("Operation {c_id} completed!");
        }
    }

    Ok(())
}
