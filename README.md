# Sigil

Rust workspace for building an AI agent orchestration harness.

Sigil currently has two real execution paths:

- `sigil run`: a one-shot internal agent loop using Sigil tools plus the configured provider.
- `sigil daemon start`: a long-running control plane that supervises projects and advisor agents, persists state, and can launch Claude Code workers in `claude_code` mode.

## Current State

- 8 crates in one Cargo workspace
- 25 top-level CLI commands
- 214 unit tests passing with `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- Implemented subsystems: task DAGs, missions, operations, memory, audit log, blackboard, schedules, watchdogs, project teams, organization kernel config, Telegram ingress, Claude Code worker execution

What changed in this documentation pass:

- The docs now describe the public surface that exists today.
- Shared `projects/shared/skills` and `projects/shared/pipelines` assets are now discovered alongside project-local assets, with project-local names overriding shared ones.
- Advisor task boards are reachable through the same task helpers as projects.

## Quick Start

```bash
cargo build

sigil setup --service
sigil secrets set OPENROUTER_API_KEY sk-or-...

# add projects/<name>/ plus config/sigil.toml entries
sigil doctor --strict
sigil team

sigil run "summarize the repository layout"
sigil daemon install --start
sigil daemon query readiness
```

Use [config/sigil.example.toml](/home/claudedev/sigil/config/sigil.example.toml) as the starting config.

## What Is Wired Today

### Operator Surface

- Core: `run`, `init`, `setup`, `doctor`, `status`, `config`, `team`, `agent`, `secrets`
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
2. Build the provider from the selected project runtime, or from the standalone leader/runtime when no project is selected
3. Load identity from the project team leader plus `projects/` context when a project is selected
4. Optionally attach SQLite memory
5. Run the LLM loop until there are no more tool calls

`sigil daemon start` uses `sigil-orchestrator`:

1. Load config plus agent discovery from disk
2. Initialize dispatch bus, cost ledger, audit log, blackboard, schedules
3. Register projects and advisor agents as supervised task owners
4. Patrol ready work and spawn workers
5. Persist orchestration state and serve daemon IPC on `~/.sigil/rm.sock`

Runtime presets now select the worker provider and execution mode per project or agent. Built-ins:

- `openrouter_agent`
- `openrouter_claude_code`
- `anthropic_agent`
- `anthropic_claude_code`
- `ollama_agent`
- `ollama_claude_code`

### Organization Kernel

Sigil now has a first-class organization model in `sigil.toml`:

- `[[organizations]]`: top-level org graphs
- `[[organizations.units]]`: teams, departments, councils, squads
- `[[organizations.roles]]`: mandate, goals, permissions, budget per agent
- `[[organizations.relationships]]`: manages, advises, reviews, delegates, escalates, collaborates
- `[[organizations.rituals]]`: recurring reviews, planning loops, incident cadences

Project teams can bind to an org unit with `team.org` and `team.unit`. Agent identity assembly now injects organizational context, so leaders, peers, advisors, direct reports, and rituals are visible in the system prompt for `sigil run`, `sigil skill run`, and daemon-supervised workers.

If an agent belongs to multiple organizations, Sigil now resolves org context explicitly: project-bound runs prefer `team.org`, otherwise the default organization is used when that agent belongs to it, and Sigil refuses to guess when neither choice is available.

Use these operator surfaces to inspect the model:

- `sigil team`
- `sigil status`
- `sigil agent list`

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
- `~/.config/systemd/user/sigil.service`: optional user service on Linux

## Current Boundaries

- There is no first-class `sigil council` CLI subcommand. Council mode is driven from the daemon's message path, most visibly through Telegram `/council ...`.
- There is no top-level `sigil cost` CLI subcommand. Use `sigil daemon query cost`.
- Readiness is daemon-driven. Use `sigil daemon query readiness` for a machine-readable “can this harness accept work now?” answer.
- The organization kernel is native, but per-role direct chat surfaces like `@ceo` or `@incident-lead` are not first-class CLI commands yet. Today the org model primarily shapes identity, project-team resolution, and operator inspection.
- Worker/provider runtime presets are wired through the CLI and daemon now, but advisor routing and usage-credit inspection are still OpenRouter-oriented.
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
