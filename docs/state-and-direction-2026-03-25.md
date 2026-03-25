# Sigil — State & Direction

Date: 2026-03-25 (evening)
Author: working session, human + Claude

This document captures where Sigil actually is, what changed today, where it should go,
and the hard decisions that need to be made. It supersedes the earlier audit from this morning.

---

## I. What Sigil Is Now

Sigil is an AI company orchestrator. You tell it what you want. It routes to the right agents,
executes via Claude Code, learns from results, and reports back.

**Backend (sigil — Rust, 9 crates, 217 tests):**
- Long-running daemon with supervisor patrol loop
- Task DAGs, missions, dependency inference
- Expertise routing (right agent for right task)
- Preflight assessment before execution
- Memory per project (SQLite + FTS5 + vector embeddings)
- Blackboard for ephemeral inter-agent knowledge
- Cost tracking with per-project budgets
- Audit log, dispatch bus, watchdogs, cron jobs
- ChatEngine: source-agnostic message processing (web, Telegram, future channels)
- Web API crate (Axum REST + WebSocket + JWT auth)
- MCP command support
- Execution via Claude Code subprocess

**Frontend (sigil-ui — Vite + React 19 + TypeScript + Zustand):**
- Chat-first UI: conversation is the home surface at /
- Collapsible sidebar with channel hierarchy + compact navigation
- Status bar: daemon health, active workers, budget, Cmd+K
- Command palette for navigation
- Context panel: dashboard overview (global) or project tasks/knowledge (project channel)
- Breadcrumb bar consistent across all pages
- 17 pages: dashboard, projects, agents, tasks, missions, operations, knowledge, etc.
- Dark theme: black/ivory/bronze palette, Inter + JetBrains Mono

**Brand:**
- Name: Sigil
- Domain: sigil.ceo
- Category: Proactive AI
- Tagline: Wake up ahead.
- One-liner: AI that works while you sleep.
- Voice: calm, direct, zero fluff. Your shadow doesn't small-talk.

**Live at:** entity.business (self-hosted, nginx → sigil-web on :8400)

---

## II. What Changed Today

### Backend
- New `sigil-web` crate (Axum REST API + WebSocket + JWT auth)
- New `ChatEngine` (1023 lines) — unified source-agnostic chat processing
- 20+ new daemon IPC commands (tasks, missions, memories, skills, pipelines, knowledge, agent files, chat)
- MCP command added to CLI
- Legacy agents retired to .retired-agents/
- Clippy clean, cargo fmt, all 217 tests passing
- Repo hygiene: fixed URLs, added crate descriptions, LICENSE

### Frontend
- Chat became the home page (/)
- Dashboard moved to /dashboard
- Status bar with branding, daemon health, metrics, Cmd+K
- Command palette (Cmd+K) for instant navigation
- Collapsible sidebar (Cmd+B) — icon-only mode at 48px
- Context panel: global overview or project-scoped tasks/knowledge
- Breadcrumb bar: unified across all pages including chat
- Chat rewritten: memoized bubbles, smart time grouping, copy-on-hover,
  auto-growing composer, scroll-to-bottom, empty state with suggestions,
  drag-and-drop zone, message status tracking
- Sigil is the identity (not Rei) — "Your shadow awaits"
- Body font switched from monospace to Inter (mono for code only)
- 27 missing CSS classes defined for card components
- Pushed to github.com/0xAEQI/sigil and github.com/0xAEQI/sigil-ui

---

## III. What's Actually Good

1. **The orchestration substrate is real.** Daemon, supervisor, workers, task DAGs, memory,
   expertise routing, audit — these are not stubs. They work together. The system runs overnight
   and produces results.

2. **The architecture is correct.** Chat as home, context as side panel, channels as project
   scope — this is the right information architecture for an orchestrator.

3. **The brand is clear.** "Proactive AI" is an unclaimed category. "Wake up ahead" is a real
   feeling people want. sigil.ceo is memorable and self-explanatory.

4. **The backend is production-quality.** 9 Rust crates, 217 tests, clippy clean. Not a prototype.

5. **The dual-path chat is a strong pattern.** Quick path for instant responses, full path for
   agent execution with polling. This is the right split for UX — most requests resolve instantly,
   complex work gets handed off without blocking.

