# entity.legal — Unified Strategy Document

**Date**: 2026-02-23
**Status**: Ready for Architect Review

---

## Executive Summary

entity.legal is a Web3-native legal entity formation service that combines Marshall Islands DAO LLC jurisdiction with on-chain cap tables on Solana. No other service offers this specific combination: sovereign jurisdiction + legally-binding on-chain equity records + self-custodial wallet infrastructure.

This document synthesizes research across legal, technical, and product dimensions into a single execution plan.

---

## 1. Legal Architecture

### Entity Types Offered

| Type | Tax | Distribution | Best For |
|------|-----|-------------|----------|
| **DAO LLC (For-Profit)** | 3% GRT (excl. cap gains) | Allowed | DeFi, funds, Web3 startups |
| **Non-Profit DAO** | Zero | Prohibited | Foundations, grants DAOs, public goods |
| **Series DAO LLC** | Per-series | Per-series | Multi-product DAOs, venture studios |
| **Traditional LLC** | Standard MI rates | Allowed | Solo founders, holding companies |

### Marshall Islands Advantages

- Smart contract IS the legal membership registry (DAO Act 2022, codified 2023)
- No US securities registration required
- Governance tokens with no economic rights explicitly NOT securities
- Open-source software liability immunity
- Delaware precedent for LLC law (century of case law)
- No requirement for directors, officers, or managers
- Member anonymity (except 25%+ governance holders for KYC)
- Zero tax for non-profits; 3% GRT for for-profit (no cap gains tax)

### Legal Hierarchy (Critical)

1. DAO Act and RMI law (highest)
2. Conventional LLC law
3. Written Operating Agreement
4. Smart Contract Code (lowest)

Written operating agreement always overrides smart contract in case of conflict. This is a feature — it provides legal certainty while enabling on-chain governance.

### Formation Requirements

1. Entity name + type selection
2. KYC for members with 25%+ governance rights
3. Smart contract address (publicly available identifier)
4. Operating agreement designating smart contract as membership registry
5. Registered agent in Marshall Islands
6. Filing with Registrar of Corporations (7-30 days)

### Ongoing Compliance

- Annual report filing
- Registered agent maintenance ($1,200-1,800/year)
- Beneficial ownership updates if membership changes
- 3% GRT filing (for-profit only)

---

## 2. Technical Architecture

### On-Chain Stack (Solana)

```
┌──────────────────────────────────────┐
│  LEGAL ENTITY                        │
│  Marshall Islands DAO LLC            │
│  Operating agreement references      │
│  smart contract address              │
├──────────────────────────────────────┤
│  DAO GOVERNANCE                      │
│  Squads Protocol v4 Multisig         │
│  M-of-N approval, time locks,       │
│  spending limits, role-based access  │
├──────────────────────────────────────┤
│  CAP TABLE PROGRAM (Anchor)          │
│  Entity → ShareClass → Position      │
│  PDAs for metadata + restrictions    │
│  Token-2022 SPL tokens for equity    │
├──────────────────────────────────────┤
│  SOLANA L1                           │
│  Sub-second finality, ~$0.001/tx    │
│  Token-2022 extensions for           │
│  transfer hooks + confidential xfers │
└──────────────────────────────────────┘
```

### Hybrid Token + PDA Architecture

- **Token-2022 SPL tokens**: Each share class = one token mint. Fungible, composable, visible in wallets/explorers.
- **PDAs**: Metadata, vesting schedules, transfer restrictions, governance config. The non-fungible control layer.
- **Squads Multisig**: Vault PDA is the `authority` on the Entity account. All critical operations require multisig approval.

### Key Token-2022 Extensions

| Extension | Purpose |
|-----------|---------|
| Transfer Hook | Enforce transfer restrictions (lock-up, accredited-only, jurisdiction) |
| Confidential Transfers | Privacy for equity positions |
| Permanent Delegate | Forced transfer for legal compliance (court orders) |
| Non-Transferable | Lock founder shares during vesting |
| Metadata | On-chain share class details |
| Interest-Bearing | Dividend/yield representation |

