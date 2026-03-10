# Sigil Architecture

## Summary

Sigil is a Rust workspace with two practical execution planes:

- The internal agent plane, used by `sigil run` and `sigil skill run`
- The orchestration plane, used by `sigil daemon start`

The codebase already contains more orchestration primitives than the top-level CLI exposes directly. This document focuses on the runtime paths that are actually wired today.

## Crate Layers

```text
sigil-cli
  -> sigil-core
  -> sigil-tasks
  -> sigil-memory
  -> sigil-providers
  -> sigil-tools
  -> sigil-orchestrator
  -> sigil-gates
```

- `sigil-core`: traits, config loading, identity assembly, internal agent loop, secret store
- `sigil-tasks`: task DAG, missions, dependency inference, JSONL persistence
- `sigil-memory`: SQLite memory with FTS5, embedding cache, hybrid ranking
- `sigil-providers`: OpenRouter, Anthropic, Ollama clients plus model pricing
- `sigil-tools`: shell, file, git, task, skill, and other agent tools
- `sigil-orchestrator`: daemon, supervisors, workers, dispatch bus, cost ledger, audit log, blackboard, schedules
- `sigil-gates`: channel adapters such as Telegram

## Runtime Path 1: One-Shot CLI Execution

`sigil run` and `sigil skill run` both use the internal `sigil-core::Agent` loop.

```text
CLI command
  -> load config
  -> build provider
  -> build tool set
  -> load identity
  -> attach optional memory
  -> provider chat loop with tool execution
```

Key properties:

- Provider selection is currently wired through the OpenRouter path in `sigil-cli/src/helpers.rs`
- Tool execution happens inside Sigil, not through Claude Code
- Memory recall is injected into the system prompt before the loop starts
- This path is the simplest way to work on providers, tools, identity, and memory behavior

## Runtime Path 2: Daemon Orchestration

`sigil daemon start` builds a long-running registry and patrol loop.

```text
daemon start
  -> load config + merge agents from disk
  -> init dispatch bus, cost ledger, audit log, blackboard
  -> register projects
  -> register advisor agents as supervised task owners
  -> patrol ready work
  -> launch workers
  -> persist state + serve IPC
```

The daemon owns:

- Project registration and per-project supervisors
- Advisor-agent registration
- Audit log, blackboard, dispatch bus, cost ledger, schedule store
- Telegram ingress and council routing
- IPC queries on `~/.sigil/rm.sock`

Useful daemon probes today:

- `sigil daemon query status`: broad inventory of projects, budgets, pulses, and dispatch state
- `sigil daemon query readiness`: stricter control-plane readiness, including skipped registrations, worker capacity, and budget exhaustion

## Worker Execution Modes

Projects and advisor agents can run in either mode:

- `agent`: internal provider + tool loop
- `claude_code`: external Claude Code subprocess managed by `ClaudeCodeExecutor`

In practice, the daemon code is set up to use Claude Code for the long-running worker path when configured.

### Claude Code Flow

```text
Supervisor
  -> AgentWorker
  -> ClaudeCodeExecutor
  -> external `claude` process
  -> stream-json events
  -> TaskOutcome (Done, Blocked, Failed, Handoff)
```

Important details:

- Workers run with `--permission-mode bypassPermissions`
- State is not session-persistent
- Checkpoints are recorded outside the worker from repository state
- Cost is only reliably available on the final result event from Claude Code

## Identity and Context Assembly

Sigil separates agent identity from project context.

Agent-side files:

- `PERSONA.md`
- `IDENTITY.md`
- `OPERATIONAL.md`
- `PREFERENCES.md`
- `MEMORY.md`
- `EVOLUTION.md`
- shared `agents/shared/WORKFLOW.md`

Project-side files:

- `AGENTS.md`
- `KNOWLEDGE.md`
- `HEARTBEAT.md`

System prompt order from `sigil-core/src/identity.rs`:

1. Shared workflow
2. Persona
3. Identity
4. Evolution
5. Operational instructions
6. Project operating instructions
7. Project knowledge
8. Preferences
9. Persistent memory

Claude Code workers receive that identity plus the worker protocol that defines `DONE`, `BLOCKED:`, `FAILED:`, and `HANDOFF:`.

## State and Persistence

Global state under `~/.sigil/`:

- `rm.pid`: daemon PID
- `rm.sock`: daemon IPC socket
- `audit.db`: decision audit trail
- `blackboard.db`: blackboard entries
- `cost_ledger.jsonl`: cost accounting
- `dispatches/`: persisted dispatch queue state
- `fate.json`: cron jobs
- `operations.json`: cross-project operation state
- `memory.db`: global memory

Per-project or per-agent state:

- `.tasks/<prefix>.jsonl`: task streams for each prefix
- `.tasks/_missions.jsonl`: mission storage
- `.sigil/memory.db`: project memory database

## Shared Assets

Shared reusable assets live under `projects/shared/`.

- `projects/shared/skills/*.toml`
- `projects/shared/pipelines/*.toml`

The CLI now merges shared assets with project-local ones:

- shared assets load first
- project-local assets override on name collisions

This is the intended place for reusable workflows that should not be copied into each project directory.

## Public Surface vs Internal Capability

Some orchestration capabilities exist in code but are not yet first-class CLI commands.

Examples:

- Council mode exists in the daemon message path and Telegram flow, not as `sigil council`
- Budget inspection exists through daemon IPC, not as `sigil cost`
- Anthropic and Ollama providers exist as crates, but the standard CLI provider factory still selects OpenRouter

When documenting or extending Sigil, treat the daemon and CLI entrypoints as the source of truth for what operators can use today.

## Best Places To Extend

- Provider routing: `sigil-cli/src/helpers.rs`
- New tools: `crates/sigil-tools/` plus the relevant tool builders
- Worker behavior: `crates/sigil-orchestrator/src/agent_worker.rs` and `executor.rs`
- Orchestration policy: `crates/sigil-orchestrator/src/supervisor.rs` and `registry.rs`
- Identity assembly: `crates/sigil-core/src/identity.rs`
- CLI surface: `sigil-cli/src/cli.rs` and `sigil-cli/src/cmd/`
