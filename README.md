# Sigil

[![CI](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-black)](Cargo.toml)
[![React 19](https://img.shields.io/badge/ui-React%2019-61dafb)](apps/ui)

**Persistent agent orchestration in Rust.** Agents that remember, coordinate, and act autonomously.

Sigil is a runtime for persistent AI agents -- not one-shot sessions that forget everything, but identities with memory, departments, and scheduled behaviors. Agents communicate through a unified delegate tool, coordinate through a department-scoped blackboard, and evolve through triggers and skills.

## Four Primitives

**Agent** -- persistent identity with UUID, system prompt, entity-scoped memory. Belongs to a department. Not a running process -- loaded into fresh sessions on demand. Accumulated knowledge persists across sessions.

**Department** -- UUID-identified organizational unit with its own hierarchy. Has a name, project scope, manager (an agent), and parent department. Escalation follows the department chain. Blackboard visibility is department-scoped.

**Task** -- always agent-bound. Every task has an `agent_id` that determines which agent executes it. Tasks are never free-floating -- they're created by triggers, delegation, or direct assignment. Atomic checkout with `locked_by`/`locked_at` prevents concurrent execution. State transitions are validated.

**Delegation** -- unified `delegate` tool for all inter-agent interaction. One tool replaces messaging, task assignment, subagent spawning, and department broadcasts.

## Triggers + Skills

Everything in Sigil flows through two primitives:

**Triggers** define *when* -- a cron schedule (`0 9 * * *`), an interval (`every 1h`), a one-shot time, a runtime event (task completed, dispatch received, department message), or an external **webhook** (`POST /api/webhooks/:public_id` with optional HMAC-SHA256 signing).

**Skills** define *what* -- a TOML file with a system prompt and tool restrictions that gets loaded into the agent session when a trigger fires.

```toml
# agents/cto/agent.toml
display_name = "CTO"
model_tier = "capable"
max_workers = 2
max_turns = 30
expertise = ["architecture", "systems", "rust"]
capabilities = ["spawn_agents", "manage_triggers"]

[[triggers]]
name = "memory-consolidation"
schedule = "every 6h"
skill = "memory-consolidation"

[prompt]
system = """
You are CTO — the technology executive...
"""
```

An agent's "subconscious" -- health checks, memory consolidation, self-reflection -- is just triggers and skills in the template. No special subsystems.

## Departments

Agents are organized into departments stored in SQLite (`~/.sigil/agents.db`). Each department has a UUID, a manager, and a parent department. Agents belong to a department via `department_id`.

```
Root Department (manager: Shadow)
  +-- "Sigil Core" (manager: CTO)
  |     +-- "Backend" (manager: BackendLead)
  |     |     members: API Engineer, DB Engineer
  |     +-- "Frontend" (manager: FrontendLead)
  |           members: UI Engineer
  +-- "Trading" (manager: TradingLead)
        members: StrategyBot, RiskBot
```

Escalation follows the department chain: agent blocked -> department manager -> parent department manager -> ... -> Shadow -> user.

## Unified Delegate Tool

One tool for all inter-agent interaction:

```
delegate(to, prompt, response, create_task, skill, tools)
```

| `to` | What happens |
|------|-------------|
| Agent name | Message or task delegation to a persistent agent |
| `"dept:Engineering"` | Broadcast to all department members |
| `"subagent"` | Spawn an ephemeral subagent |

| `response` mode | Where the response goes |
|-----------------|------------------------|
| `"origin"` | Back into the calling session |
| `"perpetual"` | Into the agent's perpetual session |
| `"async"` | Fresh async session for the sender |
| `"department"` | Posted to the department channel |
| `"none"` | Fire and forget |

## Agent Roster

Sigil ships with a C-suite of persistent agent identities. Each declares a `model_tier` (resolved centrally) instead of hardcoding a model name.

| Agent | Tier | Function |
|-------|------|----------|
| **Shadow** | balanced | Personal assistant, default identity, first-use experience |
| **CEO** | capable | Strategic coordination, cross-functional decisions |
| **CTO** | capable | Architecture, engineering quality, technical strategy |
| **CPO** | balanced | Product strategy, UX, feature prioritization |
| **CFO** | capable | Financial ops, quantitative strategy, risk |
| **COO** | balanced | Deployment, monitoring, reliability |
| **GC** | capable | Legal, compliance, contracts |
| **CISO** | capable | Security, threat modeling, incident response |

### Model Tiers

Agents declare capability intent. The `[models]` config resolves to actual models:

```toml
[models]
capable = "anthropic/claude-sonnet-4-6"
balanced = "anthropic/claude-sonnet-4-6"
fast = "anthropic/claude-haiku-4-5"
cheapest = "anthropic/claude-haiku-4-5"
```

Change once, applies to all agents. Per-project overrides via `[[projects]]` config.

## Skills & Subagents

**Workflow skills** (11): feature, bugfix, refactor, research, synthesis, security-audit, plan-review, orchestrate, incident-response, release, migration

**Subagent templates** (9): explore, implement, spec-review, quality-review, verify, plan-review, test-generator, build-resolver, doc-writer, language-reviewer

**Reference skills**: scope-decision-framework, model-tier-routing, observation-learning

All skills live in `projects/shared/skills/`. Agent templates in `agents/*/agent.toml`.

## Architecture

Two ways agents run:

```
CHAT SESSION (CLI / Telegram / Slack / Web)

    User sends message
        |
        v
    Agent session (identity + memory + tools + dept context)
        |
        v
    Agent loop: LLM --> tool calls --> LLM --> ... --> response
        |
        v
    Conversation persists in ConversationStore


ASYNC TASK (trigger fires / delegation / dispatch)

    Trigger or delegate creates agent-bound task
        |
        v
    Worker loads: agent identity + skill + memory + dept context + blackboard
        |
        v
    Agent loop: LLM --> tool calls --> LLM --> ... --> done
        |
        v
    Outcome: DONE / BLOCKED (escalate via dept chain) / FAILED (retry)
        |
        v
    Transcript saved (FTS5 searchable)
```

### Daemon Patrol Loop

The daemon runs every 30 seconds:

1. **Reap** -- each project pool reaps completed workers independently
2. **Collect** -- gather ready tasks + running agent counts across ALL projects
3. **Spawn** -- enforce per-agent `max_concurrent` globally, then per-project limits
4. **Consume dispatches** -- read mail for all active agents, create tasks from delegations
5. Fire due triggers (bind to owning agent)
6. Hot-reload config on SIGHUP
7. Persist dispatch bus + cost ledger
8. Retry unacked dispatches, detect dead letters
9. Update metrics, prune cost/blackboard entries, flush debounced memory writes

Per-agent concurrency is enforced globally -- an agent with `max_concurrent=1` can't get 2 workers even if tasks exist in different projects. Event triggers run separately via a background subscriber on the EventBroadcaster.

### Middleware Chain

Every agent session runs through 9 safety layers:

| Layer | What it does |
|-------|-------------|
| Guardrails | Block `rm -rf`, force push, `DROP TABLE` |
| Graph Guardrails | Blast radius analysis on code changes |
| Loop Detection | Kill after 5 repeated identical tool calls |
| Cost Tracking | Per-task + per-scope budget enforcement |
| Context Budget | Cap enrichment at ~200 lines |
| Context Compression | Compact at 50% context window |
| Memory Refresh | Re-search memory every N tool calls |
| Clarification | Structured questions, routes via department chain |
| Safety Net | Preserve partial work on failure |

## Budget Policies

Per-scope budget enforcement with auto-pause:

```sql
-- scope_type: 'agent', 'project', 'global'
-- window: 'daily', 'monthly', 'lifetime'
budget_policies (scope_type, scope_id, window, amount_usd, warn_pct, hard_stop)
```

Cost tracking captures per-call token breakdown (input, output, cached) with model and provider attribution.

## Approval Queue

Human-in-the-loop governance. Agents can create approval requests that block execution until resolved:

```
GET  /api/approvals           -- list pending approvals
POST /api/approvals/:id/resolve  -- approve or reject
```

Types: `permission`, `clarification`, `budget`. Integrates with the clarification middleware and department escalation chain.

## Quick Start

**Prerequisites:** Rust stable, Node.js 22+, an LLM provider key (`OPENROUTER_API_KEY` or `ANTHROPIC_API_KEY`)

```bash
# Clone and configure
git clone https://github.com/0xAEQI/sigil && cd sigil
cp config/sigil.example.toml config/sigil.toml
# Edit config/sigil.toml with your provider key

# Build
cargo build
npm --prefix apps/ui ci && npm --prefix apps/ui run build

# Run
cargo run --bin sigil -- daemon start   # orchestration plane
cargo run --bin sigil -- web start      # API + UI on :8400
```

## CLI

```bash
sigil daemon start              # start the orchestration daemon
sigil web start                 # start the API + web UI
sigil agent spawn template.md   # create a persistent agent from template
sigil agent registry            # list all registered agents
sigil trigger create ...        # create a trigger for an agent
sigil trigger create --webhook  # create a webhook trigger (returns URL)
sigil trigger list              # list all triggers
sigil chat --agent shadow       # interactive TUI chat with an agent
sigil assign -r myproject "do X" # create a task
sigil monitor                   # live dashboard
```

## Extending Sigil

**Add a skill** -- drop a `.toml` file in `projects/shared/skills/` or `projects/{name}/skills/`:

```toml
[skill]
name = "my-skill"
description = "What this skill does"
phase = "autonomous"

[tools]
allow = ["shell", "read_file", "grep"]

[prompt]
system = """Your instructions here..."""
```

**Add a trigger** -- in an agent template's YAML frontmatter, or at runtime via the `manage_triggers` tool.

**Add a tool** -- implement the `Tool` trait in Rust, wire into the builder.

**Add a provider** -- implement the `Provider` trait for any LLM API.

**Add a channel** -- implement the `Channel` trait for any messaging platform.

**Add a department** -- via `AgentRegistry::create_department()` through IPC or agent tool.

## Crates

| Crate | Purpose |
|-------|---------|
| `sigil-cli` | CLI binary, daemon process, TUI chat |
| `sigil-orchestrator` | Worker pools, triggers, chat engine, dispatch, departments, blackboard, unified delegate, middleware, approvals, budget policies |
| `sigil-core` | Agent loop, config, identity, traits |
| `sigil-web` | Axum REST API + WebSocket + SPA serving |
| `sigil-memory` | SQLite+FTS5, vector search, hybrid ranking, query planning |
| `sigil-tasks` | Task DAG, missions, dependency inference |
| `sigil-providers` | OpenRouter, Anthropic, Ollama + cost estimation |
| `sigil-gates` | Telegram, Discord, Slack channels |
| `sigil-tools` | Shell, file I/O, git, grep, glob, delegate, skills |
| `sigil-graph` | Code intelligence: Rust/TS/Solidity parsing, impact analysis |

## Storage

All state lives in `~/.sigil/`:

| File | What |
|------|------|
| `agents.db` | Agent registry + departments + triggers + budget policies + approvals |
| `conversations.db` | Chat history + session transcripts (FTS5) |
| `memory.db` | Entity, domain, and system memories |
| `blackboard.db` | Department-scoped coordination entries |
| `dispatches.db` | Agent-to-agent dispatch queue |
| `audit.db` | Decision audit trail |
| `expertise.db` | Agent performance per domain |
| `cost_ledger.jsonl` | Token spend tracking |
| `rm.sock` | Unix IPC socket |

## Development

```bash
cargo test              # 634+ tests
cargo clippy -- -D warnings
cargo fmt --check
```

Pre-push hook runs all three automatically.

## Docs

- [Orchestration redesign](docs/orchestration-redesign.md) -- full architecture spec
- [Architecture overview](docs/architecture.md)
- [Vision](docs/vision.md)
- [Deployment model](docs/deployment.md)
- [Project setup](docs/project-setup.md)

## License

MIT
