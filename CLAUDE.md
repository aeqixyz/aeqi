# Sigil

AI agent orchestration workspace in Rust.

## What Lives Here

- `sigil-cli/`: CLI commands and daemon wiring
- `crates/sigil-core/`: traits, config, identity, internal agent loop, secrets
- `crates/sigil-orchestrator/`: supervisors, daemon, Claude Code executor, dispatch, audit, blackboard, budgets
- `crates/sigil-tasks/`: task DAGs, missions, dependency inference
- `crates/sigil-memory/`: SQLite memory and hybrid retrieval
- `crates/sigil-providers/`: provider clients and pricing
- `agents/`: agent identity files and advisor task boards
- `projects/`: project instructions, skills, pipelines, task stores

## Current Quality Bar

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Current status:

- 204 unit tests passing
- Clippy clean across workspace targets

## Runtime Facts

- `sigil run` uses the internal agent loop.
- `sigil daemon start` is the long-running orchestration plane.
- `claude_code` execution mode shells out to the external `claude` binary.
- Budget and daemon state are queried with `sigil daemon query ...`, not `sigil cost`.
- Council flow exists in the daemon message path, not as a first-class CLI subcommand.

## Important Directories

- `agents/shared/WORKFLOW.md`: shared workflow context for agent identities
- `projects/shared/skills/`: shared skill catalog
- `projects/shared/pipelines/`: shared pipeline catalog
- `projects/<name>/.tasks/`: JSONL task storage
- `~/.sigil/`: daemon state, audit log, blackboard, budget ledger, IPC socket

## Extension Points

- New tool: implement `Tool`, export it, then wire it into the builder path you need
- New provider: implement `Provider`; the CLI factory currently only selects the OpenRouter path
- New channel: implement `Channel`, then register it in daemon startup
- New project capability: prefer putting reusable assets in `projects/shared/` first
