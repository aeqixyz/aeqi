# Two-Token Model + Foundation-as-Escrow Architecture Specification

**Document**: el-004 Deep Legal Architecture Spec
**Date**: 2026-02-23
**Status**: Production Specification
**Classification**: For legal counsel and development team review

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Two-Token Model](#2-two-token-model)
3. [Foundation-as-Escrow Architecture](#3-foundation-as-escrow-architecture)
4. [Technical Implementation](#4-technical-implementation)
5. [Series DAO LLC Deep Dive](#5-series-dao-llc-deep-dive)
6. [Risk Analysis](#6-risk-analysis)
7. [Appendices](#7-appendices)

---

## 1. Executive Summary

This specification defines a dual-entity, dual-token architecture for entity.legal's Marshall Islands DAO LLC offering. The architecture separates governance rights from economic rights at both the token level and the entity level, creating a legally defensible structure that satisfies securities law requirements across multiple jurisdictions while preserving the benefits of on-chain governance.

**Core thesis**: By splitting governance and economic interests into two distinct tokens — and housing them in two distinct legal entities (a non-profit Foundation and a for-profit Series DAO LLC) — we achieve three outcomes simultaneously:

1. The governance token benefits from the Marshall Islands statutory safe harbor: "a governance token conferring no economic rights shall not be deemed a security."
2. The equity token operates as a membership interest in a for-profit DAO LLC, with transfer restrictions enforced programmatically via Token-2022 hooks.
3. The Foundation acts as an independent, non-profit custodian of treasury and protocol assets, shielded from the commercial risks of the operating Series entities.

This document is intended for both legal counsel (who need to draft operating agreements and assess regulatory exposure) and the development team (who need to implement the on-chain programs). It covers the full stack: legal analysis, entity structure, fund flows, Solana program architecture, PDA hierarchy, token mechanics, and risk mitigation.

---

## 2. Two-Token Model

### 2.1 Overview

The two-token model separates the rights traditionally bundled into a single membership interest in an LLC into two distinct on-chain instruments:

| Property | Governance Token (GOV) | Equity Token (EQT) |
|----------|----------------------|---------------------|
| **Type** | SPL Token-2022 (Non-Transferable or freely transferable) | SPL Token-2022 (Transfer Hook restricted) |
| **Issuing Entity** | Foundation (Non-Profit DAO LLC) | Series Entity (For-Profit Series DAO LLC) |
| **Rights Conferred** | Voting on proposals, delegation, signal voting | Pro-rata economic interest: distributions, liquidation preference, cap table position |
| **Economic Rights** | None — explicitly excluded | Yes — proportional to token balance |
| **Securities Status (RMI)** | Statutory safe harbor: not a security | Case-by-case analysis (not automatically a security, not automatically exempt) |
| **Securities Status (US)** | Strong argument against security classification (no expectation of profit from token itself) | Likely a security under Howey; must comply with Reg D/S exemptions |
| **Transferability** | Configurable: non-transferable (soulbound), delegatable, or freely transferable | Restricted: transfer hook enforces KYC, accreditation, lock-up, ROFR |
| **Token Standard** | Token-2022 with Non-Transferable extension (if soulbound) or Metadata extension | Token-2022 with Transfer Hook, Permanent Delegate, Default Account State (Frozen), Metadata |
| **Supply** | Fixed at genesis or mintable by governance vote | Authorized by operating agreement; issued by multisig |
| **Decimals** | 0 (one token = one vote) or 6 (fractional voting) | 0 (one token = one share) recommended for cap table clarity |

### 2.2 Governance Token (GOV) — Detailed Specification

#### 2.2.1 Legal Nature

The governance token confers exclusively non-economic rights. It grants the holder:

- **Voting power** on proposals submitted to the Foundation's governance system
- **Proposal submission rights** (if holding above a minimum threshold)
- **Delegation rights** — the ability to delegate voting power to another address
- **Signal voting** — non-binding opinion polls on strategic direction
- **Council election rights** — voting for Special Delegates or council members

The governance token explicitly does NOT confer:

- Any right to distributions, dividends, or profit sharing
- Any liquidation preference or residual claim on assets
- Any right to a share of revenues, fees, or treasury returns
- Any economic interest in the Foundation or any Series entity
- Any claim on intellectual property, contracts, or commercial value

This distinction is codified in the Foundation's operating agreement with language such as:

> "GOV Tokens confer governance rights exclusively. No GOV Token, nor any right associated therewith, shall entitle the holder to any distribution of profits, revenue, assets, or economic benefit of any kind from the Foundation or any Series Entity. GOV Token holders expressly acknowledge that their tokens carry no economic value, no expectation of profit, and no investment return."

#### 2.2.2 RMI Securities Safe Harbor

Under the 2023 Amendments to the Marshall Islands DAO Act:

> "A governance token conferring no economic rights shall not be deemed a security."

This is a statutory safe harbor, not a rebuttable presumption. As long as the token genuinely confers no economic rights — meaning no dividend, no profit share, no liquidation preference, no revenue share, and no buyback obligation — it falls outside the definition of a security under Marshall Islands law.

Furthermore, because the governance token is issued by a **non-profit DAO LLC** (the Foundation), it benefits from the additional blanket exemption:

> "All digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security."

This provides a **double safe harbor**: the token is exempt both (a) as a governance token with no economic rights and (b) as a digital asset issued by a non-profit entity.

#### 2.2.3 US Securities Analysis (Howey Test)

Under the Howey test, an instrument is a security if there is (1) an investment of money, (2) in a common enterprise, (3) with a reasonable expectation of profits, (4) derived from the efforts of others.

For the governance token:

| Howey Prong | Analysis | Risk Level |
|-------------|----------|------------|
| Investment of money | If GOV is distributed via airdrop or earned via participation, no investment of money. If sold for value, this prong is likely met. | Low (if distributed, not sold) |
| Common enterprise | Pooling of resources in the Foundation could constitute a common enterprise. | Medium |
| Expectation of profits | GOV has no economic rights. No right to distributions, revenue, or appreciation. However, secondary market price appreciation is possible. SEC has argued that even non-economic tokens can create profit expectations. | Medium |
| Efforts of others | If the Foundation team's efforts drive value, this prong may be met. If governance is genuinely decentralized, argument weakens. | Medium |

**Mitigation strategy**: (a) Never sell GOV for value — distribute via airdrop, participation rewards, or governance mining. (b) Ensure no marketing materials suggest GOV will appreciate in value. (c) Achieve genuine decentralization of governance decisions. (d) Do not list GOV on exchanges or create liquidity pools. (e) If GOV must be sold, restrict sales to non-US persons under Reg S.

#### 2.2.4 EU Securities Analysis (MiCA)

Under the Markets in Crypto-Assets Regulation (MiCA), effective June 2024:

- A governance token with no economic rights would likely be classified as a **utility token** rather than an asset-referenced token or e-money token.
- Utility tokens providing access to governance services are subject to lighter regulation.
- A whitepaper must be published if the token is offered to the public in the EU.
- If GOV is freely transferable and offered to EU persons, compliance with MiCA whitepaper requirements is advisable.
- If GOV is non-transferable (soulbound), it may fall outside MiCA entirely as it cannot be traded on secondary markets.

**Recommendation**: Make GOV non-transferable (soulbound) using the Token-2022 Non-Transferable extension. This eliminates secondary market trading, strengthens the non-security argument, and may exempt the token from MiCA entirely. Governance influence is exercised via delegation, not token transfer.

#### 2.2.5 Token Issuance Flow

```
Foundation Multisig (Squads)
    │
    ├── genesis_mint()
    │   Creates GOV mint with:
    │   - Non-Transferable extension (soulbound)
    │   - Metadata extension (name, symbol, URI)
    │   - Mint authority = Foundation Squads Vault PDA
    │   - Total supply defined in operating agreement
    │
    ├── distribute_gov()
    │   Mints GOV to eligible addresses:
    │   - Founding members (at formation)
    │   - New members (on approval by governance vote)
    │   - Participation rewards (governance mining)
    │   Requires: multisig approval
    │
    ├── revoke_gov()
    │   Burns GOV from restricted persons:
    │   - Sanctions list matches
    │   - AML violations
    │   - Governance vote to remove member
    │   Requires: multisig approval + governance vote
    │   Mechanism: Permanent Delegate on mint (Foundation vault)
    │
    └── delegate_voting_power()
        On-chain delegation:
        - Holder delegates voting weight to another address
        - Delegatee votes on behalf of delegator
        - Revocable at any time by delegator
        - Does NOT transfer the token (soulbound)
```

#### 2.2.6 GOV Token Configuration

```
Mint Address: [deployed at formation]
Decimals: 0 (whole votes only)
Supply: Fixed at genesis (e.g., 10,000,000 GOV)
  or: Mintable with governance vote (requires supermajority)
Mint Authority: Foundation Squads Vault PDA
Freeze Authority: Foundation Squads Vault PDA

Token-2022 Extensions:
  - NonTransferable: true (soulbound — cannot be sent to another wallet)
  - Metadata: {
      name: "[Entity] Governance",
      symbol: "GOV",
      uri: "arweave://[hash]" // Points to governance charter JSON
    }
  - PermanentDelegate: Foundation Squads Vault PDA
    (enables forced burn for sanctions/compliance)
```

### 2.3 Equity Token (EQT) — Detailed Specification

#### 2.3.1 Legal Nature

The equity token represents a **membership interest** in a for-profit Series DAO LLC. Each token is a fractional unit of the total authorized membership interests for a given series. The holder is entitled to:

- **Pro-rata distributions** — share of any profits distributed by the Series entity, proportional to EQT balance relative to total issued supply
- **Liquidation preference** — on dissolution, claim on remaining assets after liabilities, proportional to EQT balance (or per share class preference schedule)
- **Cap table position** — the EQT balance IS the cap table entry; the blockchain is the authoritative membership registry per the operating agreement
- **Information rights** — access to annual financial reports, material event disclosures
- **Transfer rights** — subject to restrictions (lock-up, ROFR, KYC, accreditation)
- **Anti-dilution protection** — as defined in the operating agreement per share class
- **Tag-along / drag-along rights** — as encoded in the transfer hook and operating agreement

The equity token does NOT confer:

- Voting rights on Foundation governance (that is GOV's domain)
- Proposal submission rights at the Foundation level
- Any claim on Foundation treasury or assets
- Any right to influence Foundation operations

However, the equity token MAY confer **Series-level voting rights** for decisions specific to that Series (e.g., approving a Series-specific investment, hiring, or dissolution). This is distinct from Foundation-level governance.

#### 2.3.2 RMI Securities Analysis

Under Marshall Islands law, for-profit DAO LLC tokens with economic rights are NOT automatically classified as securities, but are also NOT automatically exempt. The analysis is case-by-case.

Key factors in favor of non-security classification under RMI:

1. **Membership interest, not investment contract**: The EQT represents a direct membership interest in an LLC, analogous to an LLC membership unit. LLC membership interests are generally not securities under Delaware law (which RMI follows) unless they are offered as passive investments to persons with no managerial role.
2. **Territorial limitation**: RMI securities laws only apply when tokens are sold to Marshall Islands residents. Since virtually no EQT holders will be RMI residents, RMI securities law is unlikely to apply.
3. **No public offering in RMI**: The DAO Act exempts DAOs from the Marshall Islands Securities and Investment Act "to the extent that a DAO LLC is not issuing, selling, exchanging or transferring any digital securities to residents of the Republic."

**Conclusion under RMI law**: EQT is likely not subject to RMI securities regulation, but the operating agreement should include appropriate risk disclosures and the entity should not actively market EQT to RMI residents.

#### 2.3.3 US Securities Analysis

Under US law, the EQT is **very likely a security**. It confers economic rights (distributions, liquidation preference) and holders may have a reasonable expectation of profit from the efforts of the Series management team.

Under Howey:

| Howey Prong | Analysis | Risk Level |
|-------------|----------|------------|
| Investment of money | EQT is issued in exchange for capital contribution | High — clearly met |
| Common enterprise | Pooled capital in the Series entity | High — clearly met |
| Expectation of profits | Distributions, liquidation preference, appreciation | High — clearly met |
| Efforts of others | Series operations, management decisions | High — likely met for passive holders |

**Compliance strategy** — EQT must never be offered or sold to US persons without a valid exemption:

1. **Regulation D (506(b) or 506(c))**: Private placement to accredited investors. No general solicitation (506(b)) or verified accreditation with general solicitation allowed (506(c)). No SEC registration required. No limit on amount raised. Limited to accredited investors (or up to 35 sophisticated non-accredited under 506(b)).

2. **Regulation S**: Offshore transaction exemption. EQT can be sold to non-US persons in offshore transactions without SEC registration. Requires: (a) offer and sale made outside the US, (b) no directed selling efforts in the US, (c) compliance with distribution compliance period (40 days for Category 1, 1 year for Category 3 with flow-back restrictions).

3. **Hybrid Reg D + Reg S**: Recommended approach. US accredited investors participate via Reg D 506(c). Non-US investors participate via Reg S. The transfer hook enforces both restrictions programmatically.

**Transfer hook enforcement for US compliance**:

```
On every EQT transfer:
  1. Is receiver KYC-verified? → If no, REJECT
  2. Is receiver a US person?
     a. If yes: Is receiver accredited? → If no, REJECT
     b. If yes + accredited: Is lock-up period expired? → If no, REJECT
     c. If no (non-US person): Is distribution compliance period (Reg S) expired? → If no, REJECT
  3. Is sender's holding period satisfied? (Rule 144: 6-12 months for restricted securities)
  4. Is ROFR active? → If yes, has ROFR been cleared? → If no, REJECT
  5. Would transfer cause receiver to exceed max-holder count? → If yes, REJECT
  6. Is receiver a restricted person (sanctions)? → If yes, REJECT
```

#### 2.3.4 EU Securities Analysis (MiCA)

Under MiCA, the EQT likely qualifies as a **crypto-asset** and may fall under the asset-referenced token category if its value is linked to the performance of the Series entity. Requirements include:

- Publication of a crypto-asset whitepaper
- Authorization from a competent authority if offered to EU persons
- Ongoing disclosure obligations
- Marketing communications rules

**Recommendation**: Restrict EQT sales to EU persons until MiCA compliance is confirmed. The transfer hook can enforce EU-jurisdiction blocking.

#### 2.3.5 Token Issuance Flow

```
Series Multisig (Squads) — separate from Foundation multisig
    │
    ├── create_share_class()
    │   Creates EQT mint with:
    │   - Transfer Hook extension → compliance program
    │   - Permanent Delegate → Series Squads Vault PDA (clawback)
    │   - Default Account State → Frozen (thaw on KYC verification)
    │   - Metadata extension (share class name, URI to legal docs)
    │   - Mint authority = Series Squads Vault PDA
    │   - Authorized supply defined in operating agreement
    │
    ├── issue_shares()
    │   Mints EQT to investor/member:
    │   - Investor completes KYC (off-chain, hash stored on-chain)
    │   - Investor's token account thawed (unfrozen)
    │   - EQT minted to investor's token account
    │   - MemberRecord PDA created/updated
    │   - Vesting schedule created if applicable
    │   Requires: Series multisig approval
    │
    ├── create_vesting()
    │   Attaches vesting to an equity position:
    │   - Cliff period (e.g., 1 year)
    │   - Linear vesting (e.g., 4 years total)
    │   - Revocable by Series multisig (for employee grants)
    │   - Non-revocable (for investor allocations)
    │
    ├── claim_vested()
    │   Member claims vested tokens:
    │   - Program calculates vested amount from clock timestamp
    │   - Transfers tokens from escrow to member's account
    │   - Updates released_amount on VestingSchedule PDA
    │
    ├── distribute_profits()
    │   Pro-rata distribution to all EQT holders:
    │   - Calculates each holder's share proportional to balance
    │   - Distributes USDC (or SOL) from Series treasury vault
    │   - Requires: Series multisig approval + governance vote
    │
    └── force_transfer() / burn()
        Compliance enforcement:
        - Court order requiring share transfer
        - Sanctions violation requiring token burn
        - Uses Permanent Delegate authority
        - Requires: Series multisig + legal documentation
```

#### 2.3.6 EQT Token Configuration (Per Share Class)

```
Mint Address: [deployed per share class creation]
Decimals: 0 (one token = one membership unit)
Supply: Authorized max defined in operating agreement
  Issued: minted as shares are issued to members
Mint Authority: Series Squads Vault PDA
Freeze Authority: Series Squads Vault PDA

Token-2022 Extensions:
  - TransferHook: {
      program_id: [compliance_program_id],
      // Called on every transfer — enforces KYC, accreditation,
      // lock-up, ROFR, max holders, jurisdiction restrictions
    }
  - PermanentDelegate: Series Squads Vault PDA
    (enables forced transfer/burn for legal compliance)
  - DefaultAccountState: Frozen
    (new token accounts start frozen; thawed only after KYC)
  - Metadata: {
      name: "[Series Name] Equity — [Class]",
      symbol: "EQT-[CLASS]",
      uri: "arweave://[hash]" // Points to share class terms JSON
    }
```

### 2.4 How the Two Tokens Interact

#### 2.4.1 Governance Decisions

Decisions are categorized into three tiers based on scope and which token(s) participate:

**Tier 1: Foundation Governance (GOV only)**

These are decisions affecting the Foundation itself or cross-series matters:

- Amending the Foundation operating agreement
- Creating or dissolving a new Series entity
- Appointing or removing Foundation Special Delegates
- Setting Foundation-level policies (AML, KYC thresholds)
- Allocating Foundation treasury to Series entities
- Protocol upgrades (smart contract changes)
- Emergency freeze/pause of on-chain programs

Process: GOV holder (or delegatee) submits proposal → voting period → quorum check → if approved, Foundation multisig executes.

**Tier 2: Series Governance (EQT only)**

These are decisions affecting a specific Series entity only:

- Approving Series-level investments or expenditures
- Hiring or compensation decisions within the Series
- Approving profit distributions from the Series
- Series-specific operating agreement amendments
- Approving ROFR waivers for share transfers
- Series dissolution

Process: EQT holder submits proposal to Series governance → voting period (weighted by EQT balance) → quorum check → if approved, Series multisig executes.

**Tier 3: Cross-Entity Decisions (GOV + EQT)**

These are rare decisions requiring alignment between Foundation and Series:

- Merging two Series entities
- Transferring significant assets between Foundation and Series
- Changing the fundamental economic relationship between entities
- Major restructuring or redomiciliation

Process: Proposal submitted to both governance systems → parallel voting (GOV for Foundation approval, EQT for Series approval) → both must approve → respective multisigs execute coordinated transactions.

#### 2.4.2 Economic Decisions

Economic value flows only through EQT. GOV has no economic claim. The separation works as follows:

1. **Revenue generation**: Series entity earns revenue from operations.
2. **Profit calculation**: Series management (or algorithm) calculates distributable profit after expenses, taxes (3% GRT), and reserves.
3. **Distribution proposal**: Series EQT governance votes on whether to distribute, and how much.
4. **Distribution execution**: Series multisig executes USDC transfer to EQT holders proportionally.
5. **Foundation funding**: Separately, Foundation governance (GOV) may allocate Foundation treasury to fund Series operations — but this is a grant/loan, not a distribution.

GOV holders never receive economic distributions. If a GOV holder also holds EQT (which is permitted and common for founding members), they receive distributions solely in their capacity as EQT holders.

#### 2.4.3 The Bridge: Foundation Controls Series Creation

The Foundation has structural control over the Series entities:

- Only the Foundation governance (GOV vote) can authorize creation of a new Series.
- The Foundation's operating agreement establishes the template terms for all Series.
- The Foundation multisig holds the program upgrade authority, meaning it controls the smart contract code that all Series entities depend on.
- The Foundation can freeze Series operations in an emergency (via program-level freeze instruction).

This creates a checks-and-balances system:

- GOV holders control the rules of the game (protocol, policies, Series creation).
- EQT holders control the economic decisions within each Series.
- Neither can unilaterally override the other.

### 2.5 Token Issuance Restrictions

#### 2.5.1 GOV Issuance Restrictions

| Restriction | Rationale | Enforcement |
|-------------|-----------|-------------|
| No sale for monetary consideration | Preserves non-security status | Operating agreement prohibition; mint authority limited to Foundation multisig |
| Distribution only to verified participants | Prevents wash distribution | KYC check before minting; MemberRecord PDA must exist |
| Cannot exceed authorized supply | Cap defined in operating agreement | On-chain check: total_issued <= total_authorized |
| No secondary market listings | Strengthens non-security argument | Non-Transferable extension makes listing impossible |
| Revocable for restricted persons | AML/sanctions compliance | Permanent Delegate burn authority |

#### 2.5.2 EQT Issuance Restrictions

| Restriction | Rationale | Enforcement |
|-------------|-----------|-------------|
| Only to KYC-verified members | AML compliance; securities law | Default Account State Frozen; thaw requires KYC hash on MemberRecord PDA |
| US persons must be accredited investors | Reg D 506(c) compliance | Transfer hook checks accreditation flag on MemberRecord |
| Non-US persons subject to Reg S compliance period | SEC Reg S | Transfer hook checks jurisdiction + timestamp against compliance period |
| Lock-up periods per share class | Securities law; vesting | Transfer hook checks lock-up end timestamp on ShareClass PDA |
| ROFR for existing members | Operating agreement provision | Transfer hook checks TransferApproval PDA |
| Maximum holder count per share class | Regulatory compliance (e.g., 2,000 holders under SEC Rule 12g) | Transfer hook checks current_holders < max_holders |
| Cannot exceed authorized supply | Operating agreement cap | On-chain check: total_issued <= total_authorized |

---

## 3. Foundation-as-Escrow Architecture

### 3.1 Dual-Entity Structure Overview

```
┌─────────────────────────────────────────────────────────┐
│                FOUNDATION DAO LLC                         │
│            (Non-Profit, Marshall Islands)                 │
│                                                           │
│  Purpose: Protocol stewardship, treasury custody,        │
│           Series oversight, governance coordination       │
│                                                           │
│  Tax: Zero (non-profit)                                  │
│  Securities: All tokens exempt (non-profit issuer)       │
│  Token: GOV (governance, no economic rights)             │
│                                                           │
│  Controls:                                                │
│  ├── Foundation Treasury (Squads Vault 0)                │
│  ├── Program Upgrade Authority (Squads Vault 1)          │
│  ├── Series Creation Authority                           │
│  └── Emergency Freeze Authority                          │
│                                                           │
│  Squads Multisig: 3-of-5 (Foundation Special Delegates)  │
│                                                           │
├─────────────────────────────────────────────────────────┤
│                                                           │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│   │  Series A     │  │  Series B     │  │  Series C     │  │
│   │  (For-Profit) │  │  (For-Profit) │  │  (For-Profit) │  │
│   │              │  │              │  │              │  │
│   │ Token: EQT-A │  │ Token: EQT-B │  │ Token: EQT-C │  │
│   │ Tax: 3% GRT  │  │ Tax: 3% GRT  │  │ Tax: 3% GRT  │  │
│   │              │  │              │  │              │  │
│   │ Vault: Own   │  │ Vault: Own   │  │ Vault: Own   │  │
│   │ Multisig:    │  │ Multisig:    │  │ Multisig:    │  │
│   │ 2-of-3       │  │ 2-of-3       │  │ 2-of-3       │  │
│   │              │  │              │  │              │  │
│   │ Cap Table:   │  │ Cap Table:   │  │ Cap Table:   │  │
│   │ Independent  │  │ Independent  │  │ Independent  │  │
│   └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                           │
└─────────────────────────────────────────────────────────┘
```

### 3.2 The Foundation (Non-Profit DAO LLC)

#### 3.2.1 Legal Formation

The Foundation is formed as a **Non-Profit DAO LLC** under the Marshall Islands DAO Act (2022, as amended 2023). Key characteristics:

- **Entity name**: [Project] Foundation DAO LLC
- **Type**: Non-Profit DAO LLC
- **Governance model**: Member-managed (via GOV token)
- **Tax**: Zero — no corporate income tax, no capital gains, no withholding
- **Distribution**: Prohibited — no profit distribution to members
- **Securities**: All digital assets issued are automatically exempt from RMI securities law
- **Registered agent**: MIDAO (or alternative RMI-based agent)

#### 3.2.2 Foundation Purpose

The operating agreement defines the Foundation's purpose as:

1. **Protocol stewardship**: Maintaining, upgrading, and securing the on-chain smart contracts
2. **Treasury custody**: Holding and managing protocol treasury assets in trust for the ecosystem
3. **Series oversight**: Authorizing, monitoring, and (if necessary) dissolving Series entities
4. **Governance coordination**: Operating the governance system, facilitating proposals and votes
5. **Grants and ecosystem development**: Funding development, research, audits, and community initiatives
6. **Compliance management**: Maintaining AML/KYC policies, filing annual reports, managing registered agent relationship

The Foundation may NOT:

- Distribute profits to members
- Engage in commercial operations for its own profit
- Compete with Series entities
- Make investments with expectation of financial return (treasury management for preservation is permitted)
- Engage in lobbying or political activities (with limited exceptions per RMI non-profit law)

#### 3.2.3 Foundation Treasury via Squads Multisig

The Foundation treasury is held in a Squads Protocol v4 multisig with the following vault structure:

```
Foundation Squads Multisig
├── Members: 5 Foundation Special Delegates
├── Threshold: 3-of-5
├── Time lock: 48 hours for all transactions
│
├── Vault 0: Main Treasury
│   ├── Purpose: Protocol treasury, grants, ecosystem funding
│   ├── Assets: USDC, SOL, protocol tokens
│   ├── Spending limit: None (all transactions require full multisig)
│   └── Authority for: Grants to Series, ecosystem grants, operational expenses
│
├── Vault 1: Program Upgrade Authority
│   ├── Purpose: Sole authority to upgrade on-chain programs
│   ├── Assets: None (authority vault only)
│   ├── Time lock: 72 hours (longer than standard)
│   └── Authority for: BPF Loader upgrade instruction on cap table program
│
├── Vault 2: GOV Mint Authority
│   ├── Purpose: Minting and burning GOV tokens
│   ├── Assets: None (authority vault only)
│   └── Authority for: GOV token mint, GOV token burn (Permanent Delegate)
│
└── Vault 3: Emergency Reserve
    ├── Purpose: Emergency funds (legal defense, critical bug bounty)
    ├── Assets: USDC
    ├── Spending limit: $50,000 per transaction without full multisig
    └── Note: Only accessible with 4-of-5 approval for amounts > $50K
```

#### 3.2.4 Why Non-Profit?

The choice of non-profit for the Foundation is deliberate and legally significant:

1. **Zero tax**: The Foundation holds and manages significant treasury assets. As a non-profit, appreciation, interest, and investment returns are not taxed. A for-profit entity would owe 3% GRT on earned income and interest.

2. **Automatic securities exemption**: All tokens issued by a non-profit DAO LLC are automatically exempt from RMI securities law. This gives the GOV token maximum protection.

3. **Fiduciary clarity**: The Foundation's purpose is stewardship, not profit generation. This aligns incentives: Foundation Special Delegates serve the ecosystem, not shareholders.

4. **Structural independence**: By being non-profit, the Foundation cannot be captured by economic interests. It cannot be acquired, merged into a for-profit entity, or pressured by profit-seeking members.

5. **International credibility**: Non-profit foundations managing protocol treasuries are a well-established pattern (Ethereum Foundation, Solana Foundation, etc.). Regulators, banks, and counterparties understand this structure.

### 3.3 The Series DAO LLC (For-Profit)

#### 3.3.1 Legal Formation

Each Series is formed as a **Series within a For-Profit Series DAO LLC** under the 2023 Amendments to the Marshall Islands DAO Act. The parent Series DAO LLC is a single legal entity; each Series within it has:

- Separate assets
- Separate liabilities
- Separate governance
- Separate membership (cap table)

The parent Series DAO LLC is itself distinct from the Foundation. It is a separate legal entity, separately formed and registered.

#### 3.3.2 Series Entity Characteristics

| Property | Detail |
|----------|--------|
| **Entity name** | [Project] Ventures Series DAO LLC (parent); Series A, Series B, etc. (children) |
| **Type** | For-Profit Series DAO LLC |
| **Tax** | 3% GRT per Series on earned revenue and interest (cap gains and dividends excluded) |
| **Distribution** | Permitted — pro-rata to EQT holders of the respective Series |
| **Token** | EQT-[Series] — one equity token mint per Series, per share class |
| **Governance** | Series-level decisions by EQT holders; cross-Series decisions by Foundation GOV |
| **Liability** | Ring-fenced per Series — creditors of Series A cannot reach Series B assets |

#### 3.3.3 Relationship Between Foundation and Series LLC

The Foundation does NOT own the Series DAO LLC. They are separate legal entities with a defined relationship:

```
Foundation DAO LLC (Non-Profit)
│
├── Controls: Protocol smart contracts (upgrade authority)
├── Controls: GOV token (mint/burn)
├── Authorizes: Creation of new Series (governance vote)
├── Funds: Series entities via grants/loans from treasury
├── Monitors: Series compliance with Foundation policies
│
│   [No ownership or equity relationship]
│
Series DAO LLC (For-Profit)
│
├── Operates: Commercial activities
├── Issues: EQT tokens (membership interests)
├── Distributes: Profits to EQT holders
├── Reports to: Foundation on compliance matters
└── Depends on: Foundation-controlled smart contracts
```

The Foundation's structural power over Series entities derives from:

1. **Smart contract control**: The Foundation holds the program upgrade authority. The Series entities' cap tables, governance, and operations run on Foundation-controlled code.
2. **Series creation authority**: New Series can only be created via Foundation governance vote.
3. **Policy authority**: Foundation sets AML/KYC policies that all Series must follow.
4. **Emergency freeze**: Foundation can freeze program operations, halting all Series activities.

This is NOT an ownership relationship. The Foundation has no equity in any Series. It has structural/protocol authority, analogous to how a blockchain's governance controls the protocol rules that all participants must follow.

### 3.4 Fund Flow Architecture

#### 3.4.1 Investment Flow: Investor to Series

```
Investor
  │
  │ 1. KYC/AML verification (off-chain, hash stored on-chain)
  │
  ▼
Foundation Treasury (Squads Vault 0)
  │
  │ 2. Foundation receives investment funds
  │    (USDC transfer to Foundation vault)
  │    Foundation issues receipt / escrow confirmation
  │
  │ 3. Foundation governance (GOV vote) approves
  │    allocation to specific Series
  │
  ▼
Series Vault (Series-specific Squads Vault)
  │
  │ 4. Series multisig confirms receipt
  │    Series issues EQT tokens to investor
  │    Investor's MemberRecord PDA created
  │    Investor's token account thawed
  │
  ▼
Investor holds EQT-[Series]
  (represents membership interest in that Series)
```

**Why route through Foundation?**

1. **Escrow protection**: Investor funds are held in the Foundation's non-profit custody until the Series is ready to deploy capital. If the Series fails to materialize, the Foundation can return funds.
2. **Compliance gateway**: The Foundation's KYC/AML infrastructure verifies investors before any funds reach a commercial entity.
3. **Single counterparty**: Investors deal with one entity (the Foundation) regardless of which Series they invest in. Simplifies legal documentation.
4. **Treasury management**: The Foundation can pool investment funds and allocate them across Series efficiently, rather than each Series managing its own fundraising.

#### 3.4.2 Revenue Flow: Series Operations to EQT Holders

```
Series Operations (revenue-generating activities)
  │
  │ 1. Revenue received in Series Vault (USDC/SOL)
  │
  ▼
Series Vault
  │
  │ 2. Series calculates:
  │    - Gross revenue
  │    - Operating expenses
  │    - 3% GRT (payable to RMI)
  │    - Reserve allocation
  │    - Foundation contribution (if applicable per agreement)
  │    = Net distributable profit
  │
  │ 3. Series EQT governance votes on distribution
  │
  ▼
Distribution Execution
  │
  │ 4. Series multisig executes on-chain distribution:
  │    - For each EQT holder:
  │      distribution = (holder_balance / total_supply) * distribution_pool
  │    - USDC transferred from Series Vault to each holder's wallet
  │    - Distribution event recorded on-chain
  │
  ▼
EQT Holders receive USDC proportional to their equity
```

#### 3.4.3 Foundation Funding Flow

```
Foundation Treasury
  │
  │ 1. Foundation governance (GOV vote) approves grant/loan to Series
  │    Types:
  │    a. Grant (non-repayable): Series receives USDC, no obligation to return
  │    b. Loan (repayable): Series receives USDC, must repay per terms
  │    c. Convertible: Foundation receives EQT upon conversion event
  │       (NOTE: if Foundation receives EQT, this may affect non-profit status;
  │        legal counsel must review. Preferred approach: non-convertible grants.)
  │
  ▼
Series Vault
  │
  │ 2. Series deploys capital per its mandate
  │
  │ 3. If loan: Series repays to Foundation Treasury per schedule
  │
  ▼
Foundation Treasury (repayment received, if loan)
```

**Critical note on Foundation receiving EQT**: If the Foundation receives equity tokens in a Series entity, this could compromise its non-profit status because:
- Holding EQT means holding an economic interest
- Economic interests could be construed as "distributable" to Foundation members (even if not actually distributed)
- RMI regulators may view this as inconsistent with non-profit purpose

**Recommendation**: The Foundation should NOT hold EQT in any Series. Funding should be structured as grants (non-repayable) or loans (repayable in USDC/SOL, not equity). This preserves the Foundation's non-profit integrity.

### 3.5 Clawback and Compliance Mechanisms

#### 3.5.1 EQT Clawback (Permanent Delegate)

The Token-2022 Permanent Delegate extension grants the Series Squads Vault PDA the ability to transfer or burn any EQT token at any time, regardless of the holder's consent. This is the nuclear option, used only for:

| Trigger | Action | Authorization Required |
|---------|--------|----------------------|
| Court order requiring share transfer | Force transfer EQT from party A to party B | Series multisig + legal documentation (court order hash stored on-chain) |
| Sanctions violation (OFAC, UN, EU) | Burn EQT, freeze account | Series multisig + compliance officer attestation |
| AML violation | Freeze account, initiate investigation | Series multisig + compliance officer attestation |
| Fraud by member | Burn EQT per governance vote | Series EQT governance vote + Series multisig |
| Operating agreement violation | Burn EQT per operating agreement terms | Series multisig + legal counsel attestation |

**Process safeguards**:
1. All clawback actions require multisig approval (never unilateral).
2. A ComplianceAction PDA is created on-chain documenting the reason, legal basis, and approving parties.
3. The affected member is notified (off-chain) and given opportunity to cure (if applicable under the operating agreement).
4. Dispute resolution per the operating agreement (mandatory arbitration at the International Centre for Dispute Resolution).

#### 3.5.2 GOV Revocation

Similar to EQT clawback, the Foundation can burn GOV tokens using the Permanent Delegate. Triggers:

- Member becomes a Restricted Person (sanctions list match)
- Member violates AML policies
- Foundation governance votes to remove a member
- Member voluntarily surrenders governance rights

#### 3.5.3 Account Freeze

Using the Freeze Authority on each mint, the Foundation (for GOV) or Series (for EQT) can freeze individual token accounts. This is less severe than clawback — the tokens remain but cannot be transferred. Used for:

- Pending investigation (freeze while investigating potential violation)
- Lock-up enforcement (frozen until lock-up expires; thawed automatically via governance action)
- Regulatory hold (freeze pending regulatory inquiry)

#### 3.5.4 Emergency Program Freeze

The Foundation multisig (Vault 1) can invoke an emergency freeze instruction on the entire cap table program, halting all operations across all Series. This is the most extreme measure, reserved for:

- Critical smart contract vulnerability discovered
- Protocol-wide security incident
- Regulatory action requiring immediate cessation
- Coordinated attack on the governance system

The emergency freeze uses a shorter time lock (e.g., 6 hours) than standard operations (48 hours) to enable rapid response.

### 3.6 Tax Implications

#### 3.6.1 Foundation (Non-Profit)

| Tax Event | Treatment |
|-----------|-----------|
| Receiving investment funds (as escrow) | Not taxable — funds held in trust, not income |
| Treasury investment returns (yield, interest) | Not taxable — non-profit exemption |
| Treasury appreciation (SOL price increase) | Not taxable — no capital gains tax for RMI non-profits |
| Granting funds to Series entities | Not taxable — grant expenditure, not income |
| Receiving loan repayments from Series | Not taxable — return of principal. Interest component is questionable; structure as zero-interest if possible |
| GOV token issuance | Not taxable — no monetary consideration received |

#### 3.6.2 Series Entities (For-Profit)

| Tax Event | Treatment |
|-----------|-----------|
| Receiving investment funds (equity issuance) | Not taxable — capital contribution, not revenue |
| Receiving grants from Foundation | Not taxable — capital contribution |
| Operating revenue | **Taxable at 3% GRT** |
| Interest income | **Taxable at 3% GRT** |
| Capital gains (asset appreciation) | **Not taxable** — excluded from GRT |
| Dividends received | **Not taxable** — excluded from GRT |
| EQT distributions to members | Not taxable at entity level — distributions are not income to the entity (they reduce retained earnings) |

#### 3.6.3 Member-Level Tax

The Marshall Islands does not impose pass-through taxation. However:

- Members remain subject to tax laws of their own countries of residence.
- US members: EQT distributions are likely taxable as ordinary income or capital gains depending on characterization.
- The entity should provide necessary tax documentation (K-1 equivalent information, though not a formal K-1 since this is not a US entity).

#### 3.6.4 Structural Tax Efficiency

The two-entity structure maximizes tax efficiency:

1. **Treasury appreciation**: Held in the Foundation (non-profit), appreciation is untaxed. If held in a for-profit Series, 3% GRT would apply to interest income.
2. **Operating revenue**: Only the specific Series generating revenue pays 3% GRT. Other Series and the Foundation are unaffected.
3. **Cross-entity funding**: Foundation grants to Series are not taxable events for either entity.
4. **No double taxation**: Unlike a US C-Corp where profits are taxed at the corporate level and again on distribution, RMI has no pass-through tax. The 3% GRT is the only entity-level tax, and distributions are not taxed again at the entity level.

---

## 4. Technical Implementation

### 4.1 Solana Program Architecture

The on-chain system consists of three Solana programs:

```
Program 1: Foundation Program (realm_foundation)
├── Manages: Foundation entity PDA
├── Manages: GOV token mint
├── Manages: Foundation governance (proposals, votes)
├── Manages: Series creation authorization
├── Authority: Foundation Squads Vault PDA
└── Upgrade authority: Foundation Squads Vault PDA (Vault 1)

Program 2: Series Cap Table Program (realm_cap_table)
├── Manages: Series entity PDAs
├── Manages: EQT token mints (one per Series, per share class)
├── Manages: MemberRecord PDAs
├── Manages: VestingSchedule PDAs
├── Manages: Series governance (proposals, votes)
├── Manages: Distribution execution
├── Authority: Series Squads Vault PDA (per Series)
└── Upgrade authority: Foundation Squads Vault PDA (Vault 1)
    NOTE: Foundation controls upgrades, not Series. This ensures
    no Series can unilaterally modify the cap table program.

Program 3: Compliance Transfer Hook (realm_compliance_hook)
├── Called on: Every EQT token transfer
├── Validates: KYC status, accreditation, lock-up, ROFR, max holders
├── Reads: MemberRecord PDAs, ShareClass PDAs, TransferApproval PDAs
├── Does NOT modify state (read-only validation)
└── Upgrade authority: Foundation Squads Vault PDA (Vault 1)
```

### 4.2 Token-2022 Extensions Per Token Type

#### 4.2.1 GOV Token Mint Extensions

```rust
// GOV Token-2022 Mint Configuration
let extensions = vec![
    // Soulbound — cannot be transferred between wallets
    ExtensionType::NonTransferable,

    // Foundation vault can burn tokens for compliance
    ExtensionType::PermanentDelegate,

    // On-chain metadata (name, symbol, URI to governance charter)
    ExtensionType::MetadataPointer,
    ExtensionType::TokenMetadata,
];

// Metadata content
TokenMetadata {
    name: "[Project] Governance Token".to_string(),
    symbol: "GOV".to_string(),
    uri: "https://arweave.net/[governance-charter-hash]".to_string(),
    additional_metadata: vec![
        ("entity_type".to_string(), "foundation".to_string()),
        ("economic_rights".to_string(), "none".to_string()),
        ("jurisdiction".to_string(), "marshall_islands".to_string()),
    ],
}
```

#### 4.2.2 EQT Token Mint Extensions (Per Share Class)

```rust
// EQT Token-2022 Mint Configuration
let extensions = vec![
    // Compliance checks on every transfer
    ExtensionType::TransferHook,

    // Series vault can force-transfer/burn for legal compliance
    ExtensionType::PermanentDelegate,

    // New token accounts start frozen (thawed after KYC)
    ExtensionType::DefaultAccountState, // → AccountState::Frozen

    // On-chain metadata (share class name, legal docs URI)
    ExtensionType::MetadataPointer,
    ExtensionType::TokenMetadata,

    // Optional: Interest-bearing for preferred shares with dividends
    // ExtensionType::InterestBearingConfig,

    // Optional: Transfer fee for secondary market transactions
    // ExtensionType::TransferFeeConfig,
];
```

### 4.3 Squads Multisig Configuration

#### 4.3.1 Foundation Multisig

```
Foundation Multisig Configuration:
  Members: 5 (Foundation Special Delegates)
  Threshold: 3 of 5
  Time Lock: 48 hours (standard), 72 hours (program upgrades)

  Vault 0 — Main Treasury
    Seeds: [b"squad", multisig_key, 0u8]
    Authority for:
      - Disbursement of grants to Series
      - Ecosystem funding
      - Operational expenses
      - GOV token minting

  Vault 1 — Program Authority
    Seeds: [b"squad", multisig_key, 1u8]
    Authority for:
      - realm_foundation program upgrade
      - realm_cap_table program upgrade
      - realm_compliance_hook program upgrade
    Time Lock Override: 72 hours

  Vault 2 — GOV Mint Authority
    Seeds: [b"squad", multisig_key, 2u8]
    Authority for:
      - GOV token mint authority
      - GOV token freeze authority
      - GOV permanent delegate (burn)

  Vault 3 — Emergency Reserve
    Seeds: [b"squad", multisig_key, 3u8]
    Spending Limit: $50,000 USDC without full multisig
    Threshold Override: 4-of-5 for amounts > $50K
```

#### 4.3.2 Series Multisig (One Per Series)

```
Series [X] Multisig Configuration:
  Members: 3-7 (Series managers + Foundation delegate seat)
  Threshold: 2 of 3 (or M of N proportional to member count)
  Time Lock: 24 hours (standard), 48 hours (share issuance)

  Vault 0 — Series Treasury
    Authority for:
      - Series operational expenses
      - Profit distributions to EQT holders
      - Investment deployment

  Vault 1 — EQT Mint Authority
    Authority for:
      - EQT token mint authority (per share class)
      - EQT token freeze authority
      - EQT permanent delegate (clawback)

  NOTE: Foundation delegate seat
    One seat on every Series multisig is reserved for a
    Foundation-appointed delegate. This delegate can:
    - Observe all proposals
    - Vote on compliance-related matters
    - Veto transactions that violate Foundation policies
    This provides Foundation oversight without Foundation ownership.
```

### 4.4 Transfer Hook Logic for Equity Token Compliance

The compliance transfer hook is a separate Solana program invoked automatically on every EQT token transfer via the Token-2022 Transfer Hook extension.

#### 4.4.1 Hook Program Logic

```rust
use anchor_lang::prelude::*;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

#[program]
pub mod realm_compliance_hook {
    use super::*;

    /// Called by Token-2022 on every EQT transfer
    /// This function MUST succeed for the transfer to proceed.
    /// If it returns an error, the transfer is rejected.
    pub fn execute(ctx: Context<Execute>, amount: u64) -> Result<()> {
        let sender_member = &ctx.accounts.sender_member;
        let receiver_member = &ctx.accounts.receiver_member;
        let share_class = &ctx.accounts.share_class;
        let clock = Clock::get()?;

        // 1. SENDER VALIDATION
        // Sender must be an active, KYC-verified member
        require!(
            sender_member.status == MemberStatus::Active,
            ComplianceError::SenderNotActive
        );
        require!(
            sender_member.kyc_verified,
            ComplianceError::SenderKycRequired
        );

        // 2. RECEIVER VALIDATION
        // Receiver must be an active, KYC-verified member
        require!(
            receiver_member.status == MemberStatus::Active,
            ComplianceError::ReceiverNotActive
        );
        require!(
            receiver_member.kyc_verified,
            ComplianceError::ReceiverKycRequired
        );

        // 3. ACCREDITATION CHECK (US persons)
        if receiver_member.jurisdiction == Jurisdiction::UnitedStates {
            require!(
                receiver_member.accredited,
                ComplianceError::UsPersonNotAccredited
            );
        }

        // 4. LOCK-UP PERIOD CHECK
        if share_class.lockup_end > 0 {
            require!(
                clock.unix_timestamp > share_class.lockup_end,
                ComplianceError::LockupPeriodActive
            );
        }

        // 5. REG S DISTRIBUTION COMPLIANCE PERIOD
        if sender_member.jurisdiction != Jurisdiction::UnitedStates
            && share_class.reg_s_compliance_end > 0
        {
            require!(
                clock.unix_timestamp > share_class.reg_s_compliance_end,
                ComplianceError::RegSCompliancePeriodActive
            );
        }

        // 6. RIGHT OF FIRST REFUSAL (ROFR)
        if share_class.rofr_active {
            // Check that a TransferApproval PDA exists and is approved
            let transfer_approval = &ctx.accounts.transfer_approval;
            require!(
                transfer_approval.is_some(),
                ComplianceError::RofrApprovalRequired
            );
            let approval = transfer_approval.as_ref().unwrap();
            require!(
                approval.approved,
                ComplianceError::RofrNotApproved
            );
            require!(
                approval.sender == sender_member.wallet,
                ComplianceError::RofrWrongSender
            );
            require!(
                approval.receiver == receiver_member.wallet,
                ComplianceError::RofrWrongReceiver
            );
            require!(
                approval.amount >= amount,
                ComplianceError::RofrAmountExceeded
            );
        }

        // 7. MAX HOLDER COUNT
        if share_class.max_holders > 0 {
            // If receiver does not currently hold any tokens of this class,
            // this transfer adds a new holder
            let receiver_token_balance = ctx.accounts.receiver_token_account.amount;
            if receiver_token_balance == 0 {
                require!(
                    share_class.current_holders < share_class.max_holders,
                    ComplianceError::MaxHoldersReached
                );
            }
        }

        // 8. RESTRICTED PERSON CHECK
        require!(
            !receiver_member.is_restricted,
            ComplianceError::ReceiverRestricted
        );
        require!(
            !sender_member.is_restricted,
            ComplianceError::SenderRestricted
        );

        // 9. JURISDICTION BLOCKING
        // Check receiver is not in a blocked jurisdiction
        require!(
            !is_blocked_jurisdiction(&receiver_member.jurisdiction),
            ComplianceError::BlockedJurisdiction
        );

        Ok(())
    }
}

fn is_blocked_jurisdiction(jurisdiction: &Jurisdiction) -> bool {
    matches!(
        jurisdiction,
        Jurisdiction::NorthKorea
            | Jurisdiction::Iran
            | Jurisdiction::Syria
            | Jurisdiction::Cuba
            | Jurisdiction::Crimea
    )
}
```

#### 4.4.2 Transfer Hook Account Resolution

The transfer hook requires additional accounts beyond the standard transfer instruction. These accounts are resolved via the `ExtraAccountMetaList` on the mint:

```rust
// Extra accounts needed by the transfer hook
let extra_account_metas = vec![
    // Sender's MemberRecord PDA
    ExtraAccountMeta::new_with_seeds(
        &[
            Seed::Literal { bytes: b"member".to_vec() },
            Seed::AccountKey { index: 0 }, // entity PDA
            Seed::AccountKey { index: 2 }, // sender wallet (source authority)
        ],
        false, // is_signer
        false, // is_writable
    ),
    // Receiver's MemberRecord PDA
    ExtraAccountMeta::new_with_seeds(
        &[
            Seed::Literal { bytes: b"member".to_vec() },
            Seed::AccountKey { index: 0 }, // entity PDA
            Seed::AccountKey { index: 4 }, // receiver wallet (destination authority)
        ],
        false,
        false,
    ),
    // ShareClass PDA
    ExtraAccountMeta::new_with_seeds(
        &[
            Seed::Literal { bytes: b"share_class".to_vec() },
            Seed::AccountKey { index: 0 }, // entity PDA
            Seed::AccountKey { index: 1 }, // mint (share class mint)
        ],
        false,
        false,
    ),
    // TransferApproval PDA (optional — for ROFR)
    ExtraAccountMeta::new_with_seeds(
        &[
            Seed::Literal { bytes: b"transfer_approval".to_vec() },
            Seed::AccountKey { index: 0 }, // entity PDA
            Seed::AccountKey { index: 2 }, // sender wallet
            Seed::AccountKey { index: 4 }, // receiver wallet
        ],
        false,
        false,
    ),
];
```

### 4.5 Governance Voting via GOV Token + Proposal Execution via Multisig

#### 4.5.1 Foundation Governance Flow

```
1. GOV holder creates proposal
   └── Proposal PDA created with:
       ├── proposal_type: enum (AmendAgreement, CreateSeries, AppointDelegate, ...)
       ├── description_hash: [u8; 32] (SHA-256 of full proposal text on Arweave)
       ├── voting_start: i64
       ├── voting_end: i64
       ├── quorum: u64 (minimum GOV votes needed)
       ├── approval_threshold: u16 (basis points, e.g., 6600 = 66%)
       └── status: Voting

2. GOV holders vote
   └── Vote PDA created per voter:
       ├── proposal: Pubkey
       ├── voter: Pubkey
       ├── weight: u64 (1 per GOV token held, or delegated weight)
       ├── choice: enum (For, Against, Abstain)
       └── timestamp: i64

   Voting power calculation:
   - Direct: voter's GOV balance at snapshot (voting_start timestamp)
   - Delegated: sum of delegated GOV from other holders
   - Total weight = direct + delegated

3. Voting period ends
   └── Anyone can call finalize_proposal():
       ├── Tallies For, Against, Abstain
       ├── Checks quorum (total votes >= quorum requirement)
       ├── Checks approval (For / (For + Against) >= threshold)
       ├── Sets status: Approved or Defeated

4. If Approved → Execution via Foundation Multisig
   └── Foundation Special Delegates create a Squads transaction:
       ├── The transaction executes the approved action
       ├── 3-of-5 delegates must approve in Squads
       ├── 48-hour time lock before execution
       └── On execution, the on-chain action is performed

   Examples of execution:
   - CreateSeries: Initializes new Series entity PDA + EQT mint
   - AppointDelegate: Updates Foundation governance config
   - ProgramUpgrade: Deploys new program buffer (72-hour time lock)
   - GrantFunds: Transfers USDC from Foundation Vault 0 to Series Vault
```

#### 4.5.2 Series Governance Flow

```
1. EQT holder creates Series proposal
   └── SeriesProposal PDA created with:
       ├── series: Pubkey (Series entity PDA)
       ├── proposal_type: enum (Distribute, Invest, Hire, AmendSeries, ...)
       ├── description_hash: [u8; 32]
       ├── voting_start / voting_end
       ├── quorum: u64 (minimum EQT votes)
       ├── approval_threshold: u16
       └── status: Voting

2. EQT holders vote (same structure as Foundation, but weighted by EQT)

3. Finalization (same as Foundation)

4. Execution via Series Multisig
   └── Series managers create a Squads transaction
       ├── 2-of-3 (or M-of-N) approval
       ├── 24-hour time lock
       └── Foundation delegate seat can veto compliance violations
```

### 4.6 PDA Hierarchy for Foundation + Series Entities

```
Foundation Program (realm_foundation)
│
├── [b"foundation", foundation_id]
│   └── Foundation PDA
│       ├── authority: Pubkey (Foundation Squads Vault 0)
│       ├── gov_mint: Pubkey (GOV Token-2022 mint)
│       ├── name: String
│       ├── charter_hash: [u8; 32]
│       ├── series_count: u16
│       ├── member_count: u32
│       └── bump: u8
│
├── [b"gov_config", foundation_pda]
│   └── GovernanceConfig PDA
│       ├── foundation: Pubkey
│       ├── proposal_threshold: u64 (min GOV to create proposal)
│       ├── voting_period: i64 (seconds)
│       ├── quorum: u64
│       ├── approval_threshold: u16 (basis points)
│       ├── time_lock: i64 (seconds before execution)
│       └── bump: u8
│
├── [b"foundation_member", foundation_pda, wallet]
│   └── FoundationMember PDA
│       ├── foundation: Pubkey
│       ├── wallet: Pubkey
│       ├── gov_balance_snapshot: u64 (for vote weight at proposal creation)
│       ├── delegated_to: Option<Pubkey>
│       ├── delegated_weight: u64
│       ├── joined_at: i64
│       └── bump: u8
│
├── [b"foundation_proposal", foundation_pda, &proposal_id.to_le_bytes()]
│   └── FoundationProposal PDA
│       ├── foundation: Pubkey
│       ├── proposer: Pubkey
│       ├── proposal_id: u64
│       ├── proposal_type: ProposalType
│       ├── description_hash: [u8; 32]
│       ├── voting_start: i64
│       ├── voting_end: i64
│       ├── votes_for: u64
│       ├── votes_against: u64
│       ├── votes_abstain: u64
│       ├── status: ProposalStatus
│       ├── executed_at: Option<i64>
│       └── bump: u8
│
└── [b"foundation_vote", proposal_pda, voter_wallet]
    └── FoundationVote PDA
        ├── proposal: Pubkey
        ├── voter: Pubkey
        ├── weight: u64
        ├── choice: VoteChoice
        ├── timestamp: i64
        └── bump: u8


Cap Table Program (realm_cap_table)
│
├── [b"series", foundation_pda, &series_index.to_le_bytes()]
│   └── Series PDA
│       ├── foundation: Pubkey
│       ├── authority: Pubkey (Series Squads Vault)
│       ├── series_index: u16
│       ├── name: String
│       ├── operating_agreement_hash: [u8; 32]
│       ├── share_class_count: u8
│       ├── member_count: u32
│       ├── total_distributions: u64 (cumulative USDC distributed)
│       ├── status: SeriesStatus (Active, Frozen, Dissolved)
│       ├── created_at: i64
│       └── bump: u8
│
├── [b"share_class", series_pda, class_name.as_bytes()]
│   └── ShareClass PDA
│       ├── series: Pubkey
│       ├── name: String (e.g., "common", "series_a_preferred")
│       ├── mint: Pubkey (Token-2022 mint)
│       ├── total_authorized: u64
│       ├── total_issued: u64
│       ├── current_holders: u32
│       ├── max_holders: u32 (0 = unlimited)
│       ├── par_value_lamports: u64
│       ├── voting_weight: u16 (basis points)
│       ├── liquidation_preference: u16 (basis points)
│       ├── is_transferable: bool
│       ├── lockup_end: i64 (0 = no lockup)
│       ├── reg_s_compliance_end: i64
│       ├── rofr_active: bool
│       ├── requires_accreditation: bool
│       ├── created_at: i64
│       └── bump: u8
│
├── [b"member", series_pda, member_wallet]
│   └── MemberRecord PDA
│       ├── series: Pubkey
│       ├── wallet: Pubkey
│       ├── kyc_verified: bool
│       ├── kyc_hash: [u8; 32] (SHA-256 of off-chain KYC data)
│       ├── kyc_expiry: i64 (annual renewal)
│       ├── accredited: bool
│       ├── accreditation_expiry: i64
│       ├── jurisdiction: Jurisdiction (enum)
│       ├── is_restricted: bool
│       ├── joined_at: i64
│       ├── status: MemberStatus (Active, Suspended, Removed)
│       └── bump: u8
│
├── [b"vesting", series_pda, member_wallet, share_class_pda]
│   └── VestingSchedule PDA
│       ├── series: Pubkey
│       ├── member: Pubkey
│       ├── share_class: Pubkey
│       ├── total_amount: u64
│       ├── released_amount: u64
│       ├── start_time: i64
│       ├── cliff_time: i64
│       ├── end_time: i64
│       ├── schedule_type: VestingType (Linear, Graded, Cliff)
│       ├── revocable: bool
│       ├── revoked: bool
│       └── bump: u8
│
├── [b"transfer_approval", series_pda, sender_wallet, receiver_wallet]
│   └── TransferApproval PDA
│       ├── series: Pubkey
│       ├── sender: Pubkey
│       ├── receiver: Pubkey
│       ├── share_class: Pubkey
│       ├── amount: u64
│       ├── approved: bool
│       ├── approved_by: Pubkey (Series multisig)
│       ├── expires_at: i64
│       └── bump: u8
│
├── [b"series_proposal", series_pda, &proposal_id.to_le_bytes()]
│   └── SeriesProposal PDA
│       ├── (same structure as FoundationProposal, scoped to Series)
│       └── bump: u8
│
├── [b"series_vote", series_proposal_pda, voter_wallet]
│   └── SeriesVote PDA
│       ├── (same structure as FoundationVote)
│       └── bump: u8
│
├── [b"distribution", series_pda, &distribution_id.to_le_bytes()]
│   └── Distribution PDA
│       ├── series: Pubkey
│       ├── distribution_id: u64
│       ├── total_amount: u64 (USDC)
│       ├── total_supply_at_snapshot: u64 (EQT total supply)
│       ├── snapshot_timestamp: i64
│       ├── status: DistributionStatus (Pending, Executing, Complete)
│       ├── claimed_amount: u64
│       └── bump: u8
│
└── [b"claim", distribution_pda, member_wallet]
    └── DistributionClaim PDA
        ├── distribution: Pubkey
        ├── member: Pubkey
        ├── entitled_amount: u64
        ├── claimed: bool
        ├── claimed_at: Option<i64>
        └── bump: u8
```

### 4.7 Smart Contract Interactions Between Entities

#### 4.7.1 Series Creation (Foundation → Cap Table)

```
Foundation Governance Vote (GOV holders approve "Create Series X")
    │
    ▼
Foundation Multisig creates Squads transaction
    │
    ▼
Transaction calls realm_foundation::authorize_series_creation()
    │ This sets a flag on the Foundation PDA: pending_series = true
    │ And records: pending_series_config = { name, share_classes, etc. }
    │
    ▼
Foundation Multisig calls realm_cap_table::initialize_series()
    │ This instruction requires:
    │   - Foundation PDA (with pending_series = true)
    │   - Foundation authority (Squads Vault) as signer
    │   - Series configuration from Foundation
    │
    │ Creates:
    │   - Series PDA
    │   - ShareClass PDAs (one per class)
    │   - Token-2022 mints (one per class)
    │   - Series Squads multisig is set as Series authority
    │
    ▼
Series is live. Series multisig can now issue EQT, manage members, etc.
```

#### 4.7.2 Cross-Program Invocations (CPI)

```
realm_foundation
    ├── CPI → realm_cap_table::initialize_series()
    ├── CPI → realm_cap_table::freeze_series() (emergency)
    └── CPI → realm_cap_table::dissolve_series()

realm_cap_table
    ├── CPI → token_2022::mint_to() (issue EQT)
    ├── CPI → token_2022::transfer() (clawback via Permanent Delegate)
    ├── CPI → token_2022::freeze_account() / thaw_account()
    └── CPI → token_2022::burn() (compliance burn)

realm_compliance_hook
    ├── NO CPI — read-only validation
    ├── Reads: MemberRecord, ShareClass, TransferApproval PDAs
    └── Returns: Ok(()) or Error (transfer rejected)
```

#### 4.7.3 Distribution Execution Flow

```
1. Series EQT governance approves distribution of X USDC

2. Series multisig calls realm_cap_table::create_distribution()
   └── Creates Distribution PDA:
       ├── total_amount: X USDC
       ├── total_supply_at_snapshot: current EQT total supply
       ├── snapshot_timestamp: now
       └── status: Pending

3. Each EQT holder calls realm_cap_table::claim_distribution()
   └── Program calculates:
       │  entitled = (holder_balance_at_snapshot / total_supply_at_snapshot) * total_amount
       │
       ├── Creates DistributionClaim PDA
       ├── Transfers entitled USDC from Series Vault to holder's wallet
       └── Updates Distribution PDA: claimed_amount += entitled

4. After all claims (or after expiry), unclaimed USDC returns to Series Vault
```

---

## 5. Series DAO LLC Deep Dive

### 5.1 How Series Are Created and Managed

#### 5.1.1 Creation Process

1. **Proposal**: A GOV holder submits a Foundation-level proposal to create a new Series. The proposal includes:
   - Series name and purpose
   - Initial share class definitions (name, authorized supply, voting weight, liquidation preference)
   - Initial Series multisig members and threshold
   - Initial funding amount from Foundation treasury (if any)
   - Operating agreement terms specific to the Series

2. **Governance vote**: GOV holders vote. Quorum and approval threshold per Foundation governance config (recommended: 66% supermajority for Series creation).

3. **Multisig execution**: If approved, Foundation Special Delegates execute via Squads. The transaction:
   - Creates a new Squads multisig for the Series
   - Calls `authorize_series_creation()` on the Foundation program
   - Calls `initialize_series()` on the Cap Table program
   - Transfers initial funding (if any) from Foundation treasury to Series vault

4. **Series activation**: The Series is now live. Its multisig can issue EQT, onboard members, and begin operations.

#### 5.1.2 Ongoing Management

Each Series is operationally independent once created:

| Function | Who Manages | How |
|----------|-------------|-----|
| Day-to-day operations | Series management team | Off-chain decisions, on-chain execution via Series multisig |
| Share issuance | Series multisig | `issue_shares()` instruction + KYC verification |
| Profit distributions | Series EQT governance + multisig | Governance vote → multisig execution |
| Cap table changes | Series multisig | Add/remove members, vest/unvest, modify classes |
| Compliance | Foundation-appointed delegate on Series multisig | Monitors transactions, can veto violations |
| Annual reporting | Series management + Foundation compliance | Combined report filed with RMI registrar |

#### 5.1.3 Series Dissolution

A Series can be dissolved through:

1. **Voluntary**: Series EQT governance votes to dissolve → Series multisig executes wind-down → remaining assets distributed to EQT holders pro-rata (after satisfying liabilities) → Series PDA status set to Dissolved → token accounts frozen → final distribution processed.

2. **Foundation-initiated**: Foundation governance (GOV vote) determines a Series must be dissolved (e.g., for persistent non-compliance) → Foundation multisig calls `dissolve_series()` → forced wind-down follows same asset distribution process.

3. **Regulatory**: RMI registrar orders dissolution → Foundation and Series cooperate to wind down.

### 5.2 Liability Isolation Between Series

#### 5.2.1 Legal Basis

Under the 2023 Amendments to the Marshall Islands DAO Act, liabilities of one Series do NOT affect other Series or the parent entity:

- Each Series has its own assets held in its own Squads vault.
- Creditors of Series A cannot reach Series B's vault.
- Creditors of Series A cannot reach the Foundation's vault.
- A lawsuit against Series A does not create liability for Series B.

This liability isolation is reinforced by:

1. **On-chain separation**: Each Series has its own Squads multisig, its own vault PDAs, its own token mints. Assets are physically separated on-chain.
2. **Separate membership**: Series A's EQT holders are distinct from Series B's EQT holders (though overlap is permitted).
3. **Separate governance**: Series A's governance decisions are independent of Series B's.
4. **Operating agreement**: The parent Series DAO LLC operating agreement explicitly defines the liability separation and references the on-chain structures as authoritative.

#### 5.2.2 Maintaining the Liability Shield

To prevent veil-piercing between Series:

| Requirement | Implementation |
|-------------|---------------|
| No commingling of assets | Separate Squads vaults per Series. No cross-vault transfers without governance approval and documentation. |
| Adequate capitalization per Series | Minimum reserve requirement defined in operating agreement (e.g., 3 months operating expenses). |
| Separate books and records | Per-Series transaction history on-chain. Per-Series annual financial reporting. |
| Arms-length transactions between Series | Any cross-Series transaction requires documentation, fair market value, and governance approval from both Series. |
| Distinct identity | Each Series has its own name, purpose statement, and external-facing identity. |

#### 5.2.3 What Liability Isolation Does NOT Protect Against

- **Fraud**: If the same individuals use multiple Series to perpetrate fraud, all Series (and potentially members personally) may be liable.
- **Personal guarantees**: If a Series member personally guarantees a Series obligation, they are personally liable regardless of the LLC structure.
- **Piercing for alter ego**: If Series are not maintained as genuinely separate (no separate books, no separate governance, commingled assets), courts may pierce the Series separation.
- **Tax liability**: Each Series is independently subject to the 3% GRT. Non-payment by one Series does not create liability for others, but repeated non-compliance could trigger sanctions against the parent entity.

### 5.3 Per-Series Cap Tables

Each Series maintains its own independent cap table:

```
Series A Cap Table:
├── Share Class: Common
│   ├── Authorized: 10,000,000
│   ├── Issued: 3,500,000
│   ├── Holders:
│   │   ├── Founder 1: 1,000,000 (28.6%) — 4-year vest, 1-year cliff
│   │   ├── Founder 2: 1,000,000 (28.6%) — 4-year vest, 1-year cliff
│   │   ├── Employee Pool: 500,000 (14.3%) — various vesting schedules
│   │   └── Angel Investors: 1,000,000 (28.6%) — 1-year lockup
│   └── Voting Weight: 1x
│
├── Share Class: Series A Preferred
│   ├── Authorized: 5,000,000
│   ├── Issued: 2,000,000
│   ├── Holders:
│   │   ├── VC Fund 1: 1,200,000 (60%)
│   │   └── VC Fund 2: 800,000 (40%)
│   ├── Voting Weight: 2x
│   └── Liquidation Preference: 1x
│
└── Fully Diluted Cap Table:
    ├── Total Issued: 5,500,000
    ├── Common Voting Power: 3,500,000 * 1x = 3,500,000
    ├── Preferred Voting Power: 2,000,000 * 2x = 4,000,000
    └── Total Voting Power: 7,500,000
```

The cap table is the **on-chain token balances**. There is no separate spreadsheet. The Solana blockchain is the authoritative record, per the operating agreement.

### 5.4 Per-Series Governance

Each Series has its own governance configuration:

```rust
#[account]
pub struct SeriesGovernanceConfig {
    pub series: Pubkey,
    pub proposal_threshold: u64,        // Minimum EQT to create proposal
    pub voting_period: i64,             // Voting window (seconds)
    pub quorum_percentage: u16,         // Basis points (e.g., 2000 = 20%)
    pub approval_threshold: u16,        // Basis points (e.g., 5001 = 50%+1)
    pub time_lock: i64,                 // Delay before execution (seconds)
    pub proposal_types: Vec<ProposalType>, // Allowed proposal types
    pub bump: u8,
}
```

Different proposal types may have different thresholds:

| Proposal Type | Quorum | Approval | Time Lock |
|---------------|--------|----------|-----------|
| Operational expense < $10K | 10% | 50%+1 | 24 hours |
| Operational expense >= $10K | 20% | 50%+1 | 48 hours |
| Profit distribution | 30% | 50%+1 | 48 hours |
| Share issuance (new class) | 40% | 66% | 72 hours |
| Operating agreement amendment | 50% | 75% | 7 days |
| Series dissolution | 50% | 80% | 14 days |

### 5.5 Parent Foundation Governance Over Series Creation

The Foundation's authority over Series creation is a deliberate asymmetry:

1. **Only Foundation can create Series**: No Series can self-replicate or spawn sub-Series. This prevents uncontrolled proliferation.

2. **Foundation sets template terms**: The Foundation operating agreement defines default terms that all Series inherit (e.g., minimum KYC requirements, AML policies, compliance obligations).

3. **Foundation appoints compliance delegate**: Every Series multisig includes at least one Foundation-appointed seat. This is not a veto over business decisions — it is a compliance safeguard.

4. **Foundation controls the code**: Program upgrade authority rests with the Foundation. No Series can unilaterally modify the smart contract logic that governs cap table operations, transfer hooks, or governance.

5. **Foundation can freeze or dissolve Series**: In extreme cases (persistent non-compliance, fraud, regulatory order), Foundation governance can freeze or dissolve a Series.

This creates a **hub-and-spoke** governance model:

```
              GOV Holders
                  │
                  ▼
          Foundation Governance
         /        |         \
        /         |          \
   Series A   Series B   Series C
   (EQT-A)   (EQT-B)    (EQT-C)
```

### 5.6 On-Chain Representation of Series Hierarchy

```
Foundation PDA
├── foundation_id: "project_foundation"
├── series_count: 3
│
├── Series PDA (index=0)
│   ├── name: "Trading Fund"
│   ├── ShareClass: "common" (mint: 0xAAA...)
│   ├── ShareClass: "preferred" (mint: 0xBBB...)
│   ├── Members: [MemberRecord PDAs]
│   ├── Vesting: [VestingSchedule PDAs]
│   └── Governance: [SeriesGovernanceConfig PDA]
│
├── Series PDA (index=1)
│   ├── name: "Infrastructure"
│   ├── ShareClass: "common" (mint: 0xCCC...)
│   ├── Members: [MemberRecord PDAs]
│   └── Governance: [SeriesGovernanceConfig PDA]
│
└── Series PDA (index=2)
    ├── name: "Grants"
    ├── ShareClass: "common" (mint: 0xDDD...)
    ├── Members: [MemberRecord PDAs]
    └── Governance: [SeriesGovernanceConfig PDA]
```

Each Series is enumerable by its index. The Foundation PDA tracks the total count. Clients can iterate from 0 to `series_count - 1` to discover all Series, then enumerate share classes and members within each Series.

---

## 6. Risk Analysis

### 6.1 Regulatory Risks

#### 6.1.1 United States

| Risk | Likelihood | Impact | Description |
|------|-----------|--------|-------------|
| SEC classifies GOV as security | Low | High | Despite non-economic design and RMI safe harbor, SEC could argue GOV creates profit expectations via protocol value. Mitigated by soulbound (non-transferable) design, no sales, no listing. |
| SEC classifies EQT as security | High | High | EQT almost certainly qualifies as a security under Howey. This is expected and planned for. Compliance via Reg D 506(c) for US persons + Reg S for non-US. |
| CFTC jurisdiction claim | Low | Medium | If any Series engages in derivatives or futures, CFTC may claim jurisdiction. Mitigated by Series-level compliance and Foundation oversight. |
| FinCEN BSA requirements | Medium | Medium | If any Series processes money transmissions, BSA registration may be required. Mitigated by not engaging in money transmission activities. |
| IRS phantom income for US members | Medium | Low | US EQT holders may owe tax on allocations even if no distribution occurs (partnership taxation risk). Mitigated by entity-level election and clear operating agreement terms. |

**Mitigation strategies**:
- Never sell GOV tokens; distribute only via airdrops and governance mining
- Make GOV non-transferable (soulbound) — eliminates secondary market entirely
- Restrict EQT sales to US persons via Reg D 506(c) with verified accreditation
- Restrict EQT sales to non-US persons via Reg S with compliance period
- Geo-block US persons from GOV-related marketing materials
- Engage US securities counsel to review all token-related documentation
- File Form D with SEC for each Series conducting a Reg D offering

#### 6.1.2 European Union (MiCA)

| Risk | Likelihood | Impact | Description |
|------|-----------|--------|-------------|
| MiCA whitepaper requirement for GOV | Medium | Low | If GOV is offered to EU persons, a whitepaper may be required. Soulbound design may exempt it entirely. |
| MiCA authorization for EQT | Medium | Medium | EQT may require authorization from an EU competent authority if offered to EU persons. |
| DORA compliance for infrastructure | Low | Low | If the platform serves EU financial entities, Digital Operational Resilience Act requirements may apply. |

**Mitigation strategies**:
- Restrict EQT offerings in the EU until MiCA compliance is confirmed
- If GOV is soulbound (non-transferable), it likely falls outside MiCA scope
- Prepare MiCA-compliant whitepaper as a precautionary measure
- Monitor MiCA regulatory guidance as it develops (framework still new)

#### 6.1.3 FATF (Global AML/CFT)

| Risk | Likelihood | Impact | Description |
|------|-----------|--------|-------------|
| Travel Rule compliance | Medium | Medium | FATF Travel Rule requires VASPs to share sender/receiver information for transfers > $1,000. If entity.legal is classified as a VASP, must comply. |
| Enhanced due diligence | Medium | Low | Some jurisdictions require enhanced due diligence for RMI-domiciled entities. |
| Greylisting risk | Low | High | If Marshall Islands were added to FATF greylist, all entities would face increased scrutiny. Currently NOT greylisted. |

**Mitigation strategies**:
- Implement robust KYC/AML with tiered verification (per RMI DAO Act requirements)
- Use reputable KYC provider (Sumsub, Persona) with global coverage
- Maintain Beneficial Ownership Information Report (BOIR) current at all times
- Monitor FATF evaluations of Marshall Islands
- Structure entity.legal as a formation service, not a VASP (no custody, no exchange)

#### 6.1.4 Marshall Islands Domestic

| Risk | Likelihood | Impact | Description |
|------|-----------|--------|-------------|
| TCMI pre-approval changes | Medium | Medium | Trust Company of the Marshall Islands may tighten pre-approval requirements or change policies. |
| Regulatory framework changes | Low | High | The DAO Act is young (2022). Future amendments could restrict features we depend on. |
| Registered agent risk | Medium | High | Dependency on MIDAO as registered agent. If MIDAO ceases operations or is sanctioned, entities lose their registered agent. |
| Non-compliance penalties | Low | High | $500/day violations, up to $10,000 fines, possible certificate cancellation. |

**Mitigation strategies**:
- Maintain relationship with multiple registered agent options (OCI, Otonomos as backups)
- Monitor RMI legislative developments actively
- Ensure strict compliance with all filing deadlines and reporting requirements
- Budget for compliance costs in entity formation pricing
- Engage local counsel in RMI for ongoing regulatory monitoring

### 6.2 Smart Contract Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Transfer hook bypass | Low | Critical | Token-2022 enforces hook invocation at the protocol level — cannot be bypassed by the caller. The hook is part of the transfer instruction, not a separate call. |
| Permanent Delegate key compromise | Low | Critical | Delegate is the Series Squads Vault PDA, controlled by 2-of-3 (or M-of-N) multisig. Compromise requires multiple key compromise. Add time lock to clawback transactions. |
| Program upgrade attack | Low | Critical | Upgrade authority is Foundation Squads Vault with 72-hour time lock and 3-of-5 threshold. Community can detect and respond within 72 hours. |
| PDA collision | Negligible | Critical | PDA seeds include unique identifiers (entity key + wallet key + class name). Collision probability is astronomically low. Always use canonical bump. |
| Reentrancy | Low | High | Anchor framework provides reentrancy protection by default. Follow checks-effects-interactions pattern in all instructions. |
| Integer overflow in distribution calculation | Low | High | Use checked arithmetic (Rust default in debug, explicit in release). Use u128 for intermediate calculations. Test with maximum values. |
| Frozen account lockout | Medium | Medium | If Freeze Authority key is lost, accounts remain permanently frozen. Mitigated by Squads multisig as freeze authority (no single key). |
| Rent exemption exhaustion | Low | Low | All accounts are rent-exempt at creation. If Solana changes rent model, accounts already rent-exempt are grandfathered. |

**Audit plan**:
1. Phase 1 audit (post-MVP): Focus on core cap table logic, token issuance, and transfer hook. Auditor: OtterSec (Solana specialist). Estimated cost: $30,000-50,000.
2. Phase 2 audit (post-governance): Full system audit including governance, distributions, and Foundation program. Auditor: Trail of Bits or Zellic. Estimated cost: $50,000-80,000.
3. Ongoing: Bug bounty program (Immunefi) with tiered rewards.

### 6.3 Key Management Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Foundation multisig key compromise (single) | Medium | Low | Below threshold — attacker cannot act unilaterally. Remaining members rotate compromised key via Squads member management. |
| Foundation multisig key compromise (threshold) | Low | Critical | 3 of 5 keys compromised enables program upgrade, treasury drain. Mitigated by: 72-hour time lock, geographic distribution of signers, hardware wallets, public monitoring of proposals. |
| Series multisig key compromise (threshold) | Medium | High | Enables EQT issuance, treasury drain for that Series. Liability isolated to that Series. Foundation can freeze Series via emergency action. |
| Member wallet compromise | High | Low (per member) | Attacker can vote with stolen GOV, hold stolen EQT. Mitigated by: soulbound GOV (cannot sell stolen GOV), transfer hook restrictions on EQT (receiver must be KYC-verified). |
| Loss of all multisig keys | Negligible | Catastrophic | Total loss of control. Program becomes immutable (no upgrades). Treasury and mints become inaccessible. Mitigated by: hardware wallet backups, Shamir's Secret Sharing for individual keys, social recovery via Squads Smart Accounts. |

**Key management policy**:
1. All multisig members use hardware wallets (Ledger Nano X or equivalent)
2. Hardware wallets are stored in separate physical locations
3. Multisig members are distributed across at least 3 legal jurisdictions
4. Annual key rotation schedule (rotate one member per quarter)
5. Recovery procedure documented and tested annually
6. No single organization or individual controls majority of keys

### 6.4 Legal Enforceability Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Foreign court does not recognize RMI DAO LLC | Medium | High | Not all countries recognize Marshall Islands entities. Mitigated by Delaware precedent (RMI law follows Delaware), COFA with US, and growing international recognition (250+ DAOs). |
| Smart contract bug contradicts operating agreement | Low | High | Legal hierarchy is clear: operating agreement overrides smart contract code. But executing the legal remedy (modifying on-chain state to match legal ruling) requires multisig action. Mitigated by program upgrade capability. |
| Dispute resolution enforceability | Medium | Medium | Arbitration awards (ICDR) are generally enforceable under the New York Convention. However, enforcement against anonymous DAO members is practically difficult. Mitigated by KYC for 25%+ holders and off-chain identity verification. |
| Series liability isolation untested in court | Medium | High | No RMI court has tested Series DAO LLC liability isolation. The concept is modeled on Delaware Series LLC, which has limited case law. Mitigated by strict on-chain separation, separate books, and arms-length inter-Series transactions. |
| Clawback challenged in court | Medium | Medium | A member whose EQT is force-transferred via Permanent Delegate may challenge the action. Mitigated by clear operating agreement provisions authorizing clawback, documented compliance procedures, and arbitration clause. |
| Non-profit status challenged | Low | High | If RMI regulators determine the Foundation is engaging in for-profit activities, non-profit status could be revoked. Mitigated by strict adherence to non-profit purpose, no profit distributions, no equity holdings in Series entities. |

### 6.5 Comprehensive Risk Mitigation Matrix

| Risk Category | Top Risk | Mitigation | Residual Risk |
|--------------|----------|------------|---------------|
| **Regulatory** | SEC classifies EQT as security | Reg D 506(c) + Reg S compliance | Low (planned for and mitigated) |
| **Regulatory** | SEC classifies GOV as security | Soulbound, no sale, no listing, no economic rights | Low |
| **Smart Contract** | Permanent Delegate compromise | Multisig + time lock + key distribution | Low |
| **Smart Contract** | Program upgrade attack | 72-hour time lock + 3-of-5 + public monitoring | Low |
| **Key Management** | Threshold key compromise | Geographic distribution + hardware wallets + time locks | Low |
| **Legal** | Series isolation untested | Strict on-chain separation + Delaware precedent | Medium |
| **Legal** | Foreign court non-recognition | Delaware precedent + COFA + growing adoption | Medium |
| **Jurisdiction** | RMI regulatory changes | Monitor legislation + maintain backup agent relationships | Medium |
| **Operational** | KYC/AML failure | Reputable provider + automated monitoring + annual renewal | Low |
| **Technical** | Transfer hook bypass | Token-2022 protocol-level enforcement | Negligible |

---

## 7. Appendices

### Appendix A: Glossary

| Term | Definition |
|------|-----------|
| **DAO LLC** | Decentralized Autonomous Organization Limited Liability Company, as defined by the Marshall Islands DAO Act (2022) |
| **Series DAO LLC** | A DAO LLC containing multiple Series, each with separate assets, liabilities, and governance (2023 Amendment) |
| **GOV** | Governance Token — confers voting rights, no economic rights |
| **EQT** | Equity Token — represents membership interest with economic rights in a for-profit Series |
| **Foundation** | The non-profit DAO LLC that serves as protocol steward and treasury custodian |
| **Series** | An individual for-profit entity within the Series DAO LLC, with its own cap table and treasury |
| **PDA** | Program Derived Address — a deterministic Solana address controlled by a program, not a private key |
| **Squads** | Squads Protocol v4 — the dominant multisig standard on Solana |
| **Transfer Hook** | Token-2022 extension that invokes a custom program on every token transfer for compliance checks |
| **Permanent Delegate** | Token-2022 extension granting a designated authority the ability to transfer or burn any token of that mint |
| **Soulbound** | Non-transferable token (Token-2022 NonTransferable extension) — cannot be sent to another wallet |
| **ROFR** | Right of First Refusal — existing members have priority to purchase shares before external transfer |
| **GRT** | Gross Revenue Tax — the 3% tax on earned revenue and interest for RMI for-profit entities |
| **Reg D** | SEC Regulation D — exemption from registration for private placements to accredited investors |
| **Reg S** | SEC Regulation S — exemption from registration for offshore transactions with non-US persons |
| **MiCA** | Markets in Crypto-Assets Regulation — EU regulation for crypto assets effective June 2024 |
| **BOIR** | Beneficial Owner Information Report — RMI compliance filing identifying 25%+ holders |
| **MIDAO** | Marshall Islands DAO — exclusive government-authorized registered agent for DAO LLCs |
| **UBO** | Ultimate Beneficial Owner — at least one human who must be identified for KYC |

### Appendix B: Operating Agreement Reference Provisions

The following provisions MUST be included in the Foundation and Series operating agreements to support this architecture:

**Foundation Operating Agreement — Required Clauses**:

1. "The Foundation is organized as a Non-Profit DAO LLC under the Decentralized Autonomous Organization Act of the Republic of the Marshall Islands (2022, as amended 2023)."
2. "The GOV Token, as identified by its Solana Token-2022 mint address [ADDRESS], confers governance rights exclusively. No GOV Token shall confer, represent, or be deemed to carry any economic rights, including but not limited to distributions, dividends, profit sharing, liquidation preferences, or residual asset claims."
3. "The on-chain governance system, as implemented by the realm_foundation program at Solana program address [PROGRAM_ID], is hereby designated as the authoritative governance mechanism for Foundation decisions, subject to the legal hierarchy established in the DAO Act."
4. "The Foundation shall not distribute any profits, revenue, or assets to its members, officers, or delegates, except for reasonable compensation for services rendered and reimbursement of expenses."
5. "The Foundation shall not hold equity, membership interests, or economic claims in any Series entity within the [Project] Ventures Series DAO LLC."
6. "In the event of a conflict between the smart contract code and this Operating Agreement, this Operating Agreement shall prevail."

**Series Operating Agreement — Required Clauses**:

1. "This Series is organized as a For-Profit Series within the [Project] Ventures Series DAO LLC under the Decentralized Autonomous Organization Act of the Republic of the Marshall Islands (2022, as amended 2023)."
2. "The EQT-[CLASS] Token, as identified by its Solana Token-2022 mint address [ADDRESS], represents a membership interest in this Series, conferring pro-rata economic rights including distributions, liquidation preferences, and transfer rights as defined herein."
3. "The on-chain member registry, maintained as Token-2022 balances on the Solana blockchain, is the authoritative record of membership interests and ownership in this Series."
4. "Transfers of EQT Tokens are subject to the compliance restrictions enforced by the realm_compliance_hook program at Solana program address [PROGRAM_ID], including but not limited to KYC verification, accredited investor status, lock-up periods, jurisdiction restrictions, and right of first refusal."
5. "The Series multisig, as configured in the Squads Protocol at multisig address [ADDRESS], is authorized to exercise the Permanent Delegate authority on the EQT mint for the sole purposes of: (a) enforcing court orders, (b) sanctions compliance, (c) AML/CTF enforcement, and (d) operating agreement violations, subject to the procedural safeguards defined herein."
6. "No EQT Token shall be offered or sold to any person located in the United States unless such person is a verified accredited investor within the meaning of Rule 501(a) of SEC Regulation D."

### Appendix C: Compliance Checklist

**Pre-Formation**:
- [ ] Confirm non-profit purpose statement is compliant with RMI non-profit law
- [ ] Confirm no economic rights in GOV token design
- [ ] Confirm GOV token is soulbound (Non-Transferable extension)
- [ ] Confirm EQT token has Transfer Hook, Permanent Delegate, Default Account State (Frozen)
- [ ] Draft Foundation operating agreement with required clauses
- [ ] Draft Series operating agreement template with required clauses
- [ ] Engage RMI-licensed attorney to review operating agreements
- [ ] Prepare KYC/AML policy document
- [ ] Select and engage KYC provider (Sumsub, Persona)
- [ ] Identify at least one UBO per entity

**At Formation**:
- [ ] File Certificate of Formation for Foundation with RMI Registrar
- [ ] File Certificate of Formation for Series DAO LLC with RMI Registrar
- [ ] Appoint registered agent (MIDAO)
- [ ] Complete KYC for all founders and 25%+ holders
- [ ] Deploy Foundation program, Cap Table program, and Compliance Hook program to Solana mainnet
- [ ] Configure Foundation Squads multisig (5 members, 3-of-5 threshold)
- [ ] Transfer program upgrade authority to Foundation multisig vault
- [ ] Create GOV token mint with extensions
- [ ] Document all program addresses and mint addresses in formation documents
- [ ] Submit smart contract technical summary to RMI Registrar
- [ ] File FIBL applications

**Post-Formation (Annual)**:
- [ ] File annual report (January 1 - March 31)
- [ ] Update BOIR if membership changes
- [ ] Renew KYC for all 25%+ holders
- [ ] Pay registered agent fees
- [ ] File 3% GRT for each for-profit Series
- [ ] Review and update AML/KYC policies
- [ ] Conduct annual key rotation for multisig members
- [ ] Test disaster recovery / key recovery procedures

### Appendix D: Estimated Costs

| Item | One-Time | Annual |
|------|----------|--------|
| **Legal** | | |
| Foundation formation (MIDAO) | $9,500 | — |
| Series DAO LLC formation (MIDAO) | $9,500 | — |
| Registered agent | — | $5,500 |
| Legal counsel review | $10,000-20,000 | $5,000-10,000 |
| Reg D filing (Form D, per Series) | $1,000-2,000 | — |
| **Technical** | | |
| Solana program deployment (3 programs) | ~$15-60 | — |
| On-chain account creation (Foundation + 3 Series, 100 members) | ~$150 | — |
| Transaction costs (monthly operations) | — | ~$100 |
| Smart contract audit (Phase 1) | $30,000-50,000 | — |
| Smart contract audit (Phase 2) | $50,000-80,000 | — |
| Bug bounty program | — | $10,000-50,000 |
| KYC provider | — | $2,000-10,000 |
| **Infrastructure** | | |
| RPC provider (Helius/Triton) | — | $1,200-6,000 |
| Arweave storage (legal documents) | ~$50 | ~$20 |
| Server/hosting | — | $1,200-3,600 |
| **Total Estimated** | **$110,000-220,000** | **$25,000-86,000** |

---

## Document History

| Date | Version | Author | Changes |
|------|---------|--------|---------|
| 2026-02-23 | 1.0 | entity-legal domain | Initial specification |

---

## References

1. Marshall Islands DAO Act (2022) — PL 2022-50
2. Marshall Islands DAO Act Amendments (2023)
3. Marshall Islands DAO Regulations (2024)
4. MIDAO Documentation — https://docs.midao.org/
5. Odos DAO LLC Operating Agreement — https://docs.odos.xyz/home/dao/operating-agreement
6. Pyth DAO LLC — Solana-native governance precedent
7. SEC FAQs on Transfer Agent Blockchain Use (May 2025)
8. Solana Token-2022 Extensions Documentation
9. Squads Protocol v4 Documentation — https://docs.squads.so
10. SEC Regulation D — 17 CFR 230.501-508
11. SEC Regulation S — 17 CFR 230.901-905
12. EU Markets in Crypto-Assets Regulation (MiCA) — Regulation (EU) 2023/1114
13. FATF Guidance on Virtual Assets and VASPs (Updated 2023)
14. Galaxy Digital GLXY On-Chain Cap Table (Superstate, 2025)
