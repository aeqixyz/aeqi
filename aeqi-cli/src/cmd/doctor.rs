use aeqi_core::SecretStore;
use aeqi_tools::Skill;
use anyhow::{Result, bail};
use std::path::PathBuf;

use crate::helpers::{
    build_provider_for_runtime, find_agent_dir, find_project_dir, load_config_with_agents,
    provider_secret_store_path,
};

pub(crate) async fn cmd_doctor(
    config_path: &Option<PathBuf>,
    fix: bool,
    strict: bool,
) -> Result<()> {
    println!(
        "AEQI Doctor{}\n============\n",
        match (fix, strict) {
            (true, true) => " (--fix --strict)",
            (true, false) => " (--fix)",
            (false, true) => " (--strict)",
            (false, false) => "",
        }
    );

    let mut issues_found = 0u32;
    let mut fixed = 0u32;

    match load_config_with_agents(config_path) {
        Ok((config, path)) => {
            println!("[OK] Config: {}", path.display());
            for issue in config.validate() {
                println!("[WARN] Config validation: {issue}");
                issues_found += 1;
            }
            if let Some(ref runtime) = config.aeqi.default_runtime {
                println!("[OK] Default runtime: {runtime}");
            }

            let store_path = provider_secret_store_path(&config);
            let secret_store = SecretStore::open(&store_path).ok();

            if let Some(ref or) = config.providers.openrouter {
                let api_key = if !or.api_key.is_empty() {
                    Some(or.api_key.clone())
                } else {
                    secret_store
                        .as_ref()
                        .and_then(|s| s.get("OPENROUTER_API_KEY").ok())
                };

                match api_key {
                    Some(_) => {
                        let provider = build_provider_for_runtime(
                            &config,
                            aeqi_core::ProviderKind::OpenRouter,
                            Some(&or.default_model),
                        )?;
                        match provider.health_check().await {
                            Ok(()) => println!("[OK] OpenRouter API key valid"),
                            Err(e) => {
                                println!("[FAIL] OpenRouter: {e}");
                                issues_found += 1;
                            }
                        }
                    }
                    None => {
                        println!("[WARN] OpenRouter API key not set (config or secret store)");
                        issues_found += 1;
                    }
                }
            }
            if let Some(ref anthropic) = config.providers.anthropic {
                let api_key = if !anthropic.api_key.is_empty() {
                    Some(anthropic.api_key.clone())
                } else {
                    secret_store
                        .as_ref()
                        .and_then(|s| s.get("ANTHROPIC_API_KEY").ok())
                };

                match api_key {
                    Some(_) => {
                        let provider = build_provider_for_runtime(
                            &config,
                            aeqi_core::ProviderKind::Anthropic,
                            Some(&anthropic.default_model),
                        )?;
                        match provider.health_check().await {
                            Ok(()) => println!("[OK] Anthropic API key valid"),
                            Err(e) => {
                                println!("[FAIL] Anthropic: {e}");
                                issues_found += 1;
                            }
                        }
                    }
                    None => {
                        println!("[WARN] Anthropic API key not set (config or secret store)");
                        issues_found += 1;
                    }
                }
            }
            if let Some(ref ollama) = config.providers.ollama {
                let provider = build_provider_for_runtime(
                    &config,
                    aeqi_core::ProviderKind::Ollama,
                    Some(&ollama.default_model),
                )?;
                match provider.health_check().await {
                    Ok(()) => println!("[OK] Ollama reachable at {}", ollama.url),
                    Err(e) => {
                        println!("[WARN] Ollama: {e}");
                        issues_found += 1;
                    }
                }
            }
            if config.providers.openrouter.is_none()
                && config.providers.anthropic.is_none()
                && config.providers.ollama.is_none()
            {
                println!("[WARN] No providers configured");
                issues_found += 1;
            }

            for pcfg in &config.agent_spawns {
                let runtime = config.runtime_for_company(&pcfg.name);
                let mode = config.execution_mode_for_project(&pcfg.name);
                let repo_ok = PathBuf::from(&pcfg.repo).exists();
                println!(
                    "[{}] Project '{}' repo: {} | runtime={} | mode={:?} | model={}",
                    if repo_ok { "OK" } else { "WARN" },
                    pcfg.name,
                    pcfg.repo,
                    runtime.provider,
                    mode,
                    config.model_for_company(&pcfg.name),
                );
                if !repo_ok {
                    issues_found += 1;
                }

                match find_project_dir(&pcfg.name) {
                    Ok(d) => {
                        let agents_md = d.join("AGENTS.md").exists();
                        let knowledge_md = d.join("KNOWLEDGE.md").exists();
                        let tasks_dir = d.join(".tasks");
                        let has_tasks = tasks_dir.exists();
                        if !agents_md {
                            issues_found += 1;
                        }
                        println!(
                            "    Project files: AGENTS.md={agents_md} KNOWLEDGE.md={knowledge_md} | Tasks: {has_tasks}"
                        );

                        // --fix: create missing .tasks dir
                        if fix && !has_tasks {
                            std::fs::create_dir_all(&tasks_dir)?;
                            println!("    [FIXED] Created .tasks directory");
                            fixed += 1;
                        }

                        // Check skills directory
                        let skills_dir = d.join("skills");
                        let skill_count = if skills_dir.exists() {
                            Skill::discover(&skills_dir).map(|s| s.len()).unwrap_or(0)
                        } else {
                            0
                        };
                        let pipelines_dir = if d.join("pipelines").exists() {
                            d.join("pipelines")
                        } else {
                            d.join("rituals")
                        };
                        let pipeline_count = if pipelines_dir.exists() {
                            std::fs::read_dir(&pipelines_dir)
                                .map(|e| {
                                    e.filter(|e| {
                                        e.as_ref()
                                            .ok()
                                            .map(|e| {
                                                e.path().extension().is_some_and(|x| x == "toml")
                                            })
                                            .unwrap_or(false)
                                    })
                                    .count()
                                })
                                .unwrap_or(0)
                        } else {
                            0
                        };
                        println!("    Skills: {skill_count} | Pipelines: {pipeline_count}");

                        // Check legacy per-project memory DB
                        let mem_db = d.join(".aeqi").join("memory.db");
                        if mem_db.exists() {
                            println!("    Memory (legacy): {}", mem_db.display());
                        }
                    }
                    Err(_) => {
                        println!("    [WARN] Project dir not found");
                        issues_found += 1;
                    }
                }
            }

            // Check agent identity files.
            for agent_cfg in &config.agents {
                let runtime = config.runtime_for_agent(&agent_cfg.name);
                let mode = config.execution_mode_for_agent(&agent_cfg.name);
                match find_agent_dir(&agent_cfg.name) {
                    Ok(d) => {
                        let has_persona = d.join("PERSONA.md").exists();
                        let has_identity = d.join("IDENTITY.md").exists();
                        if !has_persona {
                            issues_found += 1;
                        }
                        if !has_identity {
                            issues_found += 1;
                        }
                        println!(
                            "[{}] Agent '{}': PERSONA={has_persona} IDENTITY={has_identity} | runtime={} | mode={:?} | model={}",
                            if has_persona && has_identity {
                                "OK"
                            } else {
                                "WARN"
                            },
                            agent_cfg.name,
                            runtime.provider,
                            mode,
                            config.model_for_agent(&agent_cfg.name),
                        );
                    }
                    Err(_) => {
                        println!("[WARN] Agent dir not found for '{}'", agent_cfg.name);
                        issues_found += 1;
                    }
                }
            }

            if store_path.exists() {
                println!("[OK] Secret store: {}", store_path.display());
            } else {
                issues_found += 1;
                if fix {
                    std::fs::create_dir_all(&store_path)?;
                    println!("[FIXED] Created secret store: {}", store_path.display());
                    fixed += 1;
                } else {
                    println!("[WARN] Secret store missing: {}", store_path.display());
                }
            }

            // Check insights DB.
            let mem_path = config.data_dir().join("insights.db");
            println!(
                "[{}] Insights DB: {}",
                if mem_path.exists() { "OK" } else { "INFO" },
                mem_path.display()
            );

            // Check triggers.
            match aeqi_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir()) {
                Ok(reg) => {
                    let ts = reg.trigger_store();
                    let count = ts.count_enabled().await.unwrap_or(0);
                    println!("[OK] Triggers: {count} enabled");
                }
                Err(_) => println!("[INFO] Triggers: no agent registry"),
            }

            // Check data dir
            let data_dir = config.data_dir();
            if data_dir.exists() {
                println!("[OK] Data dir: {}", data_dir.display());
            } else {
                issues_found += 1;
                if fix {
                    std::fs::create_dir_all(&data_dir)?;
                    println!("[FIXED] Created data dir: {}", data_dir.display());
                    fixed += 1;
                } else {
                    println!("[WARN] Data dir missing: {}", data_dir.display());
                }
            }
        }
        Err(e) => {
            println!("[FAIL] Config: {e}");
            println!("       Run `aeqi setup` to create one.");
            issues_found += 1;
        }
    }

    let remaining_issues = issues_found.saturating_sub(fixed);

    println!();
    if issues_found == 0 {
        println!("All checks passed.");
    } else if remaining_issues == 0 {
        println!("{issues_found} issues found, {fixed} fixed, 0 remaining.");
    } else if fix {
        println!("{issues_found} issues found, {fixed} fixed, {remaining_issues} remaining.");
    } else {
        println!("{issues_found} issues found. Run `aeqi doctor --fix` to auto-repair.");
    }

    if strict && remaining_issues > 0 {
        bail!("doctor found {remaining_issues} unresolved issue(s)");
    }

    Ok(())
}
