# Sigil

Agent runtime, multi-agent orchestration engine, and web control plane in Rust.

## Crates

| Crate | Path | Purpose |
|-------|------|---------|
| `sigil` | `sigil-cli/` | CLI binary and command handlers |
| `sigil-core` | `crates/sigil-core/` | Config, traits, agent loop, identity, secrets |
| `sigil-orchestrator` | `crates/sigil-orchestrator/` | Daemon, Supervisor, AgentWorker, ChatEngine, AgentRegistry, TriggerStore, ConversationStore, DispatchBus, Audit, Expertise, Blackboard, Preflight, Decomposition, FailureAnalysis, Middleware Chain (8), Verification, Escalation |
| `sigil-web` | `crates/sigil-web/` | Axum REST API + WebSocket server (JWT auth, IPC proxy) |
| `sigil-tasks` | `crates/sigil-tasks/` | Task DAG (JSONL), missions, dependency inference |
| `sigil-memory` | `crates/sigil-memory/` | SQLite+FTS5, vector search, hybrid ranking, memory graph, query planning, debounced writes |
| `sigil-providers` | `crates/sigil-providers/` | OpenRouter, Anthropic, Ollama + cost estimation |
| `sigil-gates` | `crates/sigil-gates/` | Telegram, Discord, Slack channels |
| `sigil-tools` | `crates/sigil-tools/` | Shell, file, git, tasks, delegate, skills |

## Architecture

### Trigger + Skill = Everything

All agent automation flows through two primitives:

- **Trigger** (when): schedule (cron/interval), event (pattern match on ExecutionEvent), or once (one-shot). Owned by a persistent agent (FK in agents.db). Created via template frontmatter or the `manage_triggers` agent tool.
- **Skill** (what): TOML file with system prompt + tool allow/deny list. Loaded by the supervisor when a trigger fires.

Agent "subconscious" behaviors (evolution, memory consolidation, health checks, anomaly detection) are just triggers + skills in the `autonomous` phase. No special daemon subsystems.

### Persistent Agents

SQLite registry in `~/.sigil/agents.db`. Each agent has: UUID (entity memory scope), name, system_prompt, project/department scope, capabilities, model preference. Spawned from template files with YAML frontmatter (including trigger definitions).

### Two Loops

1. **Daemon patrol loop** (system-level, every 30s): manage the fleet — assign tasks, fire due triggers, housekeeping.
2. **Agent loop** (per-session): LLM → tool execution → LLM → repeat until done.

## Message Flow

```
User message (Web / Telegram)
    ↓
ChatEngine
    ├─ QUICK PATH: intent detection → immediate response
    └─ FULL PATH: create task → Supervisor assigns worker
        → Worker loads agent identity + skill + memory
        → Agent loop runs → Outcome parsed
        → ChatEngine delivers response

Trigger fires (schedule/event)
    → Daemon creates task with skill
    → Same worker path as above
```

## Daemon Patrol Loop (9 steps)

1. `registry.patrol_all()` — reap workers, assign pending tasks
2. `trigger_store.due_schedule_triggers()` — fire due triggers
3. Check SIGHUP — hot-reload config
4. Save dispatch bus + cost ledger
5. Retry unacked dispatches, detect dead letters
6. Update metrics gauges
7. Prune old cost entries (7+ days)
8. Prune expired blackboard entries
9. Flush debounced memory writes

Event triggers run in a separate `tokio::spawn` subscriber.

## Middleware Chain (8 implementations)

| Middleware | Purpose |
|-----------|---------|
| LoopDetection | MD5 hash sliding window, warn at 3, kill at 5 repeats |
| Guardrails | Block dangerous ops (rm -rf, force push, drop table) |
| CostTracking | Token/cost accumulation per task, budget ceiling |
| ContextCompression | Compress at 50% window, protect first/last messages |
| ContextBudget | Cap enrichment at ~200 lines |
| MemoryRefresh | Re-search memory every N tool calls |
| Clarification | Structured ask_clarification tool, halts execution |
| SafetyNet | On failure: preserve partial work artifacts |

## Quality Bar

```bash
cargo test --workspace    # 571 tests
cargo clippy --workspace --all-targets -- -D warnings
```

## Runtime

- `sigil daemon start` — orchestration plane (systemd: sigil-daemon.service)
- `sigil web start` — Axum REST API on :8400 (systemd: sigil-web.service)
- `sigil run` — one-shot agent execution
- IPC via Unix socket at `~/.sigil/rm.sock` (JSON-line protocol)

## IPC Commands

**Read:** ping, status, readiness, worker_progress, worker_events, projects, mail, dispatches, metrics, cost, audit, blackboard, expertise, tasks, missions, triggers, agent_identity, rate_limit, memories, skills, pipelines, project_knowledge, channel_knowledge
**Write:** create_task, close_task, post_blackboard, save_agent_file, knowledge_store, knowledge_delete
**Chat:** chat (quick), chat_full (agent execution), chat_poll (completion), chat_history, chat_channels

## Important Directories

- `config/sigil.toml` — master config
- `projects/{name}/` — project config, skills (.toml), .tasks/
- `projects/shared/skills/` — shared skills (autonomous + workflow + phase-specific)
- `~/.sigil/` — daemon state (agents.db, audit.db, blackboard.db, expertise.db, dispatches.db, memory.db, cost_ledger.jsonl, rm.sock)

## Lock Architecture (CRITICAL)

IPC handlers use `try_lock()` on task boards — return partial data rather than blocking when patrol holds locks. Never use `.lock().await` in IPC read paths.

## Config Structure

```toml
[sigil]           # System name, data_dir
[web]             # bind, cors_origins, auth_secret
[providers.*]     # OpenRouter, Anthropic, Ollama
[security]        # autonomy, budget limits
[memory]          # SQLite backend, embedding config
[team]            # System leader, router model
[orchestrator]    # Expertise routing, preflight, decomposition, retry
[[projects]]      # Each project: name, prefix, repo, team, departments, missions
```

## Extension Points

- New skill: add `.toml` to `projects/shared/skills/` or `projects/{name}/skills/`
- New trigger: add to agent template frontmatter or use `manage_triggers` tool
- New middleware: implement `Middleware` trait, add to chain in supervisor
- New tool: implement `Tool` trait, wire into builder
- New provider: implement `Provider` trait
- New channel: implement `Channel` trait, register in daemon startup
- New IPC command: add match arm in `daemon.rs` handle_socket_connection
