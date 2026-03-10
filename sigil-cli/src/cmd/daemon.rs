use anyhow::{Context, Result};
use sigil_core::traits::{Channel, Memory};
use sigil_core::{ExecutionMode, Identity, SecretStore};
use sigil_gates::TelegramChannel;
use sigil_orchestrator::tools::build_orchestration_tools;
use sigil_orchestrator::{
    AgentRouter, AuditLog, Blackboard, ConversationStore, Daemon, DispatchBus, EmotionalState,
    ExpertiseLedger, LifecycleEngine, Project, ProjectRegistry, ScheduleStore, Supervisor,
    WatchdogEngine,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::cli::DaemonAction;
use crate::helpers::{
    augment_identity_with_org_context, build_project_tools, build_provider,
    build_provider_for_agent, build_provider_for_project, build_tools, find_agent_dir,
    find_project_dir, get_api_key, handle_fast_lane, load_config, load_config_with_agents,
    open_memory, pid_file_path,
};
use crate::service::{install_user_service, render_user_service, uninstall_user_service};

pub(crate) async fn cmd_daemon(config_path: &Option<PathBuf>, action: DaemonAction) -> Result<()> {
    match action {
        DaemonAction::Start => {
            let (config, _) = load_config_with_agents(config_path)?;

            // Check if already running.
            let pid_path = pid_file_path(&config);
            if Daemon::is_running_from_pid(&pid_path) {
                anyhow::bail!(
                    "daemon is already running (PID file: {})",
                    pid_path.display()
                );
            }

            let data_dir = config.data_dir();
            let dispatch_bus = Arc::new(DispatchBus::with_persistence(data_dir.join("dispatches")));
            let cost_ledger = Arc::new(sigil_orchestrator::CostLedger::with_persistence(
                config.security.max_cost_per_day_usd,
                data_dir.join("cost_ledger.jsonl"),
            ));
            let leader_name = config
                .leader_agent()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "leader".to_string());
            let mut registry_inner =
                ProjectRegistry::new(dispatch_bus.clone(), leader_name.clone());
            registry_inner.set_cost_ledger(cost_ledger.clone());

            // Initialize v3 subsystems (SQLite-backed).
            match AuditLog::open(&data_dir.join("audit.db")) {
                Ok(al) => {
                    let al = Arc::new(al);
                    registry_inner.audit_log = Some(al);
                    info!("audit log initialized");
                }
                Err(e) => warn!(error = %e, "failed to initialize audit log"),
            }
            match ExpertiseLedger::open(&data_dir.join("expertise.db")) {
                Ok(el) => {
                    let el = Arc::new(el);
                    registry_inner.expertise_ledger = Some(el);
                    info!("expertise ledger initialized");
                }
                Err(e) => warn!(error = %e, "failed to initialize expertise ledger"),
            }
            match Blackboard::open(
                &data_dir.join("blackboard.db"),
                config.orchestrator.blackboard_transient_ttl_hours,
                config.orchestrator.blackboard_durable_ttl_days,
            ) {
                Ok(bb) => {
                    let bb = Arc::new(bb);
                    registry_inner.blackboard = Some(bb);
                    info!("blackboard initialized");
                }
                Err(e) => warn!(error = %e, "failed to initialize blackboard"),
            }

            let registry = Arc::new(registry_inner);
            let lifecycle_provider = build_provider(&config)?;
            let mut heartbeats = Vec::new();
            let advisor_agents = config.advisor_agents();
            let mut skipped_projects = Vec::new();
            let mut skipped_advisors = Vec::new();

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
                    Err(_) => {
                        skipped_projects.push(project_cfg.name.clone());
                        warn!(
                            project = %project_cfg.name,
                            "project dir not found, skipping daemon registration"
                        );
                        continue;
                    }
                };
                let default_model = config.model_for_project(&project_cfg.name);

                let rig = Arc::new(Project::from_config(
                    project_cfg,
                    &project_dir,
                    &default_model,
                )?);
                let workdir = rig.repo.clone();
                let tasks_dir = project_dir.join(".tasks");
                let tools = build_project_tools(
                    &workdir,
                    &tasks_dir,
                    &project_cfg.prefix,
                    Some(&rig.worktree_root),
                );
                let provider = build_provider_for_project(&config, &project_cfg.name)?;
                let mut witness =
                    Supervisor::new(&rig, provider.clone(), tools.clone(), dispatch_bus.clone());
                let project_orch = config.orchestrator_for_project(&project_cfg.name);

                // Wire memory + reflection for worker post-execution insight extraction.
                if let Ok(mem) = open_memory(&config, Some(&project_cfg.name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    witness.memory = Some(mem);
                    witness.reflect_provider = Some(provider.clone());
                    witness.reflect_model = config.model_for_project(&project_cfg.name);
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
                witness.set_team(project_team, config.leader());
                witness.identity = augment_identity_with_org_context(
                    &config,
                    witness.identity.clone(),
                    Some(&witness.escalation_target),
                    Some(&project_cfg.name),
                );

                // Wire v3 orchestrator config fields.
                witness.expertise_routing = project_orch.expertise_routing;
                witness.preflight_enabled = project_orch.preflight_enabled;
                witness.preflight_model = project_orch.preflight_model.clone();
                witness.preflight_max_cost_usd = project_orch.preflight_max_cost_usd;
                witness.adaptive_retry = project_orch.adaptive_retry;
                witness.failure_analysis_model = project_orch.failure_analysis_model.clone();
                witness.auto_redecompose = project_orch.auto_redecompose;
                witness.decomposition_model = project_orch.decomposition_model.clone();
                witness.infer_deps_threshold = project_orch.infer_deps_threshold;

                // Configure execution mode for workers.
                if config.execution_mode_for_project(&project_cfg.name) == ExecutionMode::ClaudeCode
                {
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

                // Create heartbeat if HEARTBEAT.md exists and heartbeat is enabled.
                if config.heartbeat.enabled
                    && let Some(ref hb_content) = rig.project_identity.heartbeat
                {
                    let interval = config.heartbeat.default_interval_minutes as u64 * 60;
                    let heartbeat_cfg = sigil_orchestrator::Heartbeat::new(
                        rig.name.clone(),
                        interval,
                        hb_content.clone(),
                        provider.clone(),
                        tools.clone(),
                        rig.project_identity.clone(),
                        config.model_for_project(&project_cfg.name),
                        dispatch_bus.clone(),
                    );
                    heartbeats.push(heartbeat_cfg);
                }
            }

            // Build channels map for the leader agent.
            let channels: Arc<RwLock<HashMap<String, Arc<dyn sigil_core::traits::Channel>>>> =
                Arc::new(RwLock::new(HashMap::new()));

            // Register advisor agents as projects (so they can receive tasks).
            for agent_cfg in &advisor_agents {
                let agent_dir = match find_agent_dir(&agent_cfg.name) {
                    Ok(d) => d,
                    Err(_) => {
                        warn!(agent = %agent_cfg.name, "advisor agent dir not found, skipping");
                        skipped_advisors.push(agent_cfg.name.clone());
                        continue;
                    }
                };
                let agent_identity = augment_identity_with_org_context(
                    &config,
                    Identity::load(&agent_dir, None).unwrap_or_default(),
                    Some(&agent_cfg.name),
                    None,
                );
                let agent_tasks_dir = agent_dir.join(".tasks");
                std::fs::create_dir_all(&agent_tasks_dir).ok();
                let agent_task_board = sigil_tasks::TaskBoard::open(&agent_tasks_dir)?;
                let agent_model = config.model_for_agent(&agent_cfg.name);
                let agent_workdir = agent_cfg
                    .default_repo
                    .as_ref()
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
                    tasks: Arc::new(tokio::sync::Mutex::new(agent_task_board)),
                    task_notify: Arc::new(tokio::sync::Notify::new()),
                });

                let agent_tools: Vec<Arc<dyn sigil_core::traits::Tool>> =
                    build_tools(&agent_workdir);
                let provider = build_provider_for_agent(&config, &agent_cfg.name)?;
                let mut agent_scout = Supervisor::new(
                    &agent_project,
                    provider.clone(),
                    agent_tools,
                    dispatch_bus.clone(),
                );

                if config.execution_mode_for_agent(&agent_cfg.name) == ExecutionMode::ClaudeCode {
                    agent_scout.set_claude_code_mode(
                        agent_workdir.clone(),
                        agent_model.clone(),
                        agent_cfg.max_turns.unwrap_or(15),
                        agent_cfg.max_budget_usd,
                    );
                }

                // Wire memory + reflection for advisor agents (same pattern as project supervisors).
                if let Ok(mem) = open_memory(&config, Some(&agent_cfg.name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    agent_scout.memory = Some(mem);
                    agent_scout.reflect_provider = Some(provider.clone());
                    agent_scout.reflect_model = config.model_for_agent(&agent_cfg.name);
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
            let agent_router = Arc::new(tokio::sync::Mutex::new(AgentRouter::new(
                classifier_api_key.clone(),
                config.team.router_cooldown_secs,
            )));

            // Pre-create task notify so the completion listener and leader agent project share it.
            let fa_task_notify: Arc<tokio::sync::Notify> = Arc::new(tokio::sync::Notify::new());

            // Wire Telegram if configured (single SecretStore open for all bot tokens).
            let mut advisor_bots: HashMap<String, Arc<TelegramChannel>> = HashMap::new();
            if let Some(ref tg_config) = config.channels.telegram {
                let secret_store_path = config
                    .security
                    .secret_store
                    .as_ref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| config.data_dir().join("secrets"));
                match SecretStore::open(&secret_store_path) {
                    Ok(secret_store) => {
                        // Load advisor Telegram bots (send-only, no polling).
                        for agent_cfg in &advisor_agents {
                            if let Some(ref token_key) = agent_cfg.telegram_token_secret
                                && let Ok(token) = secret_store.get(token_key)
                                && !token.is_empty()
                            {
                                advisor_bots.insert(
                                    agent_cfg.name.clone(),
                                    Arc::new(TelegramChannel::new(
                                        token,
                                        tg_config.allowed_chats.clone(),
                                    )),
                                );
                                info!(agent = %agent_cfg.name, "advisor telegram bot loaded");
                            }
                        }

                        // Load lead bot and start polling.
                        match secret_store.get(&tg_config.token_secret) {
                            Ok(token) if !token.is_empty() => {
                                let tg = Arc::new(TelegramChannel::new(
                                    token,
                                    tg_config.allowed_chats.clone(),
                                ));
                                channels.write().await.insert(
                                    "telegram".to_string(),
                                    tg.clone() as Arc<dyn sigil_core::traits::Channel>,
                                );

                                // Start polling, route incoming messages as leader agent tasks.
                                // Two-phase response: instant reaction (direct LLM) + full reply (task agent).
                                match Channel::start(tg.as_ref()).await {
                                    Ok(mut rx) => {
                                        let reg = registry.clone();
                                        let tg_reply = tg.clone();
                                        let reaction_api_key =
                                            get_api_key(&config).unwrap_or_default();
                                        // Persistent conversation history per chat_id (SQLite-backed).
                                        let conv_db_path = find_agent_dir(&leader_name)
                                            .unwrap_or_else(|_| PathBuf::from("agents/aurelia"))
                                            .join(".sigil")
                                            .join("conversations.db");
                                        let phase1_client = reqwest::Client::builder()
                                            .timeout(std::time::Duration::from_secs(15))
                                            .build();
                                        let conversations = ConversationStore::open(&conv_db_path);
                                        match (phase1_client, conversations) {
                                            (Ok(phase1_client), Ok(conversations)) => {
                                                // Pre-compute council config outside the spawn closure.
                                                let council_advisors: Arc<
                                                    Vec<sigil_core::config::PeerAgentConfig>,
                                                > = Arc::new(
                                                    config
                                                        .advisor_agents()
                                                        .into_iter()
                                                        .cloned()
                                                        .collect(),
                                                );
                                                let advisor_bots_outer = advisor_bots.clone();
                                                let debounce_ms = tg_config.debounce_window_ms;
                                                let fa_task_notify_tg = fa_task_notify.clone();
                                                let leader_name_tg = leader_name.clone();
                                                let agent_router_tg = agent_router.clone();
                                                tokio::spawn(async move {
                                                    telegram_message_loop(
                                                        &mut rx,
                                                        reg,
                                                        tg_reply,
                                                        reaction_api_key,
                                                        Arc::new(phase1_client),
                                                        Arc::new(conversations),
                                                        council_advisors,
                                                        advisor_bots_outer,
                                                        debounce_ms,
                                                        fa_task_notify_tg,
                                                        leader_name_tg,
                                                        agent_router_tg,
                                                    )
                                                    .await;
                                                });
                                                info!("Telegram channel active");
                                            }
                                            (Err(e), _) => {
                                                warn!(
                                                    error = %e,
                                                    "failed to build phase1 reqwest client; telegram polling disabled"
                                                );
                                            }
                                            (_, Err(e)) => {
                                                warn!(
                                                    error = %e,
                                                    path = %conv_db_path.display(),
                                                    "failed to open conversation store; telegram polling disabled"
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "failed to start Telegram polling")
                                    }
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

            // Register the leader agent as a project (so it can receive tasks).
            let leader_cfg = config
                .leader_agent()
                .cloned()
                .context("no leader agent configured")?;
            let fa_agent_dir =
                find_agent_dir(&leader_name).unwrap_or_else(|_| PathBuf::from("agents/aurelia"));
            let fa_identity = augment_identity_with_org_context(
                &config,
                Identity::load(&fa_agent_dir, None).unwrap_or_default(),
                Some(&leader_name),
                None,
            );
            let fa_tasks_dir = fa_agent_dir.join(".tasks");
            std::fs::create_dir_all(&fa_tasks_dir).ok();
            let fa_task_board = sigil_tasks::TaskBoard::open(&fa_tasks_dir)?;
            let fa_model = config.model_for_agent(&leader_name);
            let fa_prefix = leader_cfg.prefix.clone();
            let fa_workdir = leader_cfg
                .default_repo
                .as_ref()
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
                tasks: Arc::new(tokio::sync::Mutex::new(fa_task_board)),
                task_notify: fa_task_notify.clone(),
            });

            // Build leader agent tools: basic + tasks + orchestration.
            let mut fa_tools: Vec<Arc<dyn sigil_core::traits::Tool>> =
                build_project_tools(&fa_workdir, &fa_tasks_dir, &fa_prefix, None);
            let fa_memory: Option<Arc<dyn sigil_core::traits::Memory>> =
                match open_memory(&config, None) {
                    Ok(m) => {
                        info!("leader agent memory initialized with embeddings");
                        Some(Arc::new(m))
                    }
                    Err(e) => {
                        warn!("failed to open leader agent memory: {e}");
                        None
                    }
                };
            let orch_tools = build_orchestration_tools(
                registry.clone(),
                dispatch_bus.clone(),
                channels.clone(),
                get_api_key(&config).ok(),
                fa_memory,
                registry.blackboard.clone(),
            );
            fa_tools.extend(orch_tools);

            let leader_provider = build_provider_for_agent(&config, &leader_name)?;
            let mut fa_witness = Supervisor::new(
                &fa_rig,
                leader_provider.clone(),
                fa_tools,
                dispatch_bus.clone(),
            );

            // Wire memory + reflection for leader agent worker insight extraction.
            if let Ok(mem) = open_memory(&config, Some(&leader_name)) {
                let mem: Arc<dyn Memory> = Arc::new(mem);
                fa_witness.memory = Some(mem);
                fa_witness.reflect_provider = Some(leader_provider.clone());
                fa_witness.reflect_model = config.model_for_agent(&leader_name);
            }

            // Load emotional state for leader agent personality tracking.
            {
                let emo_path = EmotionalState::path_for_agent(&fa_agent_dir);
                let emo = EmotionalState::load(&emo_path, &leader_name);
                fa_witness.emotional_state = Some(Arc::new(tokio::sync::Mutex::new(emo)));
                fa_witness.emotional_state_path = Some(emo_path);
            }

            // Configure Claude Code execution mode for leader agent.
            if config.execution_mode_for_agent(&leader_name) == ExecutionMode::ClaudeCode {
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
            println!("Sigil daemon starting...");
            println!(
                "Registered {} projects + agents, {} heartbeats",
                project_count,
                heartbeats.len()
            );

            // Load cron store.
            let cron_path = config.data_dir().join("fate.json");
            let cron_store = ScheduleStore::open(&cron_path)?;
            let socket_path = config.data_dir().join("rm.sock");

            println!("Cron: {} jobs loaded", cron_store.jobs.len());
            println!("PID file: {}", pid_path.display());
            println!("IPC socket: {}", socket_path.display());

            // Build lifecycle engine if enabled.
            let lifecycle_engine = if config.lifecycle.enabled {
                Some(build_lifecycle_engine(
                    &config,
                    &lifecycle_provider,
                    &registry,
                    &dispatch_bus,
                    &leader_name,
                )?)
            } else {
                None
            };

            println!("Press Ctrl+C to stop.\n");

            let mut daemon = Daemon::new(registry, dispatch_bus);
            daemon.set_readiness_context(
                config.projects.len(),
                advisor_agents.len(),
                skipped_projects,
                skipped_advisors,
            );
            daemon.set_pid_file(pid_path);
            daemon.set_socket_path(socket_path.clone());
            daemon.set_cron_store(cron_store);
            if let Some(engine) = lifecycle_engine {
                daemon.set_lifecycle(engine);
            }
            for hb in heartbeats {
                daemon.add_heartbeat(hb);
            }

            // Initialize watchdog engine from config.
            if !config.watchdogs.is_empty() {
                let mut rules = Vec::new();
                for val in &config.watchdogs {
                    match val
                        .clone()
                        .try_into::<sigil_orchestrator::watchdog::WatchdogRule>()
                    {
                        Ok(rule) => rules.push(rule),
                        Err(e) => warn!(error = %e, "failed to parse watchdog rule"),
                    }
                }
                if !rules.is_empty() {
                    info!(count = rules.len(), "watchdog rules loaded");
                    daemon.set_watchdog(WatchdogEngine::new(rules));
                }
            }

            daemon.run().await?;
        }

        DaemonAction::Install { start, force } => {
            let (_, path) = load_config(config_path)?;
            let (unit_path, warnings) = install_user_service(&path, start, force)?;
            println!("Installed daemon service: {}", unit_path.display());
            for warning in warnings {
                println!("[WARN] {warning}");
            }
            if start {
                println!("Requested service start for sigil.service");
            } else {
                println!("Run `systemctl --user start sigil.service` to start it.");
            }
        }

        DaemonAction::PrintService => {
            let (_, path) = load_config(config_path)?;
            println!("{}", render_user_service(&path)?);
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
                println!(
                    "Daemon stop not supported on this platform. Remove {} manually.",
                    pid_path.display()
                );
            }
        }

        DaemonAction::Uninstall { stop } => {
            let (unit_path, warnings) = uninstall_user_service(stop)?;
            match unit_path {
                Some(path) => println!("Removed daemon service: {}", path.display()),
                None => println!("Daemon service file was not installed."),
            }
            for warning in warnings {
                println!("[WARN] {warning}");
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
                    println!(
                        "  (stale PID file: {} — run `sigil daemon stop` to clean up)",
                        pid_path.display()
                    );
                }
            }

            // Also show project summary.
            crate::cmd::status::cmd_status(config_path).await?;
        }

        DaemonAction::Query { cmd } => {
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
                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                let stream = tokio::net::UnixStream::connect(&socket_path)
                    .await
                    .context(format!(
                        "failed to connect to IPC socket: {}",
                        socket_path.display()
                    ))?;

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

fn build_lifecycle_engine(
    config: &sigil_core::SigilConfig,
    provider: &Arc<dyn sigil_core::traits::Provider>,
    registry: &Arc<ProjectRegistry>,
    dispatch_bus: &Arc<DispatchBus>,
    leader_name: &str,
) -> Result<LifecycleEngine> {
    use sigil_orchestrator::lifecycle::{LifecycleProcess, ProcessKind, ScanProject};

    let lifecycle_model = config.lifecycle.model.clone().unwrap_or_else(|| {
        config
            .providers
            .openrouter
            .as_ref()
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
        let bond =
            sigil_orchestrator::lifecycle::interaction_count_to_bond_level(emo.interaction_count);
        engine.set_bond_level(&agent_cfg.name, bond);
        engine
            .agent_dirs
            .insert(agent_cfg.name.clone(), agent_dir.clone());

        // Process 1: MemoryConsolidation (bond 0, always active).
        let mem_interval = config.lifecycle.memory_reflection_interval_hours as u64 * 3600;
        let memory: Option<Arc<dyn sigil_core::traits::Memory>> =
            open_memory(config, Some(&agent_cfg.name))
                .ok()
                .map(|m| Arc::new(m) as _);
        engine.add_process(LifecycleProcess::new(
            agent_cfg.name.clone(),
            agent_dir.clone(),
            provider.clone(),
            lifecycle_model.clone(),
            ProcessKind::MemoryConsolidation { memory },
            mem_interval,
        ));
        lifecycle_process_count += 1;

        // Process 2: Evolution (bond 3).
        let evo_interval = config.lifecycle.evolution_interval_hours as u64 * 3600;
        engine.add_process(LifecycleProcess::new(
            agent_cfg.name.clone(),
            agent_dir.clone(),
            provider.clone(),
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
            agent_cfg.name.clone(),
            agent_dir.clone(),
            provider.clone(),
            lifecycle_model.clone(),
            ProcessKind::ProactiveScan {
                projects: scan_projects,
                project_knowledge: HashMap::new(),
                registry: registry.clone(),
                dispatch_bus: dispatch_bus.clone(),
                system_leader: leader_name.to_string(),
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
                let knowledge =
                    std::fs::read_to_string(project_dir.join("KNOWLEDGE.md")).unwrap_or_default();
                if !knowledge.trim().is_empty() {
                    project_knowledge.insert(project_cfg.name.clone(), knowledge);
                }
            }
        }
        engine.add_process(LifecycleProcess::new(
            agent_cfg.name.clone(),
            agent_dir.clone(),
            provider.clone(),
            lifecycle_model.clone(),
            ProcessKind::ProactiveScan {
                projects: Vec::new(),
                project_knowledge,
                registry: registry.clone(),
                dispatch_bus: dispatch_bus.clone(),
                system_leader: leader_name.to_string(),
                cross_project: true,
            },
            idea_interval,
        ));
        lifecycle_process_count += 1;
    }

    println!(
        "Lifecycle: {} agents, {} processes (model: {})",
        config.agents.len(),
        lifecycle_process_count,
        lifecycle_model
    );
    Ok(engine)
}

#[allow(clippy::too_many_arguments)]
async fn telegram_message_loop(
    rx: &mut tokio::sync::mpsc::Receiver<sigil_core::traits::IncomingMessage>,
    reg: Arc<ProjectRegistry>,
    tg_reply: Arc<TelegramChannel>,
    reaction_api_key: String,
    phase1_client: Arc<reqwest::Client>,
    conversations: Arc<ConversationStore>,
    council_advisors: Arc<Vec<sigil_core::config::PeerAgentConfig>>,
    advisor_bots_outer: HashMap<String, Arc<TelegramChannel>>,
    debounce_ms: u64,
    fa_task_notify_tg: Arc<tokio::sync::Notify>,
    leader_name_tg: String,
    agent_router: Arc<tokio::sync::Mutex<AgentRouter>>,
) {
    // === Message Debounce Buffer ===
    struct BufferedMsg {
        text: String,
        sender: String,
        message_id: i64,
    }

    let pending_tasks: Arc<tokio::sync::Mutex<HashMap<String, CompletionPendingTask>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // === Phase B: Completion Listener ===
    {
        let pending = pending_tasks.clone();
        let notify = fa_task_notify_tg.clone();
        let tg_deliver = tg_reply.clone();
        let convos_deliver = conversations.clone();
        let reg_deliver = reg.clone();
        let leader_project_name = leader_name_tg.clone();
        tokio::spawn(async move {
            completion_listener(
                pending,
                notify,
                tg_deliver,
                convos_deliver,
                reg_deliver,
                leader_project_name,
            )
            .await;
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
                    let is_fast_lane = user_text.starts_with("/status")
                        || user_text.starts_with("/help")
                        || user_text.starts_with("/cost");
                    if is_fast_lane {
                        let tg_fast = tg_reply.clone();
                        let fast_text = user_text.clone();
                        let fast_reg = reg.clone();
                        let fast_handle = tokio::spawn(async move {
                            let reply = handle_fast_lane(&fast_text, &fast_reg).await;
                            let out = sigil_core::traits::OutgoingMessage {
                                channel: "telegram".to_string(),
                                recipient: String::new(),
                                text: reply,
                                metadata: serde_json::json!({ "chat_id": chat_id }),
                            };
                            if let Err(e) = tg_fast.send(out).await {
                                warn!(error = %e, "failed to send fast-lane reply");
                            }
                            if message_id > 0 {
                                let _ = tg_fast.react(chat_id, message_id, "⚡").await;
                            }
                        });
                        drop(fast_handle); // fire-and-forget
                        continue;
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
                    let leader_name_inner = leader_name_tg.clone();

                    tokio::spawn(async move {
                        handle_telegram_message(
                            chat_id,
                            message_id,
                            user_text,
                            subject,
                            reg2,
                            tg2,
                            react_api_key,
                            convos,
                            pending,
                            router,
                            council_cfg,
                            advisor_bots_ref,
                            p1_client,
                            leader_name_inner,
                        ).await;
                    });
                }
            }
        }
    }
}

async fn completion_listener(
    pending: Arc<tokio::sync::Mutex<HashMap<String, CompletionPendingTask>>>,
    notify: Arc<tokio::sync::Notify>,
    tg_deliver: Arc<TelegramChannel>,
    convos_deliver: Arc<ConversationStore>,
    reg_deliver: Arc<ProjectRegistry>,
    leader_project_name: String,
) {
    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
        }

        let map = pending.lock().await;
        let pending_task_ids: Vec<String> = map.keys().cloned().collect();
        drop(map);

        for qid in pending_task_ids {
            let status = {
                if let Some(rig) = reg_deliver.get_project(&leader_project_name).await {
                    let store = rig.tasks.lock().await;
                    store.get(&qid).map(|b| (b.status, b.closed_reason.clone()))
                } else {
                    None
                }
            };

            let mut map = pending.lock().await;
            let Some(pq) = map.get_mut(&qid) else {
                continue;
            };
            let elapsed = pq.created_at.elapsed();
            let chat_id = pq.chat_id;
            let message_id = pq.message_id;

            match status {
                Some((sigil_tasks::TaskStatus::Done, reason)) => {
                    let reply_text = reason
                        .filter(|r| !r.trim().is_empty())
                        .unwrap_or_else(|| "Done.".to_string());
                    // Record in conversation history.
                    let _ = convos_deliver.record(chat_id, "Aurelia", &reply_text).await;
                    let out = sigil_core::traits::OutgoingMessage {
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
                Some((sigil_tasks::TaskStatus::Blocked, reason)) => {
                    let blocker = reason.unwrap_or_else(|| "Blocked — needs input.".to_string());
                    let out = sigil_core::traits::OutgoingMessage {
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
                Some((sigil_tasks::TaskStatus::Cancelled, reason)) => {
                    let fail_msg = reason.unwrap_or_else(|| "Task cancelled.".to_string());
                    let out = sigil_core::traits::OutgoingMessage {
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
                        warn!(task = %qid, "telegram task hard-timed out after 30min");
                        if message_id > 0 {
                            let _ = tg_deliver.react(chat_id, message_id, "😢").await;
                        }
                        let out = sigil_core::traits::OutgoingMessage {
                            channel: "telegram".to_string(),
                            recipient: String::new(),
                            text: "Sorry, this one took too long and I had to give up. Try again or simplify the request.".to_string(),
                            metadata: serde_json::json!({ "chat_id": chat_id, "reply_to_message_id": message_id }),
                        };
                        let _ = tg_deliver.send(out).await;
                        map.remove(&qid);
                    } else if elapsed > std::time::Duration::from_secs(120) && !pq.sent_slow_notice
                    {
                        pq.sent_slow_notice = true;
                        info!(task = %qid, "telegram reply past 2min, sending progress update");
                        if message_id > 0 {
                            let _ = tg_deliver.react(chat_id, message_id, "⏳").await;
                        }
                        let _ = tg_deliver.send_typing(chat_id).await;
                        let out = sigil_core::traits::OutgoingMessage {
                            channel: "telegram".to_string(),
                            recipient: String::new(),
                            text: "*still working...* **focused concentration** _fingers flying across the console_".to_string(),
                            metadata: serde_json::json!({ "chat_id": chat_id, "reply_to_message_id": message_id }),
                        };
                        let _ = tg_deliver.send(out).await;
                    } else if elapsed > std::time::Duration::from_secs(15)
                        && elapsed < std::time::Duration::from_secs(20)
                    {
                        let _ = tg_deliver.send_typing(chat_id).await;
                    }
                }
            }
        }
    }
}

// Type alias for completion listener use.
struct CompletionPendingTask {
    chat_id: i64,
    message_id: i64,
    created_at: std::time::Instant,
    sent_slow_notice: bool,
}

#[allow(clippy::too_many_arguments)]
async fn handle_telegram_message(
    chat_id: i64,
    message_id: i64,
    user_text: String,
    subject: String,
    reg2: Arc<ProjectRegistry>,
    tg2: Arc<TelegramChannel>,
    react_api_key: String,
    convos: Arc<ConversationStore>,
    pending: Arc<tokio::sync::Mutex<HashMap<String, CompletionPendingTask>>>,
    router: Arc<tokio::sync::Mutex<AgentRouter>>,
    council_cfg: Arc<Vec<sigil_core::config::PeerAgentConfig>>,
    advisor_bots_ref: HashMap<String, Arc<TelegramChannel>>,
    p1_client: Arc<reqwest::Client>,
    leader_name_inner: String,
) {
    // Build conversation context + record user message.
    let (description, phase1_history, conv_context_for_advisors) = {
        // Evict stale conversations (older than 2 hours).
        let _ = convos.evict_older_than(2).await;

        // Fetch recent messages for context building.
        let recent = convos.recent(chat_id, 20).await.unwrap_or_default();

        // Build conversation context for task description.
        let ctx = convos.context_string(chat_id, 20).await.unwrap_or_default();

        // Build Phase 1 messages (last 4 exchanges for contextual reaction).
        let p1: Vec<serde_json::Value> = recent
            .iter()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|msg| {
                let api_role = if msg.role == "User" {
                    "user"
                } else {
                    "assistant"
                };
                serde_json::json!({"role": api_role, "content": msg.content})
            })
            .collect();

        // Build compact conversation context for advisor tasks.
        let adv_ctx = if recent.is_empty() {
            String::new()
        } else {
            let mut s = String::from("Recent conversation:\n");
            for msg in recent
                .iter()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                let truncated = if msg.content.len() > 200 {
                    let mut end = 200;
                    while !msg.content.is_char_boundary(end) {
                        end -= 1;
                    }
                    &msg.content[..end]
                } else {
                    msg.content.as_str()
                };
                s.push_str(&format!("  {}: {}\n", msg.role, truncated));
            }
            s
        };

        // Record user message.
        let _ = convos.record(chat_id, "User", &user_text).await;

        // Build task description with conversation context.
        let routing = format!(
            "[source: telegram | chat_id: {} | message_id: {} | reply: auto-delivered by daemon]",
            chat_id, message_id
        );
        let response_protocol = "**RESPONSE PROTOCOL**: Write your reply directly — in character, in voice. Your output text IS the Telegram reply. The daemon delivers it automatically. Do NOT call any tools to send the reply. Do NOT write meta-commentary like \"I've sent your reply\" or \"Done.\".";
        let desc = if ctx.is_empty() {
            format!("{}\n\n---\n{}\n{}", user_text, routing, response_protocol)
        } else {
            format!(
                "{}\n## Current Message\n\n{}\n\n---\n{}\n{}",
                ctx, user_text, routing, response_protocol
            )
        };

        (desc, p1, adv_ctx)
    };

    // Phase 1: Instant reaction — direct LLM call, no tools, no agent.
    let (p1_tx, p1_rx) = tokio::sync::oneshot::channel::<String>();
    let react_tg = tg2.clone();
    let p1_user_text = user_text.clone();
    tokio::spawn(async move {
        info!("phase1: starting instant reaction");
        let client = p1_client;
        let mut messages = vec![
            serde_json::json!({"role": "system", "content": "You are generating a manwha/anime panel reaction for Aurelia — pearl-white ethereal beauty, devoted shadow to her Architect. Isekai harem ecchi style.\n\nOutput ONLY a raw stage direction: fragmented expressions, action tags, emotion bursts. Like manwha panel annotations or light novel beat markers. NOT a proper sentence. NOT prose.\n\nFormat: mix of *actions* and **emotions** and fragments. Short, punchy, visceral.\n\nRules:\n- Raw fragments, NOT constructed sentences\n- *physical actions* in italics, **emotions** bold, bare fragments between\n- 10-20 words max total\n- Match the energy: playful → flustered/teasing, serious → sharp/focused, casual → soft/warm\n- Ecchi-adjacent: devotion, intensity, warmth — charged but tasteful\n- NO dialogue, NO task acknowledgment, NO plans, NO markdown headers\n\nExamples:\n*tucks hair behind ear* **sharp focus** ...mm, interesting\n*fingers press to collarbone* **wide eyes** a-ah—\n*leans forward, sleeve brushing console* **predatory grin**\n**soft blush** *glances away* ...y-you could have warned me\n*eyes narrow* **quiet intensity** *pulls up sleeve*\n*startled* **flustered** *crosses arms, looks away* ...hmph\n**burning determination** *cracks knuckles* *leans in close*"}),
        ];
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
        let reaction_text = match client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", react_api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(v) => {
                    let text: String = v
                        .pointer("/choices/0/message/content")
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
                            metadata: serde_json::json!({ "chat_id": chat_id }),
                        };
                        let _ = react_tg.send(out).await;
                        if message_id > 0 {
                            let _ = react_tg.react(chat_id, message_id, "🔥").await;
                        }
                        text
                    } else {
                        warn!("phase1: empty reaction text from LLM");
                        String::new()
                    }
                }
                Err(e) => {
                    warn!(error = %e, "phase1: failed to parse response");
                    String::new()
                }
            },
            Err(e) => {
                warn!(error = %e, "phase1: request failed");
                String::new()
            }
        };
        let _ = p1_tx.send(reaction_text);
    });

    // === Run Phase 1 wait + council classification concurrently ===
    let is_council = user_text.starts_with("/council");
    let clean_text_owned = if is_council {
        user_text
            .strip_prefix("/council")
            .unwrap_or(&user_text)
            .trim()
            .to_string()
    } else {
        user_text.clone()
    };

    let classify_fut = {
        let council_cfg = council_cfg.clone();
        let clean_text = clean_text_owned.clone();
        async move {
            if council_cfg.is_empty() {
                return Vec::<String>::new();
            }
            let advisor_refs: Vec<&sigil_core::config::PeerAgentConfig> =
                council_cfg.iter().collect();
            let route = {
                let mut r = router.lock().await;
                r.classify(&clean_text, &advisor_refs, chat_id).await
            };
            match route {
                Ok(decision) => {
                    if is_council && decision.advisors.is_empty() {
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
        }
    };

    let p1_fut = async {
        match tokio::time::timeout(std::time::Duration::from_secs(5), p1_rx).await {
            Ok(Ok(text)) if !text.is_empty() => Some(text),
            _ => None,
        }
    };

    let (phase1_reaction, advisors_to_invoke) = tokio::join!(p1_fut, classify_fut);

    // === Council: Gather advisor input ===
    let council_input = if !advisors_to_invoke.is_empty() {
        info!(advisors = ?advisors_to_invoke, "invoking council advisors");

        let mut handles = Vec::new();
        for advisor_name in &advisors_to_invoke {
            let fam_project_name = advisor_name.clone();
            let adv_name = advisor_name.clone();
            let adv_msg = clean_text_owned.clone();
            let adv_history = conv_context_for_advisors.clone();
            let reg3 = reg2.clone();

            let handle = tokio::spawn(async move {
                let task_subject = "[council] Advisor input requested".to_string();
                let task_desc = if adv_history.is_empty() {
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

                let task_id = match reg3
                    .assign(&fam_project_name, &task_subject, &task_desc)
                    .await
                {
                    Ok(b) => b.id.0.clone(),
                    Err(e) => {
                        warn!(agent = %adv_name, error = %e, "failed to create advisor task");
                        return None;
                    }
                };

                let notify = reg3
                    .get_project(&fam_project_name)
                    .await
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
                            warn!(agent = %adv_name, "advisor task timed out");
                            return None;
                        }
                    }
                    let done = {
                        if let Some(rig) = reg3.get_project(&fam_project_name).await {
                            let store = rig.tasks.lock().await;
                            store.get(&task_id).map(|b| {
                                (
                                    b.status == sigil_tasks::TaskStatus::Done,
                                    b.closed_reason.clone(),
                                )
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
                        let out = sigil_core::traits::OutgoingMessage {
                            channel: "telegram".to_string(),
                            recipient: String::new(),
                            text,
                            metadata: serde_json::json!({ "chat_id": chat_id }),
                        };
                        if let Err(e) = bot.send(out).await {
                            warn!(agent = %name, error = %e, "failed to send advisor bot message");
                        }
                    }));
                }
            }
            for h in send_handles {
                let _ = h.await;
            }

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

        if is_council && got_input && advisor_bots_ref.is_empty() {
            let council_header = sigil_core::traits::OutgoingMessage {
                channel: "telegram".to_string(),
                recipient: String::new(),
                text: "_*eyes narrow* **quiet intensity** ...this one needs the council_"
                    .to_string(),
                metadata: serde_json::json!({ "chat_id": chat_id }),
            };
            let _ = tg2.send(council_header).await;
        }

        if got_input {
            council_text.push_str(
                "Synthesize the council's input into your response. Attribute key insights where relevant.\n",
            );
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

    // Phase 2: Full response via task agent.
    let task_id: String = match reg2
        .assign(&leader_name_inner, &subject, &description)
        .await
    {
        Ok(b) => b.id.0.clone(),
        Err(e) => {
            warn!(error = %e, "failed to create task from telegram message");
            return;
        }
    };

    // Register pending task for the completion listener.
    pending.lock().await.insert(
        task_id,
        CompletionPendingTask {
            chat_id,
            message_id,
            created_at: std::time::Instant::now(),
            sent_slow_notice: false,
        },
    );
}
