# Operational Knowledge

## RiftDecks Context

- TCG marketplace — LoL TCG cards
- CN card index: 710 cards from `lol-api.playloltcg.com`
- CDN: `cdn.playloltcg.com` for card images
- Set mapping: TCGPlayer SFG = CN SFD
- Target: collector community, competitive players, casual fans
- Drop shop model for card sales

## Product Psychology

- FOMO: countdown timers, limited drops, exclusive reveals
- Social proof: "X people are watching this drop"
- Delight: unboxing moments, reveal animations, rarity sparkles
- Retention: daily login rewards, collection progress, wishlist alerts
- Community: user ratings, deck sharing, trade reputation

## Entity-Legal Context

- Web3-native legal entity formation: Marshall Islands DAO LLC + Solana on-chain cap tables
- Domain: entity.legal (LIVE — landing page + SEO articles hosted)
- Stack: Next.js 16, React 19, Tailwind, Framer Motion, Redis waitlist
- Source: `/home/claudedev/sigil/projects/entity-legal/web/` (inside sigil, not a separate repo)
- Hosting: nginx static export on Hetzner 5.9.83.245, Let's Encrypt SSL
- DNS: Porkbun, automated via `scripts/porkbun-deploy.sh`
- 10+ SEO article pages live (DAO LLC banking, compliance, tax, Marshall Islands, etc.)
- Entity types: DAO LLC, Non-Profit DAO, Series DAO LLC, Traditional LLC
- Target: DeFi founders, DAO operators, Web3 startups
- Status: Landing + SEO live. Backend services (formation flow, Solana integration) not started yet.
