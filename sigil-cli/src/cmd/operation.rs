use anyhow::Result;
use sigil_orchestrator::OperationStore;
use std::path::PathBuf;

use crate::cli::OperationAction;
use crate::helpers::load_config;

pub(crate) async fn cmd_operation(
    config_path: &Option<PathBuf>,
    action: OperationAction,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let operations_path = config.data_dir().join("operations.json");

    match action {
        OperationAction::Create { name, task_ids } => {
            let tasks: Vec<(sigil_tasks::TaskId, String)> = task_ids
                .iter()
                .map(|id| {
                    let prefix = id.split('-').next().unwrap_or("");
                    let project_name = config
                        .projects
                        .iter()
                        .find(|r| r.prefix == prefix)
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    (sigil_tasks::TaskId::from(id.as_str()), project_name)
                })
                .collect();

            let mut store = OperationStore::open(&operations_path)?;
            let op = store.create(&name, tasks)?;
            let (done, total) = op.progress();
            println!(
                "Created operation {} — {} ({}/{})",
                op.id, op.name, done, total
            );
        }

        OperationAction::List => {
            let store = OperationStore::open(&operations_path)?;
            let active = store.active();
            if active.is_empty() {
                println!("No active operations.");
            } else {
                for op in active {
                    let (done, total) = op.progress();
                    println!("  {} — {} ({}/{})", op.id, op.name, done, total);
                }
            }
        }

        OperationAction::Status { id } => {
            let store = OperationStore::open(&operations_path)?;
            if let Some(op) = store.get(&id) {
                let (done, total) = op.progress();
                let status = if op.closed_at.is_some() {
                    "COMPLETE"
                } else {
                    "ACTIVE"
                };
                println!("{} [{}] {} ({}/{})", op.id, status, op.name, done, total);
                for task_entry in &op.tasks {
                    let icon = if task_entry.closed { "[x]" } else { "[ ]" };
                    println!(
                        "  {} {} (project: {})",
                        icon, task_entry.task_id, task_entry.project
                    );
                }
            } else {
                println!("Operation not found: {id}");
            }
        }
    }
    Ok(())
}
