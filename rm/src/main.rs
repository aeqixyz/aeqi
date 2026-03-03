use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use system_tasks::TaskBoard;
use system_core::traits::{Channel, LogObserver, Memory, Observer, Provider, Tool};
use system_core::{Agent, AgentConfig, ExecutionMode, Identity, SecretStore, SystemConfig};
use system_memory::SqliteMemory;
use system_gates::TelegramChannel;
use system_orchestrator::{OperationStore, ScheduledJob, ScheduleStore, Daemon, DispatchBus, Pipeline, Project, ProjectRegistry, Supervisor, AgentRouter, EmotionalState, ConversationStore, LifecycleEngine};
use system_orchestrator::schedule::CronSchedule;
use system_orchestrator::tools::build_orchestration_tools;
use system_providers::{OpenRouterEmbedder, OpenRouterProvider};
use system_tools::{
    BeadsCreateTool, BeadsReadyTool, BeadsUpdateTool, BeadsCloseTool, BeadsShowTool, BeadsDepTool,
    FileReadTool, FileWriteTool, GitWorktreeTool, ListDirTool, PorkbunTool, ShellTool, Skill,
};
use system_companions::{CompanionStore, GachaEngine, Rarity, fuse, Companion};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "rm", version, about = "Realm — Multi-Agent Orchestration")]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a one-shot agent with a prompt.
    Run {
        prompt: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        #[arg(short, long)]
        model: Option<String>,
        #[arg(long, default_value = "20")]
        max_iterations: u32,
    },
    /// Initialize Realm in the current directory.
    Init,
    /// Manage encrypted secrets.
    Secrets {
        #[command(subcommand)]
        action: SecretsAction,
    },
    /// Run diagnostics.
    Doctor {
        /// Auto-fix detected issues.
        #[arg(long)]
        fix: bool,
    },
    /// Show system status.
    Status,

    // --- Phase 2: Beads ---
    /// Assign a task to a project.
    Assign {
        subject: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        #[arg(short, long, default_value = "")]
        description: String,
        #[arg(short, long)]
        priority: Option<String>,
        /// Assign to a mission by ID.
        #[arg(short, long)]
        mission: Option<String>,
    },
    /// Show unblocked (ready) work.
    Ready {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
    },
    /// Show all open quests.
    Beads {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Close a quest.
    Close {
        id: String,
        #[arg(short, long, default_value = "completed")]
        reason: String,
    },

    // --- Phase 3: Orchestrator ---
    /// Manage the daemon.
    Daemon {
        #[command(subcommand)]
        action: SummonerAction,
    },

    /// Start the web server for gacha.agency.
    Serve {
        /// Path to platform.toml config.
        #[arg(short, long)]
        platform_config: PathBuf,
    },

    // --- Phase 4: Memory ---
    /// Search collective memory.
    Recall {
        query: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        #[arg(short, long, default_value = "5")]
        top_k: usize,
    },
    /// Store a memory.
    Remember {
        key: String,
        content: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
    },

    // --- Phase 5: Rituals ---
    /// Pipeline workflow commands.
    Mol {
        #[command(subcommand)]
        action: RitualAction,
    },

    // --- Phase 6: Cron ---
    /// Manage scheduled cron jobs.
    Cron {
        #[command(subcommand)]
        action: FateAction,
    },

    // --- Phase 7: Skills ---
    /// List or run skills.
    Skill {
        #[command(subcommand)]
        action: MagicAction,
    },

    // --- Missions ---
    /// Manage missions (task groups).
    Mission {
        #[command(subcommand)]
        action: MissionAction,
    },

    // --- Cross-project ---
    /// Track work across projects.
    Operation {
        #[command(subcommand)]
        action: RaidAction,
    },

    // --- Worker management ---
    /// Pin work to a worker.
    Hook {
        worker: String,
        task_id: String,
    },
    /// Mark quest as done, trigger cleanup.
    Done {
        task_id: String,
        #[arg(short, long, default_value = "completed")]
        reason: String,
    },

    /// Show system team and per-project teams.
    Team {
        /// Show team for a specific project.
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
    },

    // --- Config ---
    /// Reload configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Seed tenant projects from system.toml definitions.
    SeedProjects {
        /// Tenant UUID to seed projects into.
        #[arg(short, long)]
        tenant: String,
        /// Path to platform.toml config.
        #[arg(short, long)]
        platform_config: PathBuf,
    },
}

#[derive(Subcommand)]
enum SecretsAction {
    Set { name: String, value: String },
    Get { name: String },
    List,
    Delete { name: String },
}

#[derive(Subcommand)]
enum SummonerAction {
    /// Start the daemon (runs in foreground).
    Start,
    /// Stop a running daemon.
    Stop,
    /// Show daemon status.
    Status,
    /// Query the running daemon via IPC socket.
    Query {
        /// Command to send (ping, status, projects, whispers).
        cmd: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Reload configuration (send SIGHUP to daemon).
    Reload,
    /// Show current config.
    Show,
}

#[derive(Subcommand)]
enum FateAction {
    /// Add a scheduled job.
    Add {
        name: String,
        #[arg(short, long)]
        schedule: Option<String>,
        #[arg(long)]
        at: Option<String>,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        #[arg(short, long)]
        prompt: String,
        #[arg(long)]
        isolated: bool,
    },
    /// List all cron jobs.
    List,
    /// Remove a cron job.
    Remove { name: String },
}

#[derive(Subcommand)]
enum MagicAction {
    /// List available skills for a project.
    List {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
    },
    /// Run a skill by name.
    Run {
        name: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        /// Additional user prompt appended after the skill's user_prefix.
        prompt: Option<String>,
    },
}

#[derive(Subcommand)]
enum RaidAction {
    /// Create a raid tracking quests across projects.
    Create {
        name: String,
        /// Task IDs to track (e.g. as-001 rd-002).
        quest_ids: Vec<String>,
    },
    /// List active raids.
    List,
    /// Show raid status.
    Status { id: String },
}

#[derive(Subcommand)]
enum MissionAction {
    /// Create a new mission.
    Create {
        name: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        #[arg(short, long, default_value = "")]
        description: String,
    },
    /// List missions.
    List {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Show mission details and its tasks.
    Status {
        id: String,
    },
    /// Close a mission.
    Close {
        id: String,
    },
}

#[derive(Subcommand)]
enum RitualAction {
    /// Pour (instantiate) a ritual workflow.
    Pour {
        template: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        /// Variables as key=value pairs.
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// List available ritual templates.
    List {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
    },
    /// Show status of a ritual (parent bead and its children).
    Status {
        id: String,
    },
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
        Commands::Run { prompt, project, model, max_iterations } => {
            cmd_run(&cli.config, &prompt, project.as_deref(), model.as_deref(), max_iterations).await
        }
        Commands::Init => cmd_init().await,
        Commands::Secrets { action } => cmd_secrets(&cli.config, action).await,
        Commands::Doctor { fix } => cmd_doctor(&cli.config, fix).await,
        Commands::Status => cmd_status(&cli.config).await,
        Commands::Assign { subject, project, description, priority, mission } => {
            cmd_assign(&cli.config, &subject, &project, &description, priority.as_deref(), mission.as_deref()).await
        }
        Commands::Ready { project } => cmd_ready(&cli.config, project.as_deref()).await,
        Commands::Beads { project, all } => cmd_beads(&cli.config, project.as_deref(), all).await,
        Commands::Close { id, reason } => cmd_close(&cli.config, &id, &reason).await,
        Commands::Daemon { action } => cmd_daemon(&cli.config, action).await,
        Commands::Serve { platform_config } => cmd_serve(&platform_config).await,
        Commands::Recall { query, project, top_k } => {
            cmd_recall(&cli.config, &query, project.as_deref(), top_k).await
        }
        Commands::Remember { key, content, project } => {
            cmd_remember(&cli.config, &key, &content, project.as_deref()).await
        }
        Commands::Mol { action } => cmd_mol(&cli.config, action).await,
        Commands::Cron { action } => cmd_cron(&cli.config, action).await,
        Commands::Skill { action } => cmd_skill(&cli.config, action).await,
        Commands::Mission { action } => cmd_mission(&cli.config, action).await,
        Commands::Operation { action } => cmd_raid(&cli.config, action).await,
        Commands::Hook { worker, task_id } => cmd_hook(&cli.config, &worker, &task_id).await,
        Commands::Done { task_id, reason } => cmd_done(&cli.config, &task_id, &reason).await,
        Commands::Team { project } => cmd_team(&cli.config, project.as_deref()).await,
        Commands::Config { action } => cmd_config(&cli.config, action).await,
        Commands::SeedProjects { tenant, platform_config } => {
            cmd_seed_projects(&cli.config, &tenant, &platform_config).await
        }
    }
}

// === Helpers ===

fn load_config(config_path: &Option<PathBuf>) -> Result<(SystemConfig, PathBuf)> {
    if let Some(path) = config_path {
        Ok((SystemConfig::load(path)?, path.clone()))
    } else {
        SystemConfig::discover()
    }
}

fn find_project_dir(name: &str) -> Result<PathBuf> {
    // Look for projects/ first, then fall back to domains/ for backward compat.
    let candidates = [
        PathBuf::from(format!("projects/{name}")),
        PathBuf::from(format!("domains/{name}")),
        PathBuf::from(format!("../projects/{name}")),
        PathBuf::from(format!("../domains/{name}")),
    ];
    for c in &candidates {
        if c.exists() { return Ok(c.clone()); }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join("projects").join(name);
            if candidate.exists() { return Ok(candidate); }
            let candidate = dir.join("domains").join(name);
            if candidate.exists() { return Ok(candidate); }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }
    anyhow::bail!("project directory not found: {name}")
}

fn find_agent_dir(name: &str) -> Result<PathBuf> {
    let candidates = [
        PathBuf::from(format!("agents/{name}")),
        PathBuf::from(format!("../agents/{name}")),
    ];
    for c in &candidates {
        if c.exists() { return Ok(c.clone()); }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join("agents").join(name);
            if candidate.exists() { return Ok(candidate); }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }
    anyhow::bail!("agent directory not found: {name}")
}

fn get_api_key(config: &SystemConfig) -> Result<String> {
    let or_config = config.providers.openrouter.as_ref()
        .context("no OpenRouter provider configured")?;
    if !or_config.api_key.is_empty() {
        return Ok(or_config.api_key.clone());
    }
    let store_path = config.security.secret_store.as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.data_dir().join("secrets"));
    let store = SecretStore::open(&store_path)?;
    store.get("OPENROUTER_API_KEY")
        .context("OPENROUTER_API_KEY not set. Use `rm secrets set OPENROUTER_API_KEY <key>`")
}

fn build_provider(config: &SystemConfig) -> Result<Arc<dyn Provider>> {
    let api_key = get_api_key(config)?;
    let model = config.providers.openrouter.as_ref()
        .map(|or| or.default_model.clone())
        .unwrap_or_else(|| "minimax/minimax-m2.5".to_string());
    Ok(Arc::new(OpenRouterProvider::new(api_key, model)))
}

fn build_tools(workdir: &Path) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ShellTool::new(workdir.to_path_buf())),
        Arc::new(FileReadTool::new(workdir.to_path_buf())),
        Arc::new(FileWriteTool::new(workdir.to_path_buf())),
        Arc::new(ListDirTool::new(workdir.to_path_buf())),
    ]
}

