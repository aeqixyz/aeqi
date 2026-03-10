use anyhow::{Context, Result};
use sigil_core::traits::{Provider, Tool};
use sigil_core::{AgentRole, SecretStore, SigilConfig};
use sigil_memory::SqliteMemory;
use sigil_orchestrator::ProjectRegistry;
use sigil_providers::{OpenRouterEmbedder, OpenRouterProvider};
use sigil_tasks::TaskBoard;
use sigil_tools::{
    FileReadTool, FileWriteTool, GitWorktreeTool, ListDirTool, PorkbunTool, ShellTool,
    TaskCloseTool, TaskCreateTool, TaskDepTool, TaskReadyTool, TaskShowTool, TaskUpdateTool,
};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::warn;

pub(crate) fn load_config(config_path: &Option<PathBuf>) -> Result<(SigilConfig, PathBuf)> {
    if let Some(path) = config_path {
        Ok((SigilConfig::load(path)?, path.clone()))
    } else {
        SigilConfig::discover()
    }
}

/// Load config and discover agents from disk, merging with any `[[agents]]` in TOML.
pub(crate) fn load_config_with_agents(
    config_path: &Option<PathBuf>,
) -> Result<(SigilConfig, PathBuf)> {
    let (mut config, path) = load_config(config_path)?;
    let agents_dir = resolve_agents_dir(&path);
    let warnings = config.discover_and_merge_agents(&agents_dir);
    for w in &warnings {
        warn!("{w}");
    }
    Ok((config, path))
}

pub(crate) fn find_project_dir(name: &str) -> Result<PathBuf> {
    let candidates = [
        PathBuf::from(format!("projects/{name}")),
        PathBuf::from(format!("../projects/{name}")),
    ];
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join("projects").join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }
    anyhow::bail!("project directory not found: {name}")
}

pub(crate) fn find_agent_dir(name: &str) -> Result<PathBuf> {
    let candidates = [
        PathBuf::from(format!("agents/{name}")),
        PathBuf::from(format!("../agents/{name}")),
    ];
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join("agents").join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }
    anyhow::bail!("agent directory not found: {name}")
}

pub(crate) fn get_api_key(config: &SigilConfig) -> Result<String> {
    let or_config = config
        .providers
        .openrouter
        .as_ref()
        .context("no OpenRouter provider configured")?;
    if !or_config.api_key.is_empty() {
        return Ok(or_config.api_key.clone());
    }
    let store_path = config
        .security
        .secret_store
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.data_dir().join("secrets"));
    let store = SecretStore::open(&store_path)?;
    store
        .get("OPENROUTER_API_KEY")
        .context("OPENROUTER_API_KEY not set. Use `sigil secrets set OPENROUTER_API_KEY <key>`")
}

pub(crate) fn build_provider(config: &SigilConfig) -> Result<Arc<dyn Provider>> {
    let api_key = get_api_key(config)?;
    let model = config
        .providers
        .openrouter
        .as_ref()
        .map(|or| or.default_model.clone())
        .unwrap_or_else(|| "minimax/minimax-m2.5".to_string());
    Ok(Arc::new(OpenRouterProvider::new(api_key, model)))
}

pub(crate) fn build_tools(workdir: &Path) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ShellTool::new(workdir.to_path_buf())),
        Arc::new(FileReadTool::new(workdir.to_path_buf())),
        Arc::new(FileWriteTool::new(workdir.to_path_buf())),
        Arc::new(ListDirTool::new(workdir.to_path_buf())),
    ]
}

