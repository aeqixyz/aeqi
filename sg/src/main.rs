use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use sigil_beads::BeadStore;
use sigil_core::traits::{Channel, IncomingMessage, LogObserver, Memory, Observer, Provider, Tool};
use sigil_core::{Agent, AgentConfig, ExecutionMode, Identity, SecretStore, SigilConfig};
use sigil_memory::SqliteMemory;
use sigil_channels::TelegramChannel;
use sigil_orchestrator::{ConvoyStore, CronJob, CronSchedule, CronStore, Daemon, MailBus, Molecule, Rig, RigRegistry, Witness};
use sigil_orchestrator::tools::build_orchestration_tools;
use sigil_providers::OpenRouterProvider;
use sigil_tools::{
    BeadsCreateTool, BeadsReadyTool, BeadsUpdateTool, BeadsCloseTool, BeadsShowTool, BeadsDepTool,
    FileReadTool, FileWriteTool, GitWorktreeTool, ListDirTool, ShellTool, Skill,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "sg", version, about = "Sigil — Multi-Agent Orchestration")]
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
        #[arg(short, long)]
        rig: Option<String>,
        #[arg(short, long)]
        model: Option<String>,
        #[arg(long, default_value = "20")]
        max_iterations: u32,
    },
    /// Initialize Sigil in the current directory.
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
    /// Assign a task to a rig.
    Assign {
        subject: String,
        #[arg(short, long)]
        rig: String,
        #[arg(short, long, default_value = "")]
        description: String,
        #[arg(short, long)]
        priority: Option<String>,
    },
    /// Show unblocked (ready) work.
    Ready {
        #[arg(short, long)]
        rig: Option<String>,
    },
    /// Show all open beads.
    Beads {
        #[arg(short, long)]
        rig: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Close a bead.
    Close {
        id: String,
        #[arg(short, long, default_value = "completed")]
        reason: String,
    },

    // --- Phase 3: Orchestrator ---
    /// Manage the daemon.
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    // --- Phase 4: Memory ---
    /// Search collective memory.
    Recall {
        query: String,
        #[arg(short, long)]
        rig: Option<String>,
        #[arg(short, long, default_value = "5")]
        top_k: usize,
    },
    /// Store a memory.
    Remember {
        key: String,
        content: String,
        #[arg(short, long)]
        rig: Option<String>,
    },

    // --- Phase 5: Molecules ---
    /// Molecule workflow commands.
    Mol {
        #[command(subcommand)]
        action: MolAction,
    },

    // --- Phase 6: Cron ---
    /// Manage scheduled cron jobs.
    Cron {
        #[command(subcommand)]
        action: CronAction,
    },

    // --- Phase 7: Skills ---
    /// List or run skills.
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },

    // --- Cross-rig ---
    /// Track work across rigs.
    Convoy {
        #[command(subcommand)]
        action: ConvoyAction,
    },

    // --- Worker management ---
    /// Pin work to a worker.
    Hook {
        worker: String,
        bead_id: String,
    },
    /// Mark worker as done, trigger cleanup.
    Done {
        bead_id: String,
        #[arg(short, long, default_value = "completed")]
        reason: String,
    },

    // --- Config ---
    /// Reload configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
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
enum DaemonAction {
    /// Start the daemon (runs in foreground).
    Start,
    /// Stop a running daemon.
    Stop,
    /// Show daemon status.
    Status,
    /// Query the running daemon via IPC socket.
    Query {
        /// Command to send (ping, status, rigs, mail).
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
enum CronAction {
    /// Add a scheduled job.
    Add {
        name: String,
        #[arg(short, long)]
        schedule: Option<String>,
        #[arg(long)]
        at: Option<String>,
        #[arg(short, long)]
        rig: String,
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
enum SkillAction {
    /// List available skills for a rig.
    List {
        #[arg(short, long)]
        rig: Option<String>,
    },
    /// Run a skill by name.
    Run {
        name: String,
        #[arg(short, long)]
        rig: String,
        /// Additional user prompt appended after the skill's user_prefix.
        prompt: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConvoyAction {
    /// Create a convoy tracking beads across rigs.
    Create {
        name: String,
        /// Bead IDs to track (e.g. as-001 rd-002).
        bead_ids: Vec<String>,
    },
    /// List active convoys.
    List,
    /// Show convoy status.
    Status { id: String },
}

#[derive(Subcommand)]
enum MolAction {
    /// Pour (instantiate) a molecule workflow.
    Pour {
        template: String,
        #[arg(short, long)]
        rig: String,
        /// Variables as key=value pairs.
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// List available molecule templates.
    List {
        #[arg(short, long)]
        rig: Option<String>,
    },
    /// Show status of a molecule (parent bead and its children).
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
        Commands::Run { prompt, rig, model, max_iterations } => {
            cmd_run(&cli.config, &prompt, rig.as_deref(), model.as_deref(), max_iterations).await
        }
        Commands::Init => cmd_init().await,
        Commands::Secrets { action } => cmd_secrets(&cli.config, action).await,
        Commands::Doctor { fix } => cmd_doctor(&cli.config, fix).await,
        Commands::Status => cmd_status(&cli.config).await,
        Commands::Assign { subject, rig, description, priority } => {
            cmd_assign(&cli.config, &subject, &rig, &description, priority.as_deref()).await
        }
        Commands::Ready { rig } => cmd_ready(&cli.config, rig.as_deref()).await,
        Commands::Beads { rig, all } => cmd_beads(&cli.config, rig.as_deref(), all).await,
        Commands::Close { id, reason } => cmd_close(&cli.config, &id, &reason).await,
        Commands::Daemon { action } => cmd_daemon(&cli.config, action).await,
        Commands::Recall { query, rig, top_k } => {
            cmd_recall(&cli.config, &query, rig.as_deref(), top_k).await
        }
        Commands::Remember { key, content, rig } => {
            cmd_remember(&cli.config, &key, &content, rig.as_deref()).await
        }
        Commands::Mol { action } => cmd_mol(&cli.config, action).await,
        Commands::Cron { action } => cmd_cron(&cli.config, action).await,
        Commands::Skill { action } => cmd_skill(&cli.config, action).await,
        Commands::Convoy { action } => cmd_convoy(&cli.config, action).await,
        Commands::Hook { worker, bead_id } => cmd_hook(&cli.config, &worker, &bead_id).await,
        Commands::Done { bead_id, reason } => cmd_done(&cli.config, &bead_id, &reason).await,
        Commands::Config { action } => cmd_config(&cli.config, action).await,
    }
}

// === Helpers ===

fn load_config(config_path: &Option<PathBuf>) -> Result<(SigilConfig, PathBuf)> {
    if let Some(path) = config_path {
        Ok((SigilConfig::load(path)?, path.clone()))
    } else {
        SigilConfig::discover()
    }
}

fn find_rig_dir(name: &str) -> Result<PathBuf> {
    let candidates = [
        PathBuf::from(format!("rigs/{name}")),
        PathBuf::from(format!("../rigs/{name}")),
    ];
    for c in &candidates {
        if c.exists() { return Ok(c.clone()); }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join("rigs").join(name);
            if candidate.exists() { return Ok(candidate); }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }
    anyhow::bail!("rig directory not found: {name}")
}

fn get_api_key(config: &SigilConfig) -> Result<String> {
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
        .context("OPENROUTER_API_KEY not set. Use `sg secrets set OPENROUTER_API_KEY <key>`")
}

fn build_provider(config: &SigilConfig) -> Result<Arc<dyn Provider>> {
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

/// Build the full tool set for a rig: basic tools + beads + git worktree.
fn build_rig_tools(
    workdir: &Path,
    beads_dir: &Path,
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
    if let Ok(t) = BeadsCreateTool::new(beads_dir.to_path_buf(), prefix.to_string()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsReadyTool::new(beads_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsUpdateTool::new(beads_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsCloseTool::new(beads_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsShowTool::new(beads_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }
    if let Ok(t) = BeadsDepTool::new(beads_dir.to_path_buf()) {
        tools.push(Arc::new(t));
    }

    // Add git worktree tool.
    let wt_root = worktree_root
        .cloned()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("worktrees"));
    tools.push(Arc::new(GitWorktreeTool::new(workdir.to_path_buf(), wt_root)));

    tools
}

/// Look up rig name from a bead prefix (e.g. "fa" → "familiar", "as" → "algostaking").
fn rig_name_for_prefix<'a>(config: &'a SigilConfig, prefix: &str) -> Option<&'a str> {
    if prefix == config.familiar.prefix {
        return Some("familiar");
    }
    config.rigs.iter()
        .find(|r| r.prefix == prefix)
        .map(|r| r.name.as_str())
}

fn open_beads_for_rig(rig_name: &str) -> Result<BeadStore> {
    let rig_dir = find_rig_dir(rig_name)?;
    let beads_dir = rig_dir.join(".beads");
    BeadStore::open(&beads_dir)
}

fn open_memory(config: &SigilConfig, rig_name: Option<&str>) -> Result<SqliteMemory> {
    let db_path = if let Some(name) = rig_name {
        let rig_dir = find_rig_dir(name)?;
        rig_dir.join(".sigil").join("memory.db")
    } else {
        config.data_dir().join("memory.db")
    };
    let halflife = config.memory.temporal_decay_halflife_days;
    SqliteMemory::open(&db_path, halflife)
}

// === Commands ===

async fn cmd_run(
    config_path: &Option<PathBuf>,
    prompt: &str,
    rig_name: Option<&str>,
    model_override: Option<&str>,
    max_iterations: u32,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let model = model_override
        .map(String::from)
        .or_else(|| rig_name.map(|r| config.model_for_rig(r)))
        .unwrap_or_else(|| {
            config.providers.openrouter.as_ref()
                .map(|or| or.default_model.clone())
                .unwrap_or_else(|| "minimax/minimax-m2.5".to_string())
        });

    let provider = build_provider(&config)?;
    let workdir = rig_name
        .and_then(|r| config.rig(r))
        .map(|r| PathBuf::from(&r.repo))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let tools = if let Some(rn) = rig_name {
        let rig_dir = find_rig_dir(rn)?;
        let beads_dir = rig_dir.join(".beads");
        let prefix = config.rig(rn).map(|r| r.prefix.as_str()).unwrap_or("sg");
        let worktree_root = config.rig(rn).and_then(|r| r.worktree_root.as_ref()).map(PathBuf::from);
        build_rig_tools(&workdir, &beads_dir, prefix, worktree_root.as_ref())
    } else {
        build_tools(&workdir)
    };
    // Default to familiar identity when no --rig is specified.
    let identity = if let Some(rn) = rig_name {
        find_rig_dir(rn).ok()
            .map(|d| Identity::load(&d).unwrap_or_default())
            .unwrap_or_default()
    } else {
        find_rig_dir("familiar").ok()
            .map(|d| Identity::load(&d).unwrap_or_default())
            .unwrap_or_default()
    };
    let observer: Arc<dyn Observer> = Arc::new(LogObserver);

    let agent_config = AgentConfig {
        model,
        max_iterations,
        name: rig_name.unwrap_or("default").to_string(),
        ..Default::default()
    };

    info!(prompt = %prompt, "starting agent");
    let agent = Agent::new(agent_config, provider, tools, observer, identity);
    let result = agent.run(prompt).await?;
    println!("{result}");
    Ok(())
}

async fn cmd_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_dir = cwd.join("config");
    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(cwd.join("rigs"))?;

    let config_path = config_dir.join("sigil.toml");
    if !config_path.exists() {
        std::fs::write(&config_path, r#"[sigil]
name = "my-sigil"
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

[heartbeat]
enabled = false
default_interval_minutes = 30
"#)?;
        println!("Created config/sigil.toml");
    }

    let data_dir = dirs::home_dir().unwrap_or_default().join(".sigil");
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(data_dir.join("secrets"))?;
    println!("Created ~/.sigil/");

    println!("\nSigil initialized. Next steps:");
    println!("  1. sg secrets set OPENROUTER_API_KEY sk-or-...");
    println!("  2. Add rigs to config/sigil.toml");
    println!("  3. sg run \"hello world\"");
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
    println!("Sigil Doctor{}\n============\n", if fix { " (--fix)" } else { "" });

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

            for rig in &config.rigs {
                let repo_ok = PathBuf::from(&rig.repo).exists();
                println!("[{}] Rig '{}' repo: {}", if repo_ok { "OK" } else { "WARN" }, rig.name, rig.repo);
                if !repo_ok { issues += 1; }

                match find_rig_dir(&rig.name) {
                    Ok(d) => {
                        let soul = d.join("SOUL.md").exists();
                        let ident = d.join("IDENTITY.md").exists();
                        let beads_dir = d.join(".beads");
                        let beads = beads_dir.exists();
                        if !soul { issues += 1; }
                        if !ident { issues += 1; }
                        println!("    Identity: SOUL={soul} IDENTITY={ident} | Beads: {beads}");

                        // --fix: create missing .beads dir
                        if fix && !beads {
                            std::fs::create_dir_all(&beads_dir)?;
                            println!("    [FIXED] Created .beads directory");
                            fixed += 1;
                        }

                        // Check skills directory
                        let skills_dir = d.join("skills");
                        let skill_count = if skills_dir.exists() {
                            Skill::discover(&skills_dir).map(|s| s.len()).unwrap_or(0)
                        } else { 0 };
                        let mol_count = if d.join("molecules").exists() {
                            std::fs::read_dir(d.join("molecules"))
                                .map(|e| e.filter(|e| e.as_ref().ok()
                                    .map(|e| e.path().extension().is_some_and(|x| x == "toml"))
                                    .unwrap_or(false)).count())
                                .unwrap_or(0)
                        } else { 0 };
                        println!("    Skills: {skill_count} | Molecules: {mol_count}");

                        // Check memory DB
                        let mem_db = d.join(".sigil").join("memory.db");
                        if mem_db.exists() {
                            println!("    Memory: {}", mem_db.display());
                        }
                    }
                    Err(_) => {
                        println!("    [WARN] Rig dir not found");
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
            let cron_path = config.data_dir().join("cron.json");
            if cron_path.exists() {
                let store = CronStore::open(&cron_path)?;
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
            println!("       Run `sg init` to create one.");
            issues += 1;
        }
    }

    println!();
    if issues == 0 {
        println!("All checks passed.");
    } else if fix {
        println!("{issues} issues found, {fixed} fixed.");
    } else {
        println!("{issues} issues found. Run `sg doctor --fix` to auto-repair.");
    }
    Ok(())
}

async fn cmd_status(config_path: &Option<PathBuf>) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    println!("Sigil: {}\n", config.sigil.name);

    // Show familiar rig status.
    let fa_prefix = &config.familiar.prefix;
    print!("  familiar [RIG] prefix={} model={} workers={}",
        fa_prefix,
        config.familiar.model.as_deref().unwrap_or("default"),
        config.familiar.max_workers,
    );
    if let Ok(store) = open_beads_for_rig("familiar") {
        let open: Vec<_> = store.by_prefix(fa_prefix).into_iter()
            .filter(|b| !b.is_closed()).collect();
        let ready = store.ready().len();
        print!(" | beads: {} open, {} ready", open.len(), ready);
    }
    println!();

    for rig_cfg in &config.rigs {
        let repo_ok = PathBuf::from(&rig_cfg.repo).exists();
        print!("  {} [{}] prefix={} model={} workers={}",
            rig_cfg.name,
            if repo_ok { "OK" } else { "MISSING" },
            rig_cfg.prefix,
            rig_cfg.model.as_deref().unwrap_or("default"),
            rig_cfg.max_workers,
        );

        // Show bead counts.
        if let Ok(store) = open_beads_for_rig(&rig_cfg.name) {
            let open: Vec<_> = store.by_prefix(&rig_cfg.prefix).into_iter()
                .filter(|b| !b.is_closed()).collect();
            let ready = store.ready().len();
            print!(" | beads: {} open, {} ready", open.len(), ready);
        }
        println!();
    }

    Ok(())
}

async fn cmd_assign(
    config_path: &Option<PathBuf>,
    subject: &str,
    rig_name: &str,
    description: &str,
    priority: Option<&str>,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    // Allow assigning to familiar or any configured rig.
    if rig_name != "familiar" {
        config.rig(rig_name).context(format!("rig not found: {rig_name}"))?;
    }

    let mut store = open_beads_for_rig(rig_name)?;
    let prefix = if rig_name == "familiar" {
        config.familiar.prefix.clone()
    } else {
        config.rig(rig_name).unwrap().prefix.clone()
    };
    let mut bead = store.create(&prefix, subject)?;

    if !description.is_empty() || priority.is_some() {
        bead = store.update(&bead.id.0, |b| {
            if !description.is_empty() {
                b.description = description.to_string();
            }
            if let Some(p) = priority {
                b.priority = match p {
                    "low" => sigil_beads::Priority::Low,
                    "high" => sigil_beads::Priority::High,
                    "critical" => sigil_beads::Priority::Critical,
                    _ => sigil_beads::Priority::Normal,
                };
            }
        })?;
    }

    println!("Created {} [{}] {}", bead.id, bead.priority, bead.subject);
    Ok(())
}

async fn cmd_ready(config_path: &Option<PathBuf>, rig_name: Option<&str>) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let rigs: Vec<&str> = if let Some(name) = rig_name {
        vec![name]
    } else {
        config.rigs.iter().map(|r| r.name.as_str()).collect()
    };

    let mut found = false;
    for name in rigs {
        if let Ok(store) = open_beads_for_rig(name) {
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

async fn cmd_beads(config_path: &Option<PathBuf>, rig_name: Option<&str>, show_all: bool) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let rigs: Vec<&str> = if let Some(name) = rig_name {
        vec![name]
    } else {
        config.rigs.iter().map(|r| r.name.as_str()).collect()
    };

    for name in rigs {
        if let Ok(store) = open_beads_for_rig(name) {
            let beads = store.all();
            let beads: Vec<_> = if show_all {
                beads
            } else {
                beads.into_iter().filter(|b| !b.is_closed()).collect()
            };

            if beads.is_empty() { continue; }

            println!("=== {} ===", name);
            for bead in beads {
                let assignee = bead.assignee.as_deref().unwrap_or("-");
                let deps = if bead.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" (needs: {})", bead.depends_on.iter().map(|d| d.0.as_str()).collect::<Vec<_>>().join(", "))
                };
                println!("  {} [{}] {} — {} assignee={}{}", bead.id, bead.status, bead.priority, bead.subject, assignee, deps);
            }
        }
    }
    Ok(())
}

async fn cmd_close(config_path: &Option<PathBuf>, id: &str, reason: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = id.split('-').next().unwrap_or("");
    let rig_name = rig_name_for_prefix(&config, prefix)
        .context(format!("no rig with prefix '{prefix}'"))?;

    let mut store = open_beads_for_rig(rig_name)?;
    let bead = store.close(id, reason)?;
    println!("Closed {} — {}", bead.id, bead.subject);
    Ok(())
}

fn pid_file_path(config: &SigilConfig) -> PathBuf {
    config.data_dir().join("sg.pid")
}

async fn cmd_daemon(config_path: &Option<PathBuf>, action: DaemonAction) -> Result<()> {
    match action {
        DaemonAction::Start => {
            let (config, _) = load_config(config_path)?;

            // Check if already running.
            let pid_path = pid_file_path(&config);
            if Daemon::is_running_from_pid(&pid_path) {
                anyhow::bail!("daemon is already running (PID file: {})", pid_path.display());
            }

            let mail_bus = Arc::new(MailBus::new());
            let registry = Arc::new(RigRegistry::new(mail_bus.clone()));
            let provider = build_provider(&config)?;
            let mut heartbeats = Vec::new();

            // Register domain rigs.
            for rig_cfg in &config.rigs {
                let rig_dir = match find_rig_dir(&rig_cfg.name) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let default_model = config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.as_str())
                    .unwrap_or("minimax/minimax-m2.5");

                let rig = Arc::new(Rig::from_config(rig_cfg, &rig_dir, default_model)?);
                let workdir = rig.repo.clone();
                let beads_dir = rig_dir.join(".beads");
                let tools = build_rig_tools(&workdir, &beads_dir, &rig_cfg.prefix, Some(&rig.worktree_root));
                let mut witness = Witness::new(&rig, provider.clone(), tools.clone(), mail_bus.clone());

                // Configure execution mode for workers.
                if rig_cfg.execution_mode == ExecutionMode::ClaudeCode {
                    let cc_model = config.model_for_rig(&rig_cfg.name);
                    let cc_max_turns = rig_cfg.max_turns.unwrap_or(25);
                    witness.set_claude_code_mode(
                        rig.repo.clone(),
                        cc_model,
                        cc_max_turns,
                        rig_cfg.max_budget_usd,
                    );
                    info!(
                        rig = %rig_cfg.name,
                        model = %witness.model,
                        max_turns = cc_max_turns,
                        "registered with claude_code execution mode"
                    );
                }

                registry.register_rig(rig.clone(), witness).await;

                // Create heartbeat if HEARTBEAT.md exists and heartbeat is enabled.
                if config.heartbeat.enabled
                    && let Some(ref hb_content) = rig.identity.heartbeat {
                        let interval = config.heartbeat.default_interval_minutes as u64 * 60;
                        let heartbeat = sigil_orchestrator::Heartbeat::new(
                            rig.name.clone(),
                            interval,
                            hb_content.clone(),
                            provider.clone(),
                            tools.clone(),
                            rig.identity.clone(),
                            rig.model.clone(),
                            mail_bus.clone(),
                        );
                        heartbeats.push(heartbeat);
                    }
            }

            // Build channels map for the familiar.
            let channels: Arc<RwLock<HashMap<String, Arc<dyn sigil_core::traits::Channel>>>> =
                Arc::new(RwLock::new(HashMap::new()));

            // Wire Telegram if configured.
            if let Some(ref tg_config) = config.channels.telegram {
                let secret_store_path = config.security.secret_store.as_ref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| config.data_dir().join("secrets"));
                match SecretStore::open(&secret_store_path)
                    .and_then(|s| s.get(&tg_config.token_secret))
                {
                    Ok(token) if !token.is_empty() => {
                        let tg = Arc::new(TelegramChannel::new(token, tg_config.allowed_chats.clone()));
                        channels.write().await.insert("telegram".to_string(), tg.clone() as Arc<dyn sigil_core::traits::Channel>);

                        // Start polling, route incoming messages as familiar beads.
                        // Two-phase response: instant reaction (direct LLM) + full reply (bead agent).
                        match Channel::start(tg.as_ref()).await {
                            Ok(mut rx) => {
                                let reg = registry.clone();
                                let tg_reply = tg.clone();
                                let reaction_api_key = get_api_key(&config).unwrap_or_default();
                                // Conversation history per chat_id for coherent multi-turn dialogue.
                                // Each entry: (role, text, timestamp). Pruned at 20 messages / 2 hour TTL.
                                let conversations: Arc<RwLock<HashMap<i64, Vec<(String, String, std::time::Instant)>>>> =
                                    Arc::new(RwLock::new(HashMap::new()));
                                tokio::spawn(async move {
                                    while let Some(msg) = <tokio::sync::mpsc::Receiver<IncomingMessage>>::recv(&mut rx).await {
                                        let chat_id = msg.metadata.get("chat_id")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);
                                        let message_id = msg.metadata.get("message_id")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);
                                        // Use "Architect" as the subject sender — not the Telegram username,
                                        // which could match a rig name and confuse routing.
                                        let subject = format!("[telegram] Architect ({})", msg.sender);
                                        let metadata_str = serde_json::to_string(&msg.metadata).unwrap_or_default();

                                        let reg2 = reg.clone();
                                        let tg2 = tg_reply.clone();
                                        let react_api_key = reaction_api_key.clone();
                                        let user_text = msg.text.clone();
                                        let convos = conversations.clone();
                                        tokio::spawn(async move {
                                            // Build conversation context + record user message.
                                            let (description, phase1_history) = {
                                                let mut conv = convos.write().await;
                                                let history = conv.entry(chat_id).or_insert_with(Vec::new);

                                                // Prune messages older than 2 hours.
                                                let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(7200);
                                                history.retain(|(_, _, ts)| *ts > cutoff);

                                                // Build conversation context for bead description.
                                                let ctx = if history.is_empty() {
                                                    String::new()
                                                } else {
                                                    let mut s = String::from("## Conversation History\n\n");
                                                    for (role, text, _) in history.iter() {
                                                        s.push_str(&format!("**{}**: {}\n\n", role, text));
                                                    }
                                                    s
                                                };

                                                // Build Phase 1 messages (last 4 exchanges for contextual reaction).
                                                let p1: Vec<serde_json::Value> = history.iter()
                                                    .rev().take(4).collect::<Vec<_>>().into_iter().rev()
                                                    .map(|(role, text, _)| {
                                                        let api_role = if role == "User" { "user" } else { "assistant" };
                                                        serde_json::json!({"role": api_role, "content": text})
                                                    })
                                                    .collect();

                                                // Record user message.
                                                history.push(("User".to_string(), user_text.clone(), std::time::Instant::now()));
                                                while history.len() > 20 {
                                                    history.remove(0);
                                                }

                                                // Build bead description with conversation context.
                                                // Embed response protocol directly so it cannot be missed
                                                // regardless of system prompt ordering.
                                                let response_protocol = "**RESPONSE PROTOCOL**: Your output text is the Telegram reply — write it directly, in character. No completion reports. Never write \"The response has been sent\", \"I sent the reply\", or any meta-commentary. The daemon delivers your output automatically. Just your reply.";
                                                let desc = if ctx.is_empty() {
                                                    format!("{}\n\n---\n{}\nchannel_metadata: {}", user_text, response_protocol, metadata_str)
                                                } else {
                                                    format!("{}\n## Current Message\n\n{}\n\n---\n{}\nchannel_metadata: {}", ctx, user_text, response_protocol, metadata_str)
                                                };

                                                (desc, p1)
                                            };

                                            // Phase 1: Instant reaction — direct LLM call, no tools, no agent.
                                            let react_tg = tg2.clone();
                                            let react_chat = chat_id;
                                            let react_mid = message_id;
                                            let p1_user_text = user_text.clone();
                                            tokio::spawn(async move {
                                                info!("phase1: starting instant reaction");
                                                let client = reqwest::Client::builder()
                                                    .timeout(std::time::Duration::from_secs(15))
                                                    .build()
                                                    .unwrap();
                                                // Build messages with conversation history for contextual reactions.
                                                let mut messages = vec![
                                                    serde_json::json!({"role": "system", "content": "You are Aurelia, the White Familiar — calm, intelligent, warm. React with ONE short sentence (max 15 words). Elegant and genuine. No emojis. Your immediate perception of what the Architect needs."}),
                                                ];
                                                messages.extend(phase1_history.into_iter());
                                                messages.push(serde_json::json!({"role": "user", "content": p1_user_text}));
                                                let body = serde_json::json!({
                                                    "model": "google/gemini-2.0-flash-001",
                                                    "messages": messages,
                                                    "max_tokens": 80,
                                                    "temperature": 0.7
                                                });
                                                info!("phase1: calling openrouter");
                                                match client.post("https://openrouter.ai/api/v1/chat/completions")
                                                    .header("Authorization", format!("Bearer {}", react_api_key))
                                                    .header("Content-Type", "application/json")
                                                    .json(&body)
                                                    .send()
                                                    .await
                                                {
                                                    Ok(resp) => {
                                                        let resp_result: Result<serde_json::Value, reqwest::Error> = resp.json().await;
                                                        match resp_result {
                                                            Ok(v) => {
                                                                let text: String = v.pointer("/choices/0/message/content")
                                                                    .and_then(|c: &serde_json::Value| c.as_str())
                                                                    .unwrap_or("")
                                                                    .trim()
                                                                    .to_string();
                                                                if !text.is_empty() {
                                                                    info!(reaction = %text, "instant reaction ready");
                                                                    let out = sigil_core::traits::OutgoingMessage {
                                                                        channel: "telegram".to_string(),
                                                                        recipient: String::new(),
                                                                        text: format!("_{}_", text),
                                                                        metadata: serde_json::json!({ "chat_id": react_chat }),
                                                                    };
                                                                    let _ = react_tg.send(out).await;
                                                                    // Fire = working on full response
                                                                    if react_mid > 0 {
                                                                        let _ = react_tg.react(react_chat, react_mid, "🔥").await;
                                                                    }
                                                                } else {
                                                                    warn!("phase1: empty reaction text from LLM");
                                                                }
                                                            }
                                                            Err(e) => warn!(error = %e, "phase1: failed to parse response"),
                                                        }
                                                    }
                                                    Err(e) => warn!(error = %e, "phase1: request failed"),
                                                }
                                            });

                                            // Phase 2: Full response via bead agent.
                                            let bead_id: String = match reg2.assign("familiar", &subject, &description).await {
                                                Ok(b) => b.id.0.clone(),
                                                Err(e) => {
                                                    warn!(error = %e, "failed to create bead from telegram message");
                                                    return;
                                                }
                                            };

                                            // Poll bead until closed (timeout 5 min).
                                            let deadline = tokio::time::Instant::now()
                                                + std::time::Duration::from_secs(300);
                                            loop {
                                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                                                let done: Option<(bool, Option<String>)> = {
                                                    if let Some(rig) = reg2.get_rig("familiar").await {
                                                        let store = rig.beads.lock().await;
                                                        store.get(&bead_id).map(|b| {
                                                            (b.status == sigil_beads::BeadStatus::Done, b.closed_reason.clone())
                                                        })
                                                    } else {
                                                        None
                                                    }
                                                };

                                                if let Some((true, reason)) = done {
                                                    let reply_text = reason
                                                        .filter(|r| !r.trim().is_empty())
                                                        .unwrap_or_else(|| "Done.".to_string());
                                                    // Record assistant response in conversation history.
                                                    {
                                                        let mut conv = convos.write().await;
                                                        if let Some(history) = conv.get_mut(&chat_id) {
                                                            history.push(("Aurelia".to_string(), reply_text.clone(), std::time::Instant::now()));
                                                            while history.len() > 20 {
                                                                history.remove(0);
                                                            }
                                                        }
                                                    }
                                                    let out = sigil_core::traits::OutgoingMessage {
                                                        channel: "telegram".to_string(),
                                                        recipient: String::new(),
                                                        text: reply_text,
                                                        metadata: serde_json::json!({ "chat_id": chat_id }),
                                                    };
                                                    if let Err(e) = tg2.send(out).await {
                                                        warn!(error = %e, "failed to reply on telegram");
                                                    }
                                                    // Done reaction
                                                    if message_id > 0 {
                                                        let _ = tg2.react(chat_id, message_id, "👍").await;
                                                    }
                                                    break;
                                                }

                                                if tokio::time::Instant::now() > deadline {
                                                    warn!(bead = %bead_id, "telegram reply timed out");
                                                    // Timeout reaction
                                                    if message_id > 0 {
                                                        let _ = tg2.react(chat_id, message_id, "😢").await;
                                                    }
                                                    break;
                                                }
                                            }
                                        });
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

            // Register the Familiar as a rig.
            let fa_rig_dir = find_rig_dir("familiar").unwrap_or_else(|_| PathBuf::from("rigs/familiar"));
            let fa_identity = Identity::load(&fa_rig_dir).unwrap_or_default();
            let fa_beads_dir = fa_rig_dir.join(".beads");
            std::fs::create_dir_all(&fa_beads_dir).ok();
            let fa_beads = sigil_beads::BeadStore::open(&fa_beads_dir)?;
            let fa_model = config.model_for_rig("familiar");
            let fa_prefix = config.familiar.prefix.clone();
            let fa_workdir = find_rig_dir("sigil")
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());

            let fa_rig = Arc::new(Rig {
                name: "familiar".to_string(),
                prefix: fa_prefix.clone(),
                repo: fa_workdir.clone(),
                worktree_root: dirs::home_dir().unwrap_or_default().join("worktrees"),
                model: fa_model,
                max_workers: config.familiar.max_workers,
                identity: fa_identity,
                beads: Arc::new(tokio::sync::Mutex::new(fa_beads)),
            });

            // Build familiar tools: basic + beads + orchestration.
            let mut fa_tools: Vec<Arc<dyn sigil_core::traits::Tool>> = build_rig_tools(
                &fa_workdir, &fa_beads_dir, &fa_prefix, None,
            );
            let orch_tools = build_orchestration_tools(registry.clone(), mail_bus.clone(), channels.clone());
            fa_tools.extend(orch_tools);

            let mut fa_witness = Witness::new(&fa_rig, provider.clone(), fa_tools, mail_bus.clone());

            // Configure Claude Code execution mode for familiar workers if configured.
            if config.familiar.execution_mode == ExecutionMode::ClaudeCode {
                let cc_model = config.model_for_rig("familiar");
                let cc_max_turns = config.familiar.max_turns.unwrap_or(25);
                fa_witness.set_claude_code_mode(
                    fa_workdir.clone(),
                    cc_model,
                    cc_max_turns,
                    config.familiar.max_budget_usd,
                );
            }

            registry.register_rig(fa_rig, fa_witness).await;

            let rig_count = registry.rig_count().await;
            println!("Sigil daemon starting...");
            println!("Registered {} rigs (including familiar), {} heartbeats", rig_count, heartbeats.len());

            // Load cron store.
            let cron_path = config.data_dir().join("cron.json");
            let cron_store = CronStore::open(&cron_path)?;
            let socket_path = config.data_dir().join("sg.sock");

            println!("Cron: {} jobs loaded", cron_store.jobs.len());
            println!("PID file: {}", pid_path.display());
            println!("IPC socket: {}", socket_path.display());
            println!("Press Ctrl+C to stop.\n");

            let mut daemon = Daemon::new(registry, mail_bus);
            daemon.set_pid_file(pid_path);
            daemon.set_socket_path(socket_path.clone());
            daemon.set_cron_store(cron_store);
            for hb in heartbeats {
                daemon.add_heartbeat(hb);
            }
            daemon.run().await?;
        }

        DaemonAction::Stop => {
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

        DaemonAction::Status => {
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

            // Also show rig summary.
            cmd_status(config_path).await?;
        }

        DaemonAction::Query { cmd } => {
            let (config, _) = load_config(config_path)?;
            let socket_path = config.data_dir().join("sg.sock");

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

async fn cmd_recall(config_path: &Option<PathBuf>, query: &str, rig_name: Option<&str>, top_k: usize) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let memory = open_memory(&config, rig_name)?;

    let results = memory.search(&sigil_core::traits::MemoryQuery::new(query, top_k)).await?;

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

async fn cmd_remember(config_path: &Option<PathBuf>, key: &str, content: &str, rig_name: Option<&str>) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let memory = open_memory(&config, rig_name)?;

    let id = memory.store(key, content, sigil_core::traits::MemoryCategory::Fact).await?;
    let scope = rig_name.unwrap_or("global");
    println!("Stored memory {id} [{scope}] {key}");
    Ok(())
}

async fn cmd_mol(config_path: &Option<PathBuf>, action: MolAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        MolAction::Pour { template, rig, vars } => {
            let rig_cfg = config.rig(&rig).context(format!("rig not found: {rig}"))?;
            let rig_dir = find_rig_dir(&rig)?;

            // Find the molecule template.
            let mol_path = rig_dir.join("molecules").join(format!("{template}.toml"));
            if !mol_path.exists() {
                anyhow::bail!("molecule template not found: {}", mol_path.display());
            }

            let molecule = Molecule::load(&mol_path)?;

            // Parse vars.
            let var_map: HashMap<String, String> = vars.iter()
                .filter_map(|v| {
                    let parts: Vec<&str> = v.splitn(2, '=').collect();
                    if parts.len() == 2 { Some((parts[0].to_string(), parts[1].to_string())) }
                    else { None }
                })
                .collect();

            // Pour into bead store.
            let mut store = open_beads_for_rig(&rig)?;
            let parent_id = molecule.pour(&mut store, &rig_cfg.prefix, &var_map)?;

            println!("Poured molecule '{template}' as {parent_id}");
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

        MolAction::List { rig } => {
            let rigs: Vec<&str> = if let Some(ref name) = rig {
                vec![name.as_str()]
            } else {
                config.rigs.iter().map(|r| r.name.as_str()).collect()
            };

            for name in rigs {
                if let Ok(rig_dir) = find_rig_dir(name) {
                    let mol_dir = rig_dir.join("molecules");
                    if mol_dir.exists() {
                        println!("=== {} ===", name);
                        if let Ok(entries) = std::fs::read_dir(&mol_dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.extension().is_some_and(|e| e == "toml")
                                    && let Ok(mol) = Molecule::load(&path) {
                                        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                                        println!("  {} — {} ({} steps)", stem, mol.molecule.description, mol.steps.len());
                                    }
                            }
                        }
                    }
                }
            }
        }

        MolAction::Status { id } => {
            let prefix = id.split('-').next().unwrap_or("");
            let rig_name = rig_name_for_prefix(&config, prefix)
                .context(format!("no rig with prefix '{prefix}'"))?;

            let store = open_beads_for_rig(rig_name)?;
            let parent_id = sigil_beads::BeadId::from(id.as_str());

            if let Some(parent) = store.get(&id) {
                println!("{} [{}] {}", parent.id, parent.status, parent.subject);
                let children = store.children(&parent_id);
                let done = children.iter().filter(|c| c.is_closed()).count();
                println!("Progress: {}/{}\n", done, children.len());
                for child in &children {
                    let status_icon = match child.status {
                        sigil_beads::BeadStatus::Done => "[x]",
                        sigil_beads::BeadStatus::InProgress => "[~]",
                        sigil_beads::BeadStatus::Cancelled => "[-]",
                        _ => "[ ]",
                    };
                    println!("  {} {} {}", status_icon, child.id, child.subject);
                }
            } else {
                println!("Bead not found: {id}");
            }
        }
    }
    Ok(())
}

async fn cmd_cron(config_path: &Option<PathBuf>, action: CronAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let cron_path = config.data_dir().join("cron.json");

    match action {
        CronAction::Add { name, schedule, at, rig, prompt, isolated } => {
            config.rig(&rig).context(format!("rig not found: {rig}"))?;

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

            let job = CronJob {
                name: name.clone(),
                schedule: cron_schedule,
                rig,
                prompt,
                isolated,
                created_at: Utc::now(),
                last_run: None,
            };

            let mut store = CronStore::open(&cron_path)?;
            store.add(job)?;
            println!("Cron job '{name}' added.");
        }

        CronAction::List => {
            let store = CronStore::open(&cron_path)?;
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
                    println!("  {} — rig={} {} last_run={}{}", job.name, job.rig, sched, last, iso);
                }
            }
        }

        CronAction::Remove { name } => {
            let mut store = CronStore::open(&cron_path)?;
            store.remove(&name)?;
            println!("Cron job '{name}' removed.");
        }
    }
    Ok(())
}

async fn cmd_skill(config_path: &Option<PathBuf>, action: SkillAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        SkillAction::List { rig } => {
            let rigs: Vec<&str> = if let Some(ref name) = rig {
                vec![name.as_str()]
            } else {
                config.rigs.iter().map(|r| r.name.as_str()).collect()
            };

            for name in rigs {
                if let Ok(rig_dir) = find_rig_dir(name) {
                    let skills_dir = rig_dir.join("skills");
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

        SkillAction::Run { name, rig, prompt } => {
            let rig_cfg = config.rig(&rig).context(format!("rig not found: {rig}"))?;
            let rig_dir = find_rig_dir(&rig)?;
            let skills_dir = rig_dir.join("skills");
            let skills = Skill::discover(&skills_dir)?;

            let skill = skills.iter()
                .find(|s| s.skill.name == name)
                .context(format!("skill not found: {name}"))?;

            // Build provider.
            let provider = build_provider(&config)?;
            let workdir = PathBuf::from(&rig_cfg.repo);
            let beads_dir = rig_dir.join(".beads");
            let worktree_root = rig_cfg.worktree_root.as_ref().map(PathBuf::from);
            let all_tools = build_rig_tools(&workdir, &beads_dir, &rig_cfg.prefix, worktree_root.as_ref());

            // Filter tools by skill policy.
            let filtered_tools: Vec<Arc<dyn Tool>> = all_tools.into_iter()
                .filter(|t| skill.is_tool_allowed(t.name()))
                .collect();

            // Build identity with skill system prompt.
            let identity = Identity::load(&rig_dir).unwrap_or_default();
            let base_prompt = identity.system_prompt();

            let mut skill_identity = identity.clone();
            // Override the system prompt to include skill instructions.
            skill_identity.soul = Some(skill.system_prompt(&base_prompt));

            let user_prompt = if let Some(ref p) = prompt {
                format!("{}{}", skill.prompt.user_prefix, p)
            } else {
                skill.prompt.user_prefix.clone()
            };

            let observer: Arc<dyn Observer> = Arc::new(LogObserver);
            let model = rig_cfg.model.clone()
                .unwrap_or_else(|| config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.clone())
                    .unwrap_or_else(|| "minimax/minimax-m2.5".to_string()));

            let agent_config = AgentConfig {
                model,
                max_iterations: 10,
                name: format!("{}-skill-{}", rig, name),
                ..Default::default()
            };

            let agent = Agent::new(agent_config, provider, filtered_tools, observer, skill_identity);
            let result = agent.run(&user_prompt).await?;
            println!("{result}");
        }
    }
    Ok(())
}

async fn cmd_convoy(config_path: &Option<PathBuf>, action: ConvoyAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let convoy_path = config.data_dir().join("convoys.json");

    match action {
        ConvoyAction::Create { name, bead_ids } => {
            let beads: Vec<(sigil_beads::BeadId, String)> = bead_ids.iter()
                .map(|id| {
                    let prefix = id.split('-').next().unwrap_or("");
                    let rig_name = config.rigs.iter()
                        .find(|r| r.prefix == prefix)
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    (sigil_beads::BeadId::from(id.as_str()), rig_name)
                })
                .collect();

            let mut store = ConvoyStore::open(&convoy_path)?;
            let convoy = store.create(&name, beads)?;
            let (done, total) = convoy.progress();
            println!("Created convoy {} — {} ({}/{})", convoy.id, convoy.name, done, total);
        }

        ConvoyAction::List => {
            let store = ConvoyStore::open(&convoy_path)?;
            let active = store.active();
            if active.is_empty() {
                println!("No active convoys.");
            } else {
                for convoy in active {
                    let (done, total) = convoy.progress();
                    println!("  {} — {} ({}/{})", convoy.id, convoy.name, done, total);
                }
            }
        }

        ConvoyAction::Status { id } => {
            let store = ConvoyStore::open(&convoy_path)?;
            if let Some(convoy) = store.get(&id) {
                let (done, total) = convoy.progress();
                let status = if convoy.closed_at.is_some() { "COMPLETE" } else { "ACTIVE" };
                println!("{} [{}] {} ({}/{})", convoy.id, status, convoy.name, done, total);
                for bead in &convoy.beads {
                    let icon = if bead.closed { "[x]" } else { "[ ]" };
                    println!("  {} {} (rig: {})", icon, bead.bead_id, bead.rig);
                }
            } else {
                println!("Convoy not found: {id}");
            }
        }
    }
    Ok(())
}

async fn cmd_hook(config_path: &Option<PathBuf>, worker: &str, bead_id: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = bead_id.split('-').next().unwrap_or("");
    let rig_name = rig_name_for_prefix(&config, prefix)
        .context(format!("no rig with prefix '{prefix}'"))?;

    let mut store = open_beads_for_rig(rig_name)?;
    let bead = store.update(bead_id, |b| {
        b.status = sigil_beads::BeadStatus::InProgress;
        b.assignee = Some(worker.to_string());
    })?;

    println!("Hooked {} to {} — {}", worker, bead.id, bead.subject);
    Ok(())
}

async fn cmd_done(config_path: &Option<PathBuf>, bead_id: &str, reason: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = bead_id.split('-').next().unwrap_or("");
    let rig_name = rig_name_for_prefix(&config, prefix)
        .context(format!("no rig with prefix '{prefix}'"))?;

    let mut store = open_beads_for_rig(rig_name)?;
    let bead = store.close(bead_id, reason)?;
    println!("Done {} — {}", bead.id, bead.subject);

    // Also update any convoys tracking this bead.
    let convoy_path = config.data_dir().join("convoys.json");
    if convoy_path.exists() {
        let mut convoy_store = ConvoyStore::open(&convoy_path)?;
        let completed = convoy_store.mark_bead_closed(&bead.id)?;
        for c_id in &completed {
            println!("Convoy {c_id} completed!");
        }
    }

    Ok(())
}

async fn cmd_config(config_path: &Option<PathBuf>, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let (config, path) = load_config(config_path)?;
            println!("Config: {}\n", path.display());
            println!("Name: {}", config.sigil.name);
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

            println!("\n[heartbeat]");
            println!("  enabled: {}", config.heartbeat.enabled);
            println!("  interval: {}min", config.heartbeat.default_interval_minutes);

            println!("\n[[rigs]]");
            for rig in &config.rigs {
                println!("  {} prefix={} model={} workers={}",
                    rig.name, rig.prefix,
                    rig.model.as_deref().unwrap_or("default"),
                    rig.max_workers);
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
