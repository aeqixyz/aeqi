use anyhow::Result;
use std::path::PathBuf;

use crate::helpers::{
    format_agent_org_hint, format_project_org_hint, load_config_with_agents, open_tasks_for_project,
};

pub(crate) async fn cmd_status(config_path: &Option<PathBuf>) -> Result<()> {
    let (config, _) = load_config_with_agents(config_path)?;

    println!("Sigil: {}\n", config.sigil.name);

    // Show system team.
    println!("Sigil Team: leader={}", config.leader());
    println!();

    // Show agents.
    if !config.agents.is_empty() {
        println!("Agents:");
        for agent_cfg in &config.agents {
            let expertise = if agent_cfg.expertise.is_empty() {
                "general".to_string()
            } else {
                agent_cfg.expertise.join(", ")
            };
            let leader_marker = if config.leader() == agent_cfg.name {
                " [SIGIL LEADER]"
            } else {
                ""
            };
            let org_hint = format_agent_org_hint(&config, &agent_cfg.name);
            let runtime = config.runtime_for_agent(&agent_cfg.name);
            println!(
                "  {} [{}] role={:?} voice={:?} runtime={} model={}{} expertise=[{}]{}",
                agent_cfg.name,
                agent_cfg.prefix,
                agent_cfg.role,
                agent_cfg.voice,
                runtime.provider,
                config.model_for_agent(&agent_cfg.name),
                leader_marker,
                expertise,
                org_hint,
            );
        }
        println!();
    }

    println!("Projects:");
    for project_cfg in &config.projects {
        let repo_ok = PathBuf::from(&project_cfg.repo).exists();
        let team = config.project_team(&project_cfg.name);
        print!(
            "  {} [{}] prefix={} runtime={} model={} workers={} leader={}",
            project_cfg.name,
            if repo_ok { "OK" } else { "MISSING" },
            project_cfg.prefix,
            config.runtime_for_project(&project_cfg.name).provider,
            config.model_for_project(&project_cfg.name),
            project_cfg.max_workers,
            team.leader,
        );

        // Show task counts.
        if let Ok(store) = open_tasks_for_project(&project_cfg.name) {
            let open: Vec<_> = store
                .by_prefix(&project_cfg.prefix)
                .into_iter()
                .filter(|b| !b.is_closed())
                .collect();
            let ready = store.ready().len();
            print!(" | tasks: {} open, {} ready", open.len(), ready);
        }
        let org_hint = format_project_org_hint(&config, &project_cfg.name);
        if !org_hint.is_empty() {
            print!(" |{}", org_hint.trim_start());
        }
        println!();
    }

    Ok(())
}