---

## IV. What's Actually Weak

### 1. The system is not proven end-to-end

The biggest risk. Sigil has all the pieces, but there is no evaluation harness that proves:
- Routing quality: does the right agent get the right task?
- Execution quality: do tasks complete correctly?
- Learning quality: does the system get better over time?
- Recovery quality: does BLOCKED/FAILED/HANDOFF work correctly?

Without this, Sigil is a promising architecture, not a proven product.

### 2. The proactive promise is unfulfilled

Sigil's entire brand is "proactive AI." But today:
- The morning brief exists but is basic
- No push notifications (Telegram/WhatsApp/email)
- No "I noticed X, should I do Y?" behavior
- No anomaly detection or drift alerts to the user
- The user still initiates everything

This is the highest-leverage gap. The moment Sigil messages YOU before you message it —
that's the product-market-fit moment.

### 3. Notes/directives don't exist yet

The core product insight — "your notes become reality" — has no implementation. Today:
- `note:` prefix in chat stores to memory + blackboard (good primitive)
- KnowledgePage lets you browse/create knowledge entries
- But there is no "living notepad" where you write what you want and it manifests

This is the differentiator that makes Sigil a product, not just a tool.

### 4. Hosted version doesn't exist

Sigil requires self-hosting (VPS, Rust build, systemd). This limits the audience to
exactly one person (the builder). For sigil.ceo to be a product people pay for:
- Cloud-hosted daemon per user
- Zero-setup onboarding ("What are you working on?")
- WhatsApp/Telegram as entry point (no app download)

### 5. Worker resumability is still brittle

When a worker restarts or fails mid-task, the reconstruction of context is incomplete.
The system needs a first-class "prime bundle" that reconstructs:
- Task state + history
- Relevant memory
- Recent audit trail
- Unread dispatches
- Blackboard context
- Previous checkpoint evidence

---

## V. The Notes System — Design Direction

### The Insight

Every note-taking app has the same problem: notes are dead. You write "redesign the landing page"
and it sits there, staring at you. Obsidian, Notion, Apple Notes — they're all graveyards of intent.

Sigil's notes are alive. You write it, it manifests. The note IS the sigil.

### How It Fits

```
Chat (left)              Notes (right — context panel evolution)
─────────────            ────────────────────────────────────────
Ephemeral conversation   Persistent directives
"what's the status?"     "Q2: launch pricing, fix bot, 100 users"
"deploy the fix"         Each line gets: ○ pending ⟳ active ✓ done
Back-and-forth           Living document that updates itself
```

Chat and notes are complementary:
- Chat is how you talk to Sigil in the moment
- Notes are what you want to be true over time
- Chat messages can become notes ("pin this")
- Notes can trigger chat responses ("I started working on line 3")

### Implementation Plan

**Phase 1: Editable context panel**
- The right panel gets a "Notes" tab alongside existing context
- Per-channel markdown textarea, persisted via existing memory API
- Plain text editing, no special syntax required
- Saved automatically on blur/pause

**Phase 2: Directive detection**
- Lines that look like imperatives get highlighted
- Status indicators appear: ○ pending, ⟳ in progress, ✓ done, ✗ failed
- Status comes from matching notes to existing tasks (fuzzy match on subject)
- One-click "activate" button to turn a note line into a task

**Phase 3: Proactive notes**
- Sigil suggests notes based on conversation ("You mentioned X — want me to track it?")
- Notes auto-update when tasks complete
- Morning brief populates with overnight changes to note status
- Telegram: "note: ship pricing by Friday" → appears in web notes panel

**Phase 4: Shared notes / collaboration**
- Notes visible to agents during execution (injected into context)
- Agents can annotate notes with findings
- Notes become the source of truth for project direction

### How It Differs From Obsidian

| Obsidian | Sigil Notes |
|----------|-------------|
| Local markdown files | Per-project, channel-scoped |
| Static graph | Living directives with status |
| You organize | Sigil infers context from channel |
| You link | Sigil connects to tasks/agents/knowledge automatically |
| You act on notes | Notes act on themselves |
| Plugin ecosystem | Agents ARE the plugins |