### Account Abstraction (Without Privy)

Solana doesn't have EVM-style AA. Our approach:

1. **Squads Smart Accounts** — Rent-free wallet creation, passkey support, programmable policies. Members get Squads Smart Accounts instead of raw keypairs.
2. **Session Keys** — Temporary authorized keys for limited-scope actions without full wallet signing.
3. **PDA-Based Smart Accounts** — Program-owned accounts validated via alternative identity (social login, email hash, passkey).
4. **Web3Auth/Turnkey** — Fallback for users who need social login. Non-custodial MPC key management.

Priority order: Squads Smart Accounts > Session Keys > Web3Auth > Turnkey

### Upgrade Safety

- Program deployed with temporary keypair as upgrade authority
- Transfer upgrade authority to Squads Vault PDA
- All upgrades require multisig approval (e.g., 3-of-5)
- CI/CD via Squads GitHub Action for automated proposal creation
- Consider eventual immutability (remove upgrade authority) once stable

### Cost Analysis (Solana Mainnet)

| Operation | Cost |
|-----------|------|
| Entity creation (all PDAs) | ~0.05 SOL (~$7.50) |
| Share class creation | ~0.01 SOL |
| Member registration | ~0.005 SOL |
| Share issuance (token mint) | ~0.001 SOL |
| Share transfer | ~0.001 SOL |
| Governance proposal | ~0.005 SOL |
| Vote recording | ~0.001 SOL |

Total entity setup: ~0.1 SOL (~$15). Negligible compared to formation fees.

---

## 3. Product & Landing Page

### Brand Position

"Sovereign entity formation for the on-chain era."

### Target Audience

1. **DAO Founders** (25-40): Need legal wrapper, frustrated by US ambiguity, technically literate
2. **Crypto Fund Managers** (30-50): Need Series LLC, care about cap table accuracy, will pay premium
3. **Protocol Teams** (mixed): Need entity for grants/contracts/hiring, want governance tied to tokens

### Pricing

| Tier | Formation | Annual Renewal | Includes |
|------|-----------|----------------|----------|
| Starter | $5,500 | $1,800/yr | DAO LLC + cap table + 3-signer multisig |
| Pro | $8,500 | $2,800/yr | + Series LLC (5) + custom governance + 7 signers |
| Enterprise | Custom | Custom | Unlimited series + dedicated support + custom program |

### Competitive Pricing Context

- MIDAO (direct competitor): $5,000 formation, $2,000/yr
- Wyoming DAO LLC: $100 filing + legal fees ($2,000-5,000)
- Cayman Foundation: $15,000-25,000 formation
- BVI Company: $10,000-15,000 formation

Our pricing is competitive with MIDAO while offering significantly more (on-chain infrastructure included).

### Landing Page

Full spec at `landing-page-spec.md` — 600+ lines of detailed design, copy, and interaction specs. Key sections:

1. Hero with provocative "cap table lives on-chain" hook
2. Three-layer interactive stack diagram (Solana → DAO → Legal Entity)
3. Six feature cards with complete copy
4. Four-step timeline (5 min → 1-3 days → 7-10 days → sovereign)
5. Four entity type cards with "best for" tags
6. Three-tier transparent pricing
7. Waitlist with live on-chain transaction counter
8. Full legal disclaimers referencing DAO Act