/// Build the full tool set for a project: basic tools + quests + git worktree.
fn build_project_tools(
    workdir: &Path,
    quests_dir: &Path,
    prefix: &str,
    worktree_root: Option<&PathBuf>,
) -> Vec<Arc<dyn Tool>> {
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ShellTool::new(workdir.to_path_buf())),
        Arc::new(FileReadTool::new(workdir.to_path_buf())),
        Arc::new(FileWriteTool::new(workdir.to_path_buf())),
        Arc::new(ListDirTool::new(workdir.to_path_buf())),
    ];

    // Add beads tools (each gets its own store instance).
    if let Ok(t) = BeadsCreateTool::new(quests_dir.to_path_buf(), prefix.to_string()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsReadyTool::new(quests_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsUpdateTool::new(quests_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsCloseTool::new(quests_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsShowTool::new(quests_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsDepTool::new(quests_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }

    // Add git worktree tool.
    let wt_root = worktree_root
        .cloned()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("worktrees"));
    tools.push(Arc::new(GitWorktreeTool::new(workdir.to_path_buf(), wt_root)));

    // Add Porkbun domain tool if credentials are available.
    if let Some(porkbun) = PorkbunTool::from_env() {
        tools.push(Arc::new(porkbun));
    }

    tools
}

/// Look up project name from a quest prefix (e.g. "as" -> "algostaking").
fn project_name_for_prefix(config: &SystemConfig, prefix: &str) -> Option<String> {
    // Check agent prefixes.
    for agent in &config.agents {
        if agent.prefix == prefix {
            return Some(agent.name.clone());
        }
    }
    config.projects.iter()
        .find(|r| r.prefix == prefix)
        .map(|r| r.name.clone())
}

fn open_quests_for_project(project_name: &str) -> Result<TaskBoard> {
    let project_dir = find_project_dir(project_name)?;
    let quests_dir = project_dir.join(".tasks");
    TaskBoard::open(&quests_dir)
}

fn open_memory(config: &SystemConfig, project_name: Option<&str>) -> Result<SqliteMemory> {
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
    let embedding_model = config.providers.openrouter.as_ref()
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

// === Fast-Lane Command Handler ===

async fn handle_fast_lane(text: &str, reg: &Arc<ProjectRegistry>) -> String {
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
        "/help" => {
            "*Available Commands*\n\n\
             /pull — Summon a companion\n\
             /pull10 — Multi-summon x10\n\
             /collection — View collection stats\n\
             /companions — List your companions\n\
             /fuse <id1> <id2> — Fuse two companions\n\
             /familiar <id> — Set active familiar\n\
             /status — Project status\n\
             /cost — Today's spend\n\
             /help — This message"
                .to_string()
        }
        "/cost" => {
            "Cost tracking: use `rm cost` from CLI for detailed breakdown.".to_string()
        }
        _ => format!("Unknown fast-lane command: {cmd}"),
    }
}

// === Gacha Command Handler ===

fn handle_gacha_command(text: &str, store: &CompanionStore, engine: &GachaEngine) -> String {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

    match cmd.as_str() {
        "/pull" => {
            let count = parts.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
            let count = count.clamp(1, 10);
            gacha_pull(store, engine, count)
        }
        "/pull10" => gacha_pull(store, engine, 10),
        "/collection" => gacha_collection(store),
        "/companions" | "/companion" => gacha_list(store),
        "/fuse" => {
            let id_a = parts.get(1).map(|s| s.to_string());
            let id_b = parts.get(2).map(|s| s.to_string());
            gacha_fuse(store, id_a, id_b)
        }
        "/familiar" => {
            let target = parts.get(1).map(|s| s.to_string());
            gacha_familiar(store, target)
        }
        _ => "Unknown command. Try: /pull, /pull10, /collection, /fuse, /familiar".to_string(),
    }
}

fn gacha_pull(store: &CompanionStore, engine: &GachaEngine, count: u32) -> String {
    let mut pity = match store.load_pity() {
        Ok(p) => p,
        Err(e) => return format!("Failed to load pity state: {e}"),
    };

    let companions: Vec<Companion> = (0..count).map(|_| engine.pull(&mut pity)).collect();

    for c in &companions {
        if let Err(e) = store.save_companion(c) {
            return format!("Failed to save companion: {e}");
        }
        if let Err(e) = store.record_pull(c) {
            return format!("Failed to record pull: {e}");
        }
    }
    if let Err(e) = store.save_pity(&pity) {
        return format!("Failed to save pity: {e}");
    }

    if count == 1 {
        let c = &companions[0];
        format_single_pull(c, &pity)
    } else {
        format_multi_pull(&companions, &pity)
    }
}

fn format_single_pull(c: &Companion, pity: &system_companions::PityState) -> String {
    let rarity_flair = match c.rarity {
        Rarity::SS => "\u{1F49C} \u{2728} *SS-RANK!!* \u{2728} \u{1F49C}\nThe heavens part. A legendary companion descends!",
        Rarity::S => "\u{1F31F} *S-Rank!* Your luck is shining!",
        Rarity::A => "\u{26A1} *A-Rank* \u{2014} a worthy addition to the court.",
        Rarity::B => "A solid pull. She'll serve well.",
        Rarity::C => "A common spirit answers the call.",
    };

    let separator = "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";

    format!(
        "\u{2728} *Summoning...* \u{2728}\n\
         {sep}\n\
         {emoji} *{rarity}* {name}\n\
         {stars}\n\
         \u{1F3AF} {archetype}\n\
         \u{1F30A} {region} \u{2022} {dere} \u{2022} {aesthetic}\n\
         {sep}\n\
         {flair}\n\n\
         \u{1F4CA} Pull #{total} | Pity: {pity_s} since S+",
        sep = separator,
        emoji = c.rarity.color_emoji(),
        rarity = c.rarity,
        name = c.name,
        stars = c.rarity.stars(),
        archetype = c.archetype.title(),
        region = c.region,
        dere = c.dere_type,
        aesthetic = c.aesthetic,
        flair = rarity_flair,
        total = pity.total_pulls,
        pity_s = pity.pulls_since_s_or_above,
    )
}

fn format_multi_pull(companions: &[Companion], pity: &system_companions::PityState) -> String {
    let mut lines = vec![format!("\u{2728} *Multi-Summon x{}* \u{2728}\n", companions.len())];

    let mut best_rarity = Rarity::C;
    let mut counts = [0u32; 5];

    for (i, c) in companions.iter().enumerate() {
        let short_id = &c.id[..6];
        let highlight = if c.rarity >= Rarity::A { " \u{26A1}" } else { "" };
        lines.push(format!(
            "{}. {} *{}* {} \u{2014} {} (`{}`){highlight}",
            i + 1,
            c.rarity.color_emoji(),
            c.rarity,
            c.name,
            c.display_name(),
            short_id,
        ));
        if c.rarity > best_rarity {
            best_rarity = c.rarity;
        }
        match c.rarity {
            Rarity::C => counts[0] += 1,
            Rarity::B => counts[1] += 1,
            Rarity::A => counts[2] += 1,
            Rarity::S => counts[3] += 1,
            Rarity::SS => counts[4] += 1,
        }
    }

    let separator = "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";
    lines.push(format!("\n{separator}"));

    let mut summary_parts = Vec::new();
    if counts[4] > 0 { summary_parts.push(format!("\u{1F7E3} {}x SS", counts[4])); }
    if counts[3] > 0 { summary_parts.push(format!("\u{1F7E1} {}x S", counts[3])); }
    if counts[2] > 0 { summary_parts.push(format!("\u{1F535} {}x A", counts[2])); }
    if counts[1] > 0 { summary_parts.push(format!("\u{1F7E2} {}x B", counts[1])); }
    if counts[0] > 0 { summary_parts.push(format!("\u{26AA} {}x C", counts[0])); }
    lines.push(summary_parts.join(" | "));

    lines.push(format!("\n\u{1F4CA} Pull #{} | Pity: {} since S+", pity.total_pulls, pity.pulls_since_s_or_above));

    lines.join("\n")
}

fn gacha_collection(store: &CompanionStore) -> String {
    let stats = match store.collection_stats() {
        Ok(s) => s,
        Err(e) => return format!("Failed to load collection: {e}"),
    };

    let familiar = store.get_familiar().ok().flatten();
    let familiar_line = match familiar {
        Some(f) => format!("\n\u{1F451} *Active Familiar:* {} {} {}", f.rarity.color_emoji(), f.rarity, f.name),
        None => "\n\u{1F451} *No familiar set.* Use `/familiar <id>` to choose one.".to_string(),
    };

    format!(
        "\u{1F4E6} *Collection*\n\
         \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
         \u{1F465} Companions: *{}*\n\
         \u{1F3B0} Total Pulls: *{}*\n\
         \u{2697}\u{FE0F} Total Fusions: *{}*\n\n\
         \u{1F7E3} SS: {} | \u{1F7E1} S: {} | \u{1F535} A: {} | \u{1F7E2} B: {} | \u{26AA} C: {}\
         {familiar}",
        stats.total_companions,
        stats.total_pulls,
        stats.total_fusions,
        stats.ss_count,
        stats.s_count,
        stats.a_count,
        stats.b_count,
        stats.c_count,
        familiar = familiar_line,
    )
}

fn gacha_list(store: &CompanionStore) -> String {
    let all = match store.list_all() {
        Ok(list) => list,
        Err(e) => return format!("Failed to load companions: {e}"),
    };

    if all.is_empty() {
        return "\u{1F4E6} Your collection is empty. Use `/pull` to summon your first companion!".to_string();
    }

    let mut lines = vec![format!("\u{1F4E6} *Your Companions* ({})\n", all.len())];
    let separator = "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";

    for rarity in &[Rarity::SS, Rarity::S, Rarity::A, Rarity::B, Rarity::C] {
        let matching: Vec<&Companion> = all.iter().filter(|c| &c.rarity == rarity).collect();
        if matching.is_empty() { continue; }

        lines.push(format!("{} *{} Rank* ({})", rarity.color_emoji(), rarity, matching.len()));
        for c in &matching {
            let short_id = &c.id[..6];
            let fam_badge = if c.is_familiar { " \u{1F451}" } else { "" };
            lines.push(format!(
                "  {} {} \u{2014} Lv.{}{fam_badge} (`{short_id}`)",
                c.rarity.stars(),
                c.name,
                c.bond_level,
            ));
        }
        lines.push(String::new());
    }

    lines.push(separator.to_string());
    lines.push("Use `/fuse <id1> <id2>` to fuse same-rarity companions.".to_string());

    lines.join("\n")
}

fn gacha_fuse(store: &CompanionStore, id_a: Option<String>, id_b: Option<String>) -> String {
    let (Some(id_a), Some(id_b)) = (id_a, id_b) else {
        let all = match store.list_all() {
            Ok(list) => list,
            Err(e) => return format!("Failed to load companions: {e}"),
        };

        let mut lines = vec!["\u{2697}\u{FE0F} *Fusion Pipeline*\n".to_string()];
        let mut has_pairs = false;

        for rarity in &[Rarity::C, Rarity::B, Rarity::A, Rarity::S] {
            let eligible: Vec<&Companion> = all.iter()
                .filter(|c| &c.rarity == rarity && !c.is_familiar)
                .collect();
            if eligible.len() >= 2 {
                has_pairs = true;
                lines.push(format!("{} *{} \u{2192} {}* ({} eligible)", rarity.color_emoji(), rarity, rarity.next().unwrap(), eligible.len()));
                for c in &eligible {
                    let short_id = &c.id[..6];
                    lines.push(format!("  {} (`{short_id}`)", c.name));
                }
                lines.push(String::new());
            }
        }

        if !has_pairs {
            lines.push("No fusion-eligible pairs found. You need 2+ same-rarity non-familiar companions.".to_string());
        } else {
            lines.push("Use: `/fuse <id1> <id2>` with the 6-char IDs above.".to_string());
        }

        return lines.join("\n");
    };

    let find_companion = |short_id: &str| -> Option<Companion> {
        let all = store.list_all().ok()?;
        all.into_iter().find(|c| c.id.starts_with(short_id))
    };

    let comp_a = match find_companion(&id_a) {
        Some(c) => c,
        None => return format!("\u{274C} No companion found with ID starting with `{id_a}`"),
    };
    let comp_b = match find_companion(&id_b) {
        Some(c) => c,
        None => return format!("\u{274C} No companion found with ID starting with `{id_b}`"),
    };

    let result = match fuse(&comp_a, &comp_b) {
        Ok(r) => r,
        Err(e) => return format!("\u{274C} Fusion failed: {e}"),
    };

    if let Err(e) = store.remove_companion(&comp_a.id) {
        return format!("Fusion error (cleanup): {e}");
    }
    if let Err(e) = store.remove_companion(&comp_b.id) {
        return format!("Fusion error (cleanup): {e}");
    }
    if let Err(e) = store.record_fusion(&comp_a, &comp_b, &result) {
        return format!("Fusion error (history): {e}");
    }
    if let Err(e) = store.save_companion(&result) {
        return format!("Fusion error (save): {e}");
    }

    let separator = "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";

    format!(
        "\u{2697}\u{FE0F} *Fusion Pipeline*\n\
         {sep}\n\
         {a_emoji} {a_name} ({a_rarity})\n\
         {b_emoji} {b_name} ({b_rarity})\n\
         {sep}\n\
         \u{1F300} *Merging souls...*\n\
         {sep}\n\n\
         {r_emoji} *{r_rarity}* {r_name}\n\
         {stars}\n\
         \u{1F3AF} {archetype}\n\
         \u{1F30A} {region} \u{2022} {dere} \u{2022} {aesthetic}\n\
         Hook Lv.{bond} ({bond_xp} XP inherited)\n\
         {sep}\n\
         \u{2728} A new companion is born from the ashes of two.",
        sep = separator,
        a_emoji = comp_a.rarity.color_emoji(),
        a_name = comp_a.name,
        a_rarity = comp_a.rarity,
        b_emoji = comp_b.rarity.color_emoji(),
        b_name = comp_b.name,
        b_rarity = comp_b.rarity,
        r_emoji = result.rarity.color_emoji(),
        r_rarity = result.rarity,
        r_name = result.name,
        stars = result.rarity.stars(),
        archetype = result.archetype.title(),
        region = result.region,
        dere = result.dere_type,
        aesthetic = result.aesthetic,
        bond = result.bond_level,
        bond_xp = result.bond_xp,
    )
}

fn gacha_familiar(store: &CompanionStore, target: Option<String>) -> String {
    let Some(target_id) = target else {
        return match store.get_familiar() {
            Ok(Some(f)) => {
                format!(
                    "\u{1F451} *Active Familiar*\n\n\
                     {} *{}* {}\n\
                     {}\n\
                     \u{1F3AF} {}\n\
                     \u{1F30A} {} \u{2022} {} \u{2022} {}\n\
                     Hook Lv.{} ({} XP)\n\n\
                     _{}_",
                    f.rarity.color_emoji(),
                    f.rarity,
                    f.name,
                    f.rarity.stars(),
                    f.archetype.title(),
                    f.region,
                    f.dere_type,
                    f.aesthetic,
                    f.bond_level,
                    f.bond_xp,
                    f.dere_type.speech_pattern(),
                )
            }
            Ok(None) => "\u{1F451} No familiar set. Use `/familiar <id>` to choose one from your collection.".to_string(),
            Err(e) => format!("Failed to load familiar: {e}"),
        };
    };

    let all = match store.list_all() {
        Ok(list) => list,
        Err(e) => return format!("Failed to load companions: {e}"),
    };

    let companion = match all.iter().find(|c| c.id.starts_with(&target_id)) {
        Some(c) => c,
        None => return format!("\u{274C} No companion found with ID starting with `{target_id}`"),
    };

    let companion_id = companion.id.clone();
    let companion_name = companion.name.clone();
    let companion_rarity = companion.rarity;

    if let Err(e) = store.set_familiar(&companion_id) {
        return format!("Failed to set familiar: {e}");
    }

    format!(
        "\u{1F451} *Familiar Hook Forged*\n\n\
         {} *{}* {} is now your active familiar.\n\n\
         _She steps forward, eyes locked on yours._",
        companion_rarity.color_emoji(),
        companion_rarity,
        companion_name,
    )
}

// === Commands ===

async fn cmd_run(
    config_path: &Option<PathBuf>,
    prompt: &str,
    project_name: Option<&str>,
    model_override: Option<&str>,
    max_iterations: u32,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let model = model_override
        .map(String::from)
        .or_else(|| project_name.map(|r| config.model_for_project(r)))
        .unwrap_or_else(|| {
            config.providers.openrouter.as_ref()
                .map(|or| or.default_model.clone())
                .unwrap_or_else(|| "minimax/minimax-m2.5".to_string())
        });

    let provider = build_provider(&config)?;
    let workdir = project_name
        .and_then(|r| config.project(r))
        .map(|r| PathBuf::from(&r.repo))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let tools = if let Some(rn) = project_name {
        let project_dir = find_project_dir(rn)?;
        let quests_dir = project_dir.join(".tasks");
        let prefix = config.project(rn).map(|r| r.prefix.as_str()).unwrap_or("sg");
        let worktree_root = config.project(rn).and_then(|r| r.worktree_root.as_ref()).map(PathBuf::from);
        build_project_tools(&workdir, &quests_dir, prefix, worktree_root.as_ref())
    } else {
        build_tools(&workdir)
    };
    // Load agent identity (from agents/) + optional project context.
    let identity = if let Some(rn) = project_name {
        let project_dir = find_project_dir(rn).ok();
        let agent_dir = find_agent_dir("aurelia").ok();
        match (agent_dir, project_dir) {
            (Some(a), Some(d)) => Identity::load(&a, Some(&d)).unwrap_or_default(),
            (Some(a), None) => Identity::load(&a, None).unwrap_or_default(),
            (None, Some(d)) => Identity::load_from_dir(&d).unwrap_or_default(),
            (None, None) => Identity::default(),
        }
    } else {
        find_agent_dir("aurelia").ok()
            .map(|d| Identity::load(&d, None).unwrap_or_default())
            .unwrap_or_default()
    };
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

async fn cmd_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_dir = cwd.join("config");
    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(cwd.join("projects"))?;

    let config_path = config_dir.join("realm.toml");
    if !config_path.exists() {
        std::fs::write(&config_path, r#"[realm]
name = "my-realm"
data_dir = "~/.sigil"

[providers.openrouter]
api_key = "${OPENROUTER_API_KEY}"
default_model = "minimax/minimax-m2.5"
fallback_model = "deepseek/deepseek-v3.2"

[security]
autonomy = "supervised"
workspace_only = true
max_cost_per_day_usd = 10.0

[memory]
backend = "sqlite"
temporal_decay_halflife_days = 30

[pulse]
enabled = false
default_interval_minutes = 30
"#)?;
        println!("Created config/realm.toml");
    }

    let data_dir = dirs::home_dir().unwrap_or_default().join(".sigil");
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(data_dir.join("secrets"))?;
    println!("Created ~/.sigil/");

    println!("\nRealm initialized. Next steps:");
    println!("  1. rm secrets set OPENROUTER_API_KEY sk-or-...");
    println!("  2. Add projects to config/realm.toml");
    println!("  3. rm run \"hello world\"");
    Ok(())
}

async fn cmd_secrets(config_path: &Option<PathBuf>, action: SecretsAction) -> Result<()> {
    let store_path = if let Ok((config, _)) = load_config(config_path) {
        config.security.secret_store.as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| config.data_dir().join("secrets"))
    } else {
        dirs::home_dir().unwrap_or_default().join(".sigil/secrets")
    };
    let store = SecretStore::open(&store_path)?;

    match action {
        SecretsAction::Set { name, value } => {
            store.set(&name, &value)?;
            println!("Secret '{name}' stored.");
        }
        SecretsAction::Get { name } => println!("{}", store.get(&name)?),
        SecretsAction::List => {
            let names = store.list()?;
            if names.is_empty() { println!("No secrets stored."); }
            else { for n in names { println!("  {n}"); } }
        }
        SecretsAction::Delete { name } => {
            store.delete(&name)?;
            println!("Secret '{name}' deleted.");
        }
    }
    Ok(())
}

async fn cmd_doctor(config_path: &Option<PathBuf>, fix: bool) -> Result<()> {
    println!("Realm Doctor{}\n============\n", if fix { " (--fix)" } else { "" });

    let mut issues = 0u32;
    let mut fixed = 0u32;

    match load_config(config_path) {
        Ok((config, path)) => {
            println!("[OK] Config: {}", path.display());

            if let Some(ref or) = config.providers.openrouter {
                // Try config api_key first, then fall back to secret store.
                let api_key = if !or.api_key.is_empty() {
                    Some(or.api_key.clone())
                } else {
                    let store_path = config.security.secret_store.as_ref()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| config.data_dir().join("secrets"));
                    SecretStore::open(&store_path).ok()
                        .and_then(|s| s.get("OPENROUTER_API_KEY").ok())
                };

                match api_key {
                    Some(key) => {
                        let provider = OpenRouterProvider::new(key, or.default_model.clone());
                        match provider.health_check().await {
                            Ok(()) => println!("[OK] OpenRouter API key valid"),
                            Err(e) => { println!("[FAIL] OpenRouter: {e}"); issues += 1; }
                        }
                    }
                    None => {
                        println!("[WARN] OpenRouter API key not set (config or secret store)");
                        issues += 1;
                    }
                }
            }

            for pcfg in &config.projects {
                let repo_ok = PathBuf::from(&pcfg.repo).exists();
                println!("[{}] Project '{}' repo: {}", if repo_ok { "OK" } else { "WARN" }, pcfg.name, pcfg.repo);
                if !repo_ok { issues += 1; }

                match find_project_dir(&pcfg.name) {
                    Ok(d) => {
                        let agents_md = d.join("AGENTS.md").exists();
                        let knowledge_md = d.join("KNOWLEDGE.md").exists();
                        let quests_dir = d.join(".tasks");
                        let has_tasks = quests_dir.exists();
                        if !agents_md { issues += 1; }
                        println!("    Project files: AGENTS.md={agents_md} KNOWLEDGE.md={knowledge_md} | Tasks: {has_tasks}");

                        // --fix: create missing .tasks dir
                        if fix && !has_tasks {
                            std::fs::create_dir_all(&quests_dir)?;
                            println!("    [FIXED] Created .tasks directory");
                            fixed += 1;
                        }

                        // Check skills directory
                        let skills_dir = d.join("skills");
                        let skill_count = if skills_dir.exists() {
                            Skill::discover(&skills_dir).map(|s| s.len()).unwrap_or(0)
                        } else { 0 };
                        let mol_count = if d.join("rituals").exists() {
                            std::fs::read_dir(d.join("rituals"))
                                .map(|e| e.filter(|e| e.as_ref().ok()
                                    .map(|e| e.path().extension().is_some_and(|x| x == "toml"))
                                    .unwrap_or(false)).count())
                                .unwrap_or(0)
                        } else { 0 };
                        println!("    Skills: {skill_count} | Rituals: {mol_count}");

                        // Check memory DB
                        let mem_db = d.join(".sigil").join("memory.db");
                        if mem_db.exists() {
                            println!("    Memory: {}", mem_db.display());
                        }
                    }
                    Err(_) => {
                        println!("    [WARN] Project dir not found");
                        issues += 1;
                    }
                }
            }

            // Check agent identity files.
            for agent_cfg in &config.agents {
                match find_agent_dir(&agent_cfg.name) {
                    Ok(d) => {
                        let has_persona = d.join("PERSONA.md").exists() || d.join("SOUL.md").exists();
                        let has_identity = d.join("IDENTITY.md").exists();
                        if !has_persona { issues += 1; }
                        if !has_identity { issues += 1; }
                        println!("[{}] Agent '{}': PERSONA/SOUL={has_persona} IDENTITY={has_identity}",
                            if has_persona && has_identity { "OK" } else { "WARN" },
                            agent_cfg.name);
                    }
                    Err(_) => {
                        println!("[WARN] Agent dir not found for '{}'", agent_cfg.name);
                        issues += 1;
                    }
                }
            }

            let store_path = config.security.secret_store.as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| config.data_dir().join("secrets"));
            if store_path.exists() {
                println!("[OK] Secret store: {}", store_path.display());
            } else {
                issues += 1;
                if fix {
                    std::fs::create_dir_all(&store_path)?;
                    println!("[FIXED] Created secret store: {}", store_path.display());
                    fixed += 1;
                } else {
                    println!("[WARN] Secret store missing: {}", store_path.display());
                }
            }

            // Check global memory DB.
            let mem_path = config.data_dir().join("memory.db");
            println!("[{}] Global memory: {}", if mem_path.exists() { "OK" } else { "INFO" }, mem_path.display());

            // Check cron store.
            let fate_path = config.data_dir().join("fate.json");
            if fate_path.exists() {
                let store = ScheduleStore::open(&fate_path)?;
                println!("[OK] Cron: {} jobs", store.jobs.len());
            } else {
                println!("[INFO] Cron: no jobs configured");
            }

            // Check data dir
            let data_dir = config.data_dir();
            if data_dir.exists() {
                println!("[OK] Data dir: {}", data_dir.display());
            } else {
                issues += 1;
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
            println!("       Run `rm init` to create one.");
            issues += 1;
        }
    }

    println!();
    if issues == 0 {
        println!("All checks passed.");
    } else if fix {
        println!("{issues} issues found, {fixed} fixed.");
    } else {
        println!("{issues} issues found. Run `rm doctor --fix` to auto-repair.");
    }
    Ok(())
}

async fn cmd_status(config_path: &Option<PathBuf>) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    println!("Realm: {}\n", config.system.name);

    // Show system team.
    println!("System Team: leader={}", config.system_leader());
    if !config.team.agents.is_empty() {
        println!("  agents: {}", config.team.agents.join(", "));
    }
    println!();

    // Show agents.
    if !config.agents.is_empty() {
        println!("Agents:");
        for agent_cfg in &config.agents {
            let expertise = if agent_cfg.expertise.is_empty() { "general".to_string() } else { agent_cfg.expertise.join(", ") };
            let leader_marker = if config.system_leader() == agent_cfg.name { " [SYSTEM LEADER]" } else { "" };
            println!("  {} [{}] role={:?} voice={:?} model={}{} expertise=[{}]",
                agent_cfg.name, agent_cfg.prefix,
                agent_cfg.role, agent_cfg.voice,
                agent_cfg.model.as_deref().unwrap_or("default"),
                leader_marker,
                expertise,
            );
        }
        println!();
    }

    println!("Projects:");
    for project_cfg in &config.projects {
        let repo_ok = PathBuf::from(&project_cfg.repo).exists();
        let team = config.project_team(&project_cfg.name);
        print!("  {} [{}] prefix={} model={} workers={} leader={}",
            project_cfg.name,
            if repo_ok { "OK" } else { "MISSING" },
            project_cfg.prefix,
            project_cfg.model.as_deref().unwrap_or("default"),
            project_cfg.max_workers,
            team.leader,
        );

        // Show quest counts.
        if let Ok(store) = open_quests_for_project(&project_cfg.name) {
            let open: Vec<_> = store.by_prefix(&project_cfg.prefix).into_iter()
                .filter(|b| !b.is_closed()).collect();
            let ready = store.ready().len();
            print!(" | tasks: {} open, {} ready", open.len(), ready);
        }
        println!();
    }

    Ok(())
}

async fn cmd_assign(
    config_path: &Option<PathBuf>,
    subject: &str,
    project_name: &str,
    description: &str,
    priority: Option<&str>,
    mission_id: Option<&str>,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    // Allow assigning to any configured project or agent.
    let prefix = if let Some(pcfg) = config.project(project_name) {
        pcfg.prefix.clone()
    } else if let Some(acfg) = config.agent(project_name) {
        acfg.prefix.clone()
    } else {
        anyhow::bail!("project or agent not found: {project_name}");
    };

    let mut store = open_quests_for_project(project_name)?;
    let mut bead = store.create(&prefix, subject)?;

    if !description.is_empty() || priority.is_some() || mission_id.is_some() {
        let mid = mission_id.map(|s| s.to_string());
        bead = store.update(&bead.id.0, |b| {
            if !description.is_empty() {
                b.description = description.to_string();
            }
            if let Some(p) = priority {
                b.priority = match p {
                    "low" => system_tasks::Priority::Low,
                    "high" => system_tasks::Priority::High,
                    "critical" => system_tasks::Priority::Critical,
                    _ => system_tasks::Priority::Normal,
                };
            }
            if let Some(ref m) = mid {
                b.mission_id = Some(m.clone());
            }
        })?;
    }

    let mission_str = if let Some(m) = mission_id { format!(" mission={m}") } else { String::new() };
    println!("Created {} [{}] {}{}", bead.id, bead.priority, bead.subject, mission_str);
    Ok(())
}

async fn cmd_ready(config_path: &Option<PathBuf>, project_name: Option<&str>) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let projects: Vec<&str> = if let Some(name) = project_name {
        vec![name]
    } else {
        config.projects.iter().map(|r| r.name.as_str()).collect()
    };

    let mut found = false;
    for name in projects {
        if let Ok(store) = open_quests_for_project(name) {
            let ready = store.ready();
            for bead in ready {
                found = true;
                println!("{} [{}] {} — {}",
                    bead.id, bead.priority, bead.subject,
                    if bead.description.is_empty() { "(no description)" } else { &bead.description }
                );
            }
        }
    }

    if !found {
        println!("No ready work.");
    }
    Ok(())
}

