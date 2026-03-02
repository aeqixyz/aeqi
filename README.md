# System

Recursive multi-agent orchestration framework in Rust. A single 7MB binary (`rm`) that coordinates autonomous AI agents across isolated projects -- each with its own repository, task DAG, memory, identity, and worker pool.

**Workers are orchestrators.** Unlike flat agent frameworks, System workers run as full Claude Code instances with unrestricted tool access, including the Task tool for spawning sub-agents. This creates a recursive execution tree where any worker can become a coordinator.

```
Emperor (Human)
    |
    +-- Team of Agents -- Kael (Financial) -- Mira (Product) -- Void (Systems)
    |         |
    |      Aurelia (Lead Agent)
    |         |
    |   +-----+------+----------+
    |   |     |      |          |
    | Supvsr Supvsr Supvsr   Supvsr
    | (AS)   (RD)   (EL)     (SG)
    |   |     |      |          |
    | Workers Workers Workers Workers    <- full Claude Code instances
    |   |                                <- can spawn sub-agents via Task tool
    |   +-- Sub-agent swarm
    |
    +-- Dispatch Bus (inter-agent messaging)
    +-- Cost Ledger (per-project budget enforcement)
    +-- Metrics (Prometheus-compatible)
    +-- Schedule (scheduled jobs)
```

## Why System

| | System | OpenClaw | Gas Town |
|---|---|---|---|
| **Language** | Rust (7MB binary) | TypeScript (1.5GB, 512+ CVEs) | Go (~20MB) |
| **Agent execution** | Claude Code CLI (unrestricted) | Own LLM loop (fragile) | Claude Code in tmux |
| **Recursive orchestration** | Workers ARE orchestrators (Task tool) | No recursion | No recursion |
| **Cost control** | Per-project budgets + global daily cap | None | $100/hr estimate |
| **Memory** | SQLite + FTS5 + vector hybrid per project | Broken shared memory | Per-agent files |
| **Identity** | 7-layer context (PERSONA->WORKFLOW->KNOWLEDGE) | System prompt | Role templates |
| **Self-improvement** | Reflection + drift detection + gap analysis | None | None |
| **Observability** | Prometheus metrics + checkpoints + dispatches | Logs only | Basic logs |
| **Multi-agent coordination** | Council debate + dispatch bus + operations | None | Shared tasks |

## Quick Start

```bash
# Build
cargo build --release    # ~7MB with LTO + strip

# Initialize
rm init

# Set API key
rm secrets set OPENROUTER_API_KEY sk-or-...

# Run a one-shot agent
rm run "list files in current directory"

# Assign work to a project
rm assign "fix the login bug" --rig algostaking --priority high

# Start the daemon
rm daemon start

# Check status
rm status
```

## Architecture

### Execution Hierarchy

| Layer | Name | Role | Model |
|-------|------|------|-------|
| 0 | Emperor | Human operator | -- |
| 1 | Council | Multi-agent advisory board | Gemini Flash (router) |
| 2 | Lead Agent (Aurelia) | Lead agent -- routes tasks, Telegram interface | Claude Opus |
| 3 | Supervisors | Per-project supervisors -- patrol, escalate, budget-gate | -- (control plane) |
| 4 | Workers | Claude Code executors -- full tool access, recursive | Claude Sonnet |
| 5 | Sub-agents | Spawned by workers via Task tool | Inherited |

### Crate Map

| Crate | Path | Purpose |
|-------|------|---------|
| `rm` | `rm/` | CLI binary -- 20+ commands |
| `system-core` | `crates/system-core/` | Traits, config, agent loop, security, identity |
| `system-tasks` | `crates/system-tasks/` | Git-native task DAG (JSONL, hierarchical IDs) |
| `system-orchestrator` | `crates/system-orchestrator/` | 23 modules: Agent, Supervisor, Worker, Daemon, Dispatch, Pipelines, Operations, Schedule, Heartbeat, Cost, Metrics, Checkpoints, Reflection, Gap Analysis, Council, Templates, Session Tracker |
| `system-memory` | `crates/system-memory/` | SQLite + FTS5, vector search, hybrid ranking, chunking |
| `system-providers` | `crates/system-providers/` | OpenRouter, Anthropic, Ollama + cost estimation |
| `system-gates` | `crates/system-gates/` | Telegram, Discord, Slack channels |
| `system-tools` | `crates/system-tools/` | Shell, file, git, tasks, delegate, magic tools |
| `system-companions` | `crates/system-companions/` | Companion gacha system (fusion, rarity, store) |

