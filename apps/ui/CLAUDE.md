# Sigil Web Dashboard

Frontend for the Sigil AI agent orchestration framework. Vite + React 19 + Zustand + TypeScript.

## Stack

- **Build:** Vite 6, React 19, TypeScript 5
- **State:** Zustand (auth store, daemon store)
- **Routing:** React Router v7
- **Styling:** CSS custom properties in `src/styles/tokens.css` (dark theme, black/ivory/bronze palette, JetBrains Mono + Inter)
- **API:** `src/lib/api.ts` — fetch wrapper with JWT auth, auto-redirect on 401

## Pages (12)

| Page | Path | What it does |
|------|------|-------------|
| Dashboard | `/` | Daily brief, animated stats, live activity feed, budget bar, project/agent overview |
| Projects | `/projects` | Project cards with team, task counts, progress bars |
| Project Detail | `/projects/:name` | Hero stats + tabs (tasks/missions/audit) + sidebar (team, progress) |
| Agents | `/agents` | Agent cards with role, model, expertise tags |
| Agent Detail | `/agents/:name` | Identity, expertise scores, active work, recent activity |
| Tasks | `/tasks` | Create/close tasks, filter by status/project, priority bars |
| Missions | `/missions` | Mission cards with progress |
| Operations | `/operations` | Tabs: Scheduled (9 cron jobs), Watchdogs (5 rules), Activity (audit trail) |
| Chat | `/chat` | Hierarchical channels (global → project → department), Rei avatar, typing indicator, command execution |
| Blackboard | `/blackboard` | Knowledge entries with post form |
| Cost | `/cost` | Budget visualization, per-project breakdown |
| Settings | `/settings` | Daemon connection, logout |

## Chat Architecture

The chat has two paths:
1. **Quick path** (`/api/chat`): instant responses for intents ("create task...", "note:...", "close task...") and status queries
2. **Full path** (`/api/chat/full` + `/api/chat/poll/{id}`): spawns agent execution via Claude Code, polls for completion

Channel sidebar shows the project hierarchy. Header shows which agents are in the current channel context.

Messages persist to localStorage (last 100). Session ID persists for conversation continuity.

## Deployment

```bash
cd /home/claudedev/sigil/apps/ui
npm run build
```

Preferred production setup:
- Build the UI in `apps/ui/dist`
- Set `[web].ui_dist_dir` in `sigil.toml`
- Run `sigil web start`
- Put a thin reverse proxy in front for TLS and host routing

Services: `sigil.service` (daemon), `sigil-web.service` (API server)

## Dev

```bash
npm run dev  # Vite dev server on :5173, proxies /api to :8400
```
