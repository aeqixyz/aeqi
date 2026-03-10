use anyhow::{Context, Result};
use sigil_orchestrator::Pipeline;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::cli::PipelineAction;
use crate::helpers::{
    find_project_dir, load_config, open_tasks_for_project, project_name_for_prefix,
};

fn pipeline_dirs(project_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(parent) = project_dir.parent() {
        dirs.push(parent.join("shared").join("pipelines"));
        dirs.push(parent.join("shared").join("rituals"));
    }
    dirs.push(project_dir.join("pipelines"));
    dirs.push(project_dir.join("rituals"));
    dirs
}

fn discover_project_pipelines(project_dir: &Path) -> Result<BTreeMap<String, Pipeline>> {
    let mut merged = BTreeMap::new();

    for dir in pipeline_dirs(project_dir) {
        if !dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                let stem = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let pipeline = Pipeline::load(&path)?;
                merged.insert(stem, pipeline);
            }
        }
    }

    Ok(merged)
}

pub(crate) async fn cmd_pipeline(
    config_path: &Option<PathBuf>,
    action: PipelineAction,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        PipelineAction::Pour {
            template,
            project,
            vars,
        } => {
            let project_cfg = config
                .project(&project)
                .context(format!("project not found: {project}"))?;
            let project_dir = find_project_dir(&project)?;
            let pipelines = discover_project_pipelines(&project_dir)?;
            let pipeline = pipelines
                .get(&template)
                .cloned()
                .context(format!("pipeline template not found: {template}"))?;

            // Parse vars.
            let var_map: HashMap<String, String> = vars
                .iter()
                .filter_map(|v| {
                    let parts: Vec<&str> = v.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        Some((parts[0].to_string(), parts[1].to_string()))
                    } else {
                        None
                    }
                })
                .collect();

            // Instantiate into task store.
            let mut store = open_tasks_for_project(&project)?;
            let parent_id = pipeline.pour(&mut store, &project_cfg.prefix, &var_map)?;

            println!("Poured pipeline '{template}' as {parent_id}");
            println!("\nSteps:");
            let children = store.children(&parent_id);
            for child in children {
                let deps = if child.depends_on.is_empty() {
                    "ready".to_string()
                } else {
                    format!(
                        "needs: {}",
                        child
                            .depends_on
                            .iter()
                            .map(|d| d.0.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                println!(
                    "  {} [{}] {} ({})",
                    child.id, child.status, child.subject, deps
                );
            }
        }

        PipelineAction::List { project } => {
            let projects: Vec<&str> = if let Some(ref name) = project {
                vec![name.as_str()]
            } else {
                config.projects.iter().map(|r| r.name.as_str()).collect()
            };

            for name in projects {
                if let Ok(project_dir) = find_project_dir(name) {
                    let pipelines = discover_project_pipelines(&project_dir)?;
                    if !pipelines.is_empty() {
                        println!("=== {} ===", name);
                        for (stem, pipeline) in pipelines {
                            println!(
                                "  {} — {} ({} steps)",
                                stem,
                                pipeline.meta.description,
                                pipeline.steps.len()
                            );
                        }
                    }
                }
            }
        }

        PipelineAction::Status { id } => {
            let prefix = id.split('-').next().unwrap_or("");
            let project_name = project_name_for_prefix(&config, prefix)
                .context(format!("no project with prefix '{prefix}'"))?;

            let store = open_tasks_for_project(&project_name)?;
            let parent_id = sigil_tasks::TaskId::from(id.as_str());

            if let Some(parent) = store.get(&id) {
                println!("{} [{}] {}", parent.id, parent.status, parent.subject);
                let children = store.children(&parent_id);
                let done = children.iter().filter(|c| c.is_closed()).count();
                println!("Progress: {}/{}\n", done, children.len());
                for child in &children {
                    let status_icon = match child.status {
                        sigil_tasks::TaskStatus::Done => "[x]",
                        sigil_tasks::TaskStatus::InProgress => "[~]",
                        sigil_tasks::TaskStatus::Cancelled => "[-]",
                        _ => "[ ]",
                    };
                    println!("  {} {} {}", status_icon, child.id, child.subject);
                }
            } else {
                println!("Task not found: {id}");
            }
        }
    }
    Ok(())
}
