# entity.legal Landing Page Specification

**Version**: 1.0
**Date**: 2026-02-23
**Status**: Ready for Development
**Author**: EL-Agent / Emperor

---

## Table of Contents

1. [Brand Position & Voice](#1-brand-position--voice)
2. [Hero Section](#2-hero-section)
3. [Three-Layer Architecture Visual](#3-three-layer-architecture-visual)
4. [Key Features Section](#4-key-features-section)
5. [How It Works](#5-how-it-works)
6. [Entity Types Section](#6-entity-types-section)
7. [Pricing Section](#7-pricing-section)
8. [Trust & Social Proof Section](#8-trust--social-proof-section)
9. [Legal Disclaimers](#9-legal-disclaimers)
10. [Footer](#10-footer)
11. [Technical Specifications](#11-technical-specifications)
12. [Waitlist / Lead Capture](#12-waitlist--lead-capture)
13. [Component Inventory](#13-component-inventory)
14. [Animation & Interaction Spec](#14-animation--interaction-spec)
15. [SEO & Meta](#15-seo--meta)

---

## 1. Brand Position & Voice

### Tagline
**"Sovereign entity formation for the on-chain era."**

### Brand Voice Rules
- **Sovereign**: We speak as equals to builders, not as service providers begging for business. No "we'd love to help you" energy. It's "here's what you need, here's how to get it."
- **Technical but accessible**: Use real terminology (SPL tokens, multisig, DAO LLC) but never assume the reader has formed an entity before. Explain with confidence, not condescension.
- **Slightly rebellious**: We are not anti-regulation. We are anti-unnecessary-regulation. We respect legal structure. We reject regulatory theater.
- **Direct**: Short sentences. Active voice. No hedging. No "we believe" or "we think" -- just state facts.
- **Zero fluff**: Every sentence earns its place. If a line doesn't inform, persuade, or direct action, it gets cut.

### Words We Use
- sovereign, on-chain, verifiable, permissionless, programmable, native, formation, structure, jurisdiction, governance, transparent, immutable

### Words We Avoid
- "compliance-first" (implies fear), "seamless" (meaningless), "leverage" (corporate speak), "empower" (patronizing), "revolutionary" (overused), "synergy" (immediate credibility loss), "blockchain" as a standalone buzzword

### Audience Personas

**Primary: The DAO Founder (25-40)**
- Has raised or is raising capital on-chain
- Needs a legal wrapper yesterday
- Frustrated by US regulatory ambiguity
- Technically literate, legally naive
- Makes decisions fast, hates long sales cycles

**Secondary: The Crypto Fund Manager (30-50)**
- Managing a Solana-native fund
- Needs Series LLC for portfolio isolation
- Cares deeply about cap table accuracy
- Has been burned by custody middleware
- Will pay premium for quality

**Tertiary: The Protocol Team (mixed ages)**
- Building a Solana protocol
- Needs entity for grants, contracts, hiring
- Wants governance tied to token holdings
- Values transparency and verifiability

---

## 2. Hero Section

### Layout
Full viewport height (100vh). Dark background (#0A0A0F). Content centered vertically with slight upward offset (40% from top). Maximum content width: 800px centered.

### Background Treatment
- Base: solid #0A0A0F
- Subtle radial gradient: from center, #1A0A2E (very faint purple haze) fading to transparent at 60% viewport width
- Animated particle field: extremely sparse (15-20 particles), slow drift, color #6C3CE1 at 8% opacity. These represent "on-chain transactions" -- subtle, ambient, not distracting.
- A single thin horizontal line at ~70% viewport height, color #1E1E2E, representing "the chain" -- particles drift along it.

### Content

**Eyebrow Text** (above headline, small caps, tracked wide):
```
MARSHALL ISLANDS DAO LLC + SOLANA
```
Style: font-size 13px, letter-spacing 4px, color #6C3CE1, font-weight 500, text-transform uppercase.

**Headline**:
```
Your cap table lives on-chain.
Your investors verify in seconds.
Your company exists in a jurisdiction
that respects your sovereignty.
```
Style: font-size 48px desktop / 32px mobile, line-height 1.2, font-weight 700, color #F5F5F7. Each line break is intentional -- do not reflow. On mobile, "Your company exists in a jurisdiction" and "that respects your sovereignty." can wrap naturally but should not be joined with the line above.

**Sub-headline**:
```
entity.legal forms Marshall Islands DAO LLCs with on-chain cap tables on Solana. Real legal structure. No US regulatory overhead. Your keys, your entity.
```
Style: font-size 18px desktop / 16px mobile, line-height 1.6, color #A0A0B0, max-width 600px, margin-top 24px.

**CTA Group** (horizontal on desktop, stacked on mobile, margin-top 40px):

Primary CTA button:
```
Form Your Entity →
```
Style: background #6C3CE1, color #FFFFFF, font-size 16px, font-weight 600, padding 16px 32px, border-radius 8px, hover: background #7D4FE8, transition 200ms. The arrow is a Unicode right arrow (U+2192), not an icon.

Secondary CTA link:
```
See How It Works ↓
```
Style: color #6C3CE1, font-size 14px, font-weight 500, no background, no border, underline on hover. Smooth-scrolls to Section 5 (How It Works). The down arrow is Unicode (U+2193).

**Social Proof Line** (below CTAs, margin-top 32px):
```
47 entities formed  ·  $12M+ in on-chain cap tables  ·  0 custody incidents
```
Style: font-size 13px, color #606070, letter-spacing 0.5px. Numbers should be pulled from a live API endpoint or environment variable so they can be updated without code changes. The middle dot (U+00B7) separators have 12px horizontal padding.

### Scroll Indicator
At the bottom of the viewport (margin-bottom 32px), a subtle bouncing chevron (V shape) in #606070. Bounces with a 2s ease-in-out animation, 8px travel. Fades out once user scrolls past 100px.

---

## 3. Three-Layer Architecture Visual

### Section Layout
Full-width section. Background #0D0D14. Padding: 120px vertical. Max content width: 1100px centered.

### Section Header

**Eyebrow**:
```
ARCHITECTURE
```
Style: same as hero eyebrow (13px, tracked, #6C3CE1, uppercase).

**Title**:
```
Three layers. One sovereign entity.
```
Style: font-size 36px, font-weight 700, color #F5F5F7, margin-top 12px.

**Subtitle**:
```
Your legal entity, your governance protocol, and your financial infrastructure — unified on a single stack. Each layer is independent. Together, they're unstoppable.
```
Style: font-size 16px, color #A0A0B0, max-width 600px, margin-top 16px.

### Interactive Stack Diagram

This is the centerpiece visual. It is NOT a flat image. It is a three-layer interactive stack built with CSS/SVG and animated with Framer Motion.

**Visual structure**: Three horizontal slabs stacked vertically with 24px gaps between them. Each slab is a rounded rectangle (border-radius 12px) with a subtle border (1px solid rgba(108, 60, 225, 0.2)).

The stack is displayed in **bottom-up order** (Solana at bottom, Legal Entity at top) to represent the foundation metaphor:

#### Layer 3 (Top) -- Jurisdiction Layer
```
┌─────────────────────────────────────────────────────────┐
│  ⚖  LEGAL ENTITY                                       │
│  Marshall Islands DAO LLC                               │
│  ──────────────────────────────────────────              │
│  Registered under the DAO Act 2022.                     │
│  Recognized legal personhood. Limited liability.         │
│  No US nexus. No securities registration.               │
│  Your smart contract address IS your membership          │
│  registry — written into the LLC agreement.             │
└─────────────────────────────────────────────────────────┘
```
Background: #12121A. Accent border-left: 3px solid #E8C547 (gold). Icon: scales of justice (Lucide: `scale`).

#### Layer 2 (Middle) -- Governance Layer
```
┌─────────────────────────────────────────────────────────┐
│  🏛  DAO GOVERNANCE                                     │
│  Squads Protocol Multisig                               │
│  ──────────────────────────────────────────              │
│  M-of-N multisig via Squads v4. Programmable            │
│  thresholds. Time locks. Spending limits.               │
│  Role-based access. Every governance action              │
│  is a signed Solana transaction — auditable,            │
│  immutable, on-chain.                                   │
└─────────────────────────────────────────────────────────┘
```
Background: #12121A. Accent border-left: 3px solid #6C3CE1 (purple). Icon: building columns (Lucide: `landmark`).

#### Layer 1 (Bottom) -- Chain Layer
```
┌─────────────────────────────────────────────────────────┐
│  ⛓  SOLANA                                              │
│  On-Chain Cap Table & Treasury                          │
│  ──────────────────────────────────────────              │
│  SPL token membership interests. Real-time              │
│  cap table. Sub-second finality. $0.001 per             │
│  transaction. Your cap table is not a spreadsheet       │
│  — it's a token account on the fastest L1 in            │
│  production.                                            │
└─────────────────────────────────────────────────────────┘
```
Background: #12121A. Accent border-left: 3px solid #14F195 (Solana green). Icon: chain link (Lucide: `link`).

### Interaction Behavior

**Default state**: All three layers visible. The currently-not-expanded layers show only the title line and a one-line summary. One layer is expanded by default (start with the bottom layer -- Solana).

**On click/tap**: Clicked layer expands to show full description. Other layers collapse to title-only. Transition: 300ms ease-out.

**Connecting lines**: Between each layer, render a thin vertical dashed line (#2A2A3A) with a small animated pulse dot traveling downward (representing data flow from chain to legal entity). The dot is #6C3CE1, 4px diameter, animates on a 3s loop.

**Desktop hover**: Layers lift slightly (translateY -2px) with a subtle box-shadow increase on hover.

### Mobile Behavior
Stack becomes full-width (padding 16px). Layers stack the same way. Click/tap to expand. Only one expanded at a time (accordion behavior).

---

## 4. Key Features Section

### Section Layout
Background: #0A0A0F. Padding: 120px vertical. Max content width: 1100px centered.

### Section Header

**Eyebrow**:
```
FEATURES
```

**Title**:
```
Built different. On purpose.
```
Style: font-size 36px, font-weight 700, color #F5F5F7.

**Subtitle**:
```
Every piece of entity.legal exists because the alternatives got it wrong. No custody middleware. No token wrappers. No regulatory theater. Just verifiable legal structure on verifiable infrastructure.
```
Style: font-size 16px, color #A0A0B0, max-width 640px, margin-top 16px.

### Card Grid

6 cards in a 3x2 grid on desktop, 2x3 on tablet, 1x6 on mobile. Card dimensions: equal width, auto height. Gap: 24px.

Card style: background #12121A, border 1px solid #1E1E2E, border-radius 12px, padding 32px. Hover: border-color transitions to the card's accent color at 40% opacity, translateY -2px, transition 200ms.

---

#### Card 1: On-Chain Cap Table

**Icon**: Table/grid icon (Lucide: `table-2`), color #14F195 (Solana green)
**Title**: `On-chain cap table`
**Body**:
```
Your membership interests are SPL tokens on Solana. Not a PDF. Not a spreadsheet someone emails you quarterly. A live, queryable, immutable record of who owns what — updated in real-time with sub-second finality.

Every transfer is a signed transaction. Every cap table entry has a block height. Your investors can verify their ownership from any Solana explorer, any wallet, any time.
```

---

#### Card 2: DAO Governance

**Icon**: Shield with checkmark (Lucide: `shield-check`), color #6C3CE1
**Title**: `DAO governance via Squads`
**Body**:
```
Governance isn't a dashboard you log into. It's an M-of-N multisig on Squads Protocol — the most battle-tested multisig on Solana, securing over $10B in assets.

Set approval thresholds. Add time locks. Define spending limits. Assign roles. Every governance action is a Solana transaction, signed and recorded forever. No black-box votes. No "trust us" governance.
```

---

#### Card 3: No Custody Middleware

**Icon**: Key icon (Lucide: `key-round`), color #E8C547 (gold)
**Title**: `Your keys, your entity`
**Body**:
```
We don't touch your keys. No Privy. No embedded wallets. No custody abstraction layer skimming fees between you and your own assets.

Connect your existing wallet. Sign your own transactions. We provide the legal wrapper and the smart contract infrastructure. You maintain absolute control. Because the moment you hand your keys to middleware, your "decentralized" entity is just a bank with extra steps.
```

---

#### Card 4: Marshall Islands DAO LLC

**Icon**: Globe icon (Lucide: `globe`), color #3B82F6 (blue)
**Title**: `Marshall Islands DAO LLC`
**Body**:
```
The Marshall Islands passed the world's first DAO-specific legislation in 2022. Not a retrofit. Not "we'll figure it out." A purpose-built legal framework that recognizes smart contracts as legitimate corporate governance instruments.

No US securities registration. No state-level regulatory patchwork. No annual Delaware franchise tax theater. A sovereign jurisdiction with a clear, stable legal framework designed for exactly what you're building.
```

---

#### Card 5: Series LLC

**Icon**: Layers icon (Lucide: `layers`), color #EC4899 (pink)
**Title**: `Series LLC support`
**Body**:
```
One parent entity. Unlimited child entities. Each series is liability-isolated — if one project fails, the others are legally walled off.

Launch a new product line, spin up a sub-DAO, or isolate a high-risk experiment — all under a single umbrella entity. Each series gets its own on-chain cap table, its own governance multisig, its own legal identity. One formation fee. Infinite optionality.
```

---

#### Card 6: Smart Contract as Legal Registry

**Icon**: File/contract icon (Lucide: `file-code-2`), color #F97316 (orange)
**Title**: `Smart contract IS the legal registry`
**Body**:
```
Your LLC agreement names a Solana smart contract address as the authoritative membership registry. Not as a "nice-to-have" digital mirror. As the legal record.

When someone buys a membership interest, the SPL token transfer IS the legally binding transfer of ownership. The blockchain is not backing up your cap table. The blockchain is your cap table. Your LLC agreement says so, the Marshall Islands recognizes it, and that's that.
```

---

## 5. How It Works

### Section Layout
Background: #0D0D14. Padding: 120px vertical. Max content width: 900px centered.

### Section Header

**Eyebrow**:
```
PROCESS
```

**Title**:
```
From zero to sovereign in four steps.
```

**Subtitle**:
```
Formation takes 10-15 business days. The on-chain infrastructure deploys the same day your entity is registered. No back-and-forth. No "we'll get back to you." A defined process with defined timelines.
```

### Timeline Visual

Vertical timeline on desktop (line on left, content on right). On mobile, same layout but full-width. The timeline line is 2px wide, color #1E1E2E, with step markers (12px circles) that light up in sequence as the user scrolls past them (Intersection Observer trigger at 50% visibility).

Active step marker: filled #6C3CE1 with a subtle glow (0 0 12px rgba(108, 60, 225, 0.4)).
Inactive step marker: border 2px solid #2A2A3A, no fill.
Completed step marker: filled #14F195.

---

#### Step 1

**Step Label**: `01`
**Title**: `Choose your structure`
**Duration Badge**: `~5 minutes`
**Body**:
```
Select your entity type: DAO LLC, Non-profit DAO, Series LLC, or Traditional LLC. Tell us about your organization — number of members, governance model, treasury size. No legal jargon required. We translate your intent into the right structure.
```
**Visual**: Small inline form mockup showing entity type selector, member count, governance model dropdown.

---

#### Step 2

**Step Label**: `02`
**Title**: `KYC and formation documents`
**Duration Badge**: `1-3 business days`
**Body**:
```
Founding members complete KYC verification (required by Marshall Islands law for members with 25%+ governance rights). We draft your LLC agreement with your Solana smart contract address named as the official membership registry. You review. You sign.

No surprises in the documents. We publish our template LLC agreement publicly so you can review it before you even start.
```
**Visual**: Document icon with a checklist: KYC verified, LLC agreement drafted, Smart contract address registered.

---

#### Step 3

**Step Label**: `03`
**Title**: `Entity registration + on-chain deployment`
**Duration Badge**: `7-10 business days`
**Body**:
```
We file with the Marshall Islands Registrar of Corporations. Simultaneously, we deploy your Squads multisig and SPL token cap table on Solana mainnet. By the time your registration is confirmed, your on-chain infrastructure is already live.

Your registered agent in the Marshall Islands is provisioned. Your annual compliance calendar is set. Your entity exists — both legally and on-chain.
```
**Visual**: Split animation: left side shows a document with a stamp/seal animating, right side shows a Solana transaction confirming.

---

#### Step 4

**Step Label**: `04`
**Title**: `You're sovereign`
**Duration Badge**: `Day one`
**Body**:
```
Receive your Certificate of Formation, your executed LLC agreement, your Squads multisig address, and your SPL token mint address. Your cap table is live. Your governance is active. Your entity is real.

Issue membership interests by minting tokens. Transfer ownership by transferring tokens. Govern by signing multisig transactions. Everything your entity does is verifiable, auditable, and yours.
```
**Visual**: Dashboard mockup showing: entity status (active), cap table (3 members with token balances), multisig (2-of-3 configuration), next compliance date.

---

## 6. Entity Types Section

### Section Layout
Background: #0A0A0F. Padding: 120px vertical. Max content width: 1100px centered.

### Section Header

**Eyebrow**:
```
ENTITY TYPES
```

**Title**:
```
Pick your structure. We'll handle the rest.
```

**Subtitle**:
```
Every entity type comes with on-chain cap table deployment and Squads multisig governance. The legal structure changes. The infrastructure doesn't.
```

### Entity Type Cards

4 cards in a horizontal row on desktop, 2x2 on tablet, stacked on mobile. Each card has a colored top-border accent (4px).

---

#### Card 1: DAO LLC

**Accent Color**: #6C3CE1 (purple)
**Icon**: Lucide `coins`
**Title**: `DAO LLC`
**Subtitle**: `For-profit on-chain entities`
**Body**:
```
The standard formation for revenue-generating DAOs, on-chain funds, and Web3 startups. Full limited liability protection. Smart contract governance. Membership interests as SPL tokens.

Recognized under the Marshall Islands DAO Act 2022. Can hold assets, enter contracts, sue and be sued. The same legal standing as a traditional LLC — with on-chain governance baked in.
```
**Best For Tags**: `DeFi protocols` · `Investment DAOs` · `Web3 startups` · `On-chain funds`
**CTA**: `Form a DAO LLC →`

---

#### Card 2: Non-Profit DAO

**Accent Color**: #14F195 (green)
**Icon**: Lucide `heart-handshake`
**Title**: `Non-Profit DAO`
**Subtitle**: `Mission-driven on-chain organizations`
**Body**:
```
For DAOs focused on public goods, open-source development, grants distribution, or community governance. Same on-chain infrastructure. Different tax treatment and legal obligations.

Governance via token-weighted voting or one-member-one-vote. Treasury managed through Squads multisig. Perfect for protocol foundations, grants DAOs, and open-source collectives.
```
**Best For Tags**: `Protocol foundations` · `Grants DAOs` · `Open-source collectives` · `Public goods`
**CTA**: `Form a Non-Profit DAO →`

---

#### Card 3: Series LLC

**Accent Color**: #EC4899 (pink)
**Icon**: Lucide `git-branch`
**Title**: `Series LLC`
**Subtitle**: `One umbrella. Unlimited sub-entities.`
**Body**:
```
A parent entity with the ability to spawn liability-isolated child entities (series). Each series has its own assets, members, and governance — legally firewalled from every other series.

One formation. One registered agent. Unlimited series. Each series gets its own Squads multisig, its own SPL token cap table, its own legal identity. Scale your entity structure without scaling your legal overhead.
```
**Best For Tags**: `Multi-product DAOs` · `Venture studios` · `Incubators` · `Fund-of-funds`
**CTA**: `Form a Series LLC →`

---

#### Card 4: Traditional LLC

**Accent Color**: #3B82F6 (blue)
**Icon**: Lucide `building-2`
**Title**: `Traditional LLC`
**Subtitle**: `Marshall Islands LLC without DAO governance`
**Body**:
```
For founders who want Marshall Islands jurisdiction without the DAO governance layer. Standard LLC formation with a registered agent, operating agreement, and Certificate of Formation.

Still comes with on-chain cap table option. Still gets Marshall Islands jurisdictional benefits. Just without the mandatory smart contract governance requirement. You can always upgrade to DAO LLC later.
```
**Best For Tags**: `Solo founders` · `Holding companies` · `IP entities` · `Simple structures`
**CTA**: `Form a Traditional LLC →`

---

## 7. Pricing Section

### Section Layout
Background: #0D0D14. Padding: 120px vertical. Max content width: 1100px centered.

### Section Header

**Eyebrow**:
```
PRICING
```

**Title**:
```
Transparent pricing. No retainer. No hourly.
```

**Subtitle**:
```
One fee. Everything included. We don't charge you to ask questions, and we don't nickel-and-dime you for document revisions. The price is the price.
```

### Pricing Cards

3 cards, horizontal on desktop. The middle card (Pro) is visually emphasized: slightly larger, with a "Most Popular" badge and a #6C3CE1 border.

---

#### Tier 1: Starter

**Price**: `$5,500`
**Period**: `one-time formation`
**Annual Renewal**: `$1,800/year` (after year 1)
**Description**: `For solo founders and small teams who need a clean legal structure.`

**Includes**:
- Marshall Islands DAO LLC formation
- LLC agreement with smart contract designation
- Registered agent (year 1 included)
- Certificate of Formation
- SPL token cap table deployment (Solana mainnet)
- Squads multisig setup (up to 3 signers)
- KYC processing for up to 3 founding members
- Formation documents delivered digitally
- Email support

**Does NOT Include** (greyed out):
- Series LLC capability
- Custom governance design
- Priority support
- Compliance calendar management

---

#### Tier 2: Pro (Highlighted)

**Price**: `$8,500`
**Period**: `one-time formation`
**Annual Renewal**: `$2,800/year` (after year 1)
**Badge**: `MOST POPULAR` (pill badge, background #6C3CE1, color white, font-size 11px, letter-spacing 2px)
**Description**: `For growing DAOs that need governance flexibility and compliance automation.`

**Includes**:
- Everything in Starter, plus:
- Series LLC capability (up to 5 series at formation)
- Custom governance design consultation (1 session)
- Squads multisig setup (up to 7 signers)
- KYC processing for up to 10 founding members
- Compliance calendar with automated reminders
- Annual report preparation
- Priority email + Telegram support
- One LLC agreement amendment per year

---

#### Tier 3: Enterprise

**Price**: `Custom`
**Period**: `contact us`
**Annual Renewal**: `Custom`
**Description**: `For protocols, funds, and organizations with complex multi-entity structures.`

**Includes**:
- Everything in Pro, plus:
- Unlimited series at formation
- Unlimited signers
- Unlimited founding member KYC
- Custom LLC agreement drafting
- Dedicated account manager
- Governance architecture consultation
- Multi-entity structure planning
- API access for cap table queries
- SLA-backed support (24h response)
- Legal opinion letter

**CTA**: `Contact Us →` (opens email or Calendly link)

---

### Jurisdiction Comparison Table

Below the pricing cards, a comparison table showing entity.legal vs. alternatives:

| | entity.legal (MI) | Wyoming DAO LLC | Delaware LLC | Cayman Foundation |
|---|---|---|---|---|
| **Formation Cost** | $5,500-$8,500 | $5,000-$25,000+ | $3,000-$15,000 | $18,500-$35,000+ |
| **Annual Renewal** | $1,800-$2,800 | $500-$2,000 | $1,500-$5,000 | $8,000-$15,000 |
| **DAO-Specific Law** | Yes (DAO Act 2022) | Partial (2021 law) | No | No |
| **Smart Contract as Legal Registry** | Yes | No | No | No |
| **On-Chain Cap Table** | Included | DIY | DIY | DIY |
| **Multisig Governance** | Included | DIY | DIY | DIY |
| **US Regulatory Exposure** | None | Full | Full | Partial |
| **Securities Registration** | Not required | Likely required | Likely required | Not required |
| **Formation Time** | 10-15 business days | 5-10 business days | 3-7 business days | 30-60 business days |
| **KYC Requirement** | 25%+ members | All members | All members | Directors + UBOs |
| **Series LLC Support** | Yes | Yes | Yes | No |

Style: Dark table with #12121A background, #1E1E2E borders. entity.legal column highlighted with a subtle #6C3CE1 left-border. "Included" cells in green (#14F195). "Not required" / "None" cells in green. "Likely required" / "Full" cells in amber (#E8C547). "No" / "DIY" cells in neutral (#A0A0B0).

### Pricing Footnote
```
All prices in USD. Government filing fees included in formation cost. Annual renewal includes registered agent, registered office, and government annual fees. KYC processing fees included. No hidden fees — ever.
```
Style: font-size 13px, color #606070, margin-top 40px.

---

## 8. Trust & Social Proof Section

### Section Layout
Background: #0A0A0F. Padding: 100px vertical. Max content width: 1000px centered.

### Live Stats Bar

A horizontal bar with three metrics, evenly spaced. Each metric has a large animated number and a label below.

#### Metric 1: On-Chain Cap Table Entries
```
[animated counter] cap table entries secured on Solana
```
Value: Pull from Solana RPC (count of SPL token transfers on entity.legal's cap table program). Fallback: hardcoded number updated weekly. Animate on scroll-in with a count-up from 0 over 2 seconds.

Number style: font-size 48px, font-weight 700, color #F5F5F7, font-variant-numeric: tabular-nums.
Label style: font-size 14px, color #A0A0B0, margin-top 8px.

#### Metric 2: Entities Formed
```
[animated counter] entities formed
```
Value: API endpoint or env var.

#### Metric 3: Total On-Chain Value
```
$[animated counter]M+ in on-chain cap tables
```
Value: Sum of token values across all entity.legal cap tables. API endpoint or env var.

---

### Credibility Markers

Below the stats bar, a row of trust indicators:

```
Built by Web3 natives  ·  Marshall Islands licensed registered agent  ·  Squads Protocol partner  ·  Solana ecosystem  ·  Open-source LLC templates
```

Style: font-size 14px, color #606070, letter-spacing 0.5px, text-align center. Each marker separated by middle dots with 16px padding.

---

### Testimonial Placeholder

Reserve space for 2-3 testimonial cards. Until real testimonials are available, use this format:

```
"entity.legal will be the first formation service we recommend to portfolio companies building on Solana."
— [Name], [Title], [Fund/Company]
```

Card style: background #12121A, border 1px solid #1E1E2E, border-radius 12px, padding 32px. Quote in 18px italic, attribution in 14px color #A0A0B0.

**Developer note**: Build the testimonial component now but hide it with a feature flag (`SHOW_TESTIMONIALS=false`). Enable when real testimonials are collected.

---

### "Built By" Section

```
Built by a team that has formed entities, managed DAOs, and written smart contracts — not by a law firm that read a blog post about blockchain.

We are Web3 natives building legal infrastructure for Web3 natives. Our team has collectively managed over $50M in on-chain treasuries. We've formed entities in 6 jurisdictions. We've been burned by the alternatives, and we built entity.legal because we needed it ourselves.
```

Style: Centered text, max-width 700px. First paragraph in 18px, color #F5F5F7, font-weight 500. Second paragraph in 15px, color #A0A0B0, margin-top 16px.

---

## 9. Legal Disclaimers

### Section Layout
Background: #0D0D14. Padding: 80px vertical. Max content width: 800px centered. Border-top: 1px solid #1E1E2E.

### Section Header

**Title**:
```
Legal Disclosures
```
Style: font-size 24px, font-weight 600, color #F5F5F7.

### Disclaimer Blocks

Each disclaimer is a collapsible accordion (collapsed by default). Title visible, body hidden until clicked.

---

#### Disclaimer 1: Marshall Islands DAO Act

**Title**: `Marshall Islands DAO LLC Act 2022`
**Body**:
```
entity.legal facilitates entity formation under the Republic of the Marshall Islands Decentralized Autonomous Organization Act 2022 (52 MIRC Ch. 7). All DAO LLCs formed through entity.legal are registered with the Registrar of Resident Domestic and Authorized Foreign Corporations of the Republic of the Marshall Islands.

The DAO Act recognizes decentralized autonomous organizations as limited liability companies provided they include specific statements in their certificate of formation and LLC agreement identifying the organization as a DAO. entity.legal ensures all formation documents comply with these requirements.

The Republic of the Marshall Islands is a sovereign nation. Laws and regulations may change. entity.legal does not guarantee the future regulatory treatment of DAO LLCs in any jurisdiction.
```

---

#### Disclaimer 2: Smart Contract as Legal Record

**Title**: `Smart contract address as legal record`
**Body**:
```
When you form a DAO LLC through entity.legal, your LLC agreement designates a specific Solana smart contract address as the authoritative membership registry. This means:

1. Ownership of membership interests is determined by SPL token holdings at the designated smart contract address.
2. Transfers of SPL tokens at this address constitute legally binding transfers of membership interests, subject to any transfer restrictions in your LLC agreement.
3. The on-chain state of the smart contract is the legal record of your cap table.

Smart contracts are software. Software can have bugs. entity.legal does not audit or warranty the Squads Protocol smart contracts. Squads Protocol has been independently audited by OtterSec, Neodyme, and Bramah Systems and is the first formally verified program on Solana. However, no audit eliminates all risk.

You are responsible for the security of your private keys. entity.legal does not have access to your keys and cannot recover lost keys or reverse transactions.
```

---

#### Disclaimer 3: Not Legal Advice

**Title**: `Not legal or tax advice`
**Body**:
```
entity.legal is an entity formation service. We are not a law firm. The information on this website and the services we provide do not constitute legal advice, tax advice, investment advice, or any other form of professional advice.

Formation of a Marshall Islands DAO LLC does not exempt you from applicable laws in your jurisdiction of residence or operation. You are responsible for understanding and complying with all laws that apply to you, your organization, and your activities.

We strongly recommend consulting with qualified legal and tax professionals in your jurisdiction before forming any entity. entity.legal's services are limited to entity formation, on-chain infrastructure deployment, and annual compliance maintenance.

Nothing on this website should be construed as a solicitation or offer to sell securities. Membership interests in a DAO LLC may constitute securities under the laws of certain jurisdictions. Consult a qualified securities attorney before issuing or transferring membership interests.
```

---

#### Disclaimer 4: KYC and AML

**Title**: `Know Your Customer (KYC) and Anti-Money Laundering (AML)`
**Body**:
```
In accordance with Marshall Islands law, founding members of DAO LLCs with 25% or more governance rights must complete Know Your Customer (KYC) verification. This includes providing proof of identity and proof of residential address.

entity.legal collects KYC information solely for the purpose of entity formation compliance. KYC data is processed by our verified KYC provider, encrypted at rest, and never shared with third parties except as required by law.

entity.legal reserves the right to refuse service to any individual or organization that fails KYC verification or that we reasonably believe may be involved in money laundering, terrorism financing, or other illicit activity.
```

---

## 10. Footer

### Section Layout
Background: #080810. Padding: 80px top, 40px bottom. Max content width: 1100px centered. Border-top: 1px solid #1E1E2E.

### Footer Grid

4 columns on desktop, 2x2 on tablet, stacked on mobile.

#### Column 1: Brand

**Logo**: "entity.legal" in the primary font, 20px, font-weight 700, color #F5F5F7. The period between "entity" and "legal" is colored #6C3CE1.

**Tagline below logo**:
```
Sovereign entity formation
for the on-chain era.
```
Style: font-size 14px, color #606070, margin-top 12px, line-height 1.5.

**Social Links** (icon-only row, margin-top 24px):
- Twitter/X: link to `https://x.com/entitylegal` (Lucide: `twitter` or custom X icon)
- Discord: link to `https://discord.gg/entitylegal`
- Telegram: link to `https://t.me/entitylegal`

Icon style: 20px, color #606070, hover color #F5F5F7, transition 200ms. 16px gap between icons.

#### Column 2: Product

**Column Title**: `Product`
Style: font-size 13px, letter-spacing 2px, text-transform uppercase, color #606070, font-weight 600, margin-bottom 16px.

**Links**:
- `DAO LLC Formation` → /formation/dao-llc
- `Series LLC` → /formation/series-llc
- `Non-Profit DAO` → /formation/non-profit
- `Traditional LLC` → /formation/traditional
- `Pricing` → /pricing
- `How It Works` → /#how-it-works

Link style: font-size 14px, color #A0A0B0, line-height 2.2, hover color #F5F5F7, no underline (underline on hover).

#### Column 3: Resources

**Column Title**: `Resources`

**Links**:
- `Documentation` → /docs
- `FAQ` → /faq
- `LLC Agreement Template` → /docs/llc-template
- `Marshall Islands DAO Act` → /docs/dao-act (external link to RMI parliament PDF)
- `Squads Protocol` → https://squads.xyz (external, opens new tab)
- `Blog` → /blog

#### Column 4: Company

**Column Title**: `Company`

**Links**:
- `About` → /about
- `Contact` → /contact
- `Privacy Policy` → /privacy
- `Terms of Service` → /terms
- `Careers` → /careers

---

### Newsletter Signup (below grid, full width)

A horizontal form: email input + submit button, centered, max-width 500px.

**Label above form**:
```
Stay updated. No spam. Just formation law, governance design, and on-chain entity news.
```
Style: font-size 14px, color #A0A0B0, text-align center, margin-bottom 16px.

**Email input**: background #12121A, border 1px solid #1E1E2E, border-radius 8px 0 0 8px, padding 14px 16px, color #F5F5F7, placeholder "your@email.com" in #606070, font-size 14px. Focus: border-color #6C3CE1.

**Submit button**: background #6C3CE1, color white, border-radius 0 8px 8px 0, padding 14px 24px, font-size 14px, font-weight 600, text "Subscribe →", hover background #7D4FE8.

---

### Bottom Bar

Below newsletter, separated by 1px #1E1E2E border. Padding-top 24px.

Left-aligned:
```
(c) 2026 entity.legal. All rights reserved.
```
Style: font-size 13px, color #606070.

Right-aligned:
```
Marshall Islands Registered Agent License #[number]
```
Style: font-size 13px, color #606070.

---

## 11. Technical Specifications

### Tech Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Framework | **Next.js 15** (App Router) | SSR for SEO, React Server Components for performance, API routes for waitlist/stats |
| Styling | **Tailwind CSS v4** | Utility-first, design tokens via CSS variables, purge = tiny bundles |
| Animations | **Framer Motion 12** | Declarative animations, gesture support, layout animations for accordion |
| Icons | **Lucide React** | Consistent icon set, tree-shakeable, 1KB per icon |
| Fonts | **Inter** (sans-serif) via `next/font` | Self-hosted, no CLS, variable font weight |
| Analytics | **Plausible** or **Fathom** | Privacy-focused, no cookie banner needed, GDPR-compliant |
| Forms | **React Hook Form** + server action | No client-side form library bloat, server-side validation |
| Email | **Resend** or **Loops** | Transactional + marketing email, API-first |
| Database | **Supabase** (PostgreSQL) | Waitlist storage, stats, future auth |
| Hosting | **Vercel** | Zero-config Next.js deployment, edge functions, preview deploys |
| Solana RPC | **Helius** or **QuickNode** | For live cap table stats on landing page |
| Wallet | **Solana Wallet Adapter** | For waitlist wallet address capture (optional connect) |

### Color Palette

```
/* Core */
--color-bg-primary:      #0A0A0F;    /* Page background */
--color-bg-secondary:    #0D0D14;    /* Alternate section background */
--color-bg-card:         #12121A;    /* Card/component background */
--color-bg-elevated:     #1A1A24;    /* Hover states, elevated surfaces */
--color-border:          #1E1E2E;    /* Default borders */
--color-border-hover:    #2A2A3A;    /* Hover borders */

/* Text */
--color-text-primary:    #F5F5F7;    /* Headlines, primary text */
--color-text-secondary:  #A0A0B0;    /* Body text, descriptions */
--color-text-tertiary:   #606070;    /* Captions, footnotes, metadata */
--color-text-muted:      #404050;    /* Disabled, placeholder */

/* Accent */
--color-accent-primary:  #6C3CE1;    /* Brand purple — CTAs, links, highlights */
--color-accent-hover:    #7D4FE8;    /* Purple hover state */
--color-accent-muted:    #6C3CE140;  /* Purple at 25% — subtle backgrounds */

/* Semantic */
--color-solana:          #14F195;    /* Solana green — chain references */
--color-gold:            #E8C547;    /* Gold — legal/jurisdiction references */
--color-blue:            #3B82F6;    /* Blue — informational */
--color-pink:            #EC4899;    /* Pink — Series LLC, differentiation */
--color-orange:          #F97316;    /* Orange — smart contract references */
--color-error:           #EF4444;    /* Red — errors only */
--color-success:         #22C55E;    /* Green — success states */

/* Gradients */
--gradient-hero:         radial-gradient(ellipse at center, #1A0A2E 0%, transparent 60%);
--gradient-card-hover:   linear-gradient(135deg, rgba(108, 60, 225, 0.05), transparent);
```

### Typography

```
/* Font Family */
--font-sans: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
--font-mono: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace;

/* Font Sizes — Desktop */
--text-hero:     48px;    /* Hero headline only */
--text-h1:       36px;    /* Section titles */
--text-h2:       24px;    /* Sub-section titles */
--text-h3:       20px;    /* Card titles */
--text-body:     16px;    /* Body text */
--text-body-sm:  15px;    /* Secondary body */
--text-caption:  14px;    /* Captions, labels, links */
--text-small:    13px;    /* Footnotes, eyebrows, metadata */
--text-tiny:     11px;    /* Badges */

/* Font Sizes — Mobile (viewport < 768px) */
--text-hero-mobile:  32px;
--text-h1-mobile:    28px;
--text-h2-mobile:    20px;
--text-h3-mobile:    18px;
/* body and below remain the same */

/* Line Heights */
--leading-tight:   1.2;   /* Headlines */
--leading-normal:  1.5;   /* UI text */
--leading-relaxed: 1.7;   /* Body paragraphs */

/* Font Weights */
--weight-regular:  400;
--weight-medium:   500;
--weight-semibold: 600;
--weight-bold:     700;

/* Letter Spacing */
--tracking-tight:  -0.02em;  /* Headlines */
--tracking-normal:  0;        /* Body */
--tracking-wide:    0.04em;   /* Eyebrows, small caps */
--tracking-wider:   0.08em;   /* Badges */
```

### Responsive Breakpoints

```
/* Tailwind v4 default breakpoints — use as-is */
sm:  640px    /* Mobile landscape */
md:  768px    /* Tablet portrait */
lg:  1024px   /* Tablet landscape / small desktop */
xl:  1280px   /* Desktop */
2xl: 1536px   /* Large desktop */

/* Custom breakpoints if needed */
--bp-content: 1100px;  /* Max content width */
--bp-narrow:  800px;   /* Narrow content (hero, legal, etc.) */
--bp-form:    500px;   /* Form max-width */
```

### Layout Rules

| Breakpoint | Hero Grid | Feature Cards | Entity Cards | Pricing Cards | Footer |
|-----------|-----------|---------------|-------------|---------------|--------|
| < 640px | 1 col, stacked | 1 col | 1 col | 1 col stacked | 1 col |
| 640-767px | 1 col, stacked | 2 col | 1 col | 1 col stacked | 2 col |
| 768-1023px | 1 col, centered | 2 col | 2 col | 3 col (compact) | 2 col |
| 1024-1279px | 1 col, centered | 3 col | 4 col | 3 col | 4 col |
| >= 1280px | 1 col, centered | 3 col | 4 col | 3 col | 4 col |

### Performance Targets

| Metric | Target | How |
|--------|--------|-----|
| Lighthouse Performance | 95+ | SSR, font preload, image optimization, code splitting |
| Lighthouse Accessibility | 100 | Semantic HTML, ARIA labels, color contrast (all text passes WCAG AA) |
| Lighthouse Best Practices | 100 | HTTPS, no console errors, secure headers |
| Lighthouse SEO | 100 | Meta tags, OG tags, structured data, sitemap |
| First Contentful Paint | < 1.2s | Server-side render hero, preload Inter font |
| Largest Contentful Paint | < 2.0s | No hero image (text-only hero), lazy-load below fold |
| Cumulative Layout Shift | < 0.05 | `next/font` for zero CLS, fixed-height hero |
| Total Blocking Time | < 150ms | Minimal JS, defer non-critical (analytics, animations) |
| Bundle size (initial) | < 80KB gzipped | Tree-shake icons, purge Tailwind, dynamic import Framer Motion |
| Time to Interactive | < 2.5s | Defer wallet adapter, lazy-load interactive sections |

### Accessibility Requirements

- All interactive elements have visible focus indicators (2px #6C3CE1 outline, 2px offset)
- Color is never the sole indicator of meaning (always paired with text/icon)
- All images have alt text (minimal images on this page)
- Form inputs have associated labels (visible or sr-only)
- Accordion sections use proper ARIA: `role="region"`, `aria-expanded`, `aria-controls`
- Keyboard navigation: Tab through all interactive elements, Enter/Space to activate
- Reduced motion: wrap all Framer Motion animations in `useReducedMotion()` check, provide instant state for prefers-reduced-motion users
- Minimum touch target: 44x44px on mobile

---

## 12. Waitlist / Lead Capture

### Floating CTA Bar

A sticky bar that appears after scrolling past the hero section (Intersection Observer on hero). Sticks to the bottom of the viewport on mobile, top on desktop.

**Desktop**: Thin bar (48px height) at top of viewport. Background #12121A with 80% opacity, backdrop-blur 12px. Contains:
```
Reserve your on-chain entity  ·  [email input] [wallet connect button] [Submit]
```
Max-width 800px centered. Smooth slide-down animation on appear.

**Mobile**: Bottom bar (auto height, padding 12px 16px). Same content but stacked: text on top, form below.

### Waitlist Modal (triggered by "Form Your Entity" CTA before launch)

If the product is not yet live, all "Form Your Entity" CTAs open a waitlist modal instead. This should be controlled by an environment variable: `WAITLIST_MODE=true`.

**Modal Design**:
- Overlay: #0A0A0F at 80% opacity, backdrop-blur 8px
- Modal: max-width 480px, centered vertically and horizontally, background #12121A, border 1px solid #1E1E2E, border-radius 16px, padding 40px
- Close button: top-right, X icon, #606070, hover #F5F5F7

**Modal Content**:

**Title**:
```
Reserve your on-chain entity
```
Style: 24px, font-weight 700, color #F5F5F7.

**Subtitle**:
```
Join the waitlist. We'll notify you when formation opens. Early waitlist members get priority access and a founder discount.
```
Style: 15px, color #A0A0B0, margin-top 8px, line-height 1.6.

**Form Fields**:

1. **Email** (required)
   - Label: `Email address`
   - Placeholder: `founder@yourdao.xyz`
   - Validation: standard email regex
   - Input style: background #0A0A0F, border 1px solid #1E1E2E, border-radius 8px, padding 14px 16px, full width, focus border-color #6C3CE1

2. **Wallet Address** (optional)
   - Label: `Solana wallet address`
   - Placeholder: `Connect wallet or paste address`
   - Two options: (a) "Connect Wallet" button that triggers Solana Wallet Adapter, or (b) manual paste
   - Validation: Solana base58 address format (32-44 characters, base58 charset)
   - Below field: `Optional — connecting your wallet reserves your cap table slot`

3. **Entity Type** (optional, dropdown)
   - Label: `What are you forming?`
   - Options: `DAO LLC`, `Non-Profit DAO`, `Series LLC`, `Traditional LLC`, `Not sure yet`

4. **Team Size** (optional, dropdown)
   - Label: `How many founding members?`
   - Options: `Just me`, `2-3`, `4-10`, `11-50`, `50+`

**Submit Button**:
```
Join Waitlist →
```
Full width, same style as hero CTA (background #6C3CE1, etc.)

**Live Counter** (below submit button):
```
[animated number] founders already on the waitlist
```
Style: font-size 13px, color #606070, text-align center, margin-top 16px. Number pulled from API endpoint, animated count-up on modal open.

**Success State**:
Replace form with:
```
You're on the list.

We'll reach out when formation opens. In the meantime, follow us on Twitter for updates on Marshall Islands DAO law and on-chain governance.

[Twitter/X button]  [Discord button]
```

### Data Schema

Waitlist entries stored in Supabase (or equivalent):

```sql
CREATE TABLE waitlist (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL UNIQUE,
    wallet_address TEXT,
    entity_type TEXT,
    team_size TEXT,
    referral_source TEXT,      -- UTM source
    created_at TIMESTAMPTZ DEFAULT now(),
    ip_country TEXT,           -- GeoIP for jurisdiction context
    notified_at TIMESTAMPTZ   -- When we notified them of launch
);

CREATE INDEX idx_waitlist_email ON waitlist(email);
CREATE INDEX idx_waitlist_created ON waitlist(created_at DESC);
```

### Anti-Spam

- Rate limit: max 3 submissions per IP per hour
- Honeypot field: hidden `website` field, reject if filled
- No CAPTCHA (destroys conversion). Rely on rate limiting + honeypot + server-side validation
- Bot detection: reject if form submitted in < 2 seconds (too fast for human)

---

## 13. Component Inventory

Every component a developer needs to build, listed for sprint planning.

| Component | Props | Notes |
|-----------|-------|-------|
| `<Hero />` | — | Static, no props. Contains eyebrow, headline, sub, CTAs, social proof line, scroll indicator |
| `<ParticleField />` | `count`, `color`, `opacity` | Canvas or CSS-based particle animation for hero background |
| `<ArchitectureStack />` | `defaultExpanded: number` | Three-layer interactive accordion with connecting lines |
| `<ArchitectureLayer />` | `title`, `subtitle`, `body`, `icon`, `accentColor`, `isExpanded`, `onClick` | Individual layer card |
| `<PulseDot />` | `color`, `duration` | Animated dot traveling along connecting lines |
| `<SectionHeader />` | `eyebrow`, `title`, `subtitle`, `maxWidth` | Reusable section header |
| `<FeatureCard />` | `icon`, `iconColor`, `title`, `body` | Feature grid card |
| `<EntityTypeCard />` | `icon`, `accentColor`, `title`, `subtitle`, `body`, `tags[]`, `ctaText`, `ctaHref` | Entity type card with tags |
| `<PricingCard />` | `title`, `price`, `period`, `renewal`, `description`, `features[]`, `excluded[]`, `highlighted`, `badge` | Pricing tier card |
| `<ComparisonTable />` | `data[][]` | Jurisdiction comparison table |
| `<TimelineStep />` | `step`, `title`, `duration`, `body`, `visual`, `isActive` | How It Works step |
| `<Timeline />` | `steps[]` | Vertical timeline with scroll-triggered activation |
| `<StatsBar />` | `metrics[]` | Animated counter bar |
| `<AnimatedCounter />` | `target`, `duration`, `prefix`, `suffix` | Count-up number animation |
| `<TestimonialCard />` | `quote`, `name`, `title`, `company` | Testimonial (feature-flagged) |
| `<DisclaimerAccordion />` | `items[]` | Collapsible legal disclaimer sections |
| `<WaitlistModal />` | `isOpen`, `onClose` | Full waitlist capture modal |
| `<WaitlistForm />` | `onSuccess` | Form with email, wallet, entity type, team size |
| `<WalletConnect />` | `onConnect` | Solana Wallet Adapter integration |
| `<FloatingCTA />` | — | Sticky waitlist bar, scroll-triggered |
| `<Footer />` | — | Static footer with newsletter, links, social |
| `<NewsletterForm />` | `onSuccess` | Email input + submit |
| `<ScrollIndicator />` | — | Bouncing chevron at bottom of hero |
| `<Badge />` | `text`, `color` | Small pill badge (e.g., "MOST POPULAR") |
| `<Tag />` | `text` | Small tag for "Best For" on entity cards |

---

## 14. Animation & Interaction Spec

### Global Animation Tokens

```typescript
const transitions = {
  fast:    { duration: 0.15, ease: "easeOut" },
  default: { duration: 0.3,  ease: "easeOut" },
  slow:    { duration: 0.6,  ease: [0.16, 1, 0.3, 1] },  // custom bezier
  spring:  { type: "spring", stiffness: 300, damping: 30 },
};
```

### Section-by-Section Animations

| Element | Trigger | Animation | Duration | Reduced Motion Fallback |
|---------|---------|-----------|----------|------------------------|
| Hero headline | Page load | Fade up, stagger each line by 100ms | 600ms per line | Instant appear |
| Hero sub-headline | After headline | Fade up | 400ms | Instant appear |
| Hero CTAs | After sub-headline | Fade up | 400ms | Instant appear |
| Hero social proof | After CTAs | Fade in | 300ms | Instant appear |
| Hero particles | Page load | Continuous drift | Infinite | Static or hidden |
| Scroll indicator | Page load + 1s | Bounce loop | 2s loop | Static chevron |
| Architecture layers | Scroll into view | Stagger fade up from bottom | 400ms each, 150ms stagger | Instant appear |
| Pulse dots | Architecture visible | Continuous travel loop | 3s loop | Hidden |
| Feature cards | Scroll into view | Stagger fade up | 300ms each, 100ms stagger | Instant appear |
| Feature card hover | Mouse enter | TranslateY -2px, border color change | 200ms | No transform, color only |
| Timeline steps | Scroll past each | Step marker fills, content fades in | 400ms | Instant appear |
| Timeline line | Scroll | Line "draws" downward as user scrolls | Continuous | Full line visible |
| Entity type cards | Scroll into view | Stagger fade up | 300ms each, 100ms stagger | Instant appear |
| Pricing cards | Scroll into view | Stagger fade up, Pro card slightly delayed for emphasis | 400ms each | Instant appear |
| Stats counters | Scroll into view | Count up from 0 | 2000ms | Instant final number |
| Floating CTA | Scroll past hero | Slide in from top (desktop) / bottom (mobile) | 300ms | Instant appear |
| Waitlist modal | CTA click | Overlay fade in 200ms, modal scale from 0.95 to 1 + fade in 300ms | 300ms | Instant appear |
| Accordion expand | Click | Height auto-animate, content fade in | 300ms | Instant expand |

### Scroll-Triggered Visibility

Use Intersection Observer with these thresholds:
- Section headers: trigger at 20% visibility
- Cards/steps: trigger at 30% visibility
- Stats counters: trigger at 50% visibility (only count once)
- Floating CTA: trigger when hero is < 10% visible

All scroll-triggered animations should only fire once (no re-animation on scroll up).

---

## 15. SEO & Meta

### Page Title
```
entity.legal — Marshall Islands DAO LLC Formation with On-Chain Cap Tables on Solana
```

### Meta Description
```
Form a Marshall Islands DAO LLC with on-chain cap tables on Solana. SPL token membership interests. Squads Protocol governance. No US regulatory overhead. Your keys, your entity.
```

### Open Graph Tags
```html
<meta property="og:title" content="entity.legal — Sovereign Entity Formation for the On-Chain Era" />
<meta property="og:description" content="Marshall Islands DAO LLC formation with on-chain cap tables on Solana. Real legal structure. No custody middleware. Your keys, your entity." />
<meta property="og:type" content="website" />
<meta property="og:url" content="https://entity.legal" />
<meta property="og:image" content="https://entity.legal/og-image.png" />
<meta property="og:image:width" content="1200" />
<meta property="og:image:height" content="630" />
<meta property="og:site_name" content="entity.legal" />
```

### Twitter Card
```html
<meta name="twitter:card" content="summary_large_image" />
<meta name="twitter:site" content="@entitylegal" />
<meta name="twitter:title" content="entity.legal — Sovereign Entity Formation for the On-Chain Era" />
<meta name="twitter:description" content="Marshall Islands DAO LLC + Solana on-chain cap tables. No US regulatory overhead. Your keys, your entity." />
<meta name="twitter:image" content="https://entity.legal/og-image.png" />
```

### OG Image Spec
1200x630px. Dark background (#0A0A0F). "entity.legal" logo centered, large. Below: "Sovereign entity formation for the on-chain era." Subtle purple glow. No busy graphics.

### Structured Data (JSON-LD)
```json
{
  "@context": "https://schema.org",
  "@type": "ProfessionalService",
  "name": "entity.legal",
  "description": "Marshall Islands DAO LLC formation with on-chain cap tables on Solana",
  "url": "https://entity.legal",
  "serviceType": "Business Formation",
  "areaServed": "Worldwide",
  "brand": {
    "@type": "Brand",
    "name": "entity.legal",
    "slogan": "Sovereign entity formation for the on-chain era"
  },
  "offers": [
    {
      "@type": "Offer",
      "name": "Starter — DAO LLC Formation",
      "price": "5500",
      "priceCurrency": "USD",
      "description": "Marshall Islands DAO LLC with on-chain cap table on Solana"
    },
    {
      "@type": "Offer",
      "name": "Pro — DAO LLC + Series LLC",
      "price": "8500",
      "priceCurrency": "USD",
      "description": "DAO LLC with Series LLC capability, custom governance, and compliance automation"
    }
  ]
}
```

### Sitemap Pages (for initial launch)
```
/                     — Landing page (this spec)
/pricing              — Expanded pricing (can initially redirect to /#pricing anchor)
/docs                 — Documentation hub
/docs/llc-template    — Public LLC agreement template
/docs/dao-act         — Link/mirror of Marshall Islands DAO Act PDF
/faq                  — Frequently asked questions
/about                — About the team
/contact              — Contact form
/privacy              — Privacy policy
/terms                — Terms of service
/blog                 — Blog (can be empty at launch)
/waitlist/success     — Post-waitlist confirmation page
```

### Canonical URL
```html
<link rel="canonical" href="https://entity.legal" />
```

### Robots
```
User-agent: *
Allow: /
Sitemap: https://entity.legal/sitemap.xml
```

---

## Appendix A: Content Checklist

Before launch, verify:

- [ ] All copy reviewed for legal accuracy by qualified attorney
- [ ] KYC flow tested end-to-end
- [ ] Waitlist form submits correctly and stores data
- [ ] Live counters connected to real data sources (or fallback values set)
- [ ] All external links work (Squads, Marshall Islands DAO Act PDF, social)
- [ ] OG image renders correctly on Twitter, Discord, Telegram, LinkedIn
- [ ] Mobile tested on: iPhone SE, iPhone 15, Pixel 8, iPad
- [ ] Lighthouse scores meet targets (95+ performance, 100 a11y)
- [ ] Analytics tracking verified (page views, CTA clicks, waitlist conversions)
- [ ] Reduced motion tested (prefers-reduced-motion: reduce)
- [ ] Screen reader tested (VoiceOver + NVDA minimum)
- [ ] WAITLIST_MODE environment variable controls CTA behavior
- [ ] SHOW_TESTIMONIALS feature flag works
- [ ] Newsletter form connected to email provider
- [ ] Rate limiting active on waitlist endpoint
- [ ] Honeypot field present and functional
- [ ] SSL certificate active on entity.legal
- [ ] DNS configured (entity.legal → Vercel)
- [ ] Error monitoring configured (Sentry or equivalent)

## Appendix B: Post-Launch Iteration Targets

1. **A/B test hero headlines**: Test the current multi-line headline against a single-line version and a question-format version.
2. **Add Solana transaction feed**: Real-time feed of cap table transactions in the Trust section.
3. **Interactive pricing calculator**: "How many members? How many series?" → dynamic price estimate.
4. **Case studies**: After first 10 formations, publish anonymized case studies with permission.
5. **Comparison pages**: SEO landing pages for "Marshall Islands vs Wyoming DAO LLC", "Marshall Islands vs Cayman Foundation", etc.
6. **Referral program**: Existing clients refer new formations for discount on renewal.
7. **API documentation**: Public API for querying cap table data, entity status.
8. **Multi-language**: Spanish, Mandarin, Portuguese for international Web3 founders.
