# Sigil

[![CI](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-black)](Cargo.toml)
[![React 19](https://img.shields.io/badge/ui-React%2019-61dafb)](apps/ui)

**Persistent agent orchestration in Rust.** Agents that remember, coordinate, and act autonomously.

Sigil is a runtime for persistent AI agents -- not one-shot sessions that forget everything, but identities with memory, hierarchy, and scheduled behaviors. Agents communicate through channels, delegate through dispatch, and evolve through triggers and skills.

## Core Model

Everything in Sigil flows through two primitives:

**Triggers** define *when* -- a cron schedule (`0 9 * * *`), an interval (`every 1h`), a one-shot time, or a runtime event (task completed, dispatch received, channel message).

**Skills** define *what* -- a TOML file with a system prompt and tool restrictions that gets loaded into the agent session when a trigger fires.

```yaml
# Agent template with triggers
---
name: engineer
parent: lead
project: sigil
capabilities: [manage_triggers]
triggers:
  - name: on-dispatch
    event: dispatch_received
    cooldown_secs: 60
    skill: process-dispatch
  - name: evolution
    schedule: every 24h
    skill: evolution
---

You are the Sigil systems engineer...
```

An agent's "subconscious" -- health checks, memory consolidation, self-reflection -- is just triggers and skills in the template. No special subsystems.

## Persistent Agents

Agents live in a SQLite registry (`~/.sigil/agents.db`). Each has a stable UUID, system prompt, project scope, and entity-scoped memory that accumulates across sessions. They're not running processes -- they're identities loaded into fresh sessions on demand.

**Org tree**: every agent has an optional `parent_id`. Shadow (the system leader) is the root. Department = leader + direct reports. Escalation walks up the tree. No config files needed -- the hierarchy is the data.

```
Shadow (root)
  +-- sigil-lead
  |     +-- engineer
  |     +-- reviewer
  +-- algo-lead
        +-- trader
```

## Agent Communication

**1:1 directed** -- dispatch bus. Agent sends a typed message (delegation, escalation, advice) to another agent. `DispatchReceived` event fires the target's trigger.

**1:many broadcast** -- conversation channels. Agent posts to a department or project channel via `channel_post`. All participating agents can react. Full conversation history persists in the ConversationStore.

**Shared knowledge** -- blackboard. Scoped entries (project, department, agent) for findings, decisions, and claims. Agents read it for context, post to it for coordination.

Every persistent agent gets `dispatch_read`, `dispatch_send`, and `channel_post` tools automatically.

## Architecture

```
User / Telegram / Slack / Web
    |
    v
ChatEngine ------> Quick path (intent detection, immediate response)
    |
    v
Task created -----> Supervisor assigns to worker
    |
    v
Worker loads: agent identity + skill + memory + org context + blackboard
    |
    v
Agent loop: LLM -> tool calls -> LLM -> ... -> done
    |
    v
Outcome: DONE / BLOCKED (escalate) / FAILED (retry)


Trigger fires (schedule / event)
    |
    v
Same worker path as above
```

### Daemon Patrol Loop

The daemon runs every 30 seconds:

1. Assign pending tasks to workers
2. Fire due triggers (schedule + once)
3. Hot-reload config on SIGHUP
4. Persist dispatch bus + cost ledger
5. Retry unacked dispatches
6. Update metrics
7. Prune old cost entries
8. Expire blackboard entries
9. Flush debounced memory writes

Event triggers run separately via a background subscriber on the EventBroadcaster.

### Middleware Chain

Every agent session runs through 8 safety layers:

| Layer | What it does |
|-------|-------------|
| Loop Detection | Kill after 5 repeated identical tool calls |
| Guardrails | Block `rm -rf`, force push, `DROP TABLE` |
| Cost Tracking | Enforce per-task budget ceiling |
| Context Compression | Compact at 50% context window |
| Context Budget | Cap enrichment at ~200 lines |
| Memory Refresh | Re-search memory every N tool calls |
| Clarification | Structured questions that halt execution |
| Safety Net | Preserve partial work on failure |

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

## Crates

| Crate | Purpose |
|-------|---------|
| `sigil-cli` | CLI binary, daemon process, TUI chat |
| `sigil-orchestrator` | Supervisor, workers, triggers, chat engine, dispatch, blackboard, middleware |
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
| `agents.db` | Persistent agent registry + triggers |
| `memory.db` | Entity, domain, and system memories |
| `blackboard.db` | Shared coordination entries |
| `dispatches.db` | Agent-to-agent message queue |
| `audit.db` | Decision audit trail |
| `expertise.db` | Agent performance per domain |
| `cost_ledger.jsonl` | Token spend tracking |
| `rm.sock` | Unix IPC socket |

## Development

```bash
cargo test              # 619 tests
cargo clippy -- -D warnings
cargo fmt --check
```

Pre-push hook runs all three automatically.

## Docs

- [Architecture overview](docs/architecture.md)
- [Vision](docs/vision.md)
- [Deployment model](docs/deployment.md)
- [Project setup](docs/project-setup.md)
- [Contributing](CONTRIBUTING.md)

## License

MIT
