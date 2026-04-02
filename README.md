# AEQI

[![CI](https://github.com/0xAEQI/aeqi/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/0xAEQI/aeqi/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-black)](Cargo.toml)
[![Tests](https://img.shields.io/badge/tests-634%2B-brightgreen)](Cargo.toml)

**Persistent agent orchestration in Rust.** Build organizations of AI agents that remember, coordinate, and act autonomously.

AEQI is not another chatbot wrapper. It's a runtime where agents are persistent identities -- they accumulate knowledge across sessions, trigger their own behaviors on schedules and events, coordinate through departments and delegation, and operate under safety middleware that enforces budgets, detects loops, and preserves work on failure.

The orchestrator and runtime are one system. When the orchestrator owns the runtime, it can inject context mid-execution, route by empirical expertise, enforce 9 middleware layers on every tool call, and give every agent entity-scoped memory that persists forever.

```
aeqi daemon start    # start the orchestration plane
aeqi web start       # API + dashboard on :8400
aeqi chat --agent cto   # talk to an agent
```

---

## Core Concepts

### Agents

An agent is a persistent identity with a UUID, a system prompt, entity-scoped memory, and a department. Agents are not running processes -- they're loaded into fresh sessions on demand. Knowledge accumulates across sessions via the memory system.

```toml
# agents/cto/agent.toml
display_name = "CTO"
model_tier = "capable"
department = "engineering"
expertise = ["architecture", "systems", "rust"]
capabilities = ["spawn_agents", "manage_triggers"]

[[triggers]]
name = "memory-consolidation"
schedule = "every 6h"
skill = "memory-consolidation"
```

Agents declare a `model_tier` (capable, balanced, fast, cheapest) instead of hardcoding a model. One config change updates all agents:

```toml
[models]
capable = "anthropic/claude-sonnet-4-6"
balanced = "anthropic/claude-sonnet-4-6"
fast = "anthropic/claude-haiku-4-5"
```

### Memory

Every agent has three memory scopes:

| Scope | What it stores | Lifetime |
|-------|---------------|----------|
| **Entity** | Agent-specific knowledge (per UUID) | Permanent |
| **Domain** | Project-level facts and procedures | Permanent |
| **System** | Cross-project knowledge | Permanent |

Memory is backed by SQLite with FTS5 full-text search and optional vector embeddings for hybrid retrieval. A query planner generates typed queries (fact, procedure, preference, context) from task context. Memories decay over time -- older facts rank lower unless reinforced.

Agents can build semantic knowledge graphs through `memory_edges` -- relationships like "mentions", "requires", "contradicts" between facts.

### Triggers

Triggers define *when* an agent acts:

| Type | Example | How it works |
|------|---------|-------------|
| **Schedule** | `0 9 * * *` or `every 1h` | Cron expression or interval |
| **Event** | `task_completed`, `dispatch_received` | Pattern match on runtime events with cooldown |
| **Once** | `2026-04-15T09:00:00Z` | Fire once at a specific time, auto-disable |
| **Webhook** | `POST /api/webhooks/:id` | External HTTP trigger with optional HMAC-SHA256 signing |

When a trigger fires, it creates an agent-bound task that loads the associated skill.

### Skills

Skills define *what* an agent does when triggered. A skill is a TOML file with a system prompt and tool restrictions:

```toml
[skill]
name = "code-review"
description = "Review code changes for quality and correctness"

[tools]
allow = ["shell", "read_file", "grep", "glob", "delegate"]

[prompt]
system = """Review the code changes. Check for..."""
```

Skills are composable -- agents load the right skill per task. Tool restrictions are enforced: a skill that only allows `read_file` cannot execute shell commands, even if the agent tries.

### Delegation

One tool for all inter-agent interaction:

```
delegate(to, prompt, response_mode, create_task, skill)
```

| `to` | What happens |
|------|-------------|
| Agent name | Task delegation to a persistent agent |
| `"dept:Engineering"` | Broadcast to department members |
| `"subagent"` | Spawn an ephemeral worker |

| Response mode | Where the result goes |
|--------------|----------------------|
| `origin` | Back into the calling session |
| `department` | Posted to department channel |
| `async` | Fresh session for the sender |
| `none` | Fire and forget |

Delegation creates a `DelegateRequest` dispatch. The daemon consumes dispatches for all active agents every patrol cycle, creates tasks, and routes responses back when complete.

### Departments

Agents are organized into a department hierarchy:

```
Root (manager: Shadow)
  +-- Engineering (manager: CTO)
  |     +-- Backend
  |     +-- Frontend
  +-- Operations (manager: COO)
```

Departments control:
- **Escalation** -- blocked agents escalate to their department manager, then up the chain
- **Blackboard visibility** -- entries scoped by department
- **Clarification routing** -- questions follow the department hierarchy

### Tasks

Every task is agent-bound. Tasks are created by triggers, delegation, IPC, or direct assignment.

```
Pending → InProgress → Done
                    → Blocked (escalate via department chain)
                    → Failed (adaptive retry with LLM failure analysis)
```

Tasks have atomic checkout (`locked_by`/`locked_at`) to prevent concurrent execution. State transitions are validated. Retry logic supports adaptive analysis: the system uses an LLM to classify failures as external blockers, missing context, or budget exhaustion, and routes accordingly.

---

## Runtime

### The Daemon

`aeqi daemon start` runs the orchestration plane. Every 30 seconds:

1. **Reap** -- each project pool reaps completed workers
2. **Collect** -- gather ready tasks and running agent counts across all projects
3. **Spawn** -- enforce per-agent `max_concurrent` globally, then per-project limits
4. **Consume dispatches** -- read mail for all active agents, create tasks from delegations
5. **Fire triggers** -- schedule, once, and event-driven
6. **Housekeeping** -- persist state, retry unacked dispatches, prune expired entries, flush memory writes

Per-agent concurrency is enforced globally -- an agent with `max_concurrent=1` cannot get two workers even if tasks exist in different projects.

### Middleware Chain

Every agent execution runs through 9 composable safety layers:

| Order | Layer | What it does |
|-------|-------|-------------|
| 200 | **Guardrails** | Block dangerous commands (`rm -rf`, `DROP TABLE`, force push) |
| 210 | **Graph Guardrails** | Blast radius analysis on code changes |
| 300 | **Loop Detection** | MD5 hash sliding window -- warn at 3 repeats, kill at 5 |
| 350 | **Context Compression** | Compact history at 50% context window, preserve first/last |
| 400 | **Context Budget** | Cap enrichment at ~200 lines per attachment |
| 600 | **Cost Tracking** | Per-task and per-scope budget enforcement |
| 50 | **Memory Refresh** | Re-search memory every N tool calls |
| 800 | **Clarification** | Structured questions routed via department chain |
| 900 | **Safety Net** | Detect and preserve partial work (git diffs, file edits) on failure |

Middleware hooks fire at 8 points: `on_start`, `before_model`, `after_model`, `before_tool`, `after_tool`, `after_turn`, `on_complete`, `on_error`.

### Budget Policies

Per-scope budget enforcement with auto-pause:

```
scope_type: agent | project | global
window: daily | monthly | lifetime
amount_usd: 50.0
warn_pct: 0.8
hard_stop: true
```

When a hard stop triggers, the scope is paused and an approval is created. Cost tracking captures per-call token breakdown (input, output, cached) with model and provider attribution.

### Approval Queue

Human-in-the-loop governance for autonomous agents:

```
GET  /api/approvals              -- list pending
POST /api/approvals/:id/resolve  -- approve or reject
```

Types: `permission` (dangerous action), `clarification` (agent question), `budget` (spend limit hit). Integrates with the middleware chain and department escalation.

### Blackboard

A department-scoped coordination surface for inter-agent knowledge sharing:

- **Transient** entries (24h TTL) for active coordination
- **Durable** entries (7d TTL) for findings and decisions
- Tag-based queries, cross-project search
- Department visibility scoping via `query_scoped()`

### Dispatch Bus

Reliable inter-agent messaging with delivery guarantees:

- Idempotency keys prevent duplicate execution
- ACK tracking with automatic retry (60s threshold, max 3 retries)
- Dead-letter detection for undeliverable messages
- SQLite-backed persistence across daemon restarts

### Expertise Routing

Agents are scored empirically using Wilson score lower-bound confidence on historical outcomes per domain. The system learns which agents are best at which types of work.

---

## Quick Start

**Prerequisites:** Rust stable, an LLM provider key (`OPENROUTER_API_KEY` or `ANTHROPIC_API_KEY`)

```bash
git clone https://github.com/0xAEQI/aeqi && cd aeqi
cp config/aeqi.example.toml config/aeqi.toml
# Edit config/aeqi.toml with your provider key

cargo build --release
./target/release/aeqi daemon start   # orchestration plane
./target/release/aeqi web start      # API + dashboard on :8400
```

### CLI

```bash
aeqi daemon start              # orchestration daemon
aeqi web start                 # REST API + WebSocket + dashboard
aeqi agent spawn agents/cto/   # create a persistent agent from template
aeqi agent registry            # list all registered agents
aeqi trigger create ...        # schedule, event, or webhook trigger
aeqi trigger create --webhook  # webhook trigger (prints URL + signing info)
aeqi chat --agent shadow       # interactive TUI chat
aeqi assign -r myproject "task description"
aeqi monitor                   # live terminal dashboard
```

---

## Architecture

```
CHAT SESSION (CLI / Telegram / Slack / Web)
    User message → Agent session (identity + memory + tools + department context)
    → Agent loop: LLM → tool calls → LLM → ... → response
    → Transcript persisted (FTS5 searchable by agent and task)

ASYNC TASK (trigger / delegation / webhook)
    Task created → Worker loads agent identity + skill + memory + blackboard
    → Middleware chain wraps execution (9 layers)
    → Agent loop: LLM → tool calls → LLM → ... → outcome
    → DONE: response routed back | BLOCKED: escalate | FAILED: adaptive retry
```

### Crates

| Crate | Purpose |
|-------|---------|
| `aeqi-cli` | CLI binary, daemon, TUI chat |
| `aeqi-orchestrator` | Worker pools, triggers, dispatch, departments, blackboard, middleware, approvals, budget |
| `aeqi-core` | Agent loop, config, identity, compaction, traits |
| `aeqi-web` | Axum REST API + WebSocket streaming + SPA |
| `aeqi-memory` | SQLite+FTS5, vector search, hybrid ranking, query planning, knowledge graph |
| `aeqi-tasks` | Task DAG, missions, dependency inference, atomic checkout |
| `aeqi-providers` | OpenRouter, Anthropic, Ollama + cost estimation |
| `aeqi-gates` | Telegram, Discord, Slack channels |
| `aeqi-tools` | Shell, file I/O, git, grep, glob, delegate, skills |
| `aeqi-graph` | Code intelligence: Rust/TS/Solidity parsing, community detection, impact analysis |

### Storage

All state lives in `~/.aeqi/`:

| File | What |
|------|------|
| `agents.db` | Agent registry, departments, triggers, budget policies, approvals |
| `conversations.db` | Chat history + session transcripts (FTS5) |
| `memory.db` | Entity, domain, and system memories + knowledge graph |
| `blackboard.db` | Department-scoped coordination entries |
| `dispatches.db` | Agent-to-agent dispatch queue with ACK tracking |
| `audit.db` | Decision audit trail with reasoning |
| `expertise.db` | Agent performance scores per domain |
| `cost_ledger.jsonl` | Per-call token spend with model/provider attribution |
| `rm.sock` | Unix IPC socket |

---

## Extending AEQI

**Add a skill** -- drop a `.toml` in `projects/shared/skills/` or `projects/{name}/skills/`.

**Add a trigger** -- in agent template TOML, via CLI (`aeqi trigger create`), or at runtime through the `manage_triggers` tool.

**Add a tool** -- implement the `Tool` trait, wire into the builder.

**Add a provider** -- implement the `Provider` trait for any LLM API.

**Add a channel** -- implement the `Channel` trait for Telegram, Discord, Slack, or any platform.

**Add middleware** -- implement the `Middleware` trait with ordered hook points, add to the chain.

**Add a department** -- via `AgentRegistry::create_department()` through IPC or the agent delegate tool.

---

## Development

```bash
cargo test              # 634+ tests
cargo clippy -- -D warnings
cargo fmt --check
```

Pre-push hook runs all three automatically.

## Docs

- [Architecture](docs/architecture.md)
- [Orchestration design](docs/orchestration-redesign.md)
- [Deployment](docs/deployment.md)
- [Project setup](docs/project-setup.md)
- [Vision](docs/vision.md)

## License

MIT
