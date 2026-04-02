# Sigil

Agent runtime, multi-agent orchestration engine, and web control plane in Rust.

## Crates

| Crate | Path | Purpose |
|-------|------|---------|
| `sigil` | `sigil-cli/` | CLI binary and command handlers |
| `sigil-core` | `crates/sigil-core/` | Config, traits, agent loop, identity, secrets |
| `sigil-orchestrator` | `crates/sigil-orchestrator/` | Daemon, Supervisor, AgentWorker, ChatEngine, AgentRegistry (agents + departments), TriggerStore, ConversationStore, DispatchBus, UnifiedDelegateTool, Audit, Expertise, Blackboard, Preflight, Decomposition, FailureAnalysis, Middleware Chain (9), Verification, Escalation |
| `sigil-web` | `crates/sigil-web/` | Axum REST API + WebSocket server (JWT auth, IPC proxy) |
| `sigil-tasks` | `crates/sigil-tasks/` | Task DAG (JSONL), missions, dependency inference |
| `sigil-memory` | `crates/sigil-memory/` | SQLite+FTS5, vector search, hybrid ranking, memory graph, query planning, debounced writes |
| `sigil-providers` | `crates/sigil-providers/` | OpenRouter, Anthropic, Ollama + cost estimation |
| `sigil-gates` | `crates/sigil-gates/` | Telegram, Discord, Slack channels |
| `sigil-tools` | `crates/sigil-tools/` | Shell, file, git, tasks, delegate, skills |

## Architecture

### Four Primitives

Full redesign spec: `docs/orchestration-redesign.md`

1. **Agent** — persistent identity (UUID, system_prompt, department_id, triggers, entity memory)
2. **Department** — UUID-identified org unit (name, manager_id, parent_id hierarchy)
3. **Task** — always agent-bound (agent_id), never free-floating
4. **Delegation** — unified delegate tool for all inter-agent interaction

### Trigger + Skill = Everything

All agent automation flows through two primitives:

- **Trigger** (when): schedule (cron/interval), event (pattern match on ExecutionEvent), or once (one-shot). Owned by a persistent agent (FK in agents.db). Created via template frontmatter or the `manage_triggers` agent tool.
- **Skill** (what): TOML file with system prompt + tool allow/deny list. Loaded by the supervisor when a trigger fires.

Agent "subconscious" behaviors (evolution, memory consolidation, health checks, anomaly detection) are just triggers + skills in the `autonomous` phase. No special daemon subsystems.

### Persistent Agents + Departments

SQLite registry in `~/.sigil/agents.db`. Each agent has: UUID (entity memory scope), name, system_prompt, department_id, capabilities, model preference. Spawned from template files with YAML frontmatter.

Departments are UUID-identified org units with their own hierarchy (`departments` table). Agents belong to a department via `department_id`. Departments have a manager (agent) and parent department. Escalation follows the department parent chain. Blackboard visibility is scoped by department.

### Unified Delegate Tool

One tool (`delegate`) for all inter-agent interaction, replacing separate dispatch_send, delegate, project_assign, and channel_post tools:

```
delegate(to, prompt, response, create_task, skill, tools)
```

