# Sigil

Rust workspace for building an AI agent orchestration harness.

Sigil currently has two real execution paths:

- `sigil run`: a one-shot internal agent loop using Sigil tools plus the configured provider.
- `sigil daemon start`: a long-running control plane that supervises projects and advisor agents, persists state, and can launch Claude Code workers in `claude_code` mode.

## Current State

- 8 crates in one Cargo workspace
- 25 top-level CLI commands
- 204 unit tests passing with `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- Implemented subsystems: task DAGs, missions, operations, memory, audit log, blackboard, schedules, watchdogs, project teams, Telegram ingress, Claude Code worker execution

What changed in this documentation pass:

- The docs now describe the public surface that exists today.
- Shared `projects/shared/skills` and `projects/shared/pipelines` assets are now discovered alongside project-local assets, with project-local names overriding shared ones.
- Advisor task boards are reachable through the same task helpers as projects.

## Quick Start

```bash
cargo build

sigil init
sigil secrets set OPENROUTER_API_KEY sk-or-...

# add agents/<name>/ and projects/<name>/ plus config/sigil.toml entries
sigil doctor --strict

sigil run "summarize the repository layout"
sigil daemon start
sigil daemon query readiness
```

Use [config/sigil.example.toml](/home/claudedev/sigil/config/sigil.example.toml) as the starting config.

## What Is Wired Today

### Operator Surface

- Core: `run`, `init`, `doctor`, `status`, `config`, `team`, `agent`, `secrets`
- Work management: `assign`, `ready`, `tasks`, `close`, `hook`, `done`, `mission`, `operation`, `deps`
- Knowledge and automation: `recall`, `remember`, `skill`, `pipeline`, `cron`
- Long-running orchestration: `daemon`, `audit`, `blackboard`

For daemon state and budget inspection, use:

```bash
sigil daemon query status
sigil daemon query readiness
sigil daemon query projects
sigil daemon query dispatches
sigil daemon query cost
sigil daemon query metrics
```

### Runtime Split

`sigil run` uses the internal `sigil-core` agent loop:

1. Load `sigil.toml`
2. Build provider and tool set
3. Load identity from `agents/` and `projects/`
4. Optionally attach SQLite memory
5. Run the LLM loop until there are no more tool calls

`sigil daemon start` uses `sigil-orchestrator`:

1. Load config plus agent discovery from disk
2. Initialize dispatch bus, cost ledger, audit log, blackboard, schedules
3. Register projects and advisor agents as supervised task owners
4. Patrol ready work and spawn workers
5. Persist orchestration state and serve daemon IPC on `~/.sigil/rm.sock`

### Claude Code Workers

In `claude_code` execution mode, supervisors launch external Claude Code subprocesses with:

- `-p`
- `--output-format stream-json`
- `--permission-mode bypassPermissions`
- `--max-turns`
- `--no-session-persistence`
- `--append-system-prompt`

The worker protocol supports `DONE`, `BLOCKED:`, `FAILED:`, and `HANDOFF:` outcomes.

## Repository Map

| Crate | Path | Role |
| --- | --- | --- |
| `sigil` | `sigil-cli/` | CLI entrypoint and command handlers |
| `sigil-core` | `crates/sigil-core/` | Traits, config, identity assembly, internal agent loop, secrets |
| `sigil-tasks` | `crates/sigil-tasks/` | JSONL task DAGs, missions, dependency inference |
| `sigil-orchestrator` | `crates/sigil-orchestrator/` | Daemon, supervisors, workers, dispatch, audit, blackboard, budgets |
| `sigil-memory` | `crates/sigil-memory/` | SQLite memory, FTS5, vector cache, reranking |
| `sigil-providers` | `crates/sigil-providers/` | Provider implementations and cost estimation |
| `sigil-gates` | `crates/sigil-gates/` | Telegram, Slack, Discord channel implementations |
| `sigil-tools` | `crates/sigil-tools/` | Shell, file, git, task, skill, delegate tools |

## Storage Layout

- `config/sigil.toml`: main configuration
- `agents/<name>/`: agent identity and optional advisor task board
- `projects/<name>/`: project instructions, local skills, local pipelines, `.tasks/`
- `projects/shared/skills/`: shared skills available to all projects
- `projects/shared/pipelines/`: shared pipeline templates available to all projects
- `~/.sigil/rm.pid`: daemon PID file
- `~/.sigil/rm.sock`: daemon IPC socket
- `~/.sigil/audit.db`: decision audit trail
- `~/.sigil/blackboard.db`: inter-agent blackboard
- `~/.sigil/cost_ledger.jsonl`: budget ledger
- `~/.sigil/dispatches/`: persisted dispatch bus state
- `~/.sigil/fate.json`: cron store
- `~/.sigil/operations.json`: cross-project operation tracking
- `~/.sigil/memory.db`: global memory
- `projects/<name>/.sigil/memory.db`: project memory

## Current Boundaries

- There is no first-class `sigil council` CLI subcommand. Council mode is driven from the daemon's message path, most visibly through Telegram `/council ...`.
- There is no top-level `sigil cost` CLI subcommand. Use `sigil daemon query cost`.
- Readiness is daemon-driven. Use `sigil daemon query readiness` for a machine-readable “can this harness accept work now?” answer.
- Provider crates exist for Anthropic and Ollama, but the main CLI/daemon provider factory is still wired through the OpenRouter path today.
- Recursive worker orchestration depends on an installed, authenticated `claude` binary when projects use `claude_code`.

## Working On Sigil

Start with:

- [docs/architecture.md](/home/claudedev/sigil/docs/architecture.md)
- [docs/SIGIL.md](/home/claudedev/sigil/docs/SIGIL.md)
- [docs/competitive-analysis.md](/home/claudedev/sigil/docs/competitive-analysis.md)
- [docs/claude-code-integration.md](/home/claudedev/sigil/docs/claude-code-integration.md)
- [agents/README.md](/home/claudedev/sigil/agents/README.md)
- [projects/README.md](/home/claudedev/sigil/projects/README.md)

Recommended validation loop:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
