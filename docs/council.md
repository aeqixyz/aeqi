# Council Mode

Council mode exists in Sigil today, but it is part of the daemon's messaging path rather than a dedicated CLI subcommand.

## How It Works

The daemon can consult advisor agents before the leader responds.

```text
incoming message
  -> AgentRouter classifies relevant advisors
  -> advisor tasks are dispatched in parallel
  -> advisor output is collected
  -> leader synthesizes the response
```

The relevant code lives in:

- `sigil-cli/src/cmd/daemon.rs`
- `crates/sigil-orchestrator/src/agent_router.rs`
- `crates/sigil-orchestrator/src/council.rs`

## Public Entry Point Today

The clearest operator-facing entry point is the Telegram flow handled by the daemon:

- normal messages may invoke advisors based on router decisions
- `/council ...` forces explicit council behavior

There is currently no top-level `sigil council` command in the CLI.

## Advisor Model

Advisor agents are discovered from `agents/<name>/agent.toml` and registered by the daemon as supervised task owners. They can:

- receive routed questions
- execute in Claude Code mode
- write to their own `.tasks/` board
- participate in the same dispatch, audit, and budget systems as projects

## Operational Notes

- advisor routing depends on the daemon being active
- council usage is constrained by the same budget ledger as other orchestration work
- the `team.router_model` config controls the router classifier setting, but the common provider factory is still OpenRouter-based today
