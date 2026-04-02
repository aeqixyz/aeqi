use anyhow::{Context, Result};
use sigil_core::config::TelegramChatRouteConfig;
use sigil_core::traits::{Channel, Memory};
use sigil_core::{Identity, SecretStore};
use sigil_gates::TelegramChannel;
use sigil_orchestrator::tools::build_orchestration_tools;
use sigil_orchestrator::{
    AgentRouter, AuditLog, Blackboard, ConversationStore, Daemon, DispatchBus, ExpertiseLedger,
    Project, ProjectRegistry, Supervisor,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::cli::DaemonAction;
use crate::helpers::{
    augment_identity_with_org_context, build_project_tools, build_provider_for_agent,
    build_provider_for_project, build_tools, daemon_ipc_request, find_agent_dir, find_project_dir,
    get_api_key, handle_fast_lane, load_config, load_config_with_agents, open_memory,
    pid_file_path,
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
            let event_broadcaster = Arc::new(sigil_orchestrator::EventBroadcaster::new());
            let mut dispatch_bus = DispatchBus::with_persistence(data_dir.join("dispatches"));
            dispatch_bus.set_event_broadcaster(event_broadcaster.clone());
            let dispatch_bus = Arc::new(dispatch_bus);
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
            registry_inner.config_project_names =
                config.projects.iter().map(|p| p.name.clone()).collect();
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
                config.orchestrator.blackboard_claim_ttl_hours,
            ) {
                Ok(bb) => {
                    let bb = Arc::new(bb);
                    registry_inner.blackboard = Some(bb);
                    info!("blackboard initialized");
                }
                Err(e) => warn!(error = %e, "failed to initialize blackboard"),
            }
            match ConversationStore::open(&data_dir.join("conversations.db")) {
                Ok(cs) => {
                    let cs = Arc::new(cs);
                    registry_inner.conversation_store = Some(cs);
                    info!("conversation store initialized");
                }
                Err(e) => warn!(error = %e, "failed to initialize conversation store"),
            }

            let registry = Arc::new(registry_inner);
            let background_automation_enabled = config.orchestrator.background_automation_enabled;
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
                witness.event_broadcaster = Some(event_broadcaster.clone());
                let project_orch = config.orchestrator_for_project(&project_cfg.name);

                // Wire memory + reflection for worker post-execution insight extraction.
                if let Ok(mem) = open_memory(&config, Some(&project_cfg.name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    witness.memory = Some(mem);
                    if background_automation_enabled {
                        witness.reflect_provider = Some(provider.clone());
                        witness.reflect_model = config.default_model_for_provider(
                            sigil_core::config::ProviderKind::OpenRouter,
                        );
                    }
                }

                // Wire escalation targets.
                witness.set_escalation_targets(config.leader(), config.leader());
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

                // Wire skill discovery directories (project-specific + shared).
                let project_skills_dir = project_dir.join("skills");
                let shared_skills_dir = project_dir
                    .parent()
                    .map(|p| p.join("shared").join("skills"))
                    .unwrap_or_default();
                witness.skills_dirs = vec![project_skills_dir, shared_skills_dir];

                witness.worker_max_budget_usd = project_cfg.max_budget_usd;

                registry.register_project(rig.clone(), witness).await;
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
                    departments: Vec::new(),
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
                agent_scout.event_broadcaster = Some(event_broadcaster.clone());

                agent_scout.worker_max_budget_usd = agent_cfg.max_budget_usd;

                // Wire memory + reflection for advisor agents (same pattern as project supervisors).
                if let Ok(mem) = open_memory(&config, Some(&agent_cfg.name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    agent_scout.memory = Some(mem);
                    if background_automation_enabled {
                        agent_scout.reflect_provider = Some(provider.clone());
                        agent_scout.reflect_model = config.default_model_for_provider(
                            sigil_core::config::ProviderKind::OpenRouter,
                        );
                    }
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

            // Build per-project memory stores for knowledge-aware chat.
            let mut chat_memory_stores: HashMap<String, Arc<dyn sigil_core::traits::Memory>> =
                HashMap::new();
            for project_cfg in &config.projects {
                if let Ok(mem) = open_memory(&config, Some(&project_cfg.name)) {
                    chat_memory_stores.insert(project_cfg.name.clone(), Arc::new(mem));
                }
            }
            info!(
                "chat memory stores initialized for {} projects",
                chat_memory_stores.len()
            );

            // Build the unified ChatEngine.
            let council_advisors: Arc<Vec<sigil_core::config::PeerAgentConfig>> =
                Arc::new(config.advisor_agents().into_iter().cloned().collect());
            let auto_council_enabled = config.team.max_background_cost_usd > 0.0;
            // Intent classifier (legacy — no longer used for routing).
            let intent_classifier: Option<Arc<sigil_orchestrator::intent::IntentClassifier>> = None;

            let chat_engine = registry.conversation_store.as_ref().map(|cs| {
                Arc::new(sigil_orchestrator::ChatEngine {
                    conversations: cs.clone(),
                    registry: registry.clone(),
                    agent_router: agent_router.clone(),
                    council_advisors: council_advisors.clone(),
                    auto_council_enabled,
                    leader_name: leader_name.clone(),
                    pending_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                    task_notify: fa_task_notify.clone(),
                    memory_stores: chat_memory_stores,
                    intent_classifier,
                })
            });

            // Shared queue for proactive Telegram messages (morning brief, completion notifications).
            let pending_telegram_messages: Arc<std::sync::Mutex<Vec<(i64, String)>>> =
                Arc::new(std::sync::Mutex::new(Vec::new()));

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

                                // Start polling and route incoming messages through the shared chat engine.
                                match Channel::start(tg.as_ref()).await {
                                    Ok(mut rx) => {
                                        let tg_reply = tg.clone();
                                        match chat_engine.clone() {
                                            Some(engine) => {
                                                let advisor_bots_outer = advisor_bots.clone();
                                                let debounce_ms = tg_config.debounce_window_ms;
                                                let ptm = pending_telegram_messages.clone();
                                                let eb = event_broadcaster.clone();
                                                let default_chat = tg_config
                                                    .main_chat_id
                                                    .or_else(|| {
                                                        tg_config.allowed_chats.first().copied()
                                                    })
                                                    .unwrap_or(0);
                                                let telegram_routes = Arc::new(
                                                    tg_config
                                                        .routes
                                                        .iter()
                                                        .cloned()
                                                        .map(|route| (route.chat_id, route))
                                                        .collect(),
                                                );
                                                tokio::spawn(async move {
                                                    telegram_message_loop(
                                                        &mut rx,
                                                        engine,
                                                        tg_reply,
                                                        advisor_bots_outer,
                                                        debounce_ms,
                                                        ptm,
                                                        eb,
                                                        default_chat,
                                                        telegram_routes,
                                                    )
                                                    .await;
                                                });
                                                info!("Telegram channel active");
                                            }
                                            None => {
                                                warn!(
                                                    "chat engine not initialized; telegram polling disabled"
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
            // Optional — daemon runs fine without a leader agent configured.
            if let Some(leader_cfg) = config.leader_agent().cloned() {
                let fa_agent_dir = find_agent_dir(&leader_name)
                    .unwrap_or_else(|_| PathBuf::from("agents/aurelia"));
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
                    departments: Vec::new(),
                });

                let mut fa_tools: Vec<Arc<dyn sigil_core::traits::Tool>> =
                    build_project_tools(&fa_workdir, &fa_tasks_dir, &fa_prefix, None);
                let fa_memory: Option<Arc<dyn sigil_core::traits::Memory>> =
                    match open_memory(&config, None) {
                        Ok(m) => {
                            info!("leader agent memory initialized");
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
                fa_witness.event_broadcaster = Some(event_broadcaster.clone());

                if let Ok(mem) = open_memory(&config, Some(&leader_name)) {
                    let mem: Arc<dyn Memory> = Arc::new(mem);
                    fa_witness.memory = Some(mem);
                    if background_automation_enabled {
                        fa_witness.reflect_provider = Some(leader_provider.clone());
                        fa_witness.reflect_model = config.default_model_for_provider(
                            sigil_core::config::ProviderKind::OpenRouter,
                        );
                    }
                }

                fa_witness.worker_max_budget_usd = leader_cfg.max_budget_usd;

                registry.register_project(fa_rig, fa_witness).await;
            } else {
                warn!("no leader agent configured — daemon will run without one");
            }

            let project_count = registry.project_count().await;
            println!("Sigil daemon starting...");
            println!("Registered {} projects + agents", project_count);

            let socket_path = config.data_dir().join("rm.sock");
            println!("PID file: {}", pid_path.display());
            println!("IPC socket: {}", socket_path.display());

            println!("Press Ctrl+C to stop.\n");

            let mut daemon = Daemon::new(registry, dispatch_bus);
            daemon.event_broadcaster = event_broadcaster;
            daemon.chat_engine = chat_engine;
            daemon.set_readiness_context(
                config.projects.len(),
                advisor_agents.len(),
                skipped_projects,
                skipped_advisors,
            );
            daemon.set_background_automation_enabled(background_automation_enabled);
            daemon.set_pid_file(pid_path);
            daemon.set_socket_path(socket_path.clone());
            // Initialize trigger store + agent registry for persistent agent triggers.
            match sigil_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir()) {
                Ok(agent_reg) => {
                    let trigger_store = Arc::new(agent_reg.trigger_store());
                    let trigger_count = trigger_store.count_enabled().await.unwrap_or(0);
                    println!("Triggers: {trigger_count} enabled");
                    let agent_reg = Arc::new(agent_reg);
                    daemon.set_trigger_store(trigger_store.clone());
                    daemon.set_agent_registry(agent_reg.clone());

                    // Wire agent_registry + trigger_store into all supervisors.
                    daemon
                        .registry
                        .wire_agent_system(
                            agent_reg,
                            trigger_store,
                            daemon
                                .chat_engine
                                .as_ref()
                                .map(|ce| ce.conversations.clone()),
                        )
                        .await;
                }
                Err(e) => {
                    warn!(error = %e, "failed to open agent registry for triggers");
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
            let response =
                daemon_ipc_request(config_path, &serde_json::json!({ "cmd": cmd })).await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn telegram_message_loop(
    rx: &mut tokio::sync::mpsc::Receiver<sigil_core::traits::IncomingMessage>,
    engine: Arc<sigil_orchestrator::ChatEngine>,
    tg_reply: Arc<TelegramChannel>,
    _advisor_bots: HashMap<String, Arc<TelegramChannel>>,
    debounce_ms: u64,
    pending_telegram_messages: Arc<std::sync::Mutex<Vec<(i64, String)>>>,
    event_broadcaster: Arc<sigil_orchestrator::EventBroadcaster>,
    default_chat_id: i64,
    telegram_routes: Arc<HashMap<i64, TelegramChatRouteConfig>>,
) {
    struct BufferedMsg {
        text: String,
        sender: String,
        message_id: i64,
    }

    // Completion listener: polls ChatEngine for completed tasks, delivers via Telegram.
    // Also drains proactive messages (morning brief, completion notifications) from the daemon.
    {
        let engine_cl = engine.clone();
        let tg_deliver = tg_reply.clone();
        let notify = engine.task_notify.clone();
        let ptm = pending_telegram_messages.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = notify.notified() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                }

                // Drain and deliver proactive messages from the daemon (morning brief, etc.).
                {
                    let messages: Vec<(i64, String)> = if let Ok(mut queue) = ptm.lock() {
                        queue.drain(..).collect()
                    } else {
                        Vec::new()
                    };
                    for (chat_id, text) in messages {
                        let out = sigil_core::traits::OutgoingMessage {
                            channel: "telegram".to_string(),
                            recipient: String::new(),
                            text,
                            metadata: serde_json::json!({ "chat_id": chat_id }),
                        };
                        if let Err(e) = tg_deliver.send(out).await {
                            warn!(error = %e, "failed to deliver proactive telegram message");
                        }
                    }
                }

                // Check for slow tasks (> 2min) and send progress.
                for (_qid, chat_id, message_id, _source) in engine_cl.get_slow_tasks().await {
                    if message_id > 0 {
                        let _ = tg_deliver.react(chat_id, message_id, "⏳").await;
                    }
                    let _ = tg_deliver.send_typing(chat_id).await;
                }

                // Check for completed tasks and deliver replies.
                for completion in engine_cl.check_completions().await {
                    let emoji = match completion.status {
                        sigil_orchestrator::chat_engine::CompletionStatus::Done => "👍",
                        sigil_orchestrator::chat_engine::CompletionStatus::Blocked => "❓",
                        sigil_orchestrator::chat_engine::CompletionStatus::Cancelled => "❌",
                        sigil_orchestrator::chat_engine::CompletionStatus::TimedOut => "😢",
                    };
                    let out = sigil_core::traits::OutgoingMessage {
                        channel: "telegram".to_string(),
                        recipient: String::new(),
                        text: completion.text,
                        metadata: serde_json::json!({ "chat_id": completion.chat_id }),
                    };
                    if let Err(e) = tg_deliver.send(out).await {
                        warn!(error = %e, "failed to deliver telegram reply");
                    }
                    if completion.message_id > 0 {
                        let _ = tg_deliver
                            .react(completion.chat_id, completion.message_id, emoji)
                            .await;
                    }
                }
            }
        });
    }

    // Proactive completion notifier: sends Telegram notifications for non-user-initiated tasks
    // (cron jobs, watchdog tasks, proactive engine tasks) when they complete.
    if default_chat_id != 0 {
        let tg_notify = tg_reply.clone();
        let engine_pending = engine.clone();
        let mut event_rx = event_broadcaster.subscribe();
        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(sigil_orchestrator::ExecutionEvent::TaskCompleted {
                        task_id,
                        outcome,
                        cost_usd,
                        ..
                    }) => {
                        // Only notify for tasks NOT originated from a user chat message.
                        let is_user_task = {
                            let pending = engine_pending.pending_tasks.lock().await;
                            pending.contains_key(&task_id)
                        };
                        if !is_user_task {
                            let summary = if outcome.len() > 80 {
                                format!("{}...", &outcome[..77])
                            } else {
                                outcome
                            };
                            let text = format!(
                                "\u{2713} Task {} completed: {} [${:.2}]",
                                task_id, summary, cost_usd
                            );
                            let out = sigil_core::traits::OutgoingMessage {
                                channel: "telegram".to_string(),
                                recipient: String::new(),
                                text,
                                metadata: serde_json::json!({ "chat_id": default_chat_id }),
                            };
                            if let Err(e) = tg_notify.send(out).await {
                                warn!(error = %e, "failed to send proactive completion notification");
                            }
                        }
                    }
                    Ok(_) => {} // Ignore other event types.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(missed = n, "proactive notifier lagged behind event stream");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
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
                    let route = resolve_telegram_route(&telegram_routes, chat_id);
                    let project_hint = route.as_ref().and_then(|route| route.project.clone());
                    let department_hint = route.as_ref().and_then(|route| route.department.clone());
                    let channel_name = route.as_ref().and_then(|route| route.name.clone());

                    // === Fast-Lane ===
                    if user_text.starts_with("/status")
                        || user_text.starts_with("/help")
                        || user_text.starts_with("/cost")
                    {
                        let tg_fast = tg_reply.clone();
                        let fast_engine = engine.clone();
                        let fast_text = user_text.clone();
                        let fast_sender = sender.clone();
                        let fast_reg = engine.registry.clone();
                        let fast_project = project_hint.clone();
                        let fast_department = department_hint.clone();
                        let fast_channel = channel_name.clone();
                        tokio::spawn(async move {
                            let reply = handle_fast_lane(&fast_text, &fast_reg).await;
                            let chat_msg = sigil_orchestrator::chat_engine::ChatMessage {
                                message: fast_text,
                                chat_id,
                                sender: fast_sender,
                                source: sigil_orchestrator::chat_engine::ChatSource::Telegram {
                                    message_id,
                                },
                                project_hint: fast_project,
                                department_hint: fast_department,
                                channel_name: fast_channel,
                                agent_id: None,
                            };
                            fast_engine.record_exchange(&chat_msg, &reply).await;
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
                        continue;
                    }

                    // === Quick intent check ===
                    let chat_msg = sigil_orchestrator::chat_engine::ChatMessage {
                        message: user_text.clone(),
                        chat_id,
                        sender: sender.clone(),
                        source: sigil_orchestrator::chat_engine::ChatSource::Telegram { message_id },
                        project_hint: project_hint.clone(),
                        department_hint: department_hint.clone(),
                        channel_name: channel_name.clone(),
                        agent_id: None,
                    };

                    if let Some(response) = engine.handle_message(&chat_msg).await {
                        // Intent matched (create task, close task, etc.) — send reply directly.
                        let tg_intent = tg_reply.clone();
                        tokio::spawn(async move {
                            let out = sigil_core::traits::OutgoingMessage {
                                channel: "telegram".to_string(),
                                recipient: String::new(),
                                text: response.context.clone(),
                                metadata: serde_json::json!({ "chat_id": chat_id }),
                            };
                            let _ = tg_intent.send(out).await;
                            if message_id > 0 {
                                let _ = tg_intent.react(chat_id, message_id, "✅").await;
                            }
                        });
                        continue;
                    }

                    // === Full pipeline: unified chat task ===
                    let engine2 = engine.clone();
                    let tg2 = tg_reply.clone();

                    tokio::spawn(async move {
                        let _ = tg2.send_typing(chat_id).await;
                        let chat_msg = sigil_orchestrator::chat_engine::ChatMessage {
                            message: user_text,
                            chat_id,
                            sender,
                            source: sigil_orchestrator::chat_engine::ChatSource::Telegram { message_id },
                            project_hint,
                            department_hint,
                            channel_name,
                            agent_id: None,
                        };

                        match engine2.handle_message_full(&chat_msg, None).await {
                            Ok(handle) => {
                                info!(task = %handle.task_id, "telegram message → task created");
                            }
                            Err(e) => {
                                warn!(error = %e, "failed to process telegram message");
                                let out = sigil_core::traits::OutgoingMessage {
                                    channel: "telegram".to_string(),
                                    recipient: String::new(),
                                    text: format!("Error: {}", e),
                                    metadata: serde_json::json!({ "chat_id": chat_id }),
                                };
                                let _ = tg2.send(out).await;
                            }
                        }
                    });
                }
            }
        }
    }
}

fn resolve_telegram_route(
    routes: &HashMap<i64, TelegramChatRouteConfig>,
    chat_id: i64,
) -> Option<TelegramChatRouteConfig> {
    let mut route = routes.get(&chat_id).cloned()?;
    if route.department.is_some() && route.project.is_none() {
        warn!(
            chat_id,
            "telegram route sets a department without a project; dropping the department scope"
        );
        route.department = None;
    }
    Some(route)
}
