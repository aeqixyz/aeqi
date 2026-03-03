# entity.legal — Project Knowledge

## Overview

Web3-native legal entity formation service combining Marshall Islands DAO LLC jurisdiction with on-chain cap tables on Solana. Domain: **entity.legal** (live, hosted).

## Current State

- **Landing page**: Live at https://entity.legal — Next.js 16, React 19, Tailwind, Framer Motion
- **Source**: `projects/entity-legal/web/` (not a separate repo — lives inside sigil)
- **Hosting**: Static export served via nginx on 5.9.83.245, Let's Encrypt SSL
- **DNS**: Porkbun, managed via `scripts/porkbun-deploy.sh`
- **SEO content**: 10+ article pages (DAO LLC banking, compliance, tax, Marshall Islands, etc.)
- **Waitlist**: Redis-backed waitlist modal on landing page
- **Status**: Landing page + SEO content live. No backend services yet.

## Tech Stack

| Layer | Tech |
|-------|------|
| Framework | Next.js 16 (App Router) |
| UI | React 19, Tailwind CSS 4, Framer Motion |
| Icons | Lucide React |
| Waitlist | Redis |
| Hosting | nginx + static export on Hetzner (5.9.83.245) |
| SSL | Let's Encrypt via certbot |
| DNS | Porkbun API |
| Domain | entity.legal |

## Directory Structure

```
projects/entity-legal/
  AGENTS.md           — operating instructions
  KNOWLEDGE.md        — this file
  scripts/
    porkbun-deploy.sh — DNS deployment automation
    nginx-entity-legal.conf — production nginx config
  research/
    landing-page-spec.md    — full landing page specification
    strategy-unified.md     — business strategy document
    marshall-islands-dao-law.md — legal research
    solana-dao-architecture.md  — technical architecture
  web/                — Next.js landing site (the actual codebase)
    src/app/          — pages (landing, articles, legal, API)
    src/components/   — Hero, Footer, WaitlistModal, Article, EntityCard
    src/lib/          — config, types
```

## Key Routes

| Route | Purpose |
|-------|---------|
| `/` | Landing page with Hero + Waitlist CTA |
| `/learn` | Educational content hub |
| `/marshall-islands-dao-llc` | SEO article |
| `/dao-llc-banking` | SEO article |
| `/dao-llc-compliance` | SEO article |
| `/dao-llc-tax` | SEO article |
| `/series-dao-llc` | SEO article |
| `/ai-agent-legal-entity` | SEO article |
| `/offshore-dao-myths` | SEO article |
| `/dao-llc-vs-wyoming` | SEO article |
| `/docs` | Documentation |
| `/privacy`, `/terms` | Legal pages |
| `/v1` | API v1 placeholder |

## Entity Types (Product)

| Type | Tax | Best For |
|------|-----|----------|
| DAO LLC (For-Profit) | 3% GRT | DeFi, funds, Web3 startups |
| Non-Profit DAO | Zero | Foundations, grants DAOs |
| Series DAO LLC | Per-series | Multi-product DAOs, venture studios |
| Traditional LLC | Standard MI rates | Solo founders, holding companies |

## Build & Deploy

```bash
cd projects/entity-legal/web
npm run build        # Next.js build
npm run dev          # Local dev server

# DNS deployment
cd projects/entity-legal/scripts
PORKBUN_API_KEY=xxx PORKBUN_SECRET_KEY=xxx ./porkbun-deploy.sh deploy
```

## Critical Rules

- All legal documents require human review before deployment
- Handle all personal data according to privacy regulations
- Never auto-deploy legal content without Emperor approval
- Never expose PII in logs, error messages, or client-side code