async fn cmd_beads(config_path: &Option<PathBuf>, project_name: Option<&str>, show_all: bool) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let projects: Vec<&str> = if let Some(name) = project_name {
        vec![name]
    } else {
        config.projects.iter().map(|r| r.name.as_str()).collect()
    };

    for name in projects {
        if let Ok(store) = open_quests_for_project(name) {
            let tasks = store.all();
            let tasks: Vec<_> = if show_all {
                tasks
            } else {
                tasks.into_iter().filter(|b| !b.is_closed()).collect()
            };

            if tasks.is_empty() { continue; }

            println!("=== {} ===", name);
            for bead in tasks {
                let assignee = bead.assignee.as_deref().unwrap_or("-");
                let deps = if bead.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" (needs: {})", bead.depends_on.iter().map(|d| d.0.as_str()).collect::<Vec<_>>().join(", "))
                };
                let checkpoints = if bead.checkpoints.is_empty() {
                    String::new()
                } else {
                    format!(" checkpoints={}", bead.checkpoints.len())
                };
                println!("  {} [{}] {} — {} assignee={}{}{}", bead.id, bead.status, bead.priority, bead.subject, assignee, deps, checkpoints);
            }
        }
    }
    Ok(())
}

async fn cmd_close(config_path: &Option<PathBuf>, id: &str, reason: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = id.split('-').next().unwrap_or("");
    let project_name = project_name_for_prefix(&config, prefix)
        .context(format!("no project with prefix '{prefix}'"))?;

    let mut store = open_quests_for_project(&project_name)?;
    let bead = store.close(id, reason)?;
    println!("Closed {} — {}", bead.id, bead.subject);

    // Check if this task's mission should auto-close.
    if let Some(ref mid) = bead.mission_id
        && store.check_mission_completion(mid)?
    {
        println!("Mission {} auto-closed (all tasks done)", mid);
    }
    Ok(())
}

