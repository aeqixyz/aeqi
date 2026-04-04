use aeqi_core::traits::{Insight, LogObserver, Observer};
use aeqi_core::{Agent, AgentConfig};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

use crate::helpers::{
    augment_prompt_with_org_context, build_project_tools, build_provider_for_one_shot, build_tools,
    find_agent_dir, find_project_dir, load_config, load_system_prompt, load_system_prompt_from_dir,
    one_shot_agent_name, open_insights,
};

pub(crate) async fn cmd_run(
    config_path: &Option<PathBuf>,
    prompt: &str,
    project_name: Option<&str>,
    model_override: Option<&str>,
    max_iterations: u32,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let execution_agent = one_shot_agent_name(&config, project_name);

    let model = model_override
        .map(String::from)
        .or_else(|| project_name.map(|r| config.model_for_company(r)))
        .unwrap_or_else(|| config.model_for_agent(&execution_agent));

    let provider = build_provider_for_one_shot(&config, project_name)?;
    let workdir = project_name
        .and_then(|r| config.company(r))
        .map(|r| PathBuf::from(&r.repo))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let tools = if let Some(rn) = project_name {
        let project_dir = find_project_dir(rn)?;
        let tasks_dir = project_dir.join(".tasks");
        let prefix = config
            .company(rn)
            .map(|r| r.prefix.as_str())
            .unwrap_or("sg");
        let worktree_root = config
            .company(rn)
            .and_then(|r| r.worktree_root.as_ref())
            .map(PathBuf::from);
        build_project_tools(&workdir, &tasks_dir, prefix, worktree_root.as_ref())
    } else {
        build_tools(&workdir)
    };
    // Load system prompt from agent identity files + optional project context.
    let system_prompt = if let Some(rn) = project_name {
        let project_dir = find_project_dir(rn).ok();
        let agent_dir = find_agent_dir(&execution_agent).ok();
        match (agent_dir, project_dir) {
            (Some(a), Some(d)) => load_system_prompt(&a, Some(&d)),
            (Some(a), None) => load_system_prompt(&a, None),
            (None, Some(d)) => load_system_prompt_from_dir(&d),
            (None, None) => "You are a helpful AI agent.".to_string(),
        }
    } else {
        find_agent_dir(&execution_agent)
            .ok()
            .map(|d| load_system_prompt(&d, None))
            .unwrap_or_else(|| "You are a helpful AI agent.".to_string())
    };
    let system_prompt = augment_prompt_with_org_context(&config, &system_prompt);
    let observer: Arc<dyn Observer> = Arc::new(LogObserver);

    let agent_config = AgentConfig {
        model,
        max_iterations,
        name: project_name.unwrap_or("default").to_string(),
        ..Default::default()
    };

    let memory: Option<Arc<dyn Insight>> = match open_insights(&config) {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            warn!("memory unavailable: {e}");
            None
        }
    };

    info!(prompt = %prompt, "starting agent");
    let mut agent = Agent::new(agent_config, provider, tools, observer, system_prompt);
    if let Some(mem) = memory {
        agent = agent.with_memory(mem);
    }
    let result = agent.run(prompt).await?;
    println!("{}", result.text);
    Ok(())
}
