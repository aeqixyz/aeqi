use anyhow::Result;
use std::path::PathBuf;

use crate::helpers::{format_project_org_hint, load_config_with_agents};

pub(crate) async fn cmd_team(
    config_path: &Option<PathBuf>,
    project_filter: Option<&str>,
) -> Result<()> {
    let (config, _) = load_config_with_agents(config_path)?;

    // Show system team.
    println!("Sigil Team");
    println!("  leader: {}", config.leader());
    println!("  cooldown: {}s", config.team.router_cooldown_secs);
    println!("  max_bg_cost: ${:.2}", config.team.max_background_cost_usd);
    println!();

    // Show per-project teams.
    let projects: Vec<_> = if let Some(name) = project_filter {
        config.projects.iter().filter(|p| p.name == name).collect()
    } else {
        config.projects.iter().collect()
    };

    if projects.is_empty() {
        if let Some(name) = project_filter {
            println!("Project not found: {name}");
        }
        return Ok(());
    }

    println!("Project Teams:");
    for project_cfg in projects {
        let team = config.project_team(&project_cfg.name);
        let source = if project_cfg.team.is_some() {
            "configured"
        } else {
            "system fallback"
        };
        let org_hint = format_project_org_hint(&config, &project_cfg.name);
        println!(
            "  {} → leader={} agents=[{}] ({}){}",
            project_cfg.name,
            team.leader,
            team.effective_agents().join(", "),
            source,
            org_hint,
        );
    }

    let mut issues = config.validate_teams();
    issues.sort();
    issues.dedup();
    if !issues.is_empty() {
        println!("\nValidation warnings:");
        for issue in &issues {
            println!("  ! {issue}");
        }
    }

    Ok(())
}