async fn cmd_mission(config_path: &Option<PathBuf>, action: MissionAction) -> Result<()> {
    match action {
        MissionAction::Create { name, project, description } => {
            let (config, _) = load_config(config_path)?;
            let prefix = if let Some(pcfg) = config.project(&project) {
                pcfg.prefix.clone()
            } else if let Some(acfg) = config.agent(&project) {
                acfg.prefix.clone()
            } else {
                anyhow::bail!("project or agent not found: {project}");
            };

            let mut store = open_quests_for_project(&project)?;
            let mut mission = store.create_mission(&prefix, &name)?;

            if !description.is_empty() {
                mission = store.update_mission(&mission.id, |m| {
                    m.description = description.clone();
                })?;
            }

            println!("Created mission {} — {}", mission.id, mission.name);
            Ok(())
        }
        MissionAction::List { project, all } => {
            let (config, _) = load_config(config_path)?;

            let projects: Vec<&str> = if let Some(ref name) = project {
                vec![name.as_str()]
            } else {
                config.projects.iter().map(|r| r.name.as_str()).collect()
            };

            for name in projects {
                if let Ok(store) = open_quests_for_project(name) {
                    let missions = if all {
                        store.missions(None)
                    } else {
                        store.active_missions(None)
                    };

                    if missions.is_empty() { continue; }

                    println!("=== {} ===", name);
                    for m in missions {
                        let task_count = store.mission_tasks(&m.id).len();
                        let done_count = store.mission_tasks(&m.id).iter().filter(|t| t.is_closed()).count();
                        println!("  {} [{}] {} — {}/{} tasks done",
                            m.id, m.status, m.name, done_count, task_count);
                    }
                }
            }
            Ok(())
        }
        MissionAction::Status { id } => {
            let (config, _) = load_config(config_path)?;
            let prefix = id.split('-').next().unwrap_or("");
            let project_name = project_name_for_prefix(&config, prefix)
                .context(format!("no project with prefix '{prefix}'"))?;

            let store = open_quests_for_project(&project_name)?;
            let mission = store.get_mission(&id)
                .ok_or_else(|| anyhow::anyhow!("mission not found: {id}"))?;

            println!("Mission: {} — {}", mission.id, mission.name);
            println!("Status: {}", mission.status);
            if !mission.description.is_empty() {
                println!("Description: {}", mission.description);
            }

            let tasks = store.mission_tasks(&id);
            if tasks.is_empty() {
                println!("No tasks assigned to this mission.");
            } else {
                let done = tasks.iter().filter(|t| t.is_closed()).count();
                println!("Progress: {}/{} tasks done", done, tasks.len());
                for t in &tasks {
                    let assignee = t.assignee.as_deref().unwrap_or("-");
                    println!("  {} [{}] {} — assignee={}", t.id, t.status, t.subject, assignee);
                }
            }
            Ok(())
        }
        MissionAction::Close { id } => {
            let (config, _) = load_config(config_path)?;
            let prefix = id.split('-').next().unwrap_or("");
            let project_name = project_name_for_prefix(&config, prefix)
                .context(format!("no project with prefix '{prefix}'"))?;

            let mut store = open_quests_for_project(&project_name)?;
            let mission = store.close_mission(&id)?;
            println!("Closed mission {} — {}", mission.id, mission.name);
            Ok(())
        }
    }
}

fn pid_file_path(config: &SystemConfig) -> PathBuf {
    config.data_dir().join("rm.pid")
}

