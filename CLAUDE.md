# System Development

## Build & Test

```bash
cargo build                    # Dev build
cargo build --release          # Release (7MB, LTO + strip)
cargo test                     # 168 tests across 10 crates
cargo clippy                   # Lint (zero warnings)
```

## Crate Map

| Crate | Path | Purpose |
|-------|------|---------|
| `rm` | `rm/src/main.rs` | CLI binary, 20+ commands |
| `system-core` | `crates/system-core/` | Traits, config, agent loop, security, identity |
| `system-tasks` | `crates/system-tasks/` | Git-native task DAG (JSONL, hierarchical IDs) |
| `system-orchestrator` | `crates/system-orchestrator/` | 23 modules: AgentRouter, Supervisor, Worker, Daemon, Dispatch, Cost Ledger, Metrics, Checkpoints, Reflection, Gap Analysis, Council, Templates, Session Tracker, Operations, Schedule, Heartbeat, Hooks |
| `system-memory` | `crates/system-memory/` | SQLite+FTS5, vector search, hybrid, chunking |
| `system-providers` | `crates/system-providers/` | OpenRouter, Anthropic, Ollama + cost estimation |
| `system-gates` | `crates/system-gates/` | Telegram, Discord, Slack |
| `system-tools` | `crates/system-tools/` | Shell, file, git, tasks, delegate, magic |
| `system-companions` | `crates/system-companions/` | Companion gacha system (fusion, rarity, store) |
| `system-tenants` | `crates/system-tenants/` | Multi-tenant SaaS: auth, provisioning, Stripe billing, economy |
| `system-web` | `crates/system-web/` | REST API + WebSocket server (gacha, chat, companions, projects) |

## Key Patterns

- **Traits over concrete types**: Provider, Tool, Memory, Observer, Channel — all traits in `system-core/src/traits/`
- **Zero Framework Cognition**: Agent loop is a thin shell. No hardcoded heuristics. LLM decides everything.
- **Workers ARE Orchestrators**: Claude Code mode gives workers Task tool access for recursive sub-agent spawning.
- **Observe, Don't Trust**: Checkpoints captured externally via git (GUPP pattern), not self-reported by agents.
- **Budget-Gated Execution**: `can_afford_project()` checked before every worker spawn.
- **Config**: TOML at `config/system.toml`, loaded via `SystemConfig::discover()` (walks up directory tree)
- **Agent identity**: PERSONA.md, IDENTITY.md, PREFERENCES.md, MEMORY.md in `agents/<name>/`
- **Project context**: AGENTS.md, KNOWLEDGE.md, HEARTBEAT.md in `projects/<name>/`
- **Two-source loading**: `Identity::load(agent_dir, project_dir)` — agent personality + project context
- **Tasks**: Each project has `.tasks/` dir with `<prefix>.jsonl` files
- **Memory**: Per-project SQLite at `projects/<name>/.sigil/memory.db`
- **Checkpoints**: Worker work-in-progress at `projects/<name>/.sigil/checkpoints/<task_id>.json`

## Adding a New Tool

1. Create struct implementing `Tool` trait in `crates/system-tools/src/`
2. Implement `execute()`, `spec()`, `name()`
3. Export from `crates/system-tools/src/lib.rs`
4. Add to `build_project_tools()` in `rm/src/main.rs`

## Adding a New Provider

1. Create struct implementing `Provider` trait in `crates/system-providers/src/`
2. Implement `chat()`, `health_check()`, `name()`
3. Export from `crates/system-providers/src/lib.rs`
4. Add config section + factory in `rm/src/main.rs`

## Adding a New Channel

1. Create struct implementing `Channel` trait in `crates/system-gates/src/`
2. Implement `start()` (returns mpsc::Receiver), `send()`, `stop()`, `name()`
3. Export from `crates/system-gates/src/lib.rs`
4. Wire into daemon channel loop

## Working in This Repo

- Use standard worktree workflow: `git worktree add ~/worktrees/feat/<name> -b feat/<name>`
- Merge to `dev` for testing, then `dev` → `master` for production
- Commit messages: `feat:`, `fix:`, `docs:`, `chore:`
- Run `cargo test` before committing
- Edition: Rust 2024
- Default model: MiniMax M2.5, fallback: DeepSeek v3.2

## Config Location

- Dev config: `config/system.toml`
- Agent definitions: `agents/<name>/` (personality: PERSONA.md, IDENTITY.md, PREFERENCES.md)
- Project definitions: `projects/<name>/` (project context: KNOWLEDGE.md, AGENTS.md, HEARTBEAT.md)
- Shared workflow: `agents/shared/WORKFLOW.md`, `projects/shared/skills/`, `projects/shared/pipelines/`
- Skills: `projects/<name>/skills/*.toml`
- Pipelines: `projects/<name>/pipelines/*.toml`
- Data dir: `~/.sigil/` (PID file, socket, schedule, operations, secrets, costs, dispatches)

## Documentation

- [Architecture Deep Dive](docs/architecture.md) — Crate internals, execution flow, all 23 orchestrator modules
- [Project Setup](docs/projects.md) — Creating and configuring projects
- [Templates & Pipelines](docs/templates.md) — Workflow templates
- [Council System](docs/council.md) — Agent peer advisory system
- [Claude Code Integration](docs/claude-code-integration.md) — Worker execution, IPC, CLI commands