Tech stack: Next.js 14 + Tailwind + Framer Motion. Dark theme (#0A0A0F base, #6C3CE1 purple accent, #14F195 Solana green).

---

## 4. Infrastructure & Deployment

### Domain: entity.legal

**Porkbun API client**: Ready at `scripts/porkbun-deploy.sh`
- Full DNS management (A, CNAME, TXT records)
- One-command deployment: `./porkbun-deploy.sh deploy`
- Dry-run mode for testing
- DNS propagation verification
- SSL via Let's Encrypt

**Nginx config**: Ready at `scripts/nginx-entity-legal.conf`
- HTTPS with modern TLS (1.2+1.3)
- Security headers (HSTS, CSP, X-Frame-Options, etc.)
- Gzip compression
- Static file caching (30 days)
- SPA support (try_files → /index.html)

### Deployment Steps (when API key arrives)

```bash
export PORKBUN_API_KEY=pk1_xxx
export PORKBUN_SECRET_KEY=sk1_xxx
./porkbun-deploy.sh deploy
```

Then on server:
```bash
sudo cp nginx-entity-legal.conf /etc/nginx/sites-available/entity-legal
sudo ln -sf /etc/nginx/sites-available/entity-legal /etc/nginx/sites-enabled/
sudo mkdir -p /var/www/entity-legal
sudo certbot --nginx -d entity.legal -d www.entity.legal
sudo nginx -t && sudo systemctl reload nginx
```

---

## 5. Execution Roadmap

### Phase 1: Landing Page (This Week)
- [ ] Receive Porkbun API key → deploy DNS
- [ ] Scaffold Next.js project from landing page spec
- [ ] Build hero, architecture diagram, features sections
- [ ] Build pricing, entity types, how-it-works sections
- [ ] Waitlist with email + wallet capture
- [ ] Deploy to server

### Phase 2: Legal Templates (Week 2)
- [ ] Draft template operating agreement (DAO LLC)
- [ ] Draft template operating agreement (Non-Profit DAO)
- [ ] Draft template operating agreement (Series LLC)
- [ ] Research MIDAO registered agent partnership
- [ ] KYC provider integration (Persona/Sumsub)

### Phase 3: Smart Contracts (Weeks 3-4)
- [ ] Anchor program: Entity + ShareClass + MemberRecord
- [ ] Token-2022 mint integration per share class
- [ ] Transfer hook for restrictions
- [ ] Squads multisig integration
- [ ] Vesting schedule implementation
- [ ] Testnet deployment + audit prep

### Phase 4: Platform (Weeks 5-8)
- [ ] Formation wizard (entity type → KYC → documents → deploy)
- [ ] Dashboard (cap table view, governance, compliance calendar)
- [ ] Squads UI integration for governance actions
- [ ] Payment processing (USDC + fiat)
- [ ] Admin panel for managing formations

### Phase 5: Launch (Week 9+)
- [ ] Security audit of smart contracts
- [ ] Legal review of template agreements
- [ ] Beta with 5-10 pilot entities
- [ ] Public launch
- [ ] Content marketing (blog, Twitter threads, DAO directories)

---

## 6. Open Questions (For Architect Decision)

1. **MIDAO Partnership**: Partner with MIDAO as registered agent, or find independent agent in MI? MIDAO is the dominant player but means dependency on a competitor.

2. **Smart Contract Audit**: Who audits? Options: OtterSec (Solana specialist), Halborn, Trail of Bits. Budget: $30K-80K depending on scope.

3. **Payment**: Accept USDC on Solana for formation fees? Or fiat-only via Stripe? Both?

4. **Legal Review**: Engage MI-licensed attorney to review template operating agreements? Recommended but adds cost + time.

5. **Waitlist Target**: How many waitlist signups before we commit to Phase 3 (smart contracts)?

---

## Deliverables Summary

| File | Size | Content |
|------|------|---------|
| `research/marshall-islands-dao-law.md` | 53KB | 20-section comprehensive legal research |
| `research/solana-dao-architecture.md` | 49KB | 11-section technical architecture |
| `research/landing-page-spec.md` | 58KB | Complete landing page design + copy |
| `research/strategy-unified.md` | This file | Synthesized strategy + roadmap |
| `scripts/porkbun-deploy.sh` | 15KB | Production-ready DNS deployment |
| `scripts/nginx-entity-legal.conf` | 3KB | Production nginx config |