/// Build the full tool set for a project: basic tools + tasks + git worktree.
pub(crate) fn build_project_tools(
    workdir: &Path,
    tasks_dir: &Path,
    prefix: &str,
    worktree_root: Option<&PathBuf>,
) -> Vec<Arc<dyn Tool>> {
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ShellTool::new(workdir.to_path_buf())),
        Arc::new(FileReadTool::new(workdir.to_path_buf())),
        Arc::new(FileWriteTool::new(workdir.to_path_buf())),
        Arc::new(ListDirTool::new(workdir.to_path_buf())),
    ];

    // Add task tools (each gets its own store instance).
    if let Ok(t) = TaskCreateTool::new(tasks_dir.to_path_buf(), prefix.to_string()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = TaskReadyTool::new(tasks_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = TaskUpdateTool::new(tasks_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = TaskCloseTool::new(tasks_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = TaskShowTool::new(tasks_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = TaskDepTool::new(tasks_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }

    // Add git worktree tool.
    let wt_root = worktree_root
        .cloned()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("worktrees"));
    tools.push(Arc::new(GitWorktreeTool::new(
        workdir.to_path_buf(),
        wt_root,
    )));

    // Add Porkbun domain tool if credentials are available.
    if let Some(porkbun) = PorkbunTool::from_env() {
        tools.push(Arc::new(porkbun));
    }

    tools
}

/// Look up project name from a task prefix (e.g. "as" -> "test-project").
pub(crate) fn project_name_for_prefix(config: &SigilConfig, prefix: &str) -> Option<String> {
    // Check agent prefixes.
    for agent in &config.agents {
        if agent.prefix == prefix {
            return Some(agent.name.clone());
        }
    }
    config
        .projects
        .iter()
        .find(|r| r.prefix == prefix)
        .map(|r| r.name.clone())
}

pub(crate) fn open_tasks_for_project(project_name: &str) -> Result<TaskBoard> {
    let owner_dir = find_project_dir(project_name).or_else(|_| find_agent_dir(project_name))?;
    let tasks_dir = owner_dir.join(".tasks");
    TaskBoard::open(&tasks_dir)
}

pub(crate) fn open_memory(
    config: &SigilConfig,
    project_name: Option<&str>,
) -> Result<SqliteMemory> {
    let db_path = if let Some(name) = project_name {
        let project_dir = find_project_dir(name)?;
        project_dir.join(".sigil").join("memory.db")
    } else {
        config.data_dir().join("memory.db")
    };
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let halflife = config.memory.temporal_decay_halflife_days;
    let mem = SqliteMemory::open(&db_path, halflife)?;

    let api_key = get_api_key(config).ok();
    let embedding_model = config
        .providers
        .openrouter
        .as_ref()
        .and_then(|or| or.embedding_model.clone());

    if let (Some(key), Some(model)) = (api_key, embedding_model) {
        let embedder = Arc::new(OpenRouterEmbedder::new(
            key,
            model,
            config.memory.embedding_dimensions,
        ));
        mem.with_embedder(
            embedder,
            config.memory.embedding_dimensions,
            config.memory.vector_weight,
            config.memory.keyword_weight,
            config.memory.mmr_lambda,
        )
    } else {
        Ok(mem)
    }
}

pub(crate) async fn handle_fast_lane(text: &str, reg: &Arc<ProjectRegistry>) -> String {
    let cmd = text.split_whitespace().next().unwrap_or("");
    match cmd {
        "/status" => {
            let projects = reg.project_names().await;
            if projects.is_empty() {
                return "No projects registered.".to_string();
            }
            let mut lines = vec!["*Project Status*\n".to_string()];
            for d in &projects {
                lines.push(format!("  {} — active", d));
            }
            lines.join("\n")
        }
        "/help" => "*Available Commands*\n\n\
             /status — Project status\n\
             /cost — Today's spend\n\
             /help — This message"
            .to_string(),
        "/cost" => {
            "Cost tracking: use `/cost` here or `sigil daemon query cost` from CLI.".to_string()
        }
        _ => format!("Unknown fast-lane command: {cmd}"),
    }
}

/// Resolve the agents/ directory relative to config file path.
pub(crate) fn resolve_agents_dir(config_path: &Path) -> PathBuf {
    // Config is typically at config/sigil.toml, so agents/ is at config/../agents
    if let Some(parent) = config_path.parent() {
        let candidate = parent.join("../agents");
        if candidate.exists() {
            return candidate;
        }
        // Try parent's parent (if config is nested deeper)
        if let Some(grandparent) = parent.parent() {
            let candidate = grandparent.join("agents");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    // Fallback: look from cwd
    PathBuf::from("agents")
}

pub(crate) fn role_str(role: &AgentRole) -> &str {
    match role {
        AgentRole::Orchestrator => "orchestrator",
        AgentRole::Worker => "worker",
        AgentRole::Advisor => "advisor",
    }
}

pub(crate) fn pid_file_path(config: &SigilConfig) -> PathBuf {
    config.data_dir().join("rm.pid")
}
