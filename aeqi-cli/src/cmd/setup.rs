use aeqi_core::{ExecutionMode, ProviderKind, RuntimePresetConfig};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::service::install_user_service;

pub(crate) async fn cmd_setup(runtime: &str, service: bool, force: bool) -> Result<()> {
    let starter = starter_runtime(runtime)
        .with_context(|| format!("unknown starter runtime preset: {runtime}"))?;
    let cwd = std::env::current_dir().context("failed to determine current directory")?;

    // Detect workspace: if CWD has config/, agents/, Cargo.toml, or .git, use
    // CWD as the workspace root. Otherwise default to ~/.aeqi/ so that
    // curl-install users get a working setup without creating a directory first.
    let is_workspace = cwd.join("config").exists()
        || cwd.join("agents").exists()
        || cwd.join("Cargo.toml").exists()
        || cwd.join(".git").exists();

    let root = if is_workspace {
        cwd.clone()
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
        home.join(".aeqi")
    };

    let system_name = if is_workspace {
        cwd.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("aeqi-workspace")
            .to_string()
    } else {
        "aeqi".to_string()
    };

    let config_dir = if is_workspace {
        root.join("config")
    } else {
        root.clone()
    };
    let projects_dir = root.join("projects");
    let agents_dir = root.join("agents");
    let shared_agents_dir = agents_dir.join("shared");
    let config_path = config_dir.join("aeqi.toml");
    let data_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".aeqi");
    let secrets_dir = data_dir.join("secrets");

    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&projects_dir)?;
    std::fs::create_dir_all(&shared_agents_dir)?;
    std::fs::create_dir_all(&secrets_dir)?;

    let worker_runtime = agent_runtime_name(starter.provider);
    let default_model = starter
        .model
        .as_deref()
        .unwrap_or_else(|| default_model_for_provider(starter.provider));

    write_file(
        &config_path,
        &render_config(
            &system_name,
            runtime,
            worker_runtime,
            default_model,
            starter.provider,
        ),
        force,
    )?;

    let starter_files = [
        (
            agents_dir.join("leader/agent.toml"),
            render_agent_toml("leader", "ld", "orchestrator", "vocal", runtime),
        ),
        (
            agents_dir.join("leader/IDENTITY.md"),
            "# Leader\n\nYou are AEQI's primary orchestrator. Break ambiguous work into clear tasks, route specialists when needed, and keep the control plane legible.\n".to_string(),
        ),
        (
            agents_dir.join("leader/PERSONA.md"),
            "Coordinate aggressively but conservatively. Prefer explicit plans, visible checkpoints, and clean handoffs over improvisation.\n".to_string(),
        ),
        (
            agents_dir.join("researcher/agent.toml"),
            render_agent_toml("researcher", "rs", "advisor", "silent", worker_runtime),
        ),
        (
            agents_dir.join("researcher/IDENTITY.md"),
            "# Researcher\n\nYou gather missing context, compare alternatives, and turn uncertainty into actionable input for the rest of the harness.\n".to_string(),
        ),
        (
            agents_dir.join("researcher/PERSONA.md"),
            "Bias toward source-backed findings, explicit tradeoffs, and concise synthesis.\n".to_string(),
        ),
        (
            agents_dir.join("reviewer/agent.toml"),
            render_agent_toml("reviewer", "rv", "advisor", "silent", worker_runtime),
        ),
        (
            agents_dir.join("reviewer/IDENTITY.md"),
            "# Reviewer\n\nYou look for regressions, missing tests, and control-plane risks before work is accepted as complete.\n".to_string(),
        ),
        (
            agents_dir.join("reviewer/PERSONA.md"),
            "Default to bug-finding, edge cases, and operational safety. Keep feedback direct.\n".to_string(),
        ),
        (
            shared_agents_dir.join("WORKFLOW.md"),
            "# Shared Workflow\n\n1. Run `aeqi doctor --strict` before starting substantial work.\n2. Keep tasks small enough for a single worker handoff.\n3. Post durable discoveries to notes.\n4. Use checkpoints and audits to resume instead of restarting from scratch.\n".to_string(),
        ),
    ];

    for (path, contents) in starter_files {
        write_file(&path, &contents, force)?;
    }

    if service {
        match install_user_service(&config_path, false, force) {
            Ok((unit_path, warnings)) => {
                println!("Installed daemon service: {}", unit_path.display());
                for warning in warnings {
                    println!("  [WARN] {warning}");
                }
            }
            Err(e) => {
                println!("[WARN] Service install skipped: {e}");
            }
        }
    }

    println!("AEQI setup complete.");
    println!("Workspace: {}", root.display());
    println!("Config: {}", config_path.display());
    println!();
    println!("Next steps:");
    match starter.provider {
        ProviderKind::OpenRouter => {
            println!("  1. aeqi secrets set OPENROUTER_API_KEY <key>");
        }
        ProviderKind::Anthropic => {
            println!("  1. aeqi secrets set ANTHROPIC_API_KEY <key>");
        }
        ProviderKind::Ollama => {
            println!("  1. Ensure Ollama is running and the configured model is pulled");
        }
    }
    println!("  2. aeqi doctor --strict");
    println!("  3. aeqi start");

    Ok(())
}

