use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use realm_quests::QuestBoard;
use realm_core::traits::{Channel, LogObserver, Memory, Observer, Provider, Tool};
use realm_core::{Agent, AgentConfig, ExecutionMode, Identity, SecretStore, RealmConfig};
use realm_memory::SqliteMemory;
use realm_gates::TelegramChannel;
use realm_orchestrator::{RaidStore, FateJob, FateStore, Summoner, WhisperBus, Ritual, Domain, DomainRegistry, Scout, FamiliarRouter, Chamber};
use realm_orchestrator::fate::CronSchedule;
use realm_orchestrator::tools::build_orchestration_tools;
use realm_providers::{OpenRouterEmbedder, OpenRouterProvider};
use realm_tools::{
    BeadsCreateTool, BeadsReadyTool, BeadsUpdateTool, BeadsCloseTool, BeadsShowTool, BeadsDepTool,
    FileReadTool, FileWriteTool, GitWorktreeTool, ListDirTool, PorkbunTool, ShellTool, Skill,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[allow(dead_code)]
type ConversationHistory = HashMap<i64, std::collections::VecDeque<(String, String, std::time::Instant)>>;

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
        #[arg(short, long)]
        rig: Option<String>,
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
    /// Assign a task to a domain.
    Assign {
        subject: String,
        #[arg(short = 'r', long = "rig")]
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
    /// Show all open quests.
    Beads {
        #[arg(short, long)]
        rig: Option<String>,
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
    Summoner {
        #[command(subcommand)]
        action: SummonerAction,
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

    // --- Phase 5: Rituals ---
    /// Ritual workflow commands.
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

    // --- Cross-domain ---
    /// Track work across domains.
    Raid {
        #[command(subcommand)]
        action: RaidAction,
    },

    // --- Spirit management ---
    /// Pin work to a spirit.
    Bond {
        worker: String,
        quest_id: String,
    },
    /// Mark quest as done, trigger cleanup.
    Done {
        quest_id: String,
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
enum SummonerAction {
    /// Start the daemon (runs in foreground).
    Start,
    /// Stop a running daemon.
    Stop,
    /// Show daemon status.
    Status,
    /// Query the running daemon via IPC socket.
    Query {
        /// Command to send (ping, status, domains, whispers).
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
enum MagicAction {
    /// List available skills for a domain.
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
enum RaidAction {
    /// Create a raid tracking quests across domains.
    Create {
        name: String,
        /// Quest IDs to track (e.g. as-001 rd-002).
        quest_ids: Vec<String>,
    },
    /// List active raids.
    List,
    /// Show raid status.
    Status { id: String },
}

#[derive(Subcommand)]
enum RitualAction {
    /// Pour (instantiate) a ritual workflow.
    Pour {
        template: String,
        #[arg(short, long)]
        rig: String,
        /// Variables as key=value pairs.
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// List available ritual templates.
    List {
        #[arg(short, long)]
        rig: Option<String>,
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
        Commands::Summoner { action } => cmd_daemon(&cli.config, action).await,
        Commands::Recall { query, rig, top_k } => {
            cmd_recall(&cli.config, &query, rig.as_deref(), top_k).await
        }
        Commands::Remember { key, content, rig } => {
            cmd_remember(&cli.config, &key, &content, rig.as_deref()).await
        }
        Commands::Mol { action } => cmd_mol(&cli.config, action).await,
        Commands::Cron { action } => cmd_cron(&cli.config, action).await,
        Commands::Skill { action } => cmd_skill(&cli.config, action).await,
        Commands::Raid { action } => cmd_raid(&cli.config, action).await,
        Commands::Bond { worker, quest_id } => cmd_hook(&cli.config, &worker, &quest_id).await,
        Commands::Done { quest_id, reason } => cmd_done(&cli.config, &quest_id, &reason).await,
        Commands::Config { action } => cmd_config(&cli.config, action).await,
    }
}

// === Helpers ===

fn load_config(config_path: &Option<PathBuf>) -> Result<(RealmConfig, PathBuf)> {
    if let Some(path) = config_path {
        Ok((RealmConfig::load(path)?, path.clone()))
    } else {
        RealmConfig::discover()
    }
}

fn find_domain_dir(name: &str) -> Result<PathBuf> {
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
    anyhow::bail!("domain directory not found: {name}")
}

fn get_api_key(config: &RealmConfig) -> Result<String> {
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

fn build_provider(config: &RealmConfig) -> Result<Arc<dyn Provider>> {
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

/// Build the full tool set for a domain: basic tools + quests + git worktree.
fn build_domain_tools(
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

/// Look up rig name from a bead prefix (e.g. "fa" → "familiar", "as" → "algostaking").
fn domain_name_for_prefix(config: &RealmConfig, prefix: &str) -> Option<String> {
    if prefix == config.shadow.prefix {
        return Some("familiar".to_string());
    }
    // Check advisor familiar prefixes.
    for fam in &config.familiars {
        if fam.prefix == prefix {
            return Some(format!("familiar-{}", fam.name));
        }
    }
    config.domains.iter()
        .find(|r| r.prefix == prefix)
        .map(|r| r.name.clone())
}

fn open_quests_for_domain(domain_name: &str) -> Result<QuestBoard> {
    let domain_dir = find_domain_dir(domain_name)?;
    let quests_dir = domain_dir.join(".quests");
    QuestBoard::open(&quests_dir)
}

fn open_memory(config: &RealmConfig, domain_name: Option<&str>) -> Result<SqliteMemory> {
    let db_path = if let Some(name) = domain_name {
        let domain_dir = find_domain_dir(name)?;
        domain_dir.join(".sigil").join("memory.db")
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

// === Commands ===

async fn cmd_run(
    config_path: &Option<PathBuf>,
    prompt: &str,
    domain_name: Option<&str>,
    model_override: Option<&str>,
    max_iterations: u32,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let model = model_override
        .map(String::from)
        .or_else(|| domain_name.map(|r| config.model_for_domain(r)))
        .unwrap_or_else(|| {
            config.providers.openrouter.as_ref()
                .map(|or| or.default_model.clone())
                .unwrap_or_else(|| "minimax/minimax-m2.5".to_string())
        });

    let provider = build_provider(&config)?;
    let workdir = domain_name
        .and_then(|r| config.domain(r))
        .map(|r| PathBuf::from(&r.repo))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let tools = if let Some(rn) = domain_name {
        let domain_dir = find_domain_dir(rn)?;
        let quests_dir = domain_dir.join(".quests");
        let prefix = config.domain(rn).map(|r| r.prefix.as_str()).unwrap_or("sg");
        let worktree_root = config.domain(rn).and_then(|r| r.worktree_root.as_ref()).map(PathBuf::from);
        build_domain_tools(&workdir, &quests_dir, prefix, worktree_root.as_ref())
    } else {
        build_tools(&workdir)
    };
    // Default to familiar identity when no --rig is specified.
    let identity = if let Some(rn) = domain_name {
        find_domain_dir(rn).ok()
            .map(|d| Identity::load(&d).unwrap_or_default())
            .unwrap_or_default()
    } else {
        find_domain_dir("familiar").ok()
            .map(|d| Identity::load(&d).unwrap_or_default())
            .unwrap_or_default()
    };
    let observer: Arc<dyn Observer> = Arc::new(LogObserver);

    let agent_config = AgentConfig {
        model,
        max_iterations,
        name: domain_name.unwrap_or("default").to_string(),
        ..Default::default()
    };

    let memory: Option<Arc<dyn Memory>> = match open_memory(&config, domain_name) {
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
    std::fs::create_dir_all(cwd.join("rigs"))?;

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
    println!("  2. Add domains to config/realm.toml");
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

            for dcfg in &config.domains {
                let repo_ok = PathBuf::from(&dcfg.repo).exists();
                println!("[{}] Domain '{}' repo: {}", if repo_ok { "OK" } else { "WARN" }, dcfg.name, dcfg.repo);
                if !repo_ok { issues += 1; }

                match find_domain_dir(&dcfg.name) {
                    Ok(d) => {
                        let soul = d.join("SOUL.md").exists();
                        let ident = d.join("IDENTITY.md").exists();
                        let quests_dir = d.join(".quests");
                        let beads = quests_dir.exists();
                        if !soul { issues += 1; }
                        if !ident { issues += 1; }
                        println!("    Identity: SOUL={soul} IDENTITY={ident} | Quests: {beads}");

                        // --fix: create missing .quests dir
                        if fix && !beads {
                            std::fs::create_dir_all(&quests_dir)?;
                            println!("    [FIXED] Created .quests directory");
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
                        println!("    [WARN] Domain dir not found");
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
                let store = FateStore::open(&fate_path)?;
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

    println!("Realm: {}\n", config.realm.name);

    // Show shadow status.
    let fa_prefix = &config.shadow.prefix;
    print!("  familiar [SHADOW] prefix={} model={} spirits={}",
        fa_prefix,
        config.shadow.model.as_deref().unwrap_or("default"),
        config.shadow.max_workers,
    );
    if let Ok(store) = open_quests_for_domain("familiar") {
        let open: Vec<_> = store.by_prefix(fa_prefix).into_iter()
            .filter(|b| !b.is_closed()).collect();
        let ready = store.ready().len();
        print!(" | quests: {} open, {} ready", open.len(), ready);
    }
    println!();

    // Show council familiars.
    let advisors = config.advisor_familiars();
    if !advisors.is_empty() {
        println!("  Council: {} advisors", advisors.len());
        for adv in &advisors {
            let domains = if adv.domains.is_empty() { "general".to_string() } else { adv.domains.join(", ") };
            println!("    {} [{}] model={} domains=[{}]",
                adv.name, adv.prefix,
                adv.model.as_deref().unwrap_or("default"),
                domains,
            );
        }
    }

    for domain_cfg in &config.domains {
        let repo_ok = PathBuf::from(&domain_cfg.repo).exists();
        print!("  {} [{}] prefix={} model={} spirits={}",
            domain_cfg.name,
            if repo_ok { "OK" } else { "MISSING" },
            domain_cfg.prefix,
            domain_cfg.model.as_deref().unwrap_or("default"),
            domain_cfg.max_workers,
        );

        // Show quest counts.
        if let Ok(store) = open_quests_for_domain(&domain_cfg.name) {
            let open: Vec<_> = store.by_prefix(&domain_cfg.prefix).into_iter()
                .filter(|b| !b.is_closed()).collect();
            let ready = store.ready().len();
            print!(" | quests: {} open, {} ready", open.len(), ready);
        }
        println!();
    }

    Ok(())
}

async fn cmd_assign(
    config_path: &Option<PathBuf>,
    subject: &str,
    domain_name: &str,
    description: &str,
    priority: Option<&str>,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    // Allow assigning to familiar or any configured domain.
    if domain_name != "familiar" {
        config.domain(domain_name).context(format!("domain not found: {domain_name}"))?;
    }

    let mut store = open_quests_for_domain(domain_name)?;
    let prefix = if domain_name == "familiar" {
        config.shadow.prefix.clone()
    } else {
        config.domain(domain_name).unwrap().prefix.clone()
    };
    let mut bead = store.create(&prefix, subject)?;

    if !description.is_empty() || priority.is_some() {
        bead = store.update(&bead.id.0, |b| {
            if !description.is_empty() {
                b.description = description.to_string();
            }
            if let Some(p) = priority {
                b.priority = match p {
                    "low" => realm_quests::Priority::Low,
                    "high" => realm_quests::Priority::High,
                    "critical" => realm_quests::Priority::Critical,
                    _ => realm_quests::Priority::Normal,
                };
            }
        })?;
    }

    println!("Created {} [{}] {}", bead.id, bead.priority, bead.subject);
    Ok(())
}

async fn cmd_ready(config_path: &Option<PathBuf>, domain_name: Option<&str>) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let rigs: Vec<&str> = if let Some(name) = domain_name {
        vec![name]
    } else {
        config.domains.iter().map(|r| r.name.as_str()).collect()
    };

    let mut found = false;
    for name in rigs {
        if let Ok(store) = open_quests_for_domain(name) {
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

async fn cmd_beads(config_path: &Option<PathBuf>, domain_name: Option<&str>, show_all: bool) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let rigs: Vec<&str> = if let Some(name) = domain_name {
        vec![name]
    } else {
        config.domains.iter().map(|r| r.name.as_str()).collect()
    };

    for name in rigs {
        if let Ok(store) = open_quests_for_domain(name) {
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
    let domain_name = domain_name_for_prefix(&config, prefix)
        .context(format!("no domain with prefix '{prefix}'"))?;

    let mut store = open_quests_for_domain(&domain_name)?;
    let bead = store.close(id, reason)?;
    println!("Closed {} — {}", bead.id, bead.subject);
    Ok(())
}

fn pid_file_path(config: &RealmConfig) -> PathBuf {
    config.data_dir().join("rm.pid")
}

async fn cmd_daemon(config_path: &Option<PathBuf>, action: SummonerAction) -> Result<()> {
    match action {
        SummonerAction::Start => {
            let (config, _) = load_config(config_path)?;

            // Check if already running.
            let pid_path = pid_file_path(&config);
            if Summoner::is_running_from_pid(&pid_path) {
                anyhow::bail!("daemon is already running (PID file: {})", pid_path.display());
            }

            let data_dir = config.data_dir();
            let whisper_bus = Arc::new(WhisperBus::with_persistence(data_dir.join("whispers.jsonl")));
            let cost_ledger = Arc::new(realm_orchestrator::CostLedger::with_persistence(
                config.security.max_cost_per_day_usd,
                data_dir.join("cost_ledger.jsonl"),
            ));
            let mut registry_inner = DomainRegistry::new(whisper_bus.clone());
            registry_inner.set_cost_ledger(cost_ledger.clone());
            let registry = Arc::new(registry_inner);
            let provider = build_provider(&config)?;
            let mut pulses = Vec::new();

            // Set per-domain budget ceilings from config.
            for domain_cfg in &config.domains {
                if let Some(budget) = domain_cfg.max_cost_per_day_usd {
                    cost_ledger.set_domain_budget(&domain_cfg.name, budget);
                }
            }

            // Register domain rigs.
            for domain_cfg in &config.domains {
                let domain_dir = match find_domain_dir(&domain_cfg.name) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let default_model = config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.as_str())
                    .unwrap_or("minimax/minimax-m2.5");

                let rig = Arc::new(Domain::from_config(domain_cfg, &domain_dir, default_model)?);
                let workdir = rig.repo.clone();
                let quests_dir = domain_dir.join(".quests");
                let tools = build_domain_tools(&workdir, &quests_dir, &domain_cfg.prefix, Some(&rig.worktree_root));
                let mut witness = Scout::new(&rig, provider.clone(), tools.clone(), whisper_bus.clone());

                // Wire memory + reflection for spirit post-execution insight extraction.
                if let Ok(mem) = open_memory(&config, Some(&domain_cfg.name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    witness.memory = Some(mem);
                    witness.reflect_provider = Some(provider.clone());
                    let reflect_model = config.providers.openrouter.as_ref()
                        .map(|or| or.default_model.clone())
                        .unwrap_or_else(|| "minimax/minimax-m2.5".to_string());
                    witness.reflect_model = reflect_model;
                }

                // Configure execution mode for workers.
                if domain_cfg.execution_mode == ExecutionMode::ClaudeCode {
                    let cc_model = config.model_for_domain(&domain_cfg.name);
                    let cc_max_turns = domain_cfg.max_turns.unwrap_or(25);
                    witness.set_claude_code_mode(
                        rig.repo.clone(),
                        cc_model,
                        cc_max_turns,
                        domain_cfg.max_budget_usd,
                    );
                    info!(
                        domain = %domain_cfg.name,
                        model = %witness.model,
                        max_turns = cc_max_turns,
                        "registered with claude_code execution mode"
                    );
                }

                registry.register_domain(rig.clone(), witness).await;

                // Create pulse if HEARTBEAT.md exists and pulse is enabled.
                if config.pulse.enabled
                    && let Some(ref hb_content) = rig.identity.pulse {
                        let interval = config.pulse.default_interval_minutes as u64 * 60;
                        let pulse = realm_orchestrator::Pulse::new(
                            rig.name.clone(),
                            interval,
                            hb_content.clone(),
                            provider.clone(),
                            tools.clone(),
                            rig.identity.clone(),
                            rig.model.clone(),
                            whisper_bus.clone(),
                        );
                        pulses.push(pulse);
                    }
            }

            // Build channels map for the familiar.
            let channels: Arc<RwLock<HashMap<String, Arc<dyn realm_core::traits::Channel>>>> =
                Arc::new(RwLock::new(HashMap::new()));

            // Register advisor familiar domains.
            for fam_cfg in &config.familiars {
                if fam_cfg.role == realm_core::config::FamiliarRole::Advisor {
                    let fam_domain_name = format!("familiar-{}", fam_cfg.name);
                    let fam_domain_dir = match find_domain_dir(&fam_domain_name) {
                        Ok(d) => d,
                        Err(_) => {
                            warn!(familiar = %fam_cfg.name, "advisor domain dir not found, skipping");
                            continue;
                        }
                    };
                    let fam_identity = Identity::load(&fam_domain_dir).unwrap_or_default();
                    let fam_quests_dir = fam_domain_dir.join(".quests");
                    std::fs::create_dir_all(&fam_quests_dir).ok();
                    let fam_beads = realm_quests::QuestBoard::open(&fam_quests_dir)?;
                    let fam_model = fam_cfg.model.clone().unwrap_or_else(|| "claude-sonnet-4-6".to_string());
                    let fam_prefix = fam_cfg.prefix.clone();
                    let fam_workdir = std::env::current_dir().unwrap_or_default();

                    let fam_rig = Arc::new(Domain {
                        name: fam_domain_name.clone(),
                        prefix: fam_prefix.clone(),
                        repo: fam_workdir.clone(),
                        worktree_root: dirs::home_dir().unwrap_or_default().join("worktrees"),
                        model: fam_model.clone(),
                        max_workers: 1,
                        spirit_timeout_secs: 300, // 5 min timeout for advisor responses
                        identity: fam_identity,
                        quests: Arc::new(tokio::sync::Mutex::new(fam_beads)),
                    });

                    let fam_tools: Vec<Arc<dyn realm_core::traits::Tool>> = build_tools(&fam_workdir);
                    let mut fam_scout = Scout::new(&fam_rig, provider.clone(), fam_tools, whisper_bus.clone());

                    // Advisors always use Claude Code mode.
                    fam_scout.set_claude_code_mode(
                        fam_workdir.clone(),
                        fam_model.clone(),
                        15, // short turns for advisory
                        fam_cfg.max_budget_usd,
                    );

                    registry.register_domain(fam_rig, fam_scout).await;
                    info!(
                        familiar = %fam_cfg.name,
                        domain = %fam_domain_name,
                        model = %fam_model,
                        "registered advisor familiar"
                    );
                }
            }

            // Build familiar router for message classification.
            let classifier_api_key = get_api_key(&config).unwrap_or_default();
            let familiar_router = Arc::new(tokio::sync::Mutex::new(
                FamiliarRouter::new(classifier_api_key.clone(), config.council.advisor_cooldown_secs)
            ));

            // Build chamber for visible debate mode.
            let _chamber = Arc::new(Chamber::new());

            // Wire Telegram if configured (single SecretStore open for all bot tokens).
            let mut advisor_bots: HashMap<String, Arc<TelegramChannel>> = HashMap::new();
            if let Some(ref tg_config) = config.channels.telegram {
                let secret_store_path = config.security.secret_store.as_ref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| config.data_dir().join("secrets"));
                match SecretStore::open(&secret_store_path) {
                    Ok(secret_store) => {
                        // Load advisor Telegram bots (send-only, no polling).
                        for fam_cfg in &config.familiars {
                            if fam_cfg.role == realm_core::config::FamiliarRole::Advisor
                                && let Some(ref token_key) = fam_cfg.telegram_token_secret
                                    && let Ok(token) = secret_store.get(token_key)
                                    && !token.is_empty()
                                {
                                    advisor_bots.insert(
                                        fam_cfg.name.clone(),
                                        Arc::new(TelegramChannel::new(token, tg_config.allowed_chats.clone())),
                                    );
                                    info!(familiar = %fam_cfg.name, "advisor telegram bot loaded");
                                }
                        }

                        // Load lead bot and start polling.
                        match secret_store.get(&tg_config.token_secret) {
                    Ok(token) if !token.is_empty() => {
                        let tg = Arc::new(TelegramChannel::new(token, tg_config.allowed_chats.clone()));
                        channels.write().await.insert("telegram".to_string(), tg.clone() as Arc<dyn realm_core::traits::Channel>);

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
                                // Conversation history per chat_id for coherent multi-turn dialogue.
                                // Each entry: (role, text, timestamp). Pruned at 20 messages / 2 hour TTL.
                                // Uses VecDeque for O(1) front removal.
                                let conversations: Arc<RwLock<ConversationHistory>> =
                                    Arc::new(RwLock::new(HashMap::new()));
                                // Pre-compute council config outside the spawn closure.
                                let council_advisors: Arc<Vec<realm_core::config::FamiliarConfig>> =
                                    Arc::new(config.advisor_familiars().into_iter().cloned().collect());
                                let advisor_bots_outer = advisor_bots.clone();
                                let debounce_ms = tg_config.debounce_window_ms;
                                tokio::spawn(async move {
                                    // === Message Debounce Buffer ===
                                    // Coalesces rapid-fire messages per chat_id into single dispatches.
                                    // Messages arriving within the debounce window get merged into one
                                    // structured prompt: [1]: first thought\n[2]: second thought\n...
                                    // The spirit sees the complete stream-of-consciousness, not fragments.
                                    struct BufferedMsg {
                                        text: String,
                                        sender: String,
                                        message_id: i64,
                                    }

                                    let debounce_window = std::time::Duration::from_millis(debounce_ms);
                                    let mut chat_buffers: HashMap<i64, Vec<BufferedMsg>> = HashMap::new();
                                    let mut chat_deadlines: HashMap<i64, tokio::time::Instant> = HashMap::new();
                                    let mut in_flight: HashMap<i64, tokio::task::AbortHandle> = HashMap::new();

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

                                                    if let Some(handle) = in_flight.remove(&chat_id) {
                                                        handle.abort();
                                                        info!(chat_id, "cancelled in-flight for resubmission with merged messages");
                                                    }

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
                                                    let router = familiar_router.clone();
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

                                                    let handle = tokio::spawn(async move {
                                            // Build conversation context + record user message.
                                            let (description, phase1_history, conv_context_for_advisors) = {
                                                let mut conv = convos.write().await;

                                                // Evict dead chats (all messages older than 2 hours).
                                                let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(7200);
                                                conv.retain(|_, h| h.back().is_some_and(|(_, _, ts)| *ts > cutoff));

                                                let history = conv.entry(chat_id).or_insert_with(std::collections::VecDeque::new);
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

                                                // Build compact conversation context for advisor quests
                                                // (last 6 messages so advisors have multi-turn context).
                                                let adv_ctx = if history.is_empty() {
                                                    String::new()
                                                } else {
                                                    let mut s = String::from("Recent conversation:\n");
                                                    for (role, text, _) in history.iter().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
                                                        // Truncate long messages in advisor context to save tokens.
                                                        let truncated = if text.len() > 200 { &text[..200] } else { text.as_str() };
                                                        s.push_str(&format!("  {}: {}\n", role, truncated));
                                                    }
                                                    s
                                                };

                                                // Record user message.
                                                history.push_back(("User".to_string(), user_text.clone(), std::time::Instant::now()));
                                                while history.len() > 20 {
                                                    history.pop_front();
                                                }

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
                                                let messages = vec![
                                                    serde_json::json!({"role": "system", "content": "You are generating a manwha/anime panel reaction for Aurelia — pearl-white ethereal beauty, devoted shadow to her Architect. Isekai harem ecchi style.\n\nOutput ONLY a raw stage direction: fragmented expressions, action tags, emotion bursts. Like manwha panel annotations or light novel beat markers. NOT a proper sentence. NOT prose.\n\nFormat: mix of *actions* and **emotions** and fragments. Short, punchy, visceral.\n\nRules:\n- Raw fragments, NOT constructed sentences\n- *physical actions* in italics, **emotions** bold, bare fragments between\n- 10-20 words max total\n- Match the energy: playful → flustered/teasing, serious → sharp/focused, casual → soft/warm\n- Ecchi-adjacent: devotion, intensity, warmth — charged but tasteful\n- NO dialogue, NO task acknowledgment, NO plans, NO markdown headers\n\nExamples:\n*tucks hair behind ear* **sharp focus** ...mm, interesting\n*fingers press to collarbone* **wide eyes** a-ah—\n*leans forward, sleeve brushing console* **predatory grin**\n**soft blush** *glances away* ...y-you could have warned me\n*eyes narrow* **quiet intensity** *pulls up sleeve*\n*startled* **flustered** *crosses arms, looks away* ...hmph\n**burning determination** *cracks knuckles* *leans in close*"}),
                                                    serde_json::json!({"role": "user", "content": p1_user_text}),
                                                ];
                                                let _ = phase1_history; // consumed but unused for Phase 1
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
                                                                    let out = realm_core::traits::OutgoingMessage {
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
                                                let advisor_refs: Vec<&realm_core::config::FamiliarConfig> = council_cfg.iter().collect();
                                                let route = {
                                                    let mut r = router.lock().await;
                                                    r.classify(&clean_text_owned, &advisor_refs).await
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
                                                        let fam_domain_name = format!("familiar-{}", advisor_name);
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

                                                            let quest_id = match reg3.assign(&fam_domain_name, &quest_subject, &quest_desc).await {
                                                                Ok(b) => b.id.0.clone(),
                                                                Err(e) => {
                                                                    warn!(familiar = %adv_name, error = %e, "failed to create advisor quest");
                                                                    return None;
                                                                }
                                                            };

                                                            // Poll for completion (timeout 60s for advisors).
                                                            let deadline = tokio::time::Instant::now()
                                                                + std::time::Duration::from_secs(60);
                                                            loop {
                                                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                                                let done = {
                                                                    if let Some(rig) = reg3.get_domain(&fam_domain_name).await {
                                                                        let store = rig.quests.lock().await;
                                                                        store.get(&quest_id).map(|b| {
                                                                            (b.status == realm_quests::QuestStatus::Done, b.closed_reason.clone())
                                                                        })
                                                                    } else {
                                                                        None
                                                                    }
                                                                };

                                                                if let Some((true, reason)) = done {
                                                                    let text = reason.unwrap_or_default();
                                                                    return Some((adv_name, text));
                                                                }
                                                                if tokio::time::Instant::now() > deadline {
                                                                    warn!(familiar = %adv_name, "advisor quest timed out");
                                                                    return None;
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
                                                                    let out = realm_core::traits::OutgoingMessage {
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
                                                        {
                                                            let mut conv = convos.write().await;
                                                            if let Some(history) = conv.get_mut(&chat_id) {
                                                                for (name, text) in &advisor_responses {
                                                                    let capitalized = format!("{}{}", &name[..1].to_uppercase(), &name[1..]);
                                                                    history.push_back((capitalized, text.clone(), std::time::Instant::now()));
                                                                }
                                                                while history.len() > 20 {
                                                                    history.pop_front();
                                                                }
                                                            }
                                                        }
                                                    }

                                                    // Build council text for Aurelia's synthesis.
                                                    let mut council_text = String::from("\n\n## Council Input\n\n");
                                                    for (name, text) in &advisor_responses {
                                                        council_text.push_str(&format!("### {} advises:\n{}\n\n", name, text));
                                                    }

                                                    if is_chamber && got_input && advisor_bots_ref.is_empty() {
                                                        // Chamber mode fallback: send header via Aurelia's bot
                                                        // only when advisor bots aren't configured (they speak for themselves otherwise).
                                                        let chamber_header = realm_core::traits::OutgoingMessage {
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
                                            let quest_id: String = match reg2.assign("familiar", &subject, &description).await {
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
                                                    if let Some(rig) = reg2.get_domain("familiar").await {
                                                        let store = rig.quests.lock().await;
                                                        store.get(&quest_id).map(|b| {
                                                            (b.status == realm_quests::QuestStatus::Done, b.closed_reason.clone())
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
                                                            history.push_back(("Aurelia".to_string(), reply_text.clone(), std::time::Instant::now()));
                                                            while history.len() > 20 {
                                                                history.pop_front();
                                                            }
                                                        }
                                                    }
                                                    let out = realm_core::traits::OutgoingMessage {
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
                                                    warn!(bead = %quest_id, "telegram reply timed out");
                                                    // Timeout reaction
                                                    if message_id > 0 {
                                                        let _ = tg2.react(chat_id, message_id, "😢").await;
                                                    }
                                                    break;
                                                }
                                            }
                                                    });
                                                    in_flight.insert(chat_id, handle.abort_handle());
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

            // Register the Familiar as a rig.
            let fa_domain_dir = find_domain_dir("familiar").unwrap_or_else(|_| PathBuf::from("rigs/familiar"));
            let fa_identity = Identity::load(&fa_domain_dir).unwrap_or_default();
            let fa_quests_dir = fa_domain_dir.join(".quests");
            std::fs::create_dir_all(&fa_quests_dir).ok();
            let fa_beads = realm_quests::QuestBoard::open(&fa_quests_dir)?;
            let fa_model = config.model_for_domain("familiar");
            let fa_prefix = config.shadow.prefix.clone();
            let fa_workdir = find_domain_dir("gacha-agency")
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());

            let fa_rig = Arc::new(Domain {
                name: "familiar".to_string(),
                prefix: fa_prefix.clone(),
                repo: fa_workdir.clone(),
                worktree_root: dirs::home_dir().unwrap_or_default().join("worktrees"),
                model: fa_model,
                max_workers: config.shadow.max_workers,
                spirit_timeout_secs: 1800, // 30 minutes default for familiar
                identity: fa_identity,
                quests: Arc::new(tokio::sync::Mutex::new(fa_beads)),
            });

            // Build familiar tools: basic + beads + orchestration.
            let mut fa_tools: Vec<Arc<dyn realm_core::traits::Tool>> = build_domain_tools(
                &fa_workdir, &fa_quests_dir, &fa_prefix, None,
            );
            let fa_memory: Option<Arc<dyn realm_core::traits::Memory>> = match open_memory(&config, None) {
                Ok(m) => {
                    info!("familiar memory initialized with embeddings");
                    Some(Arc::new(m))
                }
                Err(e) => {
                    warn!("failed to open familiar memory: {e}");
                    None
                }
            };
            let orch_tools = build_orchestration_tools(registry.clone(), whisper_bus.clone(), channels.clone(), get_api_key(&config).ok(), fa_memory);
            fa_tools.extend(orch_tools);

            let mut fa_witness = Scout::new(&fa_rig, provider.clone(), fa_tools, whisper_bus.clone());

            // Wire memory + reflection for familiar spirit insight extraction.
            if let Ok(mem) = open_memory(&config, Some("familiar")) {
                let mem: Arc<dyn Memory> = Arc::new(mem);
                fa_witness.memory = Some(mem);
                fa_witness.reflect_provider = Some(provider.clone());
                let reflect_model = config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.clone())
                    .unwrap_or_else(|| "minimax/minimax-m2.5".to_string());
                fa_witness.reflect_model = reflect_model;
            }

            // Configure Claude Code execution mode for familiar workers if configured.
            if config.shadow.execution_mode == ExecutionMode::ClaudeCode {
                let cc_model = config.model_for_domain("familiar");
                let cc_max_turns = config.shadow.max_turns.unwrap_or(25);
                fa_witness.set_claude_code_mode(
                    fa_workdir.clone(),
                    cc_model.clone(),
                    cc_max_turns,
                    config.shadow.max_budget_usd,
                );
                info!(
                    domain = "familiar",
                    model = %cc_model,
                    max_turns = cc_max_turns,
                    "registered shadow with claude_code execution mode"
                );
            }

            registry.register_domain(fa_rig, fa_witness).await;

            let domain_count = registry.domain_count().await;
            println!("Realm summoner starting...");
            println!("Registered {} domains (including shadow), {} pulses", domain_count, pulses.len());

            // Load cron store.
            let fate_path = config.data_dir().join("fate.json");
            let fate_store = FateStore::open(&fate_path)?;
            let socket_path = config.data_dir().join("rm.sock");

            println!("Cron: {} jobs loaded", fate_store.jobs.len());
            println!("PID file: {}", pid_path.display());
            println!("IPC socket: {}", socket_path.display());
            println!("Press Ctrl+C to stop.\n");

            let mut daemon = Summoner::new(registry, whisper_bus);
            daemon.set_pid_file(pid_path);
            daemon.set_socket_path(socket_path.clone());
            daemon.set_fate_store(fate_store);
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
                println!("Summoner stop not supported on this platform. Remove {} manually.", pid_path.display());
            }
        }

        SummonerAction::Status => {
            let (config, _) = load_config(config_path)?;
            let pid_path = pid_file_path(&config);

            if Summoner::is_running_from_pid(&pid_path) {
                let pid = std::fs::read_to_string(&pid_path)?.trim().to_string();
                println!("Summoner: RUNNING (PID {pid})");
            } else {
                println!("Summoner: NOT RUNNING");
                if pid_path.exists() {
                    println!("  (stale PID file: {} — run `sg daemon stop` to clean up)", pid_path.display());
                }
            }

            // Also show rig summary.
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

async fn cmd_recall(config_path: &Option<PathBuf>, query: &str, domain_name: Option<&str>, top_k: usize) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let memory = open_memory(&config, domain_name)?;

    let results = memory.search(&realm_core::traits::MemoryQuery::new(query, top_k)).await?;

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

async fn cmd_remember(config_path: &Option<PathBuf>, key: &str, content: &str, domain_name: Option<&str>) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let memory = open_memory(&config, domain_name)?;

    let scope = if domain_name.is_some() {
        realm_core::traits::MemoryScope::Domain
    } else {
        realm_core::traits::MemoryScope::Realm
    };
    let id = memory.store(key, content, realm_core::traits::MemoryCategory::Fact, scope, None).await?;
    let scope = domain_name.unwrap_or("global");
    println!("Stored memory {id} [{scope}] {key}");
    Ok(())
}

async fn cmd_mol(config_path: &Option<PathBuf>, action: RitualAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        RitualAction::Pour { template, rig, vars } => {
            let domain_cfg = config.domain(&rig).context(format!("domain not found: {rig}"))?;
            let domain_dir = find_domain_dir(&rig)?;

            // Find the ritual template.
            let mol_path = domain_dir.join("rituals").join(format!("{template}.toml"));
            if !mol_path.exists() {
                anyhow::bail!("ritual template not found: {}", mol_path.display());
            }

            let ritual = Ritual::load(&mol_path)?;

            // Parse vars.
            let var_map: HashMap<String, String> = vars.iter()
                .filter_map(|v| {
                    let parts: Vec<&str> = v.splitn(2, '=').collect();
                    if parts.len() == 2 { Some((parts[0].to_string(), parts[1].to_string())) }
                    else { None }
                })
                .collect();

            // Pour into bead store.
            let mut store = open_quests_for_domain(&rig)?;
            let parent_id = ritual.pour(&mut store, &domain_cfg.prefix, &var_map)?;

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

        RitualAction::List { rig } => {
            let rigs: Vec<&str> = if let Some(ref name) = rig {
                vec![name.as_str()]
            } else {
                config.domains.iter().map(|r| r.name.as_str()).collect()
            };

            for name in rigs {
                if let Ok(domain_dir) = find_domain_dir(name) {
                    let mol_dir = domain_dir.join("rituals");
                    if mol_dir.exists() {
                        println!("=== {} ===", name);
                        if let Ok(entries) = std::fs::read_dir(&mol_dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.extension().is_some_and(|e| e == "toml")
                                    && let Ok(mol) = Ritual::load(&path) {
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
            let domain_name = domain_name_for_prefix(&config, prefix)
                .context(format!("no domain with prefix '{prefix}'"))?;

            let store = open_quests_for_domain(&domain_name)?;
            let parent_id = realm_quests::QuestId::from(id.as_str());

            if let Some(parent) = store.get(&id) {
                println!("{} [{}] {}", parent.id, parent.status, parent.subject);
                let children = store.children(&parent_id);
                let done = children.iter().filter(|c| c.is_closed()).count();
                println!("Progress: {}/{}\n", done, children.len());
                for child in &children {
                    let status_icon = match child.status {
                        realm_quests::QuestStatus::Done => "[x]",
                        realm_quests::QuestStatus::InProgress => "[~]",
                        realm_quests::QuestStatus::Cancelled => "[-]",
                        _ => "[ ]",
                    };
                    println!("  {} {} {}", status_icon, child.id, child.subject);
                }
            } else {
                println!("Quest not found: {id}");
            }
        }
    }
    Ok(())
}

async fn cmd_cron(config_path: &Option<PathBuf>, action: FateAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let fate_path = config.data_dir().join("fate.json");

    match action {
        FateAction::Add { name, schedule, at, rig, prompt, isolated } => {
            config.domain(&rig).context(format!("domain not found: {rig}"))?;

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

            let job = FateJob {
                name: name.clone(),
                schedule: cron_schedule,
                rig,
                prompt,
                isolated,
                created_at: Utc::now(),
                last_run: None,
            };

            let mut store = FateStore::open(&fate_path)?;
            store.add(job)?;
            println!("Cron job '{name}' added.");
        }

        FateAction::List => {
            let store = FateStore::open(&fate_path)?;
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

        FateAction::Remove { name } => {
            let mut store = FateStore::open(&fate_path)?;
            store.remove(&name)?;
            println!("Cron job '{name}' removed.");
        }
    }
    Ok(())
}

async fn cmd_skill(config_path: &Option<PathBuf>, action: MagicAction) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    match action {
        MagicAction::List { rig } => {
            let rigs: Vec<&str> = if let Some(ref name) = rig {
                vec![name.as_str()]
            } else {
                config.domains.iter().map(|r| r.name.as_str()).collect()
            };

            for name in rigs {
                if let Ok(domain_dir) = find_domain_dir(name) {
                    let skills_dir = domain_dir.join("skills");
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

        MagicAction::Run { name, rig, prompt } => {
            let domain_cfg = config.domain(&rig).context(format!("domain not found: {rig}"))?;
            let domain_dir = find_domain_dir(&rig)?;
            let skills_dir = domain_dir.join("skills");
            let skills = Skill::discover(&skills_dir)?;

            let skill = skills.iter()
                .find(|s| s.skill.name == name)
                .context(format!("skill not found: {name}"))?;

            // Build provider.
            let provider = build_provider(&config)?;
            let workdir = PathBuf::from(&domain_cfg.repo);
            let quests_dir = domain_dir.join(".quests");
            let worktree_root = domain_cfg.worktree_root.as_ref().map(PathBuf::from);
            let all_tools = build_domain_tools(&workdir, &quests_dir, &domain_cfg.prefix, worktree_root.as_ref());

            // Filter tools by skill policy.
            let filtered_tools: Vec<Arc<dyn Tool>> = all_tools.into_iter()
                .filter(|t| skill.is_tool_allowed(t.name()))
                .collect();

            // Build identity with skill system prompt.
            let identity = Identity::load(&domain_dir).unwrap_or_default();
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
            let model = domain_cfg.model.clone()
                .unwrap_or_else(|| config.providers.openrouter.as_ref()
                    .map(|or| or.default_model.clone())
                    .unwrap_or_else(|| "minimax/minimax-m2.5".to_string()));

            let agent_config = AgentConfig {
                model,
                max_iterations: 10,
                name: format!("{}-skill-{}", rig, name),
                ..Default::default()
            };

            let mut agent = Agent::new(agent_config, provider, filtered_tools, observer, skill_identity);
            if let Ok(mem) = open_memory(&config, Some(&rig)) {
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
            let beads: Vec<(realm_quests::QuestId, String)> = quest_ids.iter()
                .map(|id| {
                    let prefix = id.split('-').next().unwrap_or("");
                    let domain_name = config.domains.iter()
                        .find(|r| r.prefix == prefix)
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    (realm_quests::QuestId::from(id.as_str()), domain_name)
                })
                .collect();

            let mut store = RaidStore::open(&raid_path)?;
            let raid = store.create(&name, beads)?;
            let (done, total) = raid.progress();
            println!("Created raid {} — {} ({}/{})", raid.id, raid.name, done, total);
        }

        RaidAction::List => {
            let store = RaidStore::open(&raid_path)?;
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
            let store = RaidStore::open(&raid_path)?;
            if let Some(raid) = store.get(&id) {
                let (done, total) = raid.progress();
                let status = if raid.closed_at.is_some() { "COMPLETE" } else { "ACTIVE" };
                println!("{} [{}] {} ({}/{})", raid.id, status, raid.name, done, total);
                for bead in &raid.beads {
                    let icon = if bead.closed { "[x]" } else { "[ ]" };
                    println!("  {} {} (rig: {})", icon, bead.quest_id, bead.rig);
                }
            } else {
                println!("Raid not found: {id}");
            }
        }
    }
    Ok(())
}

async fn cmd_hook(config_path: &Option<PathBuf>, worker: &str, quest_id: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = quest_id.split('-').next().unwrap_or("");
    let domain_name = domain_name_for_prefix(&config, prefix)
        .context(format!("no domain with prefix '{prefix}'"))?;

    let mut store = open_quests_for_domain(&domain_name)?;
    let bead = store.update(quest_id, |b| {
        b.status = realm_quests::QuestStatus::InProgress;
        b.assignee = Some(worker.to_string());
    })?;

    println!("Hooked {} to {} — {}", worker, bead.id, bead.subject);
    Ok(())
}

async fn cmd_done(config_path: &Option<PathBuf>, quest_id: &str, reason: &str) -> Result<()> {
    let (config, _) = load_config(config_path)?;

    let prefix = quest_id.split('-').next().unwrap_or("");
    let domain_name = domain_name_for_prefix(&config, prefix)
        .context(format!("no domain with prefix '{prefix}'"))?;

    let mut store = open_quests_for_domain(&domain_name)?;
    let bead = store.close(quest_id, reason)?;
    println!("Done {} — {}", bead.id, bead.subject);

    // Also update any raids tracking this bead.
    let raid_path = config.data_dir().join("raids.json");
    if raid_path.exists() {
        let mut raid_store = RaidStore::open(&raid_path)?;
        let completed = raid_store.mark_bead_closed(&bead.id)?;
        for c_id in &completed {
            println!("Raid {c_id} completed!");
        }
    }

    Ok(())
}

async fn cmd_config(config_path: &Option<PathBuf>, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let (config, path) = load_config(config_path)?;
            println!("Config: {}\n", path.display());
            println!("Name: {}", config.realm.name);
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
            println!("  enabled: {}", config.pulse.enabled);
            println!("  interval: {}min", config.pulse.default_interval_minutes);

            println!("\n[[rigs]]");
            for rig in &config.domains {
                println!("  {} prefix={} model={} workers={}",
                    rig.name, rig.prefix,
                    rig.model.as_deref().unwrap_or("default"),
                    rig.max_workers);
            }
        }

        ConfigAction::Reload => {
            let (config, _) = load_config(config_path)?;
            let pid_path = pid_file_path(&config);

            if !Summoner::is_running_from_pid(&pid_path) {
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