async fn cmd_daemon(config_path: &Option<PathBuf>, action: SummonerAction) -> Result<()> {
    match action {
        SummonerAction::Start => {
            let (config, _) = load_config(config_path)?;

            // Check if already running.
            let pid_path = pid_file_path(&config);
            if Daemon::is_running_from_pid(&pid_path) {
                anyhow::bail!("daemon is already running (PID file: {})", pid_path.display());
            }

            let data_dir = config.data_dir();
            let dispatch_bus = Arc::new(DispatchBus::with_persistence(data_dir.join("whispers.jsonl")));
            let cost_ledger = Arc::new(system_orchestrator::CostLedger::with_persistence(
                config.security.max_cost_per_day_usd,
                data_dir.join("cost_ledger.jsonl"),
            ));
            let leader_name = config.leader_agent().map(|a| a.name.clone()).unwrap_or_else(|| "aurelia".to_string());
            let mut registry_inner = ProjectRegistry::new(dispatch_bus.clone(), leader_name.clone());
            registry_inner.set_cost_ledger(cost_ledger.clone());
            let registry = Arc::new(registry_inner);
            let provider = build_provider(&config)?;
            let mut pulses = Vec::new();

            // Set per-project budget ceilings from config.
            for project_cfg in &config.projects {
                if let Some(budget) = project_cfg.max_cost_per_day_usd {
                    cost_ledger.set_project_budget(&project_cfg.name, budget);
                }
            }

            // Register project rigs.
            for project_cfg in &config.projects {
                let project_dir = match find_project_dir(&project_cfg.name) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let default_model = config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.as_str())
                    .unwrap_or("minimax/minimax-m2.5");

                let rig = Arc::new(Project::from_config(project_cfg, &project_dir, default_model)?);
                let workdir = rig.repo.clone();
                let quests_dir = project_dir.join(".tasks");
                let tools = build_project_tools(&workdir, &quests_dir, &project_cfg.prefix, Some(&rig.worktree_root));
                let mut witness = Supervisor::new(&rig, provider.clone(), tools.clone(), dispatch_bus.clone());

                // Wire memory + reflection for worker post-execution insight extraction.
                if let Ok(mem) = open_memory(&config, Some(&project_cfg.name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    witness.memory = Some(mem);
                    witness.reflect_provider = Some(provider.clone());
                    let reflect_model = config.providers.openrouter.as_ref()
                        .map(|or| or.default_model.clone())
                        .unwrap_or_else(|| "minimax/minimax-m2.5".to_string());
                    witness.reflect_model = reflect_model;
                }

                // Load emotional state for personality tracking.
                {
                    let emo_path = EmotionalState::path_for_agent(&project_dir);
                    let emo = EmotionalState::load(&emo_path, &project_cfg.name);
                    witness.emotional_state = Some(Arc::new(tokio::sync::Mutex::new(emo)));
                    witness.emotional_state_path = Some(emo_path);
                }

                // Wire per-project team if configured.
                let project_team = config.project_team(&project_cfg.name);
                witness.set_team(project_team, config.system_leader());

                // Configure execution mode for workers.
                if project_cfg.execution_mode == ExecutionMode::ClaudeCode {
                    let cc_model = config.model_for_project(&project_cfg.name);
                    let cc_max_turns = project_cfg.max_turns.unwrap_or(25);
                    witness.set_claude_code_mode(
                        rig.repo.clone(),
                        cc_model,
                        cc_max_turns,
                        project_cfg.max_budget_usd,
                    );
                    info!(
                        project = %project_cfg.name,
                        model = %witness.model,
                        max_turns = cc_max_turns,
                        team_leader = %witness.escalation_target,
                        "registered with claude_code execution mode"
                    );
                }

                registry.register_project(rig.clone(), witness).await;

                // Create pulse if HEARTBEAT.md exists and pulse is enabled.
                if config.heartbeat.enabled
                    && let Some(ref hb_content) = rig.project_identity.heartbeat {
                        let interval = config.heartbeat.default_interval_minutes as u64 * 60;
                        let pulse = system_orchestrator::Heartbeat::new(
                            rig.name.clone(),
                            interval,
                            hb_content.clone(),
                            provider.clone(),
                            tools.clone(),
                            rig.project_identity.clone(),
                            rig.model.clone(),
                            dispatch_bus.clone(),
                        );
                        pulses.push(pulse);
                    }
            }

            // Build channels map for the familiar.
            let channels: Arc<RwLock<HashMap<String, Arc<dyn system_core::traits::Channel>>>> =
                Arc::new(RwLock::new(HashMap::new()));

            // Register advisor agents as projects (so they can receive quests).
            for agent_cfg in config.advisor_agents() {
                let agent_dir = match find_agent_dir(&agent_cfg.name) {
                    Ok(d) => d,
                    Err(_) => {
                        warn!(agent = %agent_cfg.name, "advisor agent dir not found, skipping");
                        continue;
                    }
                };
                let agent_identity = Identity::load(&agent_dir, None).unwrap_or_default();
                let agent_quests_dir = agent_dir.join(".tasks");
                std::fs::create_dir_all(&agent_quests_dir).ok();
                let agent_beads = system_tasks::TaskBoard::open(&agent_quests_dir)?;
                let agent_model = agent_cfg.model.clone().unwrap_or_else(|| "claude-sonnet-4-6".to_string());
                let agent_workdir = agent_cfg.default_repo.as_ref()
                    .map(|r| config.resolve_repo(r))
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

                let agent_project = Arc::new(Project {
                    name: agent_cfg.name.clone(),
                    prefix: agent_cfg.prefix.clone(),
                    repo: agent_workdir.clone(),
                    worktree_root: dirs::home_dir().unwrap_or_default().join("worktrees"),
                    model: agent_model.clone(),
                    max_workers: agent_cfg.max_workers,
                    worker_timeout_secs: 300, // 5 min timeout for advisor responses
                    project_identity: agent_identity,
                    tasks: Arc::new(tokio::sync::Mutex::new(agent_beads)),
                    task_notify: Arc::new(tokio::sync::Notify::new()),
                });

                let agent_tools: Vec<Arc<dyn system_core::traits::Tool>> = build_tools(&agent_workdir);
                let mut agent_scout = Supervisor::new(&agent_project, provider.clone(), agent_tools, dispatch_bus.clone());

                // Advisors always use Claude Code mode.
                agent_scout.set_claude_code_mode(
                    agent_workdir.clone(),
                    agent_model.clone(),
                    agent_cfg.max_turns.unwrap_or(15),
                    agent_cfg.max_budget_usd,
                );

                // Wire memory + reflection for advisor agents (same pattern as project scouts).
                if let Ok(mem) = open_memory(&config, Some(&agent_cfg.name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    agent_scout.memory = Some(mem);
                    agent_scout.reflect_provider = Some(provider.clone());
                    let reflect_model = config.providers.openrouter.as_ref()
                        .map(|or| or.default_model.clone())
                        .unwrap_or_else(|| "minimax/minimax-m2.5".to_string());
                    agent_scout.reflect_model = reflect_model;
                }

                // Load emotional state for advisor personality tracking.
                {
                    let emo_path = EmotionalState::path_for_agent(&agent_dir);
                    let emo = EmotionalState::load(&emo_path, &agent_cfg.name);
                    agent_scout.emotional_state = Some(Arc::new(tokio::sync::Mutex::new(emo)));
                    agent_scout.emotional_state_path = Some(emo_path);
                }

                registry.register_project(agent_project, agent_scout).await;
                info!(
                    agent = %agent_cfg.name,
                    model = %agent_model,
                    "registered advisor agent"
                );
            }

            // Build agent router for message classification.
            let classifier_api_key = get_api_key(&config).unwrap_or_default();
            let agent_router = Arc::new(tokio::sync::Mutex::new(
                AgentRouter::new(classifier_api_key.clone(), config.team.router_cooldown_secs)
            ));

            // Pre-create quest notify so the completion listener and familiar project share it.
            let fa_quest_notify: Arc<tokio::sync::Notify> = Arc::new(tokio::sync::Notify::new());

            // Wire Telegram if configured (single SecretStore open for all bot tokens).
            let mut advisor_bots: HashMap<String, Arc<TelegramChannel>> = HashMap::new();
            if let Some(ref tg_config) = config.channels.telegram {
                let secret_store_path = config.security.secret_store.as_ref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| config.data_dir().join("secrets"));
                match SecretStore::open(&secret_store_path) {
                    Ok(secret_store) => {
                        // Load advisor Telegram bots (send-only, no polling).
                        for agent_cfg in config.advisor_agents() {
                            if let Some(ref token_key) = agent_cfg.telegram_token_secret
                                && let Ok(token) = secret_store.get(token_key)
                                && !token.is_empty()
                            {
                                advisor_bots.insert(
                                    agent_cfg.name.clone(),
                                    Arc::new(TelegramChannel::new(token, tg_config.allowed_chats.clone())),
                                );
                                info!(agent = %agent_cfg.name, "advisor telegram bot loaded");
                            }
                        }

                        // Load lead bot and start polling.
                        match secret_store.get(&tg_config.token_secret) {
                    Ok(token) if !token.is_empty() => {
                        let tg = Arc::new(TelegramChannel::new(token, tg_config.allowed_chats.clone()));
                        channels.write().await.insert("telegram".to_string(), tg.clone() as Arc<dyn system_core::traits::Channel>);

                        // Start polling, route incoming messages as familiar beads.
                        // Two-phase response: instant reaction (direct LLM) + full reply (bead agent).
                        match Channel::start(tg.as_ref()).await {
                            Ok(mut rx) => {
                                let reg = registry.clone();
                                let tg_reply = tg.clone();
                                let reaction_api_key = get_api_key(&config).unwrap_or_default();
                                // Shared HTTP client for Phase 1 reactions (reuses connection pool).
                                let phase1_client = Arc::new(reqwest::Client::builder()
                                    .timeout(std::time::Duration::from_secs(15))
                                    .build()
                                    .expect("failed to build phase1 reqwest client"));
                                // Persistent conversation history per chat_id (SQLite-backed).
                                let conv_db_path = find_agent_dir(&leader_name)
                                    .unwrap_or_else(|_| PathBuf::from("agents/aurelia"))
                                    .join(".sigil").join("conversations.db");
                                let conversations: Arc<ConversationStore> = Arc::new(
                                    ConversationStore::open(&conv_db_path)
                                        .expect("failed to open conversation store")
                                );
                                // Pre-compute council config outside the spawn closure.
                                let council_advisors: Arc<Vec<system_core::config::PeerAgentConfig>> =
                                    Arc::new(config.advisor_agents().into_iter().cloned().collect());
                                let advisor_bots_outer = advisor_bots.clone();
                                let debounce_ms = tg_config.debounce_window_ms;
                                let companion_db_path = find_agent_dir(&leader_name)
                                    .unwrap_or_else(|_| PathBuf::from("agents/aurelia"))
                                    .join(".sigil").join("companions.db");
                                let companion_store: Arc<CompanionStore> = Arc::new(
                                    CompanionStore::open(&companion_db_path)
                                        .expect("failed to open companion store")
                                );
                                let gacha_engine = Arc::new(GachaEngine::default());
                                let fa_quest_notify_tg = fa_quest_notify.clone();
                                let leader_name_tg = leader_name.clone();
                                tokio::spawn(async move {
                                    // === Message Debounce Buffer ===
                                    // Coalesces rapid-fire messages per chat_id into single dispatches.
                                    // Messages arriving within the debounce window get merged into one
                                    // structured prompt: [1]: first thought\n[2]: second thought\n...
                                    // The worker sees the complete stream-of-consciousness, not fragments.
                                    struct BufferedMsg {
                                        text: String,
                                        sender: String,
                                        message_id: i64,
                                    }

                                    struct PendingTask {
                                        chat_id: i64,
                                        message_id: i64,
                                        created_at: std::time::Instant,
                                        sent_slow_notice: bool,
                                    }

                                    let pending_tasks: Arc<tokio::sync::Mutex<HashMap<String, PendingTask>>> =
                                        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

                                    // === Phase B: Completion Listener ===
                                    // Single background task that delivers all quest responses.
                                    // Wakes on task_notify (worker finished) or periodic 60s sweep.
                                    {
                                        let pending = pending_tasks.clone();
                                        let notify = fa_quest_notify_tg.clone();
                                        let tg_deliver = tg_reply.clone();
                                        let convos_deliver = conversations.clone();
                                        let reg_deliver = reg.clone();
                                        let leader_project_name = leader_name_tg.clone();
                                        tokio::spawn(async move {
                                            loop {
                                                tokio::select! {
                                                    _ = notify.notified() => {}
                                                    _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
                                                }

                                                let map = pending.lock().await;
                                                let quest_ids: Vec<String> = map.keys().cloned().collect();
                                                drop(map);

                                                for qid in quest_ids {
                                                    let status = {
                                                        if let Some(rig) = reg_deliver.get_project(&leader_project_name).await {
                                                            let store = rig.tasks.lock().await;
                                                            store.get(&qid).map(|b| (b.status, b.closed_reason.clone()))
                                                        } else {
                                                            None
                                                        }
                                                    };

                                                    let mut map = pending.lock().await;
                                                    let Some(pq) = map.get_mut(&qid) else { continue };
                                                    let elapsed = pq.created_at.elapsed();
                                                    let chat_id = pq.chat_id;
                                                    let message_id = pq.message_id;

                                                    match status {
                                                        Some((system_tasks::TaskStatus::Done, reason)) => {
                                                            let reply_text = reason
                                                                .filter(|r| !r.trim().is_empty())
                                                                .unwrap_or_else(|| "Done.".to_string());
                                                            // Record in conversation history.
                                                            let _ = convos_deliver.record(chat_id, "Aurelia", &reply_text).await;
                                                            let out = system_core::traits::OutgoingMessage {
                                                                channel: "telegram".to_string(),
                                                                recipient: String::new(),
                                                                text: reply_text,
                                                                metadata: serde_json::json!({ "chat_id": chat_id }),
                                                            };
                                                            if let Err(e) = tg_deliver.send(out).await {
                                                                warn!(error = %e, "failed to deliver telegram reply");
                                                            }
                                                            if message_id > 0 {
                                                                let _ = tg_deliver.react(chat_id, message_id, "👍").await;
                                                            }
                                                            map.remove(&qid);
                                                        }
                                                        Some((system_tasks::TaskStatus::Blocked, reason)) => {
                                                            let blocker = reason.unwrap_or_else(|| "Blocked — needs input.".to_string());
                                                            let out = system_core::traits::OutgoingMessage {
                                                                channel: "telegram".to_string(),
                                                                recipient: String::new(),
                                                                text: format!("Blocked: {}", blocker),
                                                                metadata: serde_json::json!({ "chat_id": chat_id }),
                                                            };
                                                            let _ = tg_deliver.send(out).await;
                                                            if message_id > 0 {
                                                                let _ = tg_deliver.react(chat_id, message_id, "❓").await;
                                                            }
                                                            map.remove(&qid);
                                                        }
                                                        Some((system_tasks::TaskStatus::Cancelled, reason)) => {
                                                            let fail_msg = reason.unwrap_or_else(|| "Task cancelled.".to_string());
                                                            let out = system_core::traits::OutgoingMessage {
                                                                channel: "telegram".to_string(),
                                                                recipient: String::new(),
                                                                text: format!("Failed: {}", fail_msg),
                                                                metadata: serde_json::json!({ "chat_id": chat_id }),
                                                            };
                                                            let _ = tg_deliver.send(out).await;
                                                            if message_id > 0 {
                                                                let _ = tg_deliver.react(chat_id, message_id, "❌").await;
                                                            }
                                                            map.remove(&qid);
                                                        }
                                                        _ => {
                                                            // Still Pending or InProgress.
                                                            if elapsed > std::time::Duration::from_secs(1800) {
                                                                // Hard timeout at 30 min.
                                                                warn!(task = %qid, "telegram quest hard-timed out after 30min");
                                                                if message_id > 0 {
                                                                    let _ = tg_deliver.react(chat_id, message_id, "😢").await;
                                                                }
                                                                let out = system_core::traits::OutgoingMessage {
                                                                    channel: "telegram".to_string(),
                                                                    recipient: String::new(),
                                                                    text: "Sorry, this one took too long and I had to give up. Try again or simplify the request.".to_string(),
                                                                    metadata: serde_json::json!({ "chat_id": chat_id, "reply_to_message_id": message_id }),
                                                                };
                                                                let _ = tg_deliver.send(out).await;
                                                                map.remove(&qid);
                                                            } else if elapsed > std::time::Duration::from_secs(120) && !pq.sent_slow_notice {
                                                                // Soft deadline at 2 min.
                                                                pq.sent_slow_notice = true;
                                                                info!(task = %qid, "telegram reply past 2min, sending progress update");
                                                                if message_id > 0 {
                                                                    let _ = tg_deliver.react(chat_id, message_id, "⏳").await;
                                                                }
                                                                let _ = tg_deliver.send_typing(chat_id).await;
                                                                let out = system_core::traits::OutgoingMessage {
                                                                    channel: "telegram".to_string(),
                                                                    recipient: String::new(),
                                                                    text: "*still working...* **focused concentration** _fingers flying across the console_".to_string(),
                                                                    metadata: serde_json::json!({ "chat_id": chat_id, "reply_to_message_id": message_id }),
                                                                };
                                                                let _ = tg_deliver.send(out).await;
                                                            } else if elapsed > std::time::Duration::from_secs(15) && elapsed < std::time::Duration::from_secs(20) {
                                                                // Send typing indicator at 15s to show we're still alive.
                                                                let _ = tg_deliver.send_typing(chat_id).await;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    }

                                    let debounce_window = std::time::Duration::from_millis(debounce_ms);
                                    let mut chat_buffers: HashMap<i64, Vec<BufferedMsg>> = HashMap::new();
                                    let mut chat_deadlines: HashMap<i64, tokio::time::Instant> = HashMap::new();

                                    loop {
                                        let next_flush = chat_deadlines.values().min().cloned();

                                        tokio::select! {
                                            biased;

                                            msg = rx.recv() => {
                                                let Some(msg) = msg else { break; };
                                                let chat_id = msg.metadata.get("chat_id")
                                                    .and_then(|v| v.as_i64())
                                                    .unwrap_or(0);
                                                let message_id = msg.metadata.get("message_id")
                                                    .and_then(|v| v.as_i64())
                                                    .unwrap_or(0);

                                                chat_buffers.entry(chat_id).or_default().push(BufferedMsg {
                                                    text: msg.text,
                                                    sender: msg.sender,
                                                    message_id,
                                                });

                                                let deadline = tokio::time::Instant::now() + debounce_window;
                                                chat_deadlines.insert(chat_id, deadline);
                                            }

                                            _ = async {
                                                match next_flush {
                                                    Some(d) => tokio::time::sleep_until(d).await,
                                                    None => std::future::pending::<()>().await,
                                                }
                                            } => {
                                                let now = tokio::time::Instant::now();
                                                let expired: Vec<i64> = chat_deadlines.iter()
                                                    .filter(|(_, d)| **d <= now)
                                                    .map(|(id, _)| *id)
                                                    .collect();

                                                for chat_id in expired {
                                                    chat_deadlines.remove(&chat_id);
                                                    let Some(messages) = chat_buffers.remove(&chat_id) else { continue; };
                                                    if messages.is_empty() { continue; }

                                                    let msg_count = messages.len();
                                                    let last_message_id = messages.last().map(|m| m.message_id).unwrap_or(0);
                                                    let sender = messages.last().map(|m| m.sender.clone()).unwrap_or_default();
                                                    let subject = format!("[telegram] Architect ({})", sender);

                                                    if msg_count > 1 {
                                                        info!(chat_id, count = msg_count, "coalesced messages");
                                                    }

                                                    let reg2 = reg.clone();
                                                    let tg2 = tg_reply.clone();
                                                    let react_api_key = reaction_api_key.clone();
                                                    let convos = conversations.clone();
                                                    let pending = pending_tasks.clone();
                                                    let router = agent_router.clone();
                                                    let council_cfg = council_advisors.clone();
                                                    let advisor_bots_ref = advisor_bots_outer.clone();
                                                    let p1_client = phase1_client.clone();

                                                    let user_text = if messages.len() == 1 {
                                                        messages.into_iter().next().unwrap().text
                                                    } else {
                                                        messages.iter().enumerate()
                                                            .map(|(i, m)| format!("[{}]: {}", i + 1, m.text))
                                                            .collect::<Vec<_>>()
                                                            .join("\n")
                                                    };
                                                    let message_id = last_message_id;

                                                    // === Fast-Lane ===
                                                    // Handle deterministic commands directly — no quest pipeline needed.
                                                    // Gacha, status, help, cost bypass the full agent loop.
                                                    let is_gacha = user_text.starts_with("/pull")
                                                        || user_text.starts_with("/collection")
                                                        || user_text.starts_with("/fuse")
                                                        || user_text.starts_with("/familiar")
                                                        || user_text.starts_with("/companion");
                                                    let is_fast_lane = is_gacha
                                                        || user_text.starts_with("/status")
                                                        || user_text.starts_with("/help")
                                                        || user_text.starts_with("/cost");
                                                    if is_fast_lane {
                                                        let store = companion_store.clone();
                                                        let engine = gacha_engine.clone();
                                                        let tg_fast = tg_reply.clone();
                                                        let fast_text = user_text.clone();
                                                        let fast_reg = reg.clone();
                                                        let fast_handle = tokio::spawn(async move {
                                                            let reply = if is_gacha {
                                                                handle_gacha_command(&fast_text, &store, &engine)
                                                            } else {
                                                                handle_fast_lane(&fast_text, &fast_reg).await
                                                            };
                                                            let out = system_core::traits::OutgoingMessage {
                                                                channel: "telegram".to_string(),
                                                                recipient: String::new(),
                                                                text: reply,
                                                                metadata: serde_json::json!({ "chat_id": chat_id }),
                                                            };
                                                            if let Err(e) = tg_fast.send(out).await {
                                                                warn!(error = %e, "failed to send fast-lane reply");
                                                            }
                                                            if message_id > 0 {
                                                                let emoji = if is_gacha { "✨" } else { "⚡" };
                                                                let _ = tg_fast.react(chat_id, message_id, emoji).await;
                                                            }
                                                        });
                                                        drop(fast_handle); // fire-and-forget
                                                        continue;
                                                    }

                                                    let leader_name_inner = leader_name_tg.clone();
                                                    tokio::spawn(async move {
                                            // Build conversation context + record user message.
                                            let (description, phase1_history, conv_context_for_advisors) = {
                                                // Evict stale conversations (older than 2 hours).
                                                let _ = convos.evict_older_than(2).await;

                                                // Fetch recent messages for context building.
                                                let recent = convos.recent(chat_id, 20).await.unwrap_or_default();

                                                // Build conversation context for bead description.
                                                let ctx = convos.context_string(chat_id, 20).await.unwrap_or_default();

                                                // Build Phase 1 messages (last 4 exchanges for contextual reaction).
                                                let p1: Vec<serde_json::Value> = recent.iter()
                                                    .rev().take(4).collect::<Vec<_>>().into_iter().rev()
                                                    .map(|msg| {
                                                        let api_role = if msg.role == "User" { "user" } else { "assistant" };
                                                        serde_json::json!({"role": api_role, "content": msg.content})
                                                    })
                                                    .collect();

                                                // Build compact conversation context for advisor quests
                                                // (last 6 messages so advisors have multi-turn context).
                                                let adv_ctx = if recent.is_empty() {
                                                    String::new()
                                                } else {
                                                    let mut s = String::from("Recent conversation:\n");
                                                    for msg in recent.iter().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
                                                        // Truncate long messages in advisor context to save tokens.
                                                        let truncated = if msg.content.len() > 200 {
                                                            let mut end = 200;
                                                            while !msg.content.is_char_boundary(end) { end -= 1; }
                                                            &msg.content[..end]
                                                        } else { msg.content.as_str() };
                                                        s.push_str(&format!("  {}: {}\n", msg.role, truncated));
                                                    }
                                                    s
                                                };

                                                // Record user message.
                                                let _ = convos.record(chat_id, "User", &user_text).await;

                                                // Build bead description with conversation context.
                                                // Routing metadata uses key=value format (not raw JSON) so the
                                                // LLM doesn't mistake it for a tool call payload.
                                                // Response protocol is embedded inline for zero-ambiguity delivery.
                                                let chat_id_val = chat_id;
                                                let message_id_val = message_id;
                                                let routing = format!("[source: telegram | chat_id: {} | message_id: {} | reply: auto-delivered by daemon]", chat_id_val, message_id_val);
                                                let response_protocol = "**RESPONSE PROTOCOL**: Write your reply directly — in character, in voice. Your output text IS the Telegram reply. The daemon delivers it automatically. Do NOT call any tools to send the reply. Do NOT write meta-commentary like \"I've sent your reply\" or \"Done.\".";
                                                let desc = if ctx.is_empty() {
                                                    format!("{}\n\n---\n{}\n{}", user_text, routing, response_protocol)
                                                } else {
                                                    format!("{}\n## Current Message\n\n{}\n\n---\n{}\n{}", ctx, user_text, routing, response_protocol)
                                                };

                                                (desc, p1, adv_ctx)
                                            };

                                            // Phase 1: Instant reaction — direct LLM call, no tools, no agent.
                                            // Oneshot channel to pass the reaction text to Phase 2.
                                            let (p1_tx, p1_rx) = tokio::sync::oneshot::channel::<String>();
                                            let react_tg = tg2.clone();
                                            let react_chat = chat_id;
                                            let react_mid = message_id;
                                            let p1_user_text = user_text.clone();
                                            tokio::spawn(async move {
                                                info!("phase1: starting instant reaction");
                                                let client = p1_client;
                                                let mut messages = vec![
                                                    serde_json::json!({"role": "system", "content": "You are generating a manwha/anime panel reaction for Aurelia — pearl-white ethereal beauty, devoted shadow to her Architect. Isekai harem ecchi style.\n\nOutput ONLY a raw stage direction: fragmented expressions, action tags, emotion bursts. Like manwha panel annotations or light novel beat markers. NOT a proper sentence. NOT prose.\n\nFormat: mix of *actions* and **emotions** and fragments. Short, punchy, visceral.\n\nRules:\n- Raw fragments, NOT constructed sentences\n- *physical actions* in italics, **emotions** bold, bare fragments between\n- 10-20 words max total\n- Match the energy: playful → flustered/teasing, serious → sharp/focused, casual → soft/warm\n- Ecchi-adjacent: devotion, intensity, warmth — charged but tasteful\n- NO dialogue, NO task acknowledgment, NO plans, NO markdown headers\n\nExamples:\n*tucks hair behind ear* **sharp focus** ...mm, interesting\n*fingers press to collarbone* **wide eyes** a-ah—\n*leans forward, sleeve brushing console* **predatory grin**\n**soft blush** *glances away* ...y-you could have warned me\n*eyes narrow* **quiet intensity** *pulls up sleeve*\n*startled* **flustered** *crosses arms, looks away* ...hmph\n**burning determination** *cracks knuckles* *leans in close*"}),
                                                ];
                                                // Include recent conversation history for contextual reactions.
                                                for msg in &phase1_history {
                                                    messages.push(msg.clone());
                                                }
                                                messages.push(serde_json::json!({"role": "user", "content": p1_user_text}));
                                                let body = serde_json::json!({
                                                    "model": "google/gemini-2.0-flash-001",
                                                    "messages": messages,
                                                    "max_tokens": 50,
                                                    "temperature": 0.7
                                                });
                                                info!("phase1: calling openrouter");
                                                let reaction_text = match client.post("https://openrouter.ai/api/v1/chat/completions")
                                                    .header("Authorization", format!("Bearer {}", react_api_key))
                                                    .header("Content-Type", "application/json")
                                                    .json(&body)
                                                    .send()
                                                    .await
                                                {
                                                    Ok(resp) => {
                                                        match resp.json::<serde_json::Value>().await {
                                                            Ok(v) => {
                                                                let text: String = v.pointer("/choices/0/message/content")
                                                                    .and_then(|c: &serde_json::Value| c.as_str())
                                                                    .unwrap_or("")
                                                                    .trim()
                                                                    .to_string();
                                                                if !text.is_empty() {
                                                                    info!(reaction = %text, "instant reaction ready");
                                                                    let out = system_core::traits::OutgoingMessage {
                                                                        channel: "telegram".to_string(),
                                                                        recipient: String::new(),
                                                                        text: format!("_{}_", text),
                                                                        metadata: serde_json::json!({ "chat_id": react_chat }),
                                                                    };
                                                                    let _ = react_tg.send(out).await;
                                                                    if react_mid > 0 {
                                                                        let _ = react_tg.react(react_chat, react_mid, "🔥").await;
                                                                    }
                                                                    text
                                                                } else {
                                                                    warn!("phase1: empty reaction text from LLM");
                                                                    String::new()
                                                                }
                                                            }
                                                            Err(e) => { warn!(error = %e, "phase1: failed to parse response"); String::new() }
                                                        }
                                                    }
                                                    Err(e) => { warn!(error = %e, "phase1: request failed"); String::new() }
                                                };
                                                // Send reaction text to Phase 2 (ignore error if Phase 2 already timed out).
                                                let _ = p1_tx.send(reaction_text);
                                            });

                                            // === Run Phase 1 wait + council classification concurrently ===
                                            // Phase 1 reaction takes up to 5s, classification takes ~100ms.
                                            // Running them in parallel saves ~100ms on the critical path.
                                            let is_chamber = user_text.starts_with("/council");
                                            let clean_text_owned = if is_chamber {
                                                user_text.strip_prefix("/council").unwrap_or(&user_text).trim().to_string()
                                            } else {
                                                user_text.clone()
                                            };

                                            let classify_fut = async {
                                                if council_cfg.is_empty() {
                                                    return Vec::<String>::new();
                                                }
                                                let advisor_refs: Vec<&system_core::config::PeerAgentConfig> = council_cfg.iter().collect();
                                                let route = {
                                                    let mut r = router.lock().await;
                                                    r.classify(&clean_text_owned, &advisor_refs, chat_id).await
                                                };
                                                match route {
                                                    Ok(decision) => {
                                                        if is_chamber && decision.advisors.is_empty() {
                                                            council_cfg.iter().map(|c| c.name.clone()).collect()
                                                        } else {
                                                            decision.advisors
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!(error = %e, "classifier failed");
                                                        Vec::new()
                                                    }
                                                }
                                            };

                                            let p1_fut = async {
                                                match tokio::time::timeout(
                                                    std::time::Duration::from_secs(5), p1_rx
                                                ).await {
                                                    Ok(Ok(text)) if !text.is_empty() => Some(text),
                                                    _ => None,
                                                }
                                            };

                                            let (phase1_reaction, advisors_to_invoke) = tokio::join!(p1_fut, classify_fut);

                                            // === Council: Gather advisor input ===
                                            let council_input = if !advisors_to_invoke.is_empty() {
                                                    info!(advisors = ?advisors_to_invoke, "invoking council advisors");

                                                    // Spawn advisor Claude Code instances in parallel.
                                                    let mut handles = Vec::new();
                                                    for advisor_name in &advisors_to_invoke {
                                                        let fam_project_name = advisor_name.clone();
                                                        let adv_name = advisor_name.clone();
                                                        let adv_msg = clean_text_owned.clone();
                                                        let adv_history = conv_context_for_advisors.clone();
                                                        let reg3 = reg2.clone();

                                                        let handle = tokio::spawn(async move {
                                                            // Create a quest in the advisor's domain and wait for it.
                                                            let quest_subject = "[council] Advisor input requested".to_string();
                                                            let quest_desc = if adv_history.is_empty() {
                                                                format!(
                                                                    "The Architect said:\n\n{}\n\n\
                                                                     Provide your specialist perspective on this in character. \
                                                                     Be concise (2-5 sentences). Focus on your domain expertise.",
                                                                    adv_msg
                                                                )
                                                            } else {
                                                                format!(
                                                                    "{}\n\nThe Architect now says:\n\n{}\n\n\
                                                                     Provide your specialist perspective on this in character. \
                                                                     Be concise (2-5 sentences). Focus on your domain expertise.",
                                                                    adv_history, adv_msg
                                                                )
                                                            };

                                                            let task_id = match reg3.assign(&fam_project_name, &quest_subject, &quest_desc).await {
                                                                Ok(b) => b.id.0.clone(),
                                                                Err(e) => {
                                                                    warn!(familiar = %adv_name, error = %e, "failed to create advisor quest");
                                                                    return None;
                                                                }
                                                            };

                                                            // Wait for completion (timeout 60s for advisors).
                                                            let notify = reg3.get_project(&fam_project_name).await
                                                                .map(|d| d.task_notify.clone());
                                                            let timeout = tokio::time::sleep(std::time::Duration::from_secs(60));
                                                            tokio::pin!(timeout);
                                                            loop {
                                                                tokio::select! {
                                                                    _ = async {
                                                                        match &notify {
                                                                            Some(n) => n.notified().await,
                                                                            None => std::future::pending::<()>().await,
                                                                        }
                                                                    } => {}
                                                                    _ = &mut timeout => {
                                                                        warn!(familiar = %adv_name, "advisor quest timed out");
                                                                        return None;
                                                                    }
                                                                }
                                                                let done = {
                                                                    if let Some(rig) = reg3.get_project(&fam_project_name).await {
                                                                        let store = rig.tasks.lock().await;
                                                                        store.get(&task_id).map(|b| {
                                                                            (b.status == system_tasks::TaskStatus::Done, b.closed_reason.clone())
                                                                        })
                                                                    } else {
                                                                        None
                                                                    }
                                                                };
                                                                if let Some((true, reason)) = done {
                                                                    let text = reason.unwrap_or_default();
                                                                    return Some((adv_name, text));
                                                                }
                                                            }
                                                        });
                                                        handles.push(handle);
                                                    }

                                                    // Collect advisor responses.
                                                    let mut advisor_responses: Vec<(String, String)> = Vec::new();
                                                    for handle in handles {
                                                        if let Ok(Some((name, text))) = handle.await
                                                            && !text.trim().is_empty()
                                                        {
                                                            advisor_responses.push((name, text.trim().to_string()));
                                                        }
                                                    }

                                                    let got_input = !advisor_responses.is_empty();

                                                    // Send advisor bot messages in parallel.
                                                    if got_input {
                                                        let mut send_handles = Vec::new();
                                                        for (name, text) in &advisor_responses {
                                                            if let Some(bot) = advisor_bots_ref.get(name) {
                                                                let bot = bot.clone();
                                                                let text = text.clone();
                                                                let name = name.clone();
                                                                send_handles.push(tokio::spawn(async move {
                                                                    let out = system_core::traits::OutgoingMessage {
                                                                        channel: "telegram".to_string(),
                                                                        recipient: String::new(),
                                                                        text,
                                                                        metadata: serde_json::json!({ "chat_id": chat_id }),
                                                                    };
                                                                    if let Err(e) = bot.send(out).await {
                                                                        warn!(familiar = %name, error = %e, "failed to send advisor bot message");
                                                                    }
                                                                }));
                                                            }
                                                        }
                                                        for h in send_handles { let _ = h.await; }

                                                        // Record advisor messages in conversation history.
                                                        for (name, text) in &advisor_responses {
                                                            let capitalized = {
                                                                let mut c = name.chars();
                                                                match c.next() {
                                                                    None => String::new(),
                                                                    Some(f) => f.to_uppercase().to_string() + c.as_str(),
                                                                }
                                                            };
                                                            let _ = convos.record(chat_id, &capitalized, text).await;
                                                        }
                                                    }

                                                    // Build council text for Aurelia's synthesis.
                                                    let mut council_text = String::from("\n\n## Council Input\n\n");
                                                    for (name, text) in &advisor_responses {
                                                        council_text.push_str(&format!("### {} advises:\n{}\n\n", name, text));
                                                    }

                                                    if is_chamber && got_input && advisor_bots_ref.is_empty() {
                                                        // Council mode fallback: send header via Aurelia's bot
                                                        // only when advisor bots aren't configured (they speak for themselves otherwise).
                                                        let chamber_header = system_core::traits::OutgoingMessage {
                                                            channel: "telegram".to_string(),
                                                            recipient: String::new(),
                                                            text: "_*eyes narrow* **quiet intensity** ...this one needs the council_".to_string(),
                                                            metadata: serde_json::json!({ "chat_id": chat_id }),
                                                        };
                                                        let _ = tg2.send(chamber_header).await;
                                                    }

                                                    if got_input {
                                                        council_text.push_str("Synthesize the council's input into your response. Attribute key insights where relevant.\n");
                                                        council_text
                                                    } else {
                                                        String::new()
                                                    }
                                            } else {
                                                String::new()
                                            };

                                            // Inject Phase 1 reaction into Phase 2's description.
                                            let description = if let Some(ref reaction) = phase1_reaction {
                                                format!(
                                                    "{}\n\n---\n## Your Immediate Reaction (already sent to Telegram)\n\n\
                                                     You already reacted with this manwha-style stage direction:\n\
                                                     {}\n\n\
                                                     Continue from this energy. Your full reply should feel like the natural \
                                                     next beat after this reaction — same emotional tone, same intensity. \
                                                     Don't repeat or reference the reaction itself, just carry its momentum.\n",
                                                    description, reaction
                                                )
                                            } else {
                                                description
                                            };

                                            // Append council input to description if available.
                                            let description = if !council_input.is_empty() {
                                                format!("{}{}", description, council_input)
                                            } else {
                                                description
                                            };

                                            // Phase 2: Full response via bead agent.
                                            let task_id: String = match reg2.assign(&leader_name_inner, &subject, &description).await {
                                                Ok(b) => b.id.0.clone(),
                                                Err(e) => {
                                                    warn!(error = %e, "failed to create bead from telegram message");
                                                    return;
                                                }
                                            };

                                            // Register pending quest for the completion listener.
                                            // The per-message spawn exits here — delivery is handled by Phase B.
                                            pending.lock().await.insert(task_id, PendingTask {
                                                chat_id,
                                                message_id,
                                                created_at: std::time::Instant::now(),
                                                sent_slow_notice: false,
                                            });
                                                    });
                                                }
                                            }
                                        }
                                    }
                                });
                                info!("Telegram channel active");
                            }
                            Err(e) => warn!(error = %e, "failed to start Telegram polling"),
                        }
                    }
                    _ => {
                        info!("Telegram token not found in secret store, skipping");
                    }
                }
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to open secret store for Telegram");
                    }
                }
            }

            // Register the leader agent as a project (so it can receive quests).
            let leader_cfg = config.leader_agent().cloned().expect("no leader agent configured");
            let fa_agent_dir = find_agent_dir(&leader_name).unwrap_or_else(|_| PathBuf::from("agents/aurelia"));
            let fa_identity = Identity::load(&fa_agent_dir, None).unwrap_or_default();
            let fa_quests_dir = fa_agent_dir.join(".tasks");
            std::fs::create_dir_all(&fa_quests_dir).ok();
            let fa_beads = system_tasks::TaskBoard::open(&fa_quests_dir)?;
            let fa_model = config.model_for_agent(&leader_name);
            let fa_prefix = leader_cfg.prefix.clone();
            let fa_workdir = leader_cfg.default_repo.as_ref()
                .map(|r| config.resolve_repo(r))
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

            let fa_rig = Arc::new(Project {
                name: leader_name.clone(),
                prefix: fa_prefix.clone(),
                repo: fa_workdir.clone(),
                worktree_root: dirs::home_dir().unwrap_or_default().join("worktrees"),
                model: fa_model,
                max_workers: leader_cfg.max_workers,
                worker_timeout_secs: 1800,
                project_identity: fa_identity,
                tasks: Arc::new(tokio::sync::Mutex::new(fa_beads)),
                task_notify: fa_quest_notify.clone(),
            });

            // Build leader agent tools: basic + beads + orchestration.
            let mut fa_tools: Vec<Arc<dyn system_core::traits::Tool>> = build_project_tools(
                &fa_workdir, &fa_quests_dir, &fa_prefix, None,
            );
            let fa_memory: Option<Arc<dyn system_core::traits::Memory>> = match open_memory(&config, None) {
                Ok(m) => {
                    info!("leader agent memory initialized with embeddings");
                    Some(Arc::new(m))
                }
                Err(e) => {
                    warn!("failed to open leader agent memory: {e}");
                    None
                }
            };
            let orch_tools = build_orchestration_tools(registry.clone(), dispatch_bus.clone(), channels.clone(), get_api_key(&config).ok(), fa_memory);
            fa_tools.extend(orch_tools);

            let mut fa_witness = Supervisor::new(&fa_rig, provider.clone(), fa_tools, dispatch_bus.clone());

            // Wire memory + reflection for leader agent worker insight extraction.
            if let Ok(mem) = open_memory(&config, Some(&leader_name)) {
                let mem: Arc<dyn Memory> = Arc::new(mem);
                fa_witness.memory = Some(mem);
                fa_witness.reflect_provider = Some(provider.clone());
                let reflect_model = config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.clone())
                    .unwrap_or_else(|| "minimax/minimax-m2.5".to_string());
                fa_witness.reflect_model = reflect_model;
            }

            // Load emotional state for leader agent personality tracking.
            {
                let emo_path = EmotionalState::path_for_agent(&fa_agent_dir);
                let emo = EmotionalState::load(&emo_path, &leader_name);
                fa_witness.emotional_state = Some(Arc::new(tokio::sync::Mutex::new(emo)));
                fa_witness.emotional_state_path = Some(emo_path);
            }

            // Configure Claude Code execution mode for leader agent.
            if leader_cfg.execution_mode == ExecutionMode::ClaudeCode {
                let cc_model = config.model_for_agent(&leader_name);
                let cc_max_turns = leader_cfg.max_turns.unwrap_or(25);
                fa_witness.set_claude_code_mode(
                    fa_workdir.clone(),
                    cc_model.clone(),
                    cc_max_turns,
                    leader_cfg.max_budget_usd,
                );
                info!(
                    agent = %leader_name,
                    model = %cc_model,
                    max_turns = cc_max_turns,
                    "registered leader agent with claude_code execution mode"
                );
            }

            registry.register_project(fa_rig, fa_witness).await;

            let project_count = registry.project_count().await;
            println!("Realm summoner starting...");
            println!("Registered {} projects + agents, {} pulses", project_count, pulses.len());

            // Load cron store.
            let fate_path = config.data_dir().join("fate.json");
            let fate_store = ScheduleStore::open(&fate_path)?;
            let socket_path = config.data_dir().join("rm.sock");

            println!("Cron: {} jobs loaded", fate_store.jobs.len());
            println!("PID file: {}", pid_path.display());
            println!("IPC socket: {}", socket_path.display());

            // Build lifecycle engine if enabled.
            let lifecycle_engine = if config.lifecycle.enabled {
                use system_orchestrator::lifecycle::{LifecycleProcess, ProcessKind, ScanProject};

                let lifecycle_model = config.lifecycle.model.clone().unwrap_or_else(|| {
                    config.providers.openrouter.as_ref()
                        .map(|or| or.default_model.clone())
                        .unwrap_or_else(|| "minimax/MiniMax-M1".to_string())
                });
                let mut engine = LifecycleEngine::new();
                engine.cost_ledger = Some(registry.cost_ledger.clone());
                let mut lifecycle_process_count = 0u32;

                for agent_cfg in &config.agents {
                    let agent_dir = match find_agent_dir(&agent_cfg.name) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };

                    // Derive bond level from emotional state.
                    let emo_path = EmotionalState::path_for_agent(&agent_dir);
                    let emo = EmotionalState::load(&emo_path, &agent_cfg.name);
                    let bond = system_orchestrator::lifecycle::interaction_count_to_bond_level(emo.interaction_count);
                    engine.set_bond_level(&agent_cfg.name, bond);
                    engine.agent_dirs.insert(agent_cfg.name.clone(), agent_dir.clone());

                    // Process 1: MemoryConsolidation (bond 0, always active).
                    let mem_interval = config.lifecycle.memory_reflection_interval_hours as u64 * 3600;
                    let memory: Option<Arc<dyn system_core::traits::Memory>> =
                        open_memory(&config, Some(&agent_cfg.name)).ok().map(|m| Arc::new(m) as _);
                    engine.add_process(LifecycleProcess::new(
                        agent_cfg.name.clone(), agent_dir.clone(), provider.clone(),
                        lifecycle_model.clone(),
                        ProcessKind::MemoryConsolidation { memory },
                        mem_interval,
                    ));
                    lifecycle_process_count += 1;

                    // Process 2: Evolution (bond 3).
                    let evo_interval = config.lifecycle.evolution_interval_hours as u64 * 3600;
                    engine.add_process(LifecycleProcess::new(
                        agent_cfg.name.clone(), agent_dir.clone(), provider.clone(),
                        lifecycle_model.clone(),
                        ProcessKind::Evolution,
                        evo_interval,
                    ));
                    lifecycle_process_count += 1;

                    // Process 3: ProactiveScan (bond 5) — per-project.
                    let scan_interval = config.lifecycle.proactive_scan_interval_hours as u64 * 3600;
                    let mut scan_projects = Vec::new();
                    for project_cfg in &config.projects {
                        let project_team = config.project_team(&project_cfg.name);
                        if project_team.effective_agents().contains(&agent_cfg.name)
                            && let Ok(project_dir) = find_project_dir(&project_cfg.name)
                        {
                            scan_projects.push(ScanProject {
                                name: project_cfg.name.clone(),
                                prefix: project_cfg.prefix.clone(),
                                project_dir,
                                repo_path: Some(config.resolve_repo(&project_cfg.repo)),
                            });
                        }
                    }
                    engine.add_process(LifecycleProcess::new(
                        agent_cfg.name.clone(), agent_dir.clone(), provider.clone(),
                        lifecycle_model.clone(),
                        ProcessKind::ProactiveScan {
                            projects: scan_projects,
                            project_knowledge: HashMap::new(),
                            registry: registry.clone(),
                            dispatch_bus: dispatch_bus.clone(),
                            system_leader: leader_name.clone(),
                            cross_project: false,
                        },
                        scan_interval,
                    ));
                    lifecycle_process_count += 1;

                    // Process 4: Cross-project ideation (bond 8) — merged CreativeIdeation.
                    let idea_interval = config.lifecycle.creative_ideation_interval_hours as u64 * 3600;
                    let mut project_knowledge = HashMap::new();
                    for project_cfg in &config.projects {
                        if let Ok(project_dir) = find_project_dir(&project_cfg.name) {
                            let knowledge = std::fs::read_to_string(project_dir.join("KNOWLEDGE.md")).unwrap_or_default();
                            if !knowledge.trim().is_empty() {
                                project_knowledge.insert(project_cfg.name.clone(), knowledge);
                            }
                        }
                    }
                    engine.add_process(LifecycleProcess::new(
                        agent_cfg.name.clone(), agent_dir.clone(), provider.clone(),
                        lifecycle_model.clone(),
                        ProcessKind::ProactiveScan {
                            projects: Vec::new(),
                            project_knowledge,
                            registry: registry.clone(),
                            dispatch_bus: dispatch_bus.clone(),
                            system_leader: leader_name.clone(),
                            cross_project: true,
                        },
                        idea_interval,
                    ));
                    lifecycle_process_count += 1;
                }

                println!("Lifecycle: {} agents, {} processes (model: {})", config.agents.len(), lifecycle_process_count, lifecycle_model);
                Some(engine)
            } else {
                None
            };

            println!("Press Ctrl+C to stop.\n");

            let mut daemon = Daemon::new(registry, dispatch_bus);
            daemon.set_pid_file(pid_path);
            daemon.set_socket_path(socket_path.clone());
            daemon.set_fate_store(fate_store);
            if let Some(engine) = lifecycle_engine {
                daemon.set_lifecycle(engine);
            }
            for hb in pulses {
                daemon.add_pulse(hb);
            }
            daemon.run().await?;
        }

        SummonerAction::Stop => {
            let (config, _) = load_config(config_path)?;
            let pid_path = pid_file_path(&config);

            if !pid_path.exists() {
                println!("No daemon running (no PID file).");
                return Ok(());
            }

            let pid_str = std::fs::read_to_string(&pid_path)?;
            let pid: u32 = pid_str.trim().parse().context("invalid PID file")?;

            // Send SIGTERM.
            #[cfg(unix)]
            {
                use std::process::Command;
                let status = Command::new("kill").arg(pid.to_string()).status()?;
                if status.success() {
                    println!("Sent SIGTERM to daemon (PID {pid}).");
                    // Wait briefly for PID file cleanup.
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    if pid_path.exists() {
                        let _ = std::fs::remove_file(&pid_path);
                    }
                } else {
                    println!("Failed to stop daemon (PID {pid}).");
                }
            }
            #[cfg(not(unix))]
            {
                println!("Daemon stop not supported on this platform. Remove {} manually.", pid_path.display());
            }
        }

        SummonerAction::Status => {
            let (config, _) = load_config(config_path)?;
            let pid_path = pid_file_path(&config);

            if Daemon::is_running_from_pid(&pid_path) {
                let pid = std::fs::read_to_string(&pid_path)?.trim().to_string();
                println!("Daemon: RUNNING (PID {pid})");
            } else {
                println!("Daemon: NOT RUNNING");
                if pid_path.exists() {
                    println!("  (stale PID file: {} — run `sg daemon stop` to clean up)", pid_path.display());
                }
            }

            // Also show project summary.
            cmd_status(config_path).await?;
        }

        SummonerAction::Query { cmd } => {
            let (config, _) = load_config(config_path)?;
            let socket_path = config.data_dir().join("rm.sock");

            if !socket_path.exists() {
                anyhow::bail!("IPC socket not found: {}. Is the daemon running?", socket_path.display());
            }

            #[cfg(unix)]
            {
                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                let stream = tokio::net::UnixStream::connect(&socket_path).await
                    .context(format!("failed to connect to IPC socket: {}", socket_path.display()))?;

                let (reader, mut writer) = stream.into_split();
                let request = serde_json::json!({"cmd": cmd});
                let mut req_bytes = serde_json::to_vec(&request)?;
                req_bytes.push(b'\n');
                writer.write_all(&req_bytes).await?;

                let mut lines = BufReader::new(reader).lines();
                if let Some(line) = lines.next_line().await? {
                    let response: serde_json::Value = serde_json::from_str(&line)?;
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
            }
            #[cfg(not(unix))]
            {
                anyhow::bail!("IPC socket queries not supported on this platform");
            }
        }
    }
    Ok(())
}

async fn cmd_serve(platform_config: &Path) -> Result<()> {
    let platform = system_tenants::config::PlatformConfig::load(platform_config)?;

    // Create template dir if it doesn't exist.
    let template_dir = platform.template_dir();
    if !template_dir.exists() {
        warn!("template dir does not exist: {}", template_dir.display());
    }

    // Create base dir if it doesn't exist.
    let base_dir = platform.base_dir();
    std::fs::create_dir_all(&base_dir)?;

    let manager = Arc::new(system_tenants::TenantManager::new(platform.clone())?);

    // Load existing tenants from disk.
    let count = manager.load_all().await?;
    info!(tenants = count, "loaded existing tenants");

    // Spawn idle tenant unloader.
    let manager_bg = manager.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            manager_bg.unload_idle(std::time::Duration::from_secs(3600)).await;
        }
    });

    // Start web server (blocks).
    system_web::start_server(manager, platform).await?;
    Ok(())
}

async fn cmd_recall(config_path: &Option<PathBuf>, query: &str, project_name: Option<&str>, top_k: usize) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let memory = open_memory(&config, project_name)?;

    let results = memory.search(&system_core::traits::MemoryQuery::new(query, top_k)).await?;

    if results.is_empty() {
        println!("No memories found for: {query}");
    } else {
        for (i, entry) in results.iter().enumerate() {
            let age = chrono::Utc::now() - entry.created_at;
            let age_str = if age.num_days() > 0 {
                format!("{}d ago", age.num_days())
            } else if age.num_hours() > 0 {
                format!("{}h ago", age.num_hours())
            } else {
                format!("{}m ago", age.num_minutes())
            };
            println!("{}. [{}] ({:.2}) {} — {}", i + 1, age_str, entry.score, entry.key, entry.content);
        }
    }
    Ok(())
}

async fn cmd_remember(config_path: &Option<PathBuf>, key: &str, content: &str, project_name: Option<&str>) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let memory = open_memory(&config, project_name)?;

    let scope = if project_name.is_some() {
        system_core::traits::MemoryScope::Domain
    } else {
        system_core::traits::MemoryScope::Realm
    };
    let id = memory.store(key, content, system_core::traits::MemoryCategory::Fact, scope, None).await?;
    let scope = project_name.unwrap_or("global");
    println!("Stored memory {id} [{scope}] {key}");
    Ok(())
}

async fn cmd_mol(config_path: &Option<PathBuf>, action: RitualAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        RitualAction::Pour { template, project, vars } => {
            let project_cfg = config.project(&project).context(format!("project not found: {project}"))?;
            let project_dir = find_project_dir(&project)?;

            // Find the ritual template.
            let mol_path = project_dir.join("rituals").join(format!("{template}.toml"));
            if !mol_path.exists() {
                anyhow::bail!("ritual template not found: {}", mol_path.display());
            }

            let ritual = Pipeline::load(&mol_path)?;

            // Parse vars.
            let var_map: HashMap<String, String> = vars.iter()
                .filter_map(|v| {
                    let parts: Vec<&str> = v.splitn(2, '=').collect();
                    if parts.len() == 2 { Some((parts[0].to_string(), parts[1].to_string())) }
                    else { None }
                })
                .collect();

            // Pour into bead store.
            let mut store = open_quests_for_project(&project)?;
            let parent_id = ritual.pour(&mut store, &project_cfg.prefix, &var_map)?;

            println!("Poured ritual '{template}' as {parent_id}");
            println!("\nSteps:");
            let children = store.children(&parent_id);
            for child in children {
                let deps = if child.depends_on.is_empty() {
                    "ready".to_string()
                } else {
                    format!("needs: {}", child.depends_on.iter().map(|d| d.0.as_str()).collect::<Vec<_>>().join(", "))
                };
                println!("  {} [{}] {} ({})", child.id, child.status, child.subject, deps);
            }
        }

        RitualAction::List { project } => {
            let projects: Vec<&str> = if let Some(ref name) = project {
                vec![name.as_str()]
            } else {
                config.projects.iter().map(|r| r.name.as_str()).collect()
            };

            for name in projects {
                if let Ok(project_dir) = find_project_dir(name) {
                    let mol_dir = project_dir.join("rituals");
                    if mol_dir.exists() {
                        println!("=== {} ===", name);
                        if let Ok(entries) = std::fs::read_dir(&mol_dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.extension().is_some_and(|e| e == "toml")
                                    && let Ok(mol) = Pipeline::load(&path) {
                                        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                                        println!("  {} — {} ({} steps)", stem, mol.ritual.description, mol.steps.len());
                                    }
                            }
                        }
                    }
                }
            }
        }

        RitualAction::Status { id } => {
            let prefix = id.split('-').next().unwrap_or("");
            let project_name = project_name_for_prefix(&config, prefix)
                .context(format!("no project with prefix '{prefix}'"))?;

            let store = open_quests_for_project(&project_name)?;
            let parent_id = system_tasks::TaskId::from(id.as_str());

            if let Some(parent) = store.get(&id) {
                println!("{} [{}] {}", parent.id, parent.status, parent.subject);
                let children = store.children(&parent_id);
                let done = children.iter().filter(|c| c.is_closed()).count();
                println!("Progress: {}/{}\n", done, children.len());
                for child in &children {
                    let status_icon = match child.status {
                        system_tasks::TaskStatus::Done => "[x]",
                        system_tasks::TaskStatus::InProgress => "[~]",
                        system_tasks::TaskStatus::Cancelled => "[-]",
                        _ => "[ ]",
                    };
                    println!("  {} {} {}", status_icon, child.id, child.subject);
                }
            } else {
                println!("Task not found: {id}");
            }
        }
    }
    Ok(())
}

