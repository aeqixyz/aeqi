# Competitive Analysis: OpenClaw and Gas Town

This document compares Sigil against the latest local copies of:

- `openclaw@ef9597541`
- `gastown@8da798be`

The goal is not to copy either system wholesale. The goal is to identify what they already prove out in practice, where Sigil is weaker today, and where Sigil can win by staying simpler and more coherent.

## What OpenClaw Is Really Good At

OpenClaw is not just an agent runtime. It is a polished operator product.

From the current codebase:

- It has a real onboarding flow with risk acknowledgment, configuration handling, and guided setup in `src/wizard/onboarding.ts`.
- It has a deep `doctor` surface that checks config validity, gateway health, service state, sandboxing, auth, sessions, and migration paths in `src/commands/doctor.ts`.
- It has cross-platform background service management for macOS, Linux, and Windows in `src/daemon/service.ts`.
- It exposes readiness and channel health directly in the gateway server path in `src/gateway/server/readiness.ts`.
- It has a large plugin SDK and channel/plugin abstraction surface in `src/plugin-sdk/index.ts` plus `extensions/*`.
- It already treats context engines as a pluggable slot in `src/context-engine/`.

### Lessons for Sigil

OpenClaw's advantage is not "better orchestration theory". Its advantage is operator trust.

What Sigil should learn:

1. Setup and repair are product features, not side chores.
2. Health, readiness, and service state must be first-class, not implicit.
3. Extensibility needs a supported contract, not just internal traits.
4. Security posture needs explicit user-facing language and guardrails.

### Specific Gaps Sigil Has Relative to OpenClaw

- No guided setup or bootstrap wizard
- No service install/manage flow for the daemon
- No clear readiness endpoint or structured health contract in the CLI
- No first-class plugin SDK for channels, tools, or context providers
- No operator-facing security policy model comparable to pairing/allowlists/approval flows
- No control UI or polished dashboard layer

## What Gas Town Is Really Good At

Gas Town is much closer to Sigil's target shape than OpenClaw is.

From the current codebase and docs:

- It has a strong process model: Mayor, Witness, Polecat, Crew, Deacon.
- It persists work outside the agent session via hooks and beads.
- It uses `gt prime` to reconstruct work context at session start.
- It has durable work orchestration primitives: convoys, formulas, molecules, wisps, gates.
- It supports multiple runtime presets and hook installers through `internal/hooks/installer.go`.
- Its daemon is explicitly recovery-focused in `internal/daemon/daemon.go`.
- It treats work tracking as the core substrate rather than an add-on.

### Lessons for Sigil

Gas Town's advantage is durable operational state.

What Sigil should learn:

1. Sessions should be cheap; identity and work state should survive them.
2. Every worker needs a strong, resumable handoff path.
3. Runtime integration should be preset-driven instead of one-off.
4. Workflow templates should become more declarative and stateful.
5. Recovery loops are a core subsystem, not just timeout handling.

### Specific Gaps Sigil Has Relative to Gas Town

- No `prime`-style reconstruction step that unifies task, mail, checkpoint, and project state into a single restart contract
- No worker preset registry for multiple runtimes beyond the current OpenRouter internal loop and Claude Code path
- No durable per-worker slot model
- No explicit molecule/formula/gate execution layer beyond current pipelines and cron jobs
- No equivalent to hooks as a universal, persistent inbox abstraction
- No reputation or federation model

## Where Sigil Already Has an Advantage

Sigil should not become OpenClaw or Gas Town.

It already has strong structural advantages:

- A smaller conceptual core
- A single Rust workspace with a simpler deployment story
- Native budget accounting, audit log, blackboard, and memory in the orchestration layer
- A straightforward task DAG with missions and inferred dependencies
- A cleaner split between agent identity and project context
- A thinner, more inspectable internal agent loop

Sigil can win if it keeps those strengths while closing the most painful product gaps.

## Recommended Sigil Roadmap

### Priority 0: Operator Trust

Build the things that make the system feel safe and usable every day.

1. Add `sigil setup` as a guided bootstrap command.
2. Add daemon service install/manage commands for launchd/systemd.
3. Promote readiness and health to a first-class interface:
   - `sigil daemon query readiness`
   - `sigil doctor --strict`
   - hard checks for config, secrets, provider availability, Claude Code availability, project reachability
4. Add a risk model to config and docs:
   - local trusted mode
   - supervised shared mode
   - hardened remote mode

### Priority 1: Runtime Abstraction

Stop hardwiring orchestration around one provider path and one external runtime.

1. Replace the current provider/runtime factory with a preset registry:
   - internal agent loop
   - Claude Code
   - Codex CLI
   - future custom runtimes
2. Add runtime capability metadata:
   - supports tools
   - supports subagents
   - supports streaming cost
   - supports approvals
3. Make runtime choice explicit per project and per agent, not just via an enum and ad hoc wiring.

### Priority 1: Durable Worker State

Push closer to Gas Town's resilience without inheriting its full complexity.

1. Add a `prime`-style worker bootstrap contract:
   - task
   - previous checkpoint
   - blackboard state
   - unread dispatches
   - recent audit events
   - relevant memory
2. Add durable worker slots and resumable ownership.
3. Unify handoff, checkpoint, and escalation into one restartable state machine.

### Priority 1: Extensibility

Borrow OpenClaw's plugin discipline without dragging in its product surface.

1. Define supported extension slots:
   - provider
   - tool pack
   - memory backend
   - channel/gate
   - context assembler
2. Add a versioned Sigil extension manifest.
3. Turn `projects/shared/skills` and `projects/shared/pipelines` into the start of a broader reusable asset system.

### Priority 2: Better Control Plane UX

1. Add a TUI or minimal web dashboard for:
   - projects
   - workers
   - budgets
   - dispatches
   - audit
   - blackboard
2. Add richer daemon query surfaces before building UI:
   - `workers`
   - `tasks`
   - `readiness`
   - `blackboard`
   - `audit`
3. Make `sigil status` and `sigil doctor` far more opinionated.

### Priority 2: Security and Policy

1. Add execution policy by host and permission mode:
   - local workspace only
   - sandbox
   - external runtime unrestricted
2. Add approval and allowlist concepts for any future multi-user/channel surface.
3. Treat remote/shared use as a separate operating mode in both config and docs.

### Priority 3: Advanced Coordination

This is where Sigil can evolve past its current state once the basics are solid.

1. Expand pipelines into formula-like long-running workflows with wait gates.
2. Add richer convoy-style cross-project operations.
3. Add a real council surface instead of keeping it daemon-message-only.
4. Explore federation or shared reputation only after local single-install UX is strong.

## Concrete Next Steps

If the goal is to make Sigil materially better fast, the next three implementation passes should be:

1. `setup + doctor + daemon install`
   - Make first-run and recovery obvious.
2. runtime preset registry
   - Make Sigil capable of orchestrating more than one execution backend cleanly.
3. worker priming and resumability
   - Make crashes and handoffs feel routine instead of lossy.

After that:

4. extension manifest and plugin slots
5. richer daemon query surfaces
6. TUI or dashboard

## Bottom Line

OpenClaw proves that product polish, onboarding, diagnostics, and extensibility create operator trust.

Gas Town proves that durable roles, resumable work state, and persistent coordination primitives make multi-agent systems actually survive contact with reality.

Sigil should take:

- OpenClaw's operator UX discipline
- Gas Town's durability discipline
- and keep its own strengths: Rust, simplicity, budgets, auditability, and a tighter orchestration core

That combination is the path to a genuinely strong AI agent orchestrator harness rather than just another agent shell.
