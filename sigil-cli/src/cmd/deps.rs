use anyhow::Result;
use std::path::PathBuf;

use crate::helpers::{load_config, open_tasks_for_project};

pub(crate) async fn cmd_deps(
    config_path: &Option<PathBuf>,
    project: &str,
    apply: Option<f64>,
) -> Result<()> {
    let (_config, _) = load_config(config_path)?;
    let mut store = open_tasks_for_project(project)?;

    let threshold = apply.unwrap_or(0.3);

    if let Some(apply_threshold) = apply {
        let applied = store.apply_inferred_dependencies(apply_threshold)?;
        println!("Applied {applied} inferred dependencies (threshold: {apply_threshold:.1}).");
    } else {
        let suggestions = store.suggest_dependencies(threshold);
        if suggestions.is_empty() {
            println!("No dependency suggestions above threshold {threshold:.1}.");
            return Ok(());
        }
        println!("Suggested dependencies (threshold: {threshold:.1}):\n");
        for dep in &suggestions {
            println!(
                "  {} → {} (confidence: {:.0}%)\n    Reason: {}\n",
                dep.from,
                dep.to,
                dep.confidence * 100.0,
                dep.reason,
            );
        }
        println!("Use --apply {threshold:.1} to auto-apply these dependencies.");
    }

    Ok(())
}