async fn cmd_cron(config_path: &Option<PathBuf>, action: FateAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let fate_path = config.data_dir().join("fate.json");

    match action {
        FateAction::Add { name, schedule, at, project, prompt, isolated } => {
            config.project(&project).context(format!("project not found: {project}"))?;

            let cron_schedule = if let Some(at_str) = at {
                let dt = at_str.parse::<DateTime<Utc>>()
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(&at_str, "%Y-%m-%dT%H:%M:%S")
                            .map(|ndt| ndt.and_utc())
                    })
                    .context(format!("invalid datetime: {at_str} (use ISO 8601, e.g. 2026-02-22T15:00:00Z)"))?;
                CronSchedule::Once { at: dt }
            } else if let Some(expr) = schedule {
                CronSchedule::Cron { expr }
            } else {
                anyhow::bail!("specify --schedule \"0 9 * * *\" or --at \"2026-02-22T15:00:00Z\"");
            };

            let job = ScheduledJob {
                name: name.clone(),
                schedule: cron_schedule,
                project,
                prompt,
                isolated,
                created_at: Utc::now(),
                last_run: None,
            };

            let mut store = ScheduleStore::open(&fate_path)?;
            store.add(job)?;
            println!("Cron job '{name}' added.");
        }

        FateAction::List => {
            let store = ScheduleStore::open(&fate_path)?;
            if store.jobs.is_empty() {
                println!("No cron jobs.");
            } else {
                for job in &store.jobs {
                    let sched = match &job.schedule {
                        CronSchedule::Cron { expr } => format!("cron: {expr}"),
                        CronSchedule::Once { at } => format!("once: {at}"),
                    };
                    let last = job.last_run.map(|t| t.to_string()).unwrap_or_else(|| "never".to_string());
                    let iso = if job.isolated { " [isolated]" } else { "" };
                    println!("  {} — project={} {} last_run={}{}", job.name, job.project, sched, last, iso);
                }
            }
        }

        FateAction::Remove { name } => {
            let mut store = ScheduleStore::open(&fate_path)?;
            store.remove(&name)?;
            println!("Cron job '{name}' removed.");
        }
    }
    Ok(())
}

