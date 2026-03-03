# Sigil — Agent Orchestration Framework

This is the orchestrator. All projects, agents, and worker execution are managed from this repo.

## What Lives Here

- **Agents** (`agents/<name>/`): Personality, identity, preferences, memory — WHO does the work
- **Projects** (`projects/<name>/`): Knowledge, operating instructions, tasks, skills — WHAT gets done
- **Shared** (`agents/shared/`, `projects/shared/`): Workflow, code standards, skills, pipelines
- **Config** (`config/system.toml`): Agent definitions, project config, teams, budgets

## Managed Projects

| Project | Repo | Domain |
|---------|------|--------|
| algostaking | `/home/claudedev/algostaking-backend` | HFT trading (12 Rust microservices) |
| riftdecks-shop | `/home/claudedev/riftdecks` | TCG drop shop (Next.js) |
| entity-legal | — | Legal entity formation |
| gacha-agency | `/home/claudedev/sigil` | This framework (Rust) |

Each managed repo has a CLAUDE.md that points back here for canonical knowledge.

## Build & Test

```bash
cargo build                    # Dev build
cargo build --release          # Release (7MB, LTO + strip)
cargo test                     # ~170 tests across 10 crates
cargo clippy                   # Lint (zero warnings)
```

## Crate Map

| Crate | Path | Purpose |
|-------|------|---------|
| `rm` | `rm/src/main.rs` | CLI binary, 20+ commands |
| `system-core` | `crates/system-core/` | Traits, config, agent loop, security, identity |
| `system-tasks` | `crates/system-tasks/` | Git-native task DAG (JSONL, hierarchical IDs) |
| `system-orchestrator` | `crates/system-orchestrator/` | Router, Supervisor, Worker, Daemon, Dispatch, Ledger, Metrics |
| `system-memory` | `crates/system-memory/` | SQLite+FTS5, vector search, hybrid, chunking |
| `system-providers` | `crates/system-providers/` | OpenRouter, Anthropic, Ollama + cost estimation |
| `system-gates` | `crates/system-gates/` | Telegram, Discord, Slack |
| `system-tools` | `crates/system-tools/` | Shell, file, git, tasks, delegate, magic |
| `system-companions` | `crates/system-companions/` | Companion gacha system (fusion, rarity, store) |
| `system-tenants` | `crates/system-tenants/` | Multi-tenant SaaS: auth, provisioning, Stripe billing |
| `system-web` | `crates/system-web/` | REST API + WebSocket server |

## Key Patterns

- **Traits over concrete types**: Provider, Tool, Memory, Observer, Channel — all in `system-core/src/traits/`
- **Zero Framework Cognition**: Agent loop is a thin shell. LLM decides everything.
- **Workers ARE Orchestrators**: Claude Code mode gives workers Task tool access for sub-agent spawning.
- **Two-source identity**: `Identity::load(agent_dir, project_dir)` — agent personality + project context
- **Budget-Gated Execution**: `can_afford_project()` checked before every worker spawn
- **Config discovery**: `SystemConfig::discover()` walks up directory tree for `system.toml` (fallback `realm.toml`)

## Adding Things

- **New tool**: Implement `Tool` trait in `system-tools`, export from lib.rs, add to `build_project_tools()`
- **New provider**: Implement `Provider` trait in `system-providers`, export, add factory
- **New channel**: Implement `Channel` trait in `system-gates`, export, wire into daemon
- **New project**: Create `projects/<name>/` with AGENTS.md + KNOWLEDGE.md, add to `config/system.toml`

## Development Workflow

```bash
# Standard worktree workflow
git worktree add ~/worktrees/sigil/feat/<name> -b feat/<name>
cd ~/worktrees/sigil/feat/<name>

# Develop, test, commit
cargo test && cargo clippy
git commit -m "feat: description"

# Merge to dev for testing, then dev → master for production
cd /home/claudedev/sigil
git merge feat/<name>
```

- Commit messages: `feat:`, `fix:`, `docs:`, `chore:`
- Run `cargo test` before committing
- Edition: Rust 2024

## Critical Rules

- Traits over concrete types — everything through Provider, Tool, Memory, Observer, Channel
- Zero Framework Cognition — agent loop is thin, LLM decides everything
- No hardcoded heuristics in the agent loop
- Shared templates in `projects/shared/` — never duplicate per-project what can be shared
