# Claude Code Integration

Sigil uses Claude Code in the daemon worker path, not in the basic `sigil run` path.

## Where It Is Used

- Project supervisors can run workers in `claude_code` mode
- Advisor agents registered by the daemon also use Claude Code mode
- `sigil run` and `sigil skill run` do not shell out to Claude Code; they use the internal agent loop

## Executor Behavior

`crates/sigil-orchestrator/src/executor.rs` launches an external `claude` process with:

```text
claude -p "<task context>"
  --output-format stream-json
  --permission-mode bypassPermissions
  --model <model>
  --max-turns <n>
  --no-session-persistence
  --append-system-prompt "<identity + worker protocol>"
```

Additional behavior:

- the process working directory is set with `current_dir(...)`
- `--max-budget-usd` is passed when configured
- `CLAUDECODE` and `CLAUDE_CODE` are stripped from the environment
- the executor retries transient failures with exponential backoff

## Worker Protocol

Every Claude Code worker gets a fixed protocol block that teaches it how to report:

- success: normal summary or `DONE`
- blocker: `BLOCKED:`
- technical failure: `FAILED:`
- context handoff: `HANDOFF:`

The daemon converts that final text into `TaskOutcome`.

## Context Assembly

The worker system prompt is built from:

1. shared workflow
2. agent persona and identity
3. operational and evolution files
4. project `AGENTS.md`
5. project `KNOWLEDGE.md`
6. architect preferences and persistent memory files
7. worker protocol
8. optional checkpoint context

Claude Code then discovers repo-local `CLAUDE.md` files on its own from the working directory.

## Cost and Progress

Claude Code progress is streamed as JSON events.

- tool usage and turn progress are visible mid-run
- total cost is only reliably known on the final result event
- budget enforcement therefore becomes final-result-aware rather than true live metering

## IPC and Inspection

When the daemon is running, the IPC socket is:

- `~/.sigil/rm.sock`

Use it through the CLI:

```bash
sigil daemon query status
sigil daemon query dispatches
sigil daemon query cost
sigil daemon query metrics
```

## Current Boundary

Claude Code support is a runtime dependency. If `claude` is not installed or authenticated, `claude_code` workers cannot execute even though the rest of the daemon can still run.