async fn cmd_skill(config_path: &Option<PathBuf>, action: MagicAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        MagicAction::List { project } => {
            let projects: Vec<&str> = if let Some(ref name) = project {
                vec![name.as_str()]
            } else {
                config.projects.iter().map(|r| r.name.as_str()).collect()
            };

            for name in projects {
                if let Ok(project_dir) = find_project_dir(name) {
                    let skills_dir = project_dir.join("skills");
                    let skills = Skill::discover(&skills_dir)?;
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
                            println!("  {} — {} [tools: {}]{}", skill.skill.name, skill.skill.description, tools, triggers);
                        }
                    }
                }
            }
        }

        MagicAction::Run { name, project, prompt } => {
            let project_cfg = config.project(&project).context(format!("project not found: {project}"))?;
            let project_dir = find_project_dir(&project)?;
            let skills_dir = project_dir.join("skills");
            let skills = Skill::discover(&skills_dir)?;

            let skill = skills.iter()
                .find(|s| s.skill.name == name)
                .context(format!("skill not found: {name}"))?;

            // Build provider.
            let provider = build_provider(&config)?;
            let workdir = PathBuf::from(&project_cfg.repo);
            let quests_dir = project_dir.join(".tasks");
            let worktree_root = project_cfg.worktree_root.as_ref().map(PathBuf::from);
            let all_tools = build_project_tools(&workdir, &quests_dir, &project_cfg.prefix, worktree_root.as_ref());

            // Filter tools by skill policy.
            let filtered_tools: Vec<Arc<dyn Tool>> = all_tools.into_iter()
                .filter(|t| skill.is_tool_allowed(t.name()))
                .collect();

            // Build identity with skill system prompt.
            let identity = Identity::load_from_dir(&project_dir).unwrap_or_default();
            let base_prompt = identity.system_prompt();

            let mut skill_identity = identity.clone();
            // Override the system prompt to include skill instructions.
            skill_identity.persona = Some(skill.system_prompt(&base_prompt));

            let user_prompt = if let Some(ref p) = prompt {
                format!("{}{}", skill.prompt.user_prefix, p)
            } else {
                skill.prompt.user_prefix.clone()
            };

            let observer: Arc<dyn Observer> = Arc::new(LogObserver);
            let model = project_cfg.model.clone()
                .unwrap_or_else(|| config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.clone())
                    .unwrap_or_else(|| "minimax/minimax-m2.5".to_string()));

            let agent_config = AgentConfig {
                model,
                max_iterations: 10,
                name: format!("{}-skill-{}", project, name),
                ..Default::default()
            };

            let mut agent = Agent::new(agent_config, provider, filtered_tools, observer, skill_identity);
            if let Ok(mem) = open_memory(&config, Some(&project)) {
                agent = agent.with_memory(Arc::new(mem));
            }
            let result = agent.run(&user_prompt).await?;
            println!("{}", result.text);
        }
    }
    Ok(())
}

