# Sigil Reference

This is the live reference for the Sigil workspace as it exists today.

## Workspace Snapshot

- Cargo workspace with 8 crates
- CLI binary in `sigil-cli/`
- 25 top-level CLI commands
- 204 unit tests currently passing

## Command Surface

### Core

- `sigil init`
- `sigil doctor [--fix]`
- `sigil status`
- `sigil config show`
- `sigil config reload`
- `sigil team`
- `sigil agent list`
- `sigil agent migrate`
- `sigil secrets set|get|list|delete`

### One-Shot Execution

- `sigil run "prompt" [--project NAME]`
- `sigil skill list [--project NAME]`
- `sigil skill run NAME --project NAME [prompt]`

Notes:

- `sigil run` uses the internal agent loop
- `sigil skill run` also uses the internal agent loop, but filters tools by the selected skill policy

### Task and Mission Flow

- `sigil assign "subject" --project NAME`
- `sigil ready [--project NAME]`
- `sigil tasks [--project NAME] [--all]`
- `sigil close TASK_ID`
- `sigil hook WORKER TASK_ID`
- `sigil done TASK_ID`
- `sigil mission create|list|status|close`
- `sigil deps --project NAME [--apply THRESHOLD]`

Task IDs use a prefix-based hierarchy:

- root task: `mp-001`
- child task: `mp-001.1`
- grandchild task: `mp-001.1.1`

Mission IDs use `prefix-mNNN`, for example `mp-m001`.

### Pipelines and Operations

- `sigil pipeline list [--project NAME]`
- `sigil pipeline pour TEMPLATE --project NAME --var key=value`
- `sigil pipeline status TASK_ID`
- `sigil operation create NAME TASK_ID...`
- `sigil operation list`
- `sigil operation status OP_ID`

Pipeline discovery order:

1. `projects/shared/pipelines/`
2. `projects/shared/rituals/`
3. `projects/<name>/pipelines/`
4. `projects/<name>/rituals/`

Project-local pipeline names override shared names.

### Memory and Observability

- `sigil recall "query" [--project NAME]`
- `sigil remember KEY CONTENT [--project NAME]`
- `sigil audit [--project NAME] [--task TASK_ID] [--last N]`
- `sigil blackboard list|post|query`

### Daemon and Scheduling

- `sigil daemon start`
- `sigil daemon stop`
- `sigil daemon status`
- `sigil daemon query CMD`
- `sigil cron add|list|remove`

Useful daemon IPC commands:

- `ping`
- `status`
- `projects`
- `mail`
- `dispatches`
- `metrics`
- `cost`
- `audit`

## Directory Layout

```text
sigil/
  config/
    sigil.toml
    sigil.example.toml
  agents/
    shared/WORKFLOW.md
    <agent>/
      agent.toml
      PERSONA.md
      IDENTITY.md
      OPERATIONAL.md
      PREFERENCES.md
      MEMORY.md
      EVOLUTION.md
      .tasks/
  projects/
    shared/
      skills/*.toml
      pipelines/*.toml
    <project>/
      AGENTS.md
      KNOWLEDGE.md
      HEARTBEAT.md
      skills/*.toml
      pipelines/*.toml
      rituals/*.toml
      .tasks/
      .sigil/memory.db
```

## Configuration Model

Top-level sections in `sigil.toml`:

- `[sigil]`: workspace name, data dir, patrol interval
- `[providers.*]`: OpenRouter, Anthropic, Ollama configs
- `[security]`: autonomy mode, workspace restriction, daily budget
- `[memory]`: backend and ranking parameters
- `[heartbeat]`: periodic heartbeats and reflections
- `[team]`: leader, advisor roster, router model, background budget
- `[session]`, `[context_budget]`, `[lifecycle]`, `[orchestrator]`: orchestration tuning
- `[repos]`: named repository pool
- `[[projects]]`: project definitions
- `[[watchdogs]]`: event-driven automation rules

Provider reality:

- OpenRouter is the provider path used by the main CLI and daemon factory today
- Anthropic and Ollama clients exist in the workspace, but are not selected by the common factory yet

## Identity Assembly

Identity comes from files, not a large TOML schema.

Loaded agent-side files:

- `PERSONA.md`
- `IDENTITY.md`
- `OPERATIONAL.md`
- `PREFERENCES.md`
- `MEMORY.md`
- `EVOLUTION.md`
- `agents/shared/WORKFLOW.md`

Loaded project-side files:

- `AGENTS.md`
- `KNOWLEDGE.md`
- `HEARTBEAT.md`

This is the main prompt-building path for both the internal agent loop and Claude Code workers.

## Persistence

Global data dir contents:

- `~/.sigil/rm.pid`
- `~/.sigil/rm.sock`
- `~/.sigil/audit.db`
- `~/.sigil/blackboard.db`
- `~/.sigil/cost_ledger.jsonl`
- `~/.sigil/dispatches/`
- `~/.sigil/fate.json`
- `~/.sigil/operations.json`
- `~/.sigil/memory.db`

Task storage:

- `.tasks/<prefix>.jsonl`: append-only task records
- `.tasks/_missions.jsonl`: append-only mission records

## Extension Points

Traits live under `crates/sigil-core/src/traits/`.

- `Provider`
- `Tool`
- `Memory`
- `Observer`
- `Channel`
- `Embedder`

Useful extension targets:

- New provider: `crates/sigil-providers/`
- New tool: `crates/sigil-tools/`
- New daemon surface: `sigil-cli/src/cmd/daemon.rs` and `crates/sigil-orchestrator/src/daemon.rs`
- New orchestration policy: `crates/sigil-orchestrator/src/supervisor.rs`

## Current Boundaries

- No dedicated `sigil council` subcommand
- No dedicated `sigil cost` subcommand
- Council routing is daemon-driven, most visibly from Telegram `/council ...`
- Cost inspection is daemon-driven via `sigil daemon query cost`
- Readiness inspection is daemon-driven via `sigil daemon query readiness`
- Claude Code recursion requires an external `claude` installation and authentication

## Recommended Validation

```bash
sigil doctor --strict
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
