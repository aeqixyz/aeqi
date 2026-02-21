# Sigil

Multi-agent orchestration framework in Rust. A single binary (`sg`) that coordinates autonomous AI agents across isolated Business Units using OpenRouter for LLM access.

## Quick Start

```bash
# Build
cargo build --release

# Initialize
sg init

# Set your OpenRouter API key
sg secrets set OPENROUTER_API_KEY sk-or-...

# Run a one-shot agent
sg run "list files in current directory"

# Run against a specific rig
sg run "what work is ready?" --rig algostaking
```

## Architecture

```
              Emperor (Human)
              Claude Code CLI
                    |
              SIGIL DAEMON (sg daemon)
              |            |
           Familiar     Mail Bus
           (Mayor)    (agent msgs)
              |
    +---------+---------+
    |         |         |
 Witness   Witness   Witness
 (per-rig) (per-rig) (per-rig)
    |         |         |
 Workers   Workers   Workers
 (tokio)   (tokio)   (tokio)
```

**Key concepts:**

- **Rig**: An isolated Business Unit with its own repo, beads (tasks), memory DB, identity files, and workers.
- **Familiar**: Global coordinator that routes work to the right rig.
- **Witness**: Per-rig supervisor that patrols workers, detects stuck/crashed tasks, and respawns.
- **Worker**: Ephemeral tokio task that executes a single bead (task).
- **Beads**: Git-native task DAG with hierarchical IDs, dependencies, and priority.
- **Molecule**: Workflow template (TOML) that creates a chain of dependent beads.
- **Heartbeat**: Periodic health check driven by HEARTBEAT.md instructions.

## Commands

```
sg init                           Initialize Sigil in current directory
sg run "prompt" [--rig NAME]      One-shot agent execution
sg daemon start|stop|status       Manage the background daemon
sg assign "task" --rig NAME       Route work to a rig
sg ready [--rig NAME]             Show unblocked work
sg beads [--rig NAME] [--all]     Show open beads
sg close ID [--reason "..."]      Close a bead
sg done ID                        Mark bead done + update convoys
sg hook WORKER BEAD_ID            Pin work to a worker
sg mol pour TEMPLATE --rig NAME   Start a molecule workflow
sg mol list [--rig NAME]          List molecule templates
sg mol status ID                  Check molecule progress
sg convoy create "name" IDs...    Track work across rigs
sg cron add|list|remove           Manage scheduled jobs
sg skill list|run                 List or run rig skills
sg recall "query" [--rig NAME]    Search memory
sg remember KEY CONTENT           Store a memory
sg secrets set|get|list|delete    Manage encrypted secrets
sg config show|reload             View or hot-reload config
sg doctor [--fix]                 Run diagnostics
```

## Configuration

Create `config/sigil.toml`:

```toml
[sigil]
name = "my-sigil"
data_dir = "~/.sigil"

[providers.openrouter]
api_key = "${OPENROUTER_API_KEY}"
default_model = "minimax/minimax-m2.5"
fallback_model = "deepseek/deepseek-v3.2"

[security]
autonomy = "supervised"
workspace_only = true
max_cost_per_day_usd = 10.0

[memory]
backend = "sqlite"
temporal_decay_halflife_days = 30

[heartbeat]
enabled = true
default_interval_minutes = 30

[[rigs]]
name = "algostaking"
prefix = "as"
repo = "/home/user/algostaking-backend"
model = "anthropic/claude-sonnet-4-20250514"
max_workers = 4
```

## Rig Structure

Each rig lives in `rigs/<name>/`:

```
rigs/algostaking/
  SOUL.md           System prompt ("You are the AlgoStaking agent...")
  IDENTITY.md       Name, style, expertise
  AGENTS.md         Operating instructions
  HEARTBEAT.md      Periodic health check instructions
  molecules/        Workflow templates (TOML)
  skills/           Skill definitions (TOML)
  .beads/           Task storage (JSONL, git-native)
  .sigil/memory.db  Per-rig memory (SQLite + FTS5)
```

See [docs/rigs.md](docs/rigs.md) for setup details.

## Crate Structure

| Crate | Purpose |
|-------|---------|
| `sg` | CLI binary |
| `sigil-core` | Traits, config, agent loop, security, identity |
| `sigil-beads` | Git-native task DAG (hierarchical IDs, dependencies) |
| `sigil-orchestrator` | Familiar, Witness, Worker, Daemon, Mail, Molecules, Convoys, Cron |
| `sigil-memory` | SQLite + FTS5, vector similarity, hybrid search, chunking |
| `sigil-providers` | OpenRouter, Anthropic, Ollama LLM providers |
| `sigil-channels` | Telegram, Discord, Slack messaging |
| `sigil-tools` | Shell, file, git, beads, delegate, skill tools |

## Design Principles

1. **Zero Framework Cognition**: All decisions delegated to LLM. Rust code is a thin, safe, deterministic shell.
2. **Discovery Over Tracking**: No master scheduler. Agents discover state from observables (beads, git, process table).
3. **GUPP**: "If there is work on your hook, you MUST run it." Agents resume work on restart.
4. **Trait-Driven Swappability**: Every subsystem is a trait + factory. Swap providers, channels, memory without touching core.
5. **Bootstrap Files Not Config Objects**: SOUL.md, IDENTITY.md, AGENTS.md — human-readable, git-versioned, agent-editable.

## Release Binary

```bash
cargo build --release  # ~7MB with LTO + strip
```

## License

MIT