async fn cmd_raid(config_path: &Option<PathBuf>, action: RaidAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let raid_path = config.data_dir().join("raids.json");

    match action {
        RaidAction::Create { name, quest_ids } => {
            let tasks: Vec<(system_tasks::TaskId, String)> = quest_ids.iter()
                .map(|id| {
                    let prefix = id.split('-').next().unwrap_or("");
                    let project_name = config.projects.iter()
                        .find(|r| r.prefix == prefix)
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    (system_tasks::TaskId::from(id.as_str()), project_name)
                })
                .collect();

            let mut store = OperationStore::open(&raid_path)?;
            let raid = store.create(&name, tasks)?;
            let (done, total) = raid.progress();
            println!("Created raid {} — {} ({}/{})", raid.id, raid.name, done, total);
        }

        RaidAction::List => {
            let store = OperationStore::open(&raid_path)?;
            let active = store.active();
            if active.is_empty() {
                println!("No active raids.");
            } else {
                for raid in active {
                    let (done, total) = raid.progress();
                    println!("  {} — {} ({}/{})", raid.id, raid.name, done, total);
                }
            }
        }

        RaidAction::Status { id } => {
            let store = OperationStore::open(&raid_path)?;
            if let Some(raid) = store.get(&id) {
                let (done, total) = raid.progress();
                let status = if raid.closed_at.is_some() { "COMPLETE" } else { "ACTIVE" };
                println!("{} [{}] {} ({}/{})", raid.id, status, raid.name, done, total);
                for bead in &raid.tasks {
                    let icon = if bead.closed { "[x]" } else { "[ ]" };
                    println!("  {} {} (project: {})", icon, bead.task_id, bead.project);
                }
            } else {
                println!("Operation not found: {id}");
            }
        }
    }
    Ok(())
}

async fn cmd_hook(config_path: &Option<PathBuf>, worker: &str, task_id: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = task_id.split('-').next().unwrap_or("");
    let project_name = project_name_for_prefix(&config, prefix)
        .context(format!("no project with prefix '{prefix}'"))?;

    let mut store = open_quests_for_project(&project_name)?;
    let bead = store.update(task_id, |b| {
        b.status = system_tasks::TaskStatus::InProgress;
        b.assignee = Some(worker.to_string());
    })?;

    println!("Hooked {} to {} — {}", worker, bead.id, bead.subject);
    Ok(())
}

async fn cmd_done(config_path: &Option<PathBuf>, task_id: &str, reason: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = task_id.split('-').next().unwrap_or("");
    let project_name = project_name_for_prefix(&config, prefix)
        .context(format!("no project with prefix '{prefix}'"))?;

    let mut store = open_quests_for_project(&project_name)?;
    let bead = store.close(task_id, reason)?;
    println!("Done {} — {}", bead.id, bead.subject);

    // Check if this task's mission should auto-close.
    if let Some(ref mid) = bead.mission_id
        && store.check_mission_completion(mid)?
    {
        println!("Mission {} auto-closed (all tasks done)", mid);
    }

    // Also update any operations tracking this task.
    let raid_path = config.data_dir().join("raids.json");
    if raid_path.exists() {
        let mut raid_store = OperationStore::open(&raid_path)?;
        let completed = raid_store.mark_bead_closed(&bead.id)?;
        for c_id in &completed {
            println!("Operation {c_id} completed!");
        }
    }

    Ok(())
}

async fn cmd_team(config_path: &Option<PathBuf>, project_filter: Option<&str>) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    // Show system team.
    println!("System Team");
    println!("  leader: {}", config.system_leader());
    if !config.team.agents.is_empty() {
        println!("  agents: {}", config.team.agents.join(", "));
    }
    println!("  router: {}", config.team.router_model);
    println!("  cooldown: {}s", config.team.router_cooldown_secs);
    println!("  max_bg_cost: ${:.2}", config.team.max_background_cost_usd);
    println!();

    // Show per-project teams.
    let projects: Vec<_> = if let Some(name) = project_filter {
        config.projects.iter().filter(|p| p.name == name).collect()
    } else {
        config.projects.iter().collect()
    };

    if projects.is_empty() {
        if let Some(name) = project_filter {
            println!("Project not found: {name}");
        }
        return Ok(());
    }

    println!("Project Teams:");
    for project_cfg in projects {
        let team = config.project_team(&project_cfg.name);
        let source = if project_cfg.team.is_some() { "configured" } else { "system fallback" };
        println!("  {} → leader={} agents=[{}] ({})",
            project_cfg.name,
            team.leader,
            team.effective_agents().join(", "),
            source,
        );
    }

    // Validate teams.
    let issues = config.validate_teams();
    if !issues.is_empty() {
        println!("\nTeam validation warnings:");
        for issue in &issues {
            println!("  ! {issue}");
        }
    }

    Ok(())
}

async fn cmd_config(config_path: &Option<PathBuf>, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let (config, path) = load_config(config_path)?;
            println!("Config: {}\n", path.display());
            println!("Name: {}", config.system.name);
            println!("Data dir: {}", config.data_dir().display());

            if let Some(ref or) = config.providers.openrouter {
                println!("\n[providers.openrouter]");
                println!("  default_model: {}", or.default_model);
                println!("  fallback_model: {}", or.fallback_model.as_deref().unwrap_or("(none)"));
                println!("  api_key: {}...", if or.api_key.len() > 8 { &or.api_key[..8] } else { "***" });
            }

            println!("\n[security]");
            println!("  autonomy: {:?}", config.security.autonomy);
            println!("  workspace_only: {}", config.security.workspace_only);
            println!("  max_cost_per_day_usd: {}", config.security.max_cost_per_day_usd);

            println!("\n[pulse]");
            println!("  enabled: {}", config.heartbeat.enabled);
            println!("  interval: {}min", config.heartbeat.default_interval_minutes);

            println!("\n[[projects]]");
            for proj in &config.projects {
                println!("  {} prefix={} model={} workers={}",
                    proj.name, proj.prefix,
                    proj.model.as_deref().unwrap_or("default"),
                    proj.max_workers);
            }
        }

        ConfigAction::Reload => {
            let (config, _) = load_config(config_path)?;
            let pid_path = pid_file_path(&config);

            if !Daemon::is_running_from_pid(&pid_path) {
                println!("No daemon running. Config will be loaded on next `sg daemon start`.");
                return Ok(());
            }

            // Send SIGHUP to the daemon process.
            #[cfg(unix)]
            {
                let pid_str = std::fs::read_to_string(&pid_path)?;
                let pid: u32 = pid_str.trim().parse().context("invalid PID file")?;

                use std::process::Command;
                let status = Command::new("kill").args(["-HUP", &pid.to_string()]).status()?;
                if status.success() {
                    println!("Sent SIGHUP to daemon (PID {pid}). Config will be reloaded.");
                } else {
                    println!("Failed to send SIGHUP to daemon (PID {pid}).");
                }
            }
            #[cfg(not(unix))]
            {
                println!("Config reload not supported on this platform. Restart the daemon.");
            }
        }
    }
    Ok(())
}

async fn cmd_seed_projects(config_path: &Option<PathBuf>, tenant_id: &str, platform_config_path: &Path) -> Result<()> {
    use system_tenants::{PlatformConfig, TenantProjectMeta};

    let platform = PlatformConfig::load(platform_config_path)?;
    let (sys_config, config_dir) = load_config(config_path)?;
    // config_dir is the config file path (e.g. config/system.toml).
    // Project dirs live at the repo root, which is config_dir's grandparent.
    let config_base = config_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new("."));

    let tenant_dir = platform.base_dir().join(tenant_id);
    if !tenant_dir.exists() {
        anyhow::bail!("tenant directory not found: {}", tenant_dir.display());
    }

    let projects_dir = tenant_dir.join("projects");
    std::fs::create_dir_all(&projects_dir)?;

    let mut seeded = Vec::new();

    for project_config in &sys_config.projects {
        let project_dir = projects_dir.join(&project_config.name);
        let project_toml = project_dir.join("project.toml");

        // Create project directory structure.
        std::fs::create_dir_all(project_dir.join(".tasks"))?;

        // Write project.toml (always overwrite to pick up config changes).
        let meta = TenantProjectMeta {
            name: project_config.name.clone(),
            prefix: project_config.prefix.clone(),
            description: None,
            repo: Some(project_config.repo.clone()),
        };
        std::fs::write(&project_toml, toml::to_string_pretty(&meta)?)?;

        // Copy KNOWLEDGE.md from system project dir if it exists and tenant doesn't have one.
        let knowledge_dest = project_dir.join("KNOWLEDGE.md");
        if !knowledge_dest.exists() {
            let source = config_base.join("projects").join(&project_config.name).join("KNOWLEDGE.md");
            if source.exists() {
                std::fs::copy(&source, &knowledge_dest)?;
            }
        }

        // Copy AGENTS.md from system project dir if it exists and tenant doesn't have one.
        let agents_dest = project_dir.join("AGENTS.md");
        if !agents_dest.exists() {
            let source = config_base.join("projects").join(&project_config.name).join("AGENTS.md");
            if source.exists() {
                std::fs::copy(&source, &agents_dest)?;
            }
        }

        seeded.push(project_config.name.clone());
        println!("  seeded: {}", project_config.name);
    }

    // Regenerate chat/KNOWLEDGE.md with real project list.
    let chat_dir = projects_dir.join("chat");
    if chat_dir.exists() {
        let mut knowledge = String::from("# Chat Knowledge\n\n## Available Projects\n");
        for project_config in &sys_config.projects {
            knowledge.push_str(&format!(
                "- **{}** (prefix: `{}`): repo at `{}`\n",
                project_config.name, project_config.prefix, project_config.repo,
            ));
        }
        knowledge.push_str("\n## Your Role\nYou are a companion in the user's agency. Help them navigate their projects,\nanswer questions, and assist with tasks.\n");
        std::fs::write(chat_dir.join("KNOWLEDGE.md"), &knowledge)?;
        println!("  updated: chat/KNOWLEDGE.md");
    }

    println!("\nSeeded {} projects for tenant {}", seeded.len(), tenant_id);
    Ok(())
}
