use anyhow::{Context, Result};
use sigil_core::traits::{Provider, Tool};
use sigil_core::{Identity, ProviderKind, SecretStore, SigilConfig};

/// Resolve `${ENV_VAR}` patterns in a config value. Returns empty string if
/// the value is a `${...}` pattern and the env var is not set.
fn resolve_env_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("${") && trimmed.ends_with('}') {
        let var_name = &trimmed[2..trimmed.len() - 1];
        std::env::var(var_name).unwrap_or_default()
    } else {
        trimmed.to_string()
    }
}
use sigil_memory::SqliteMemory;
use sigil_orchestrator::ProjectRegistry;
use sigil_providers::{AnthropicProvider, OllamaProvider, OpenRouterEmbedder, OpenRouterProvider};
use sigil_tasks::TaskBoard;
use sigil_tools::{
    ExecutePlanTool, FileEditTool, FileReadTool, FileWriteTool, GitWorktreeTool, GlobTool,
    GrepTool, ListDirTool, PorkbunTool, SecretsTool, ShellTool, TaskCloseTool, TaskCreateTool,
    TaskDepTool, TaskReadyTool, TaskShowTool, TaskUpdateTool,
};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
    resolve_web_paths(&mut config, &path);
    let agents_dir = resolve_agents_dir(&path);
    let warnings = config.discover_and_merge_agents(&agents_dir);
    for w in &warnings {
        warn!("{w}");
    }
    Ok((config, path))
}

fn resolve_web_paths(config: &mut SigilConfig, config_path: &Path) {
    let Some(ui_dist_dir) = config.web.ui_dist_dir.as_mut() else {
        return;
    };

    let path = PathBuf::from(ui_dist_dir.as_str());
    if path.is_absolute() {
        return;
    }

    if let Some(parent) = config_path.parent() {
        *ui_dist_dir = parent.join(path).to_string_lossy().into_owned();
    }
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
    // Resolve ${ENV_VAR} patterns, then fall back to SecretStore.
    let key = resolve_env_value(&or_config.api_key);
    if !key.is_empty() {
        return Ok(key);
    }
    let store_path = provider_secret_store_path(config);
    let store = SecretStore::open(&store_path)?;
    store
        .get("OPENROUTER_API_KEY")
        .context("OPENROUTER_API_KEY not set. Use `sigil secrets set OPENROUTER_API_KEY <key>`")
}

pub(crate) fn provider_secret_store_path(config: &SigilConfig) -> PathBuf {
    config
        .security
        .secret_store
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.data_dir().join("secrets"))
}

fn get_anthropic_api_key(config: &SigilConfig) -> Result<String> {
    let anthropic = config
        .providers
        .anthropic
        .as_ref()
        .context("no Anthropic provider configured")?;
    let key = resolve_env_value(&anthropic.api_key);
    if !key.is_empty() {
        return Ok(key);
    }
    let store = SecretStore::open(&provider_secret_store_path(config))?;
    store
        .get("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY not set. Use `sigil secrets set ANTHROPIC_API_KEY <key>`")
}

pub(crate) fn build_provider_for_runtime(
    config: &SigilConfig,
    provider_kind: ProviderKind,
    model_override: Option<&str>,
) -> Result<Arc<dyn Provider>> {
    let model = model_override
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| config.default_model_for_provider(provider_kind));

    match provider_kind {
        ProviderKind::OpenRouter => {
            let api_key = get_api_key(config)?;
            Ok(Arc::new(OpenRouterProvider::new(api_key, model)))
        }
        ProviderKind::Anthropic => {
            let api_key = get_anthropic_api_key(config)?;
            Ok(Arc::new(AnthropicProvider::new(api_key, model)))
        }
        ProviderKind::Ollama => {
            let ollama = config.providers.ollama.as_ref();
            let url = ollama
                .map(|cfg| cfg.url.clone())
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            Ok(Arc::new(OllamaProvider::new(url, model)))
        }
    }
}

pub(crate) fn one_shot_agent_name(config: &SigilConfig, project_name: Option<&str>) -> String {
    project_name
        .map(|name| config.project_team(name).leader)
        .unwrap_or_else(|| {
            config
                .leader_agent()
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| config.leader().to_string())
        })
}

pub(crate) fn build_provider_for_one_shot(
    config: &SigilConfig,
    project_name: Option<&str>,
) -> Result<Arc<dyn Provider>> {
    if let Some(project_name) = project_name {
        build_provider_for_project(config, project_name)
    } else {
        let agent_name = one_shot_agent_name(config, None);
        build_provider_for_agent(config, &agent_name)
    }
}

