use clap::Subcommand;

#[derive(Subcommand)]
pub enum Commands {
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
    /// Initialize Sigil in the current directory.
    Init,
    /// Bootstrap a ready-to-run Sigil workspace.
    Setup {
        /// Default runtime preset (for example: openrouter_agent, anthropic_agent, ollama_agent).
        #[arg(long, default_value = "openrouter_agent")]
        runtime: String,
        /// Install a per-user daemon service after bootstrapping the workspace.
        #[arg(long)]
        service: bool,
        /// Overwrite starter files that already exist.
        #[arg(long)]
        force: bool,
    },
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
        /// Exit with a non-zero status if any issues remain.
        #[arg(long)]
        strict: bool,
    },
    /// Show system status.
    Status,
    /// Show a consolidated operator monitor view.
    Monitor {
        /// Focus on a single project.
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        /// Refresh the monitor continuously.
        #[arg(long)]
        watch: bool,
        /// Refresh interval in seconds when --watch is enabled.
        #[arg(long, default_value = "5")]
        interval_secs: u64,
        /// Emit the monitor report as JSON.
        #[arg(long)]
        json: bool,
    },

    // --- Phase 2: Tasks ---
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
    /// Show all open tasks.
    Tasks {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Close a task.
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

    // --- Phase 5: Pipelines ---
    /// Pipeline workflow commands.
    #[command(alias = "mol")]
    Pipeline {
        #[command(subcommand)]
        action: PipelineAction,
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
        action: OperationAction,
    },

    // --- Worker management ---
    /// Pin work to a worker.
    Hook { worker: String, task_id: String },
    /// Mark task as done, trigger cleanup.
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

    /// Manage agent discovery and configuration.
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },

    /// Query the decision audit trail.
    Audit {
        /// Filter by project name.
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        /// Filter by task ID.
        #[arg(short, long)]
        task: Option<String>,
        /// Show last N events.
        #[arg(short, long, default_value = "20")]
        last: u32,
    },

    /// Query or post to the inter-agent blackboard.
    Blackboard {
        #[command(subcommand)]
        action: BlackboardAction,
    },

    /// Suggest or apply inferred task dependencies.
    Deps {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        /// Auto-apply dependencies above this confidence threshold.
        #[arg(long)]
        apply: Option<f64>,
    },

    /// Start the web API server.
    Web {
        #[command(subcommand)]
        action: WebAction,
    },

    /// Run as an MCP (Model Context Protocol) server.
    Mcp,
}

#[derive(Subcommand)]
pub enum BlackboardAction {
    /// List blackboard entries for a project.
    List {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        #[arg(short, long, default_value = "20")]
        limit: u32,
    },
    /// Post a new entry to the blackboard.
    Post {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        key: String,
        content: String,
        #[arg(short, long)]
        tags: Vec<String>,
        #[arg(long, default_value = "transient")]
        durability: String,
    },
    /// Query blackboard by tags.
    Query {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        #[arg(short, long)]
        tags: Vec<String>,
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
}

#[derive(Subcommand)]
pub enum AgentAction {
    /// List all discovered agents (from disk + TOML).
    List,
    /// Migrate `[[agents]]` from sigil.toml to agent.toml files on disk.
    Migrate {
        /// Overwrite existing agent.toml files.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum SecretsAction {
    Set { name: String, value: String },
    Get { name: String },
    List,
    Delete { name: String },
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start the daemon (runs in foreground).
    Start,
    /// Install a per-user daemon service.
    Install {
        /// Start the service immediately after installing it.
        #[arg(long)]
        start: bool,
        /// Overwrite an existing service definition.
        #[arg(long)]
        force: bool,
    },
    /// Print the generated service definition.
    PrintService,
    /// Stop a running daemon.
    Stop,
    /// Uninstall the per-user daemon service.
    Uninstall {
        /// Stop the service before removing it.
        #[arg(long)]
        stop: bool,
    },
    /// Show daemon status.
    Status,
    /// Query the running daemon via IPC socket.
    Query {
        /// Command to send (ping, status, readiness, projects, dispatches, cost, metrics, audit, blackboard, expertise).
        cmd: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Reload configuration (send SIGHUP to daemon).
    Reload,
    /// Show current config.
    Show,
}

#[derive(Subcommand)]
pub enum CronAction {
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
pub enum SkillAction {
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
pub enum OperationAction {
    /// Create an operation tracking tasks across projects.
    Create {
        name: String,
        /// Task IDs to track (e.g. as-001 rd-002).
        task_ids: Vec<String>,
    },
    /// List active operations.
    List,
    /// Show operation status.
    Status { id: String },
}

#[derive(Subcommand)]
pub enum MissionAction {
    /// Create a new mission.
    Create {
        name: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        #[arg(short, long, default_value = "")]
        description: String,
        /// Auto-decompose into sub-tasks (requires LLM).
        #[arg(long)]
        decompose: bool,
    },
    /// List missions.
    List {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Show mission details and its tasks.
    Status { id: String },
    /// Close a mission.
    Close { id: String },
}

#[derive(Subcommand)]
pub enum PipelineAction {
    /// Pour (instantiate) a pipeline workflow.
    Pour {
        template: String,
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: String,
        /// Variables as key=value pairs.
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// List available pipeline templates.
    List {
        #[arg(short = 'r', long = "project", alias = "rig")]
        project: Option<String>,
    },
    /// Show status of a pipeline (parent task and its children).
    Status { id: String },
}

#[derive(Subcommand)]
pub enum WebAction {
    /// Start the web API server.
    Start {
        /// Override bind address (default: from config or 0.0.0.0:8400).
        #[arg(long)]
        bind: Option<String>,
    },
}
