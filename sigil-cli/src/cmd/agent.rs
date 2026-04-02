use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::helpers::{format_agent_org_hint, load_config, resolve_agents_dir};

pub(crate) async fn cmd_agent(
    config_path: &Option<PathBuf>,
    action: crate::cli::AgentAction,
) -> Result<()> {
    match action {
        crate::cli::AgentAction::List => {
            let (config, config_path_resolved) = load_config(config_path)?;
            let agents_dir = resolve_agents_dir(&config_path_resolved);

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
                all_agents.push((&a.name, source, &a.role));
            }
            for a in &disk_agents {
                if !toml_names.contains(a.name.as_str()) {
                    all_agents.push((&a.name, "disk", &a.role));
                }
            }
            all_agents.sort_by_key(|a| a.0);

            println!("Discovered Agents ({}):\n", all_agents.len());
            for (name, source, role) in &all_agents {
                let org_hint = format_agent_org_hint(&config, name);
                println!("  {name:<15} role={role:<12} source={source}{org_hint}");
            }
            Ok(())
        }
        crate::cli::AgentAction::Migrate { force } => {
            let (config, config_path_resolved) = load_config(config_path)?;
            let agents_dir = resolve_agents_dir(&config_path_resolved);

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
            Ok(())
        }
        crate::cli::AgentAction::Spawn { template, project } => {
            let (config, _) = load_config(config_path)?;
            let registry =
                sigil_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir())?;
            let content = std::fs::read_to_string(&template)?;
            let agent = registry
                .spawn_from_template(&content, project.as_deref())
                .await?;
            println!("Spawned persistent agent:");
            println!("  ID:      {}", agent.id);
            println!("  Name:    {}", agent.name);
            println!(
                "  Display: {}",
                agent.display_name.as_deref().unwrap_or("-")
            );
            println!(
                "  Project: {}",
                agent.project.as_deref().unwrap_or("(root)")
            );
            println!(
                "  Model:   {}",
                agent.model.as_deref().unwrap_or("(default)")
            );
            println!("  Caps:    {:?}", agent.capabilities);
            Ok(())
        }
        crate::cli::AgentAction::Show { name } => {
            let (config, _) = load_config(config_path)?;
            let registry =
                sigil_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir())?;
            let agents = registry.get_by_name(&name).await?;
            if agents.is_empty() {
                println!("No agents named '{name}' in registry.");
            }
            for a in &agents {
                println!("Agent: {} ({})", a.name, a.id);
                if let Some(d) = &a.display_name {
                    println!("  Display:  {d}");
                }
                println!("  Status:   {}", a.status);
                println!("  Project:  {}", a.project.as_deref().unwrap_or("(root)"));
                println!("  Model:    {}", a.model.as_deref().unwrap_or("(default)"));
                println!("  Caps:     {:?}", a.capabilities);
                println!("  Sessions: {}", a.session_count);
                println!("  Tokens:   {}", a.total_tokens);
                println!("  Created:  {}", a.created_at);
                if let Some(la) = &a.last_active {
                    println!("  Active:   {la}");
                }
                println!("\n--- System Prompt ---\n{}", a.system_prompt);
                println!();
            }
            Ok(())
        }
        crate::cli::AgentAction::Retire { name } => {
            let (config, _) = load_config(config_path)?;
            let registry =
                sigil_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir())?;
            registry
                .set_status(
                    &name,
                    sigil_orchestrator::agent_registry::AgentStatus::Retired,
                )
                .await?;
            println!("Agent '{name}' retired. Memory preserved.");
            Ok(())
        }
        crate::cli::AgentAction::Activate { name } => {
            let (config, _) = load_config(config_path)?;
            let registry =
                sigil_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir())?;
            registry
                .set_status(
                    &name,
                    sigil_orchestrator::agent_registry::AgentStatus::Active,
                )
                .await?;
            println!("Agent '{name}' activated.");
            Ok(())
        }
        crate::cli::AgentAction::Registry { project } => {
            let (config, _) = load_config(config_path)?;
            let registry =
                sigil_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir())?;
            let agents = registry.list(project.as_deref(), None).await?;
            if agents.is_empty() {
                println!("No persistent agents registered.");
                println!("Spawn one: sigil agent spawn <template.md>");
                return Ok(());
            }
            println!(
                "{:<20} {:<10} {:<15} {:<10} {:<8}",
                "NAME", "STATUS", "PROJECT", "SESSIONS", "TOKENS"
            );
            println!("{}", "-".repeat(63));
            for a in &agents {
                println!(
                    "{:<20} {:<10} {:<15} {:<10} {:<8}",
                    a.name,
                    a.status.to_string(),
                    a.project.as_deref().unwrap_or("(root)"),
                    a.session_count,
                    a.total_tokens,
                );
            }
            Ok(())
        }
    }
}