fn write_file(path: &Path, contents: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        println!("Preserved {}", path.display());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, contents)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn render_agent_toml(name: &str, prefix: &str, role: &str, voice: &str, runtime: &str) -> String {
    format!(
        "name = \"{name}\"\n\
prefix = \"{prefix}\"\n\
role = \"{role}\"\n\
voice = \"{voice}\"\n\
runtime = \"{runtime}\"\n\
max_workers = 1\n"
    )
}

fn render_config(
    system_name: &str,
    runtime: &str,
    worker_runtime: &str,
    default_model: &str,
    provider: ProviderKind,
) -> String {
    format!(
        "[aeqi]\n\
name = \"{system_name}\"\n\
data_dir = \"~/.aeqi\"\n\
default_runtime = \"{runtime}\"\n\
\n\
{}\n\
[security]\n\
autonomy = \"supervised\"\n\
workspace_only = true\n\
max_cost_per_day_usd = 25.0\n\
\n\
[memory]\n\
backend = \"sqlite\"\n\
temporal_decay_halflife_days = 30\n\
\n\
[team]\n\
leader = \"leader\"\n\
router_cooldown_secs = 60\n\
max_background_cost_usd = 0.5\n\
\n\
[[organizations]]\n\
name = \"core\"\n\
kind = \"workspace\"\n\
default = true\n\
mission = \"Maintain the workspace, keep execution moving, and preserve operator trust.\"\n\
\n\
[[organizations.units]]\n\
name = \"control-plane\"\n\
kind = \"core\"\n\
mission = \"Coordinate work, gather evidence, and verify delivery.\"\n\
lead = \"leader\"\n\
members = [\"researcher\", \"reviewer\"]\n\
\n\
[[organizations.roles]]\n\
agent = \"leader\"\n\
title = \"Orchestrator\"\n\
unit = \"control-plane\"\n\
mandate = \"Break work down, route specialists, and keep the system legible.\"\n\
goals = [\"Keep work moving\", \"Protect operator trust\"]\n\
permissions = [\"delegate\", \"approve\", \"escalate\"]\n\
\n\
[[organizations.roles]]\n\
agent = \"researcher\"\n\
title = \"Research Lead\"\n\
unit = \"control-plane\"\n\
mandate = \"Turn ambiguity into evidence and options.\"\n\
permissions = [\"research\", \"brief\"]\n\
\n\
[[organizations.roles]]\n\
agent = \"reviewer\"\n\
title = \"Quality Lead\"\n\
unit = \"control-plane\"\n\
mandate = \"Catch regressions and verify completion.\"\n\
permissions = [\"review\", \"block\"]\n\
\n\
[[organizations.relationships]]\n\
from = \"leader\"\n\
to = \"researcher\"\n\
kind = \"delegates_to\"\n\
\n\
[[organizations.relationships]]\n\
from = \"leader\"\n\
to = \"reviewer\"\n\
kind = \"delegates_to\"\n\
\n\
[[organizations.relationships]]\n\
from = \"reviewer\"\n\
to = \"leader\"\n\
kind = \"advises\"\n\
\n\
[[organizations.rituals]]\n\
name = \"Daily Ops Review\"\n\
owner = \"leader\"\n\
cadence = \"daily\"\n\
participants = [\"researcher\", \"reviewer\"]\n\
purpose = \"Review readiness, open work, and blocked tasks.\"\n\
\n\
[orchestrator]\n\
background_automation_enabled = true\n\
expertise_routing = true\n\
adaptive_retry = true\n\
failure_analysis_model = \"{default_model}\"\n\
infer_deps_threshold = 0.85\n\
dispatch_ttl_secs = 3600\n\
\n\
[[agents]]\n\
name = \"leader\"\n\
prefix = \"ld\"\n\
role = \"orchestrator\"\n\
voice = \"vocal\"\n\
runtime = \"{runtime}\"\n\
max_workers = 1\n\
\n\
[[agents]]\n\
name = \"researcher\"\n\
prefix = \"rs\"\n\
role = \"advisor\"\n\
voice = \"silent\"\n\
runtime = \"{worker_runtime}\"\n\
max_workers = 1\n\
\n\
[[agents]]\n\
name = \"reviewer\"\n\
prefix = \"rv\"\n\
role = \"advisor\"\n\
voice = \"silent\"\n\
runtime = \"{worker_runtime}\"\n\
max_workers = 1\n\
\n\
# Add projects below. Runtime can be overridden per project.\n\
# [[projects]]\n\
# name = \"aeqi\"\n\
# prefix = \"sg\"\n\
# repo = \"/absolute/path/to/repo\"\n\
# team.leader = \"leader\"\n\
# runtime = \"{runtime}\"\n",
        render_provider_block(provider, default_model),
    )
}