pub(crate) fn build_provider_for_project(
    config: &SigilConfig,
    project_name: &str,
) -> Result<Arc<dyn Provider>> {
    let runtime = config.runtime_for_project(project_name);
    let model = config.model_for_project(project_name);
    build_provider_for_runtime(config, runtime.provider, Some(&model))
}

pub(crate) fn build_provider_for_agent(
    config: &SigilConfig,
    agent_name: &str,
) -> Result<Arc<dyn Provider>> {
    let runtime = config.runtime_for_agent(agent_name);
    let model = config.model_for_agent(agent_name);
    build_provider_for_runtime(config, runtime.provider, Some(&model))
}

pub(crate) fn build_tools(workdir: &Path) -> Vec<Arc<dyn Tool>> {
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ShellTool::new(workdir.to_path_buf())),
        Arc::new(FileReadTool::new(workdir.to_path_buf())),
        Arc::new(FileWriteTool::new(workdir.to_path_buf())),
        Arc::new(FileEditTool::new(workdir.to_path_buf())),
        Arc::new(ListDirTool::new(workdir.to_path_buf())),
        Arc::new(GrepTool::new(workdir.to_path_buf())),
        Arc::new(GlobTool::new(workdir.to_path_buf())),
    ];

    // Execute plan — batch multiple tool calls in one turn (context compression).
    tools.push(Arc::new(ExecutePlanTool::new(tools.clone())));

    // Secrets management — encrypted credential store.
    let secrets_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".sigil")
        .join("secrets");
    tools.push(Arc::new(SecretsTool::new(secrets_path)));

    tools
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
        Arc::new(FileEditTool::new(workdir.to_path_buf())),
        Arc::new(ListDirTool::new(workdir.to_path_buf())),
        Arc::new(GrepTool::new(workdir.to_path_buf())),
        Arc::new(GlobTool::new(workdir.to_path_buf())),
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

    let has_key = api_key.is_some();
    if let (Some(key), Some(model)) = (api_key, embedding_model) {
        tracing::info!(model = %model, "memory embedder initialized");
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
        if !has_key {
            tracing::warn!("memory initialized WITHOUT embeddings (no API key)");
        } else {
            tracing::warn!("memory initialized WITHOUT embeddings (no embedding model configured)");
        }
        Ok(mem)
    }
}

pub(crate) fn format_project_org_hint(_config: &SigilConfig, _project_name: &str) -> String {
    String::new()
}

pub(crate) fn format_agent_org_hint(_config: &SigilConfig, _agent_name: &str) -> String {
    String::new()
}

pub(crate) fn augment_identity_with_org_context(
    config: &SigilConfig,
    mut identity: Identity,
    _agent_name: Option<&str>,
    project_name: Option<&str>,
) -> Identity {
    if let Some(project_name) = project_name {
        let team = config.project_team(project_name);
        let mut lines = vec![format!("Project team leader: {}", team.leader)];
        lines.push(format!(
            "Project team agents: {}",
            team.effective_agents().join(", ")
        ));
        let section = format!("# Project Team Context\n\n{}", lines.join("\n"));
        let existing = identity.operational.unwrap_or_default();
        identity.operational = Some(if existing.is_empty() {
            section
        } else {
            format!("{existing}\n\n---\n\n{section}")
        });
    }

    identity
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

pub(crate) fn pid_file_path(config: &SigilConfig) -> PathBuf {
    config.data_dir().join("rm.pid")
}

pub(crate) async fn daemon_ipc_request(
    config_path: &Option<PathBuf>,
    request: &serde_json::Value,
) -> Result<serde_json::Value> {
    let (config, _) = load_config(config_path)?;
    let socket_path = config.data_dir().join("rm.sock");

    if !socket_path.exists() {
        anyhow::bail!(
            "IPC socket not found: {}. Is the daemon running?",
            socket_path.display()
        );
    }

    #[cfg(unix)]
    {
        let stream = tokio::net::UnixStream::connect(&socket_path)
            .await
            .with_context(|| {
                format!("failed to connect to IPC socket: {}", socket_path.display())
            })?;

        let (reader, mut writer) = stream.into_split();
        let mut req_bytes = serde_json::to_vec(request)?;
        req_bytes.push(b'\n');
        writer.write_all(&req_bytes).await?;

        let mut lines = BufReader::new(reader).lines();
        let Some(line) = lines.next_line().await? else {
            anyhow::bail!(
                "IPC socket closed without a response: {}",
                socket_path.display()
            );
        };

        let response: serde_json::Value = serde_json::from_str(&line)?;
        Ok(response)
    }
    #[cfg(not(unix))]
    {
        let _ = request;
        anyhow::bail!("IPC socket queries not supported on this platform");
    }
}
