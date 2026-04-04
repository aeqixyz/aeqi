use aeqi_core::traits::{LogObserver, Observer, Tool};
use aeqi_core::{Agent, AgentConfig};
use aeqi_tools::Skill;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::cli::SkillAction;
use crate::helpers::{
    augment_prompt_with_org_context, build_project_tools, build_provider_for_project,
    find_agent_dir, find_project_dir, load_config, load_system_prompt, load_system_prompt_from_dir,
    one_shot_agent_name, open_insights,
};

fn discover_project_skills(project_dir: &Path) -> Result<Vec<Skill>> {
    let mut merged = BTreeMap::new();
    let mut dirs = Vec::new();
    if let Some(parent) = project_dir.parent() {
        dirs.push(parent.join("shared").join("skills"));
    }
    dirs.push(project_dir.join("skills"));

    for dir in dirs {
        for skill in Skill::discover(&dir)? {
            merged.insert(skill.skill.name.clone(), skill);
        }
    }

    Ok(merged.into_values().collect())
}

pub(crate) async fn cmd_skill(config_path: &Option<PathBuf>, action: SkillAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        SkillAction::List { company } => {
            let projects: Vec<&str> = if let Some(ref name) = company {
                vec![name.as_str()]
            } else {
                config
                    .agent_spawns
                    .iter()
                    .map(|r| r.name.as_str())
                    .collect()
            };

            for name in projects {
                if let Ok(project_dir) = find_project_dir(name) {
                    let skills = discover_project_skills(&project_dir)?;
                    if !skills.is_empty() {
                        println!("=== {} ===", name);
                        for skill in &skills {
                            let triggers = if skill.skill.triggers.is_empty() {
                                String::new()
                            } else {
                                format!(" (triggers: {})", skill.skill.triggers.join(", "))
                            };
                            let tools = if skill.tools.allow.is_empty() {
                                "all".to_string()
                            } else {
                                skill.tools.allow.join(", ")
                            };
                            println!(
                                "  {} — {} [tools: {}]{}",
                                skill.skill.name, skill.skill.description, tools, triggers
                            );
                        }
                    }
                }
            }
        }

        SkillAction::Run {
            name,
            company,
            prompt,
        } => {
            let project_cfg = config
                .company(&company)
                .context(format!("company not found: {company}"))?;
            let project_dir = find_project_dir(&company)?;
            let skills = discover_project_skills(&project_dir)?;

            let skill = skills
                .iter()
                .find(|s| s.skill.name == name)
                .context(format!("skill not found: {name}"))?;

            // Build provider.
            let provider = build_provider_for_project(&config, &company)?;
            let workdir = PathBuf::from(&project_cfg.repo);
            let tasks_dir = project_dir.join(".tasks");
            let worktree_root = project_cfg.worktree_root.as_ref().map(PathBuf::from);
            let all_tools = build_project_tools(
                &workdir,
                &tasks_dir,
                &project_cfg.prefix,
                worktree_root.as_ref(),
            );

            // Filter tools by skill policy.
            let filtered_tools: Vec<Arc<dyn Tool>> = all_tools
                .into_iter()
                .filter(|t| skill.is_tool_allowed(t.name()))
                .collect();

            // Build system prompt with skill overlay.
            let execution_agent = one_shot_agent_name(&config, Some(&company));
            let base_prompt = find_agent_dir(&execution_agent)
                .ok()
                .map(|agent_dir| load_system_prompt(&agent_dir, Some(&project_dir)))
                .unwrap_or_else(|| load_system_prompt_from_dir(&project_dir));
            let base_prompt = augment_prompt_with_org_context(&config, &base_prompt);
            let final_prompt = skill.system_prompt(&base_prompt);

            let user_prompt = if let Some(ref p) = prompt {
                format!("{}{}", skill.prompt.user_prefix, p)
            } else {
                skill.prompt.user_prefix.clone()
            };

            let observer: Arc<dyn Observer> = Arc::new(LogObserver);
            let model = config.model_for_company(&company);

            let agent_config = AgentConfig {
                model,
                max_iterations: 10,
                name: format!("{}-skill-{}", company, name),
                ..Default::default()
            };

            let mut agent = Agent::new(
                agent_config,
                provider,
                filtered_tools,
                observer,
                final_prompt,
            );
            if let Ok(mem) = open_insights(&config) {
                agent = agent.with_memory(Arc::new(mem));
            }
            let result = agent.run(&user_prompt).await?;
            println!("{}", result.text);
        }
    }
    Ok(())
}