### Key Systems

**Cost Ledger** -- Per-project + global daily budget enforcement with JSONL persistence. Supervisors check `can_afford_project()` before spawning workers. 7-day auto-prune.

**Worker Checkpoints** -- External git state capture (GUPP pattern from Gas Town). On timeout or handoff, the supervisor captures `git status`, last commit, branch -- not self-reported by the agent. Successor workers receive checkpoint context.

**Reflection** -- Autonomous identity drift detection. FNV-1a fingerprints tracked files (PERSONA.md, IDENTITY.md, etc.), feeds diffs to a cheap LLM, applies updates to MEMORY.md/HEARTBEAT.md. Budget-capped at 2k tokens.

**Gap Analysis** -- When a project's queue empties, analyzes MEMORY.md + AGENTS.md to propose high-impact work. Confidence >= 0.70 auto-creates tasks; below threshold surfaces proposals via dispatches to the Lead Agent.

**Dispatch Bus** -- Indexed inter-agent messaging with O(1) recipient lookup, TTL expiry (1hr), bounded queues (1000), JSONL persistence. Message types: PatrolReport, WorkerCrashed, TaskProposal, CostAlert, and more.

**Context Budget** -- Per-layer character limits (40k total) with checkpoint summarization. Old checkpoints compressed to one-liners, recent kept verbatim. Prevents context window overflow.

**Prometheus Metrics** -- Zero-dependency text exposition format. Counters, gauges (fixed-point x1000), histograms with bucket inference. Per-project breakdowns: tasks completed, workers active, cost USD, patrol cycle time.

**Session Tracker** -- Telegram notifications for daemon state transitions (idle->active, deadline reached), periodic sprint check-ins, idle reminders. Anti-flood protection.

### Escalation Chain

```
Worker BLOCKED -> Project resolver (1 attempt) -> Lead Agent (cross-project knowledge) -> Human (Telegram)
```

## Naming Convention

| Concept | Name | Description |
|---------|------|-------------|
| Framework | System | The orchestration system |
| Business Unit | Project | Isolated agent container |
| Global Coordinator | Lead Agent | Lead agent (Aurelia) |
| Executor | Worker | Claude Code instance |
| Supervisor | Supervisor | Per-project patrol loop |
| Task | Task | JSONL DAG unit |
| Background Process | Daemon | Daemon controller |
| Messaging | Dispatch | Inter-agent bus |
| Event Callback | Hook | Hook system |
| Workflow Template | Pipeline / Template | Multi-step pipeline |
| Cross-project Op | Operation | Tracks tasks across projects |
| Scheduled Job | Schedule | Cron expressions |
| Health Check | Heartbeat | Periodic heartbeat |
| Skill | Magic | Agent capability |

## CLI Commands

```
rm init                              Initialize System
rm run "prompt" [--rig NAME]         One-shot agent execution
rm status                            System-wide status
rm doctor [--fix]                    Diagnostics

rm assign "task" --rig NAME          Create a task
rm ready [--rig NAME]                Show unblocked tasks
rm beads [--rig NAME] [--all]        Show all open tasks
rm close ID [--reason "..."]         Close a task
rm done ID                           Mark task done + update operations

rm daemon start|stop|status|query    Daemon management
rm hook WORKER TASK_ID               Pin work to a worker

rm mol pour TEMPLATE --rig NAME      Start a pipeline workflow
rm mol list [--rig NAME]             List pipeline templates
rm mol status ID                     Check pipeline progress

rm raid create "name" IDs...         Track cross-project work
rm raid list                         List active operations
rm raid status ID                    Check operation progress

rm cron add|list|remove              Manage schedule (scheduled jobs)
rm skill list|run                    List or run magic skills

rm recall "query" [--rig NAME]       Search memory
rm remember KEY CONTENT              Store a memory

rm secrets set|get|list|delete       Encrypted secret store
rm config show|reload                Config management (SIGHUP for hot reload)
```