5 response modes: `origin` (inject back into calling session), `perpetual` (into agent's perpetual session), `async` (spawn fresh session), `department` (post to dept channel), `none` (fire and forget).

### Two Loops

1. **Daemon patrol loop** (system-level, every 30s): manage the fleet — spawn workers for agent-bound tasks, fire due triggers, housekeeping.
2. **Agent loop** (per-session): LLM → tool execution → LLM → repeat until done.

## Message Flow

```
User message (Web / Telegram)
    ↓
ChatEngine → create task (agent-bound)
    → Worker loads agent identity + skill + memory
    → Agent loop runs → Outcome parsed
    → ChatEngine delivers response

Trigger fires (schedule/event)
    → Daemon creates task bound to trigger's owning agent
    → Same worker path as above

Agent delegates to another agent
    → delegate tool sends DelegateRequest dispatch
    → Target agent's trigger fires → async session
    → Response routed back per response mode
```

## Daemon Patrol Loop (9 steps)

1. `registry.patrol_all()` — reap workers, spawn workers for agent-bound tasks
2. `trigger_store.due_schedule_triggers()` — fire due triggers (bind to owning agent)
3. Check SIGHUP — hot-reload config
4. Save dispatch bus + cost ledger
5. Retry unacked dispatches, detect dead letters
6. Update metrics gauges
7. Prune old cost entries (7+ days)
8. Prune expired blackboard entries
9. Flush debounced memory writes

Event triggers run in a separate `tokio::spawn` subscriber.

## Middleware Chain (9 implementations)

| Middleware | Purpose |
|-----------|---------|
| Guardrails | Block dangerous ops (rm -rf, force push, drop table) |
| GraphGuardrails | Block dangerous git ops |
| MemoryRefresh | Re-search memory every N tool calls |
| LoopDetection | MD5 hash sliding window, warn at 3, kill at 5 repeats |
| CostTracking | Token/cost accumulation per task, budget ceiling |
| ContextBudget | Cap enrichment at ~200 lines |
| ContextCompression | Compress at 50% window, protect first/last messages |
| Clarification | Structured ask_clarification tool, halts execution, routes via department chain |
| SafetyNet | On failure: preserve partial work artifacts |

## Quality Bar

```bash
cargo test --workspace    # 643+ tests
cargo clippy --workspace --all-targets -- -D warnings
```

## Runtime

- `sigil daemon start` — orchestration plane (systemd: sigil.service)
- `sigil web start` — Axum REST API on :8400
- `sigil run` — one-shot agent execution
- IPC via Unix socket at `~/.sigil/rm.sock` (JSON-line protocol)

## IPC Commands

**Read:** ping, status, readiness, worker_progress, worker_events, projects, mail, dispatches, metrics, cost, audit, blackboard, expertise, tasks, missions, triggers, agent_identity, rate_limit, memories, skills, pipelines, project_knowledge, channel_knowledge
**Write:** create_task (with agent_id), close_task, post_blackboard, save_agent_file, knowledge_store, knowledge_delete
**Chat:** chat (quick), chat_full (agent execution), chat_poll (completion), chat_history, chat_channels

## Important Directories

- `config/sigil.toml` — master config
- `agents/{name}/` — agent templates (agent.toml + agent.md)
- `projects/{name}/` — project config, skills (.toml), .tasks/
- `projects/shared/skills/` — shared skills (autonomous + workflow + phase-specific)
- `~/.sigil/` — daemon state (agents.db with departments table, audit.db, blackboard.db, expertise.db, dispatches.db, memory.db, cost_ledger.jsonl, rm.sock)
- `docs/orchestration-redesign.md` — full architecture redesign spec

## Lock Architecture (CRITICAL)

IPC handlers use `try_lock()` on task boards — return partial data rather than blocking when patrol holds locks. Never use `.lock().await` in IPC read paths.

## Config Structure

```toml
[sigil]           # System name, data_dir
[web]             # bind, cors_origins, auth_secret
[providers.*]     # OpenRouter, Anthropic, Ollama
[security]        # autonomy, budget limits
[memory]          # SQLite backend, embedding config
[team]            # System leader
[orchestrator]    # Preflight, decomposition, retry
[repos]           # Global repo pool
[[projects]]      # Each project: name, prefix, repo, departments, missions
```

## Extension Points

- New skill: add `.toml` to `projects/shared/skills/` or `projects/{name}/skills/`
- New trigger: add to agent template frontmatter or use `manage_triggers` tool
- New middleware: implement `Middleware` trait, add to chain in supervisor
- New tool: implement `Tool` trait, wire into builder
- New provider: implement `Provider` trait
- New channel: implement `Channel` trait, register in daemon startup
- New department: `AgentRegistry::create_department()` via IPC or agent tool
- New IPC command: add match arm in `daemon.rs` handle_socket_connection

## Remaining Cleanup (Technical Debt)

- Old DispatchKind variants (TaskDone, TaskBlocked, etc.) coexist with new DelegateRequest/DelegateResponse — should be consolidated once all callers migrate
- Task.agent_id is Optional — should become required once all creation paths bind to agents
- Supervisor still iterates per-project, not per-agent — to be refactored when agent worker pools replace project supervisors
- Old separate tools (dispatch_send, channel_post) coexist with unified delegate tool
- Subagent spawning not yet wired through unified delegate tool (uses old DelegateTool)
