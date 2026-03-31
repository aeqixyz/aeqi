mod cli;
mod cmd;
mod helpers;
mod service;

use anyhow::Result;
use clap::Parser;
use cli::Commands;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sigil", version, about = "Sigil — Multi-Agent Orchestration")]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level)),
        )
        .with_target(false)
        .init();

    match cli.command {
        None => cmd::chat::cmd_chat(&cli.config).await,
        Some(Commands::Run {
            prompt,
            project,
            model,
            max_iterations,
        }) => {
            cmd::run::cmd_run(
                &cli.config,
                &prompt,
                project.as_deref(),
                model.as_deref(),
                max_iterations,
            )
            .await
        }
        Some(Commands::Init) => cmd::init::cmd_init().await,
        Some(Commands::Setup {
            runtime,
            service,
            force,
        }) => cmd::setup::cmd_setup(&runtime, service, force).await,
        Some(Commands::Secrets { action }) => cmd::secrets::cmd_secrets(&cli.config, action).await,
        Some(Commands::Doctor { fix, strict }) => {
            cmd::doctor::cmd_doctor(&cli.config, fix, strict).await
        }
        Some(Commands::Status) => cmd::status::cmd_status(&cli.config).await,
        Some(Commands::Monitor {
            project,
            watch,
            interval_secs,
            json,
        }) => {
            cmd::monitor::cmd_monitor(&cli.config, project.as_deref(), watch, interval_secs, json)
                .await
        }
        Some(Commands::Assign {
            subject,
            project,
            description,
            priority,
            mission,
        }) => {
            cmd::tasks::cmd_assign(
                &cli.config,
                &subject,
                &project,
                &description,
                priority.as_deref(),
                mission.as_deref(),
            )
            .await
        }
        Some(Commands::Ready { project }) => {
            cmd::tasks::cmd_ready(&cli.config, project.as_deref()).await
        }
        Some(Commands::Tasks { project, all }) => {
            cmd::tasks::cmd_tasks(&cli.config, project.as_deref(), all).await
        }
        Some(Commands::Close { id, reason }) => {
            cmd::tasks::cmd_close(&cli.config, &id, &reason).await
        }
        Some(Commands::Daemon { action }) => cmd::daemon::cmd_daemon(&cli.config, action).await,
        Some(Commands::Recall {
            query,
            project,
            top_k,
        }) => cmd::memory::cmd_recall(&cli.config, &query, project.as_deref(), top_k).await,
        Some(Commands::Remember {
            key,
            content,
            project,
        }) => cmd::memory::cmd_remember(&cli.config, &key, &content, project.as_deref()).await,
        Some(Commands::Pipeline { action }) => {
            cmd::pipeline::cmd_pipeline(&cli.config, action).await
        }
        Some(Commands::Cron { action }) => cmd::cron::cmd_cron(&cli.config, action).await,
        Some(Commands::Skill { action }) => cmd::skill::cmd_skill(&cli.config, action).await,
        Some(Commands::Mission { action }) => cmd::mission::cmd_mission(&cli.config, action).await,
        Some(Commands::Operation { action }) => {
            cmd::operation::cmd_operation(&cli.config, action).await
        }
        Some(Commands::Hooks { action }) => cmd::hooks::cmd_hooks(action).await,
        Some(Commands::Hook { worker, task_id }) => {
            cmd::tasks::cmd_hook(&cli.config, &worker, &task_id).await
        }
        Some(Commands::Done { task_id, reason }) => {
            cmd::tasks::cmd_done(&cli.config, &task_id, &reason).await
        }
        Some(Commands::Team { project }) => {
            cmd::team::cmd_team(&cli.config, project.as_deref()).await
        }
        Some(Commands::Config { action }) => cmd::config::cmd_config(&cli.config, action).await,
        Some(Commands::Agent { action }) => cmd::agent::cmd_agent(&cli.config, action).await,
        Some(Commands::Audit {
            project,
            task,
            last,
        }) => cmd::audit::cmd_audit(&cli.config, project.as_deref(), task.as_deref(), last).await,
        Some(Commands::Blackboard { action }) => {
            cmd::blackboard::cmd_blackboard(&cli.config, action).await
        }
        Some(Commands::Deps { project, apply }) => {
            cmd::deps::cmd_deps(&cli.config, &project, apply).await
        }
        Some(Commands::Web { action }) => cmd::web::cmd_web(&cli.config, action).await,
        Some(Commands::Graph { action }) => cmd::graph::cmd_graph(&cli.config, action).await,
        Some(Commands::Mcp) => cmd::mcp::cmd_mcp(&cli.config).map(|_| ()),
    }
}
