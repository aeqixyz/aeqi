mod cli;
mod cmd;
mod helpers;

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
    command: Commands,
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
        Commands::Run {
            prompt,
            project,
            model,
            max_iterations,
        } => {
            cmd::run::cmd_run(
                &cli.config,
                &prompt,
                project.as_deref(),
                model.as_deref(),
                max_iterations,
            )
            .await
        }
        Commands::Init => cmd::init::cmd_init().await,
        Commands::Secrets { action } => cmd::secrets::cmd_secrets(&cli.config, action).await,
        Commands::Doctor { fix, strict } => cmd::doctor::cmd_doctor(&cli.config, fix, strict).await,
        Commands::Status => cmd::status::cmd_status(&cli.config).await,
        Commands::Assign {
            subject,
            project,
            description,
            priority,
            mission,
        } => {
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
        Commands::Ready { project } => cmd::tasks::cmd_ready(&cli.config, project.as_deref()).await,
        Commands::Tasks { project, all } => {
            cmd::tasks::cmd_tasks(&cli.config, project.as_deref(), all).await
        }
        Commands::Close { id, reason } => cmd::tasks::cmd_close(&cli.config, &id, &reason).await,
        Commands::Daemon { action } => cmd::daemon::cmd_daemon(&cli.config, action).await,
        Commands::Recall {
            query,
            project,
            top_k,
        } => cmd::memory::cmd_recall(&cli.config, &query, project.as_deref(), top_k).await,
        Commands::Remember {
            key,
            content,
            project,
        } => cmd::memory::cmd_remember(&cli.config, &key, &content, project.as_deref()).await,
        Commands::Pipeline { action } => cmd::pipeline::cmd_pipeline(&cli.config, action).await,
        Commands::Cron { action } => cmd::cron::cmd_cron(&cli.config, action).await,
        Commands::Skill { action } => cmd::skill::cmd_skill(&cli.config, action).await,
        Commands::Mission { action } => cmd::mission::cmd_mission(&cli.config, action).await,
        Commands::Operation { action } => cmd::operation::cmd_operation(&cli.config, action).await,
        Commands::Hook { worker, task_id } => {
            cmd::tasks::cmd_hook(&cli.config, &worker, &task_id).await
        }
        Commands::Done { task_id, reason } => {
            cmd::tasks::cmd_done(&cli.config, &task_id, &reason).await
        }
        Commands::Team { project } => cmd::team::cmd_team(&cli.config, project.as_deref()).await,
        Commands::Config { action } => cmd::config::cmd_config(&cli.config, action).await,
        Commands::Agent { action } => cmd::agent::cmd_agent(&cli.config, action).await,
        Commands::Audit {
            project,
            task,
            last,
        } => cmd::audit::cmd_audit(&cli.config, project.as_deref(), task.as_deref(), last).await,
        Commands::Blackboard { action } => {
            cmd::blackboard::cmd_blackboard(&cli.config, action).await
        }
        Commands::Deps { project, apply } => {
            cmd::deps::cmd_deps(&cli.config, &project, apply).await
        }
    }
}