fn render_provider_block(provider: ProviderKind, default_model: &str) -> String {
    match provider {
        ProviderKind::OpenRouter => format!(
            "[providers.openrouter]\n\
api_key = \"${{OPENROUTER_API_KEY}}\"\n\
default_model = \"{default_model}\"\n\
embedding_model = \"openai/text-embedding-3-small\"\n\
\n"
        ),
        ProviderKind::Anthropic => format!(
            "[providers.anthropic]\n\
api_key = \"${{ANTHROPIC_API_KEY}}\"\n\
default_model = \"{default_model}\"\n\
\n"
        ),
        ProviderKind::Ollama => format!(
            "[providers.ollama]\n\
url = \"http://localhost:11434\"\n\
default_model = \"{default_model}\"\n\
\n"
        ),
    }
}

fn starter_runtime(name: &str) -> Result<RuntimePresetConfig> {
    match name {
        "openrouter_agent" => Ok(RuntimePresetConfig {
            provider: ProviderKind::OpenRouter,
            execution_mode: Some(ExecutionMode::Agent),
            model: Some("xiaomi/mimo-v2-pro".to_string()),
        }),
        "anthropic_agent" => Ok(RuntimePresetConfig {
            provider: ProviderKind::Anthropic,
            execution_mode: Some(ExecutionMode::Agent),
            model: Some("claude-sonnet-4-20250514".to_string()),
        }),
        "ollama_agent" => Ok(RuntimePresetConfig {
            provider: ProviderKind::Ollama,
            execution_mode: Some(ExecutionMode::Agent),
            model: Some("llama3.1:8b".to_string()),
        }),
        // Legacy aliases retained so older invocations keep working.
        "openrouter_claude_code" => Ok(RuntimePresetConfig {
            provider: ProviderKind::OpenRouter,
            execution_mode: Some(ExecutionMode::Agent),
            model: Some("xiaomi/mimo-v2-pro".to_string()),
        }),
        "anthropic_claude_code" => Ok(RuntimePresetConfig {
            provider: ProviderKind::Anthropic,
            execution_mode: Some(ExecutionMode::Agent),
            model: Some("claude-sonnet-4-20250514".to_string()),
        }),
        "ollama_claude_code" => Ok(RuntimePresetConfig {
            provider: ProviderKind::Ollama,
            execution_mode: Some(ExecutionMode::Agent),
            model: Some("llama3.1:8b".to_string()),
        }),
        _ => anyhow::bail!(
            "supported starter runtimes: openrouter_agent, anthropic_agent, ollama_agent"
        ),
    }
}

fn agent_runtime_name(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::OpenRouter => "openrouter_agent",
        ProviderKind::Anthropic => "anthropic_agent",
        ProviderKind::Ollama => "ollama_agent",
    }
}

fn default_model_for_provider(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::OpenRouter => "xiaomi/mimo-v2-pro",
        ProviderKind::Anthropic => "claude-sonnet-4-20250514",
        ProviderKind::Ollama => "llama3.1:8b",
    }
}
