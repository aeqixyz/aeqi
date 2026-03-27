# Agents Directory

Each subdirectory in `agents/` defines who does the work.

## Layout

```text
agents/
  shared/
    WORKFLOW.md
  <agent>/
    agent.toml
    PERSONA.md
    IDENTITY.md
    OPERATIONAL.md
    PREFERENCES.md
    MEMORY.md
    EVOLUTION.md
    .tasks/
```

Only files that exist are loaded. Empty files are ignored.

## `agent.toml`

Typical fields:

```toml
name = "reviewer"
prefix = "rv"
role = "advisor"       # orchestrator | worker | advisor
voice = "vocal"        # vocal | silent
model = "xiaomi/mimo-v2-pro"
expertise = ["sigil"]
max_workers = 1
max_budget_usd = 1.0
```

Agents are discovered from disk and merged with any legacy `[[agents]]` blocks in `sigil.toml`.

## Identity Assembly

Agent-side identity comes from:

- `agents/shared/WORKFLOW.md`
- `PERSONA.md`
- `IDENTITY.md`
- `OPERATIONAL.md`
- `PREFERENCES.md`
- `MEMORY.md`
- `EVOLUTION.md`

Project context is layered on top separately from `projects/<name>/`.

## Notes

- Advisor agents can receive tasks from the daemon and therefore may have their own `.tasks/` store.
- Use `sigil agent list` to see discovered agents.
- Use `sigil agent migrate` to write disk `agent.toml` files from legacy config entries.
