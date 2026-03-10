use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::helpers::{format_agent_org_hint, load_config, resolve_agents_dir, role_str};

pub(crate) async fn cmd_agent(
    config_path: &Option<PathBuf>,
    action: crate::cli::AgentAction,
) -> Result<()> {
    let (config, config_path_resolved) = load_config(config_path)?;
    let agents_dir = resolve_agents_dir(&config_path_resolved);

    match action {
        crate::cli::AgentAction::List => {
            // Show agents from TOML.
            let toml_names: std::collections::HashSet<&str> =
                config.agents.iter().map(|a| a.name.as_str()).collect();

            // Discover from disk.
            let disk_agents = sigil_core::discover_agents(&agents_dir).unwrap_or_default();
            let disk_names: std::collections::HashSet<&str> =
                disk_agents.iter().map(|a| a.name.as_str()).collect();

            // Merge: all unique agents.
            let mut all_agents: Vec<(&str, &str, &str)> = Vec::new(); // (name, source, role)
            for a in &config.agents {
                let source = if disk_names.contains(a.name.as_str()) {
                    "both"
                } else {
                    "toml"
                };
                all_agents.push((&a.name, source, role_str(&a.role)));
            }
            for a in &disk_agents {
                if !toml_names.contains(a.name.as_str()) {
                    all_agents.push((&a.name, "disk", role_str(&a.role)));
                }
            }
            all_agents.sort_by_key(|a| a.0);

            println!("Discovered Agents ({}):\n", all_agents.len());
            for (name, source, role) in &all_agents {
                let org_hint = format_agent_org_hint(&config, name);
                println!("  {name:<15} role={role:<12} source={source}{org_hint}");
            }
        }
        crate::cli::AgentAction::Migrate { force } => {
            println!("Migrating [[agents]] from sigil.toml to agent.toml files...\n");
            let mut migrated = 0;
            let mut skipped = 0;

            for agent_cfg in &config.agents {
                let agent_dir = agents_dir.join(&agent_cfg.name);
                let toml_path = agent_dir.join("agent.toml");

                if toml_path.exists() && !force {
                    println!(
                        "  {} — skipped (agent.toml exists, use --force)",
                        agent_cfg.name
                    );
                    skipped += 1;
                    continue;
                }

                if !agent_dir.exists() {
                    println!("  {} — skipped (agent dir not found)", agent_cfg.name);
                    skipped += 1;
                    continue;
                }

                let toml_str = toml::to_string_pretty(agent_cfg)
                    .context(format!("failed to serialize config for {}", agent_cfg.name))?;
                std::fs::write(&toml_path, &toml_str)?;
                println!("  {} — written: {}", agent_cfg.name, toml_path.display());
                migrated += 1;
            }

            println!("\nMigrated: {migrated}, Skipped: {skipped}");
            if migrated > 0 {
                println!("\nYou can now remove the [[agents]] blocks from sigil.toml.");
            }
        }
    }
    Ok(())
}
