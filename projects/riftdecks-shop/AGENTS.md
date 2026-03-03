# Operating Instructions

Inherits from `projects/shared/WORKFLOW.md` for git workflow, code standards, R→D→R pipeline, and escalation.

## Build & Deploy

- Build: `npm run build`
- Dev: `npm run dev`
- Deploy: merge to `master` → post-merge hook (next build + systemctl restart)
- Service: `riftdecks.service` (port 3200 internal)
- Infrastructure: Next.js at https://riftdecks.shop, Redis localhost:6379
- Analytics: Plausible at https://analytics.riftdecks.shop (port 8000)
- **WARNING: Stripe is LIVE** — test purchases charge real money

## RiftDecks-Specific Workflow

1. Work in worktrees: `git worktree add ~/worktrees/riftdecks-shop/feat/<name> -b feat/<name>`
2. Test: `npm run dev` locally
3. Build: `npm run build` (must pass before commit)
4. Merge to `master` → auto-deploys via post-merge hook (next build + systemctl restart)

## Available Skills

### R→D→R Archetypes (project-specific overrides)
- **researcher**: Next.js/React codebase exploration — components, API routes, data flow, Stripe integration
- **developer**: Next.js implementation — component patterns, Stripe checkout, Redis inventory
- **reviewer**: E-commerce code review — security (client-side trust), inventory atomicity, Stripe webhook handling

## Critical Rules

- NEVER trust client-side prices — recompute server-side from TCGCSV
- NEVER SET inventory higher than INITIAL_STOCK — releaseUnit() caps at it
- Stripe is LIVE — test purchases charge real money
- .env.local is gitignored — never commit Stripe keys
- Redis inventory must use atomic DECR/INCR for reserve/release
- Plausible script.js proxied through Next.js rewrites (adblocker bypass)