The key difference: Obsidian is a vault. Sigil is a will. You write your will,
Sigil executes it.

---

## VI. Product Roadmap

### Now (weeks)
1. **Notes panel** — editable context panel with per-channel persistence
2. **Polish the chat** — suggestion pills wired up, better markdown rendering
3. **Push notifications** — morning brief via Telegram, task completion alerts
4. **End-to-end eval** — prove routing + execution + recovery works

### Next (months)
5. **Directive detection** — notes become live with status indicators
6. **Proactive behavior** — Sigil initiates conversations ("I noticed X")
7. **WhatsApp integration** — meet users where they are
8. **Landing page** — sigil.ceo with signup, waitlist, brand story
9. **Worker resumability** — first-class prime bundles

### Later (quarters)
10. **Hosted version** — cloud daemon per user, zero setup
11. **Free tier** — quick path only, 10 executions/month
12. **Paid tiers** — $49 pro, $199 founder
13. **Mobile PWA** — add to home screen
14. **API access** — developers build on Sigil

---

## VII. Architecture Decisions Still Open

### 1. Notes storage: where?

Options:
- **A) Extend memory system** — notes are just memory entries with a "note" category.
  Pro: reuses existing infra. Con: memory is key-value, not a document.
- **B) New notes table per project** — `notes.db` alongside `memory.db`.
  Pro: proper document model (id, channel, content, updated_at).
  Con: another storage layer.
- **C) Markdown files on disk** — `projects/{name}/.sigil/notes/`.
  Pro: git-friendly, Obsidian-compatible. Con: no real-time sync.

Recommendation: **B**. Notes are documents, not key-value pairs. They need proper
document semantics (full content replacement, versioning, channel scoping).
But store them in the existing project `.sigil/` directory for locality.

### 2. Note → task matching: how?

When a note says "fix the trading bot" and a task exists with subject "fix trading bot
parameter overshoot", how do they link?

Options:
- **Exact match** — note line must match task subject. Too brittle.
- **Fuzzy match** — embedding similarity between note lines and task subjects. Better.
- **Explicit link** — user clicks "activate" to create task, note stores task ID. Cleanest.
- **Hybrid** — explicit link when user activates, fuzzy match as suggestion otherwise.

Recommendation: **Hybrid**. Start with explicit (user clicks to activate), add fuzzy
suggestion overlay in phase 2.

### 3. Hosted architecture: how?

For sigil.ceo as a multi-tenant product:
- Each user gets an isolated daemon? (expensive but simple)
- Shared daemon with tenant isolation? (complex but efficient)
- Serverless execution with persistent state? (modern but unproven for long-running orchestration)

Recommendation: **Isolated daemon per user** to start. One small VM or container per paying
customer. Complexity of multi-tenancy isn't worth it until 100+ users. At $49-199/mo per
user, the unit economics work even with dedicated instances.

---

## VIII. What Matters Most Right Now

In priority order:

1. **Make the proactive loop work.** One morning brief via Telegram that actually tells you
   what happened overnight. This is the product-market-fit moment. Everything else is a tool.
   This is the CEO.

2. **Build the notes panel.** Editable context panel, persisted per channel. No directive
   detection yet — just let people write. The "notes that become real" story starts with
   "you can write notes in Sigil."

3. **Prove the orchestration works.** Build 5 real end-to-end scenarios and run them repeatedly.
   Does routing work? Does execution complete? Does retry work? Does memory help? If yes,
   you have a product. If no, fix it before adding features.

4. **Ship the landing page.** sigil.ceo needs to exist. "Proactive AI. Wake up ahead."
   Email signup. The brand is ready. The product is usable. Start collecting interest.

Everything else is a distraction until these four things are done.

---

## IX. The Hard Truth

Sigil has more architecture than proof. More ambition than validation. More backend than product.

That's normal for this stage. But the risk is clear: building more features on top of
unproven orchestration is building on sand.

The path from here is not "add more things." It's:

1. Prove what exists works (eval)
2. Make it feel alive (proactive + notes)
3. Let other people try it (hosted + landing page)
4. Charge for it

The architecture is there. The UI is getting there. The brand is there.
Now prove it works, and ship it.