## Configuration

`config/system.toml`:

```toml
[system]
name = "my-system"
data_dir = "~/.sigil"

[providers.openrouter]
api_key = "${OPENROUTER_API_KEY}"
default_model = "minimax/minimax-m2.5"
fallback_model = "deepseek/deepseek-v3.2"
embedding_model = "openai/text-embedding-3-small"

[security]
autonomy = "supervised"          # readonly | supervised | full
workspace_only = true
max_cost_per_day_usd = 10.0

[memory]
backend = "sqlite"
embedding_dimensions = 1536
vector_weight = 0.6
keyword_weight = 0.4
temporal_decay_halflife_days = 30

[[agents]]
name = "aurelia"
prefix = "fa"
model = "claude-opus-4-6"
role = "orchestrator"
execution_mode = "claude_code"

[[agents]]
name = "kael"
role = "advisor"
model = "claude-opus-4-6"
projects = ["algostaking"]        # Financial advisor scope

[team]
leader = "aurelia"
router_model = "gemini-flash"
router_cooldown_secs = 60

[[projects]]
name = "algostaking"
prefix = "as"
repo = "/path/to/repo"
model = "claude-sonnet-4-6"
max_workers = 3
execution_mode = "claude_code"
worker_timeout_secs = 3600
worktree_root = "/path/to/worktrees"
```

## Project Structure

Each project lives in `projects/<name>/`:

```
projects/algostaking/
  PERSONA.md             Personality and purpose (system prompt)
  IDENTITY.md            Name, expertise, repos
  AGENTS.md              Operating instructions
  KNOWLEDGE.md           Project-specific knowledge base
  HEARTBEAT.md           Periodic heartbeat check instructions
  PREFERENCES.md         Learned preferences (reflection-updated)
  MEMORY.md              Persistent memory notes
  skills/                Magic definitions (TOML)
  pipelines/             Pipeline templates (TOML)
  .tasks/                Task storage (JSONL, git-native)
  .sigil/
    memory.db            Per-project SQLite + FTS5
    checkpoints/         Worker checkpoint JSONs
```

## Design Principles

1. **Zero Framework Cognition** -- All decisions delegated to the LLM. Rust code is a thin, safe, deterministic shell. No hardcoded heuristics.

2. **Workers ARE Orchestrators** -- Every worker runs as a full Claude Code instance with Task tool access. Any worker can spawn sub-agents, creating recursive execution trees. This is the key architectural advantage over flat frameworks.

3. **Observe, Don't Trust** -- Checkpoints are captured externally via git inspection (GUPP pattern). Agents are unreliable self-reporters; git is the source of truth.

4. **Budget-Gated Execution** -- No worker spawns without passing `can_afford_project()`. Per-project budgets + global daily caps prevent runaway costs.

5. **Discovery Over Tracking** -- No master scheduler. Supervisors discover state from observables (tasks, git, process table). Gap analysis proposes work when queues empty.

6. **Trait-Driven Swappability** -- Provider, Tool, Memory, Observer, Channel -- all traits. Swap LLM providers, messaging channels, or memory backends without touching core.

7. **Bootstrap Files Not Config** -- PERSONA.md, IDENTITY.md, AGENTS.md -- human-readable, git-versioned, agent-editable via reflection.

## Documentation

- [Architecture Deep Dive](docs/architecture.md) -- Crate internals, execution flow, all 23 orchestrator modules
- [Project Setup](docs/projects.md) -- Creating and configuring projects
- [Templates & Pipelines](docs/templates.md) -- Workflow templates
- [Council System](docs/council.md) -- Multi-agent advisory board
- [Claude Code Integration](docs/claude-code-integration.md) -- Wiring Claude Code as a worker executor

## Build

```bash
cargo build                     # Dev
cargo build --release           # Release (7MB, LTO + strip)
cargo test                      # 110 tests across 8 crates
cargo clippy                    # Lint (zero warnings)
```

## License

MIT
