use anyhow::Result;
use sigil_core::traits::{LogObserver, Memory, Observer};
use sigil_core::{Agent, AgentConfig, Identity};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

use crate::helpers::{
    augment_identity_with_org_context, build_project_tools, build_provider_for_one_shot,
    build_tools, find_agent_dir, find_project_dir, load_config, one_shot_agent_name, open_memory,
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
        .or_else(|| project_name.map(|r| config.model_for_project(r)))
        .unwrap_or_else(|| config.model_for_agent(&execution_agent));

    let provider = build_provider_for_one_shot(&config, project_name)?;
    let workdir = project_name
        .and_then(|r| config.project(r))
        .map(|r| PathBuf::from(&r.repo))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let tools = if let Some(rn) = project_name {
        let project_dir = find_project_dir(rn)?;
        let tasks_dir = project_dir.join(".tasks");
        let prefix = config
            .project(rn)
            .map(|r| r.prefix.as_str())
            .unwrap_or("sg");
        let worktree_root = config
            .project(rn)
            .and_then(|r| r.worktree_root.as_ref())
            .map(PathBuf::from);
        build_project_tools(&workdir, &tasks_dir, prefix, worktree_root.as_ref())
    } else {
        build_tools(&workdir)
    };
    // Load agent identity (from agents/) + optional project context.
    let identity = if let Some(rn) = project_name {
        let project_dir = find_project_dir(rn).ok();
        let agent_dir = find_agent_dir(&execution_agent).ok();
        match (agent_dir, project_dir) {
            (Some(a), Some(d)) => Identity::load(&a, Some(&d)).unwrap_or_default(),
            (Some(a), None) => Identity::load(&a, None).unwrap_or_default(),
            (None, Some(d)) => Identity::load_from_dir(&d).unwrap_or_default(),
            (None, None) => Identity::default(),
        }
    } else {
        find_agent_dir(&execution_agent)
            .ok()
            .map(|d| Identity::load(&d, None).unwrap_or_default())
            .unwrap_or_default()
    };
    let identity =
        augment_identity_with_org_context(&config, identity, Some(&execution_agent), project_name);
    let observer: Arc<dyn Observer> = Arc::new(LogObserver);

    let agent_config = AgentConfig {
        model,
        max_iterations,
        name: project_name.unwrap_or("default").to_string(),
        ..Default::default()
    };

    let memory: Option<Arc<dyn Memory>> = match open_memory(&config, project_name) {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            warn!("memory unavailable: {e}");
            None
        }
    };

    info!(prompt = %prompt, "starting agent");
    let mut agent = Agent::new(agent_config, provider, tools, observer, identity);
    if let Some(mem) = memory {
        agent = agent.with_memory(mem);
    }
    let result = agent.run(prompt).await?;
    println!("{}", result.text);
    Ok(())
}
