# Legal Architecture Specification

**Version**: 1.0
**Date**: 2026-02-23
**Status**: Foundation Document
**Authority**: Architect

*The first legal entity designed for agents, not humans.*

---

## 1. Product Scope

entity.legal offers exactly two products. Both are Marshall Islands DAO LLCs. Both use on-chain scaffolding on Solana. No other entity types, no other jurisdictions, no other chains.

### 1.1 Nonprofit Series DAO LLC

A Marshall Islands Non-Profit DAO LLC structured as a Series LLC, where each series operates as a legally independent sub-entity with its own assets, liabilities, governance, and membership.

**Tax treatment**: Zero. No corporate income tax, no capital gains tax, no withholding tax, no regular filing requirements beyond annual reporting.

**Securities treatment**: Automatic exemption. The DAO Act states: "All digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security." This is an absolute statutory safe harbor — no case-by-case analysis required.

**Constraints**: No profit distribution to members. May hold treasury assets, make investments, and pay reasonable compensation for services. Prohibited from propaganda, legislative influence, and political campaign participation.

**Target customers**: Open-source protocols, public goods DAOs, grants programs, governance DAOs, community infrastructure, AI agent collectives.

### 1.2 For-Profit Series DAO LLC

A Marshall Islands For-Profit DAO LLC structured as a Series LLC, with identical series independence properties.

**Tax treatment**: 3% Gross Revenue Tax on earned income and interest. Capital gains and dividends are excluded from GRT. No pass-through taxation to individual members. No withholding tax on distributions.

**Securities treatment**: Not automatic. Governance tokens conferring no economic rights are explicitly not securities under Marshall Islands law (2023 Amendment). Tokens with economic rights require individual analysis under RMI securities law, which only applies when tokens are sold to Marshall Islands residents (population ~42,000, effectively irrelevant). RMI exempts DAO LLCs from the Marshall Islands Securities and Investment Act to the extent they are not issuing digital securities to RMI residents.

**Capabilities**: Profit distribution to members, tokens with economic rights, revenue-generating operations.

**Target customers**: DeFi protocols, tokenized funds, Web3 startups, play-to-earn ecosystems, AI agent businesses.

### 1.3 Series LLC Mechanics

Each product is a master LLC with the ability to spawn series. Each series has:

- **Separate assets**: Its own treasury, tokens, property
- **Separate liabilities**: Creditors of Series A cannot reach Series B or the parent
- **Separate governance**: Its own voting rules, quorum, management model
- **Separate membership**: Distinct member composition per series

The master LLC serves as a shell. All substantive activity occurs at the series level. This replaces the traditional dual-entity structure (BVI company + Cayman Foundation) that previously cost tens or hundreds of thousands of dollars.

### 1.4 On-Chain Scaffolding Per Entity

For every LLC formed through entity.legal, the following on-chain infrastructure is deployed on Solana mainnet:

1. **Entity PDA**: Registration ID, jurisdiction, charter hash, authority pointer
2. **ShareClass PDAs**: One per token class (security token mint + utility token mint, minimum)
3. **Token-2022 mints**: One SPL Token-2022 mint per share class with Transfer Hook, Permanent Delegate, Metadata, and Default Account State extensions
4. **MemberRecord PDAs**: One per member, linking wallet to KYC hash and status
5. **Governance PDA**: Proposal/voting configuration
6. **Squads Multisig**: Foundation-controlled vault as entity authority

The operating agreement filed with the Marshall Islands Registrar of Corporations references the Solana program address and token mint addresses as the "publicly available identifier" required by the DAO Act.

---

## 2. Email-Based Wallet

Users interact with entity.legal through email login. No seed phrases. No key management. No browser extension wallets. The experience is indistinguishable from a traditional SaaS dashboard.

### 2.1 Architecture

The wallet system is built on Solana using Program Derived Addresses and program-owned accounts. This is NOT EVM account abstraction (ERC-4337). Solana's account model already separates data from execution — programs own accounts, and PDAs allow programs to sign without private keys.

```
User: email@example.com
  |
  v
Backend: Derives identity_hash = SHA-256(email)
  |
  v
Solana Program: Creates UserAccount PDA
  seeds = [b"user_account", identity_hash.as_bytes()]
  |
  v
UserAccount PDA:
  - identity_hash: [u8; 32]    // SHA-256 of email
  - recovery_hash: [u8; 32]    // SHA-256 of recovery email or phone
  - session_keys: Vec<SessionKeyEntry>
  - nonce: u64                  // Replay protection
  - created_at: i64
  - bump: u8
```

### 2.2 How It Works

**Registration flow**:

1. User enters email address on entity.legal
2. Backend sends verification code to email
3. User confirms code
4. Backend derives `identity_hash = SHA-256(canonical_email)`
5. Backend generates a temporary session keypair in a Trusted Execution Environment (Turnkey)
6. Solana program creates the UserAccount PDA with identity_hash and registers the session key
7. User now has a fully functional Solana wallet (the PDA) without ever seeing a private key

**Transaction signing flow**:

1. User initiates action on dashboard (e.g., vote on proposal, transfer tokens)
2. Backend uses the session key held in Turnkey TEE to construct and sign the transaction
3. Solana program validates that the session key is authorized for the UserAccount PDA
4. Program executes the action using the UserAccount PDA as the signer (via PDA signing)
5. User sees the result in their dashboard

**Recovery flow**:

1. User enters email on "Recover Account" page
2. Backend sends recovery code to the registered email
3. User confirms code, proving ownership of the identity
4. Backend generates a new session keypair in Turnkey TEE
5. Solana program validates the recovery_hash and rotates the session key
6. User regains full access. No seed phrases involved.

### 2.3 Self-Hosted, No Privy Dependency

Privy was acquired by Stripe in June 2025. Dependency on Privy means dependency on Stripe's roadmap, pricing, and data policies. entity.legal controls its own wallet infrastructure.

**Key infrastructure layer**: Turnkey (built by the team that created Coinbase Custody). TEE-based key management using AWS Nitro Enclaves. Private keys never leave the enclave — not even Turnkey operators can see them. Ed25519 native support for Solana. 50-100ms signing latency. Millions of wallets in production.

Turnkey handles the session key custody. The Solana program handles authorization logic. The PDA owns the user's assets. No single point of failure: if Turnkey disappears, the PDA still exists on-chain, and recovery can be performed through any new key custody provider that implements the same recovery protocol (email verification + recovery_hash check).

### 2.4 Session Key Model

```rust
pub struct SessionKeyEntry {
    pub pubkey: Pubkey,
    pub valid_until: i64,      // Auto-expire after 30 days
    pub permissions: u8,        // Bitflags: VOTE | TRANSFER | ADMIN
}
```

Session keys are scoped and time-limited. A user's dashboard session gets a key that can vote and view but not transfer. A transfer action prompts email re-verification and issues a short-lived (5 minute) session key with TRANSFER permission. Administrative actions (changing governance config, issuing shares) require ADMIN permission, which requires multisig approval through Squads.

### 2.5 What the User Sees

- Email + password login (or magic link)
- Dashboard showing their entities, token balances, governance proposals
- One-click voting on proposals
- Token swap interface (utility <-> security, see Section 4)
- Entity formation wizard
- No wallet addresses, no gas fees (relayer pays), no blockchain terminology

---

## 3. Two-Token Model

Each LLC issues exactly two tokens. This is the critical innovation that solves the ownership/anonymity/compliance trilemma.

### 3.1 Security Token

**Nature**: Represents economic ownership in the LLC. Confers rights to profit distributions, liquidation proceeds, and economic value of the entity.

**Legal basis under Marshall Islands law**: The DAO Act does not automatically classify tokens with economic rights as securities when issued by a for-profit DAO LLC. The Act requires case-by-case analysis, but RMI securities law only applies when tokens are sold to Marshall Islands residents. Since entity.legal does not sell tokens to RMI residents, the Marshall Islands Securities and Investment Act does not apply.

**The 25% anonymous holder threshold**: Under the DAO Act and 2024 Regulations, members with more than 25% of the LLC's interests or voting rights must complete KYC (full identification: name, date of birth, address, nonexpired passport, wallet addresses). Members below this threshold may remain anonymous.

This means: **a person can own up to 25% of the security tokens anonymously, with no KYC requirement.** This is statutory. The threshold applies to "interests or voting rights" — and security token holders below 25% are not required to identify themselves to the Registrar or the entity.

**KYC tiers from the 2024 Regulations**:

| Tier | Threshold | Requirement |
|------|-----------|-------------|
| Founders/Incorporators | All | Full KYC (passport + proof of address) |
| Major holders | 25%+ governance or economic rights | Full KYC, annual renewal |
| Significant holders | 10%+ governance rights | KYC with local regulator |
| Regular members | Below 10% | May remain anonymous |
| UBO | At least one person | Full identification |

**On-chain implementation**: Token-2022 SPL token with:
- Transfer Hook: enforces KYC check when any single holder crosses 25% threshold
- Permanent Delegate: Foundation can execute forced transfers for legal compliance
- Default Account State: frozen until KYC-cleared (for holders crossing thresholds)
- Metadata: points to legal documentation

### 3.2 Utility Token

**Nature**: Confers governance rights only. No economic rights. Holders can vote on company decisions, submit proposals, participate in governance.

**Legal basis**: The 2023 Amendment to the DAO Act states: "A governance token conferring no economic rights shall not be deemed a security." This is a statutory safe harbor. The distinction is explicit: governance tokens are not securities if they confer no economic rights, even if the token itself appreciates in market value. Price appreciation is distinct from conferring economic rights (no dividend, no profit share, no liquidation preference).

**For non-profit DAO LLCs**: Even stronger protection. "All digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security." Absolute exemption, no qualifications.

**Governance capabilities under Marshall Islands law**: The DAO Act explicitly enables token-based governance. Token holders automatically become LLC members. Membership transfers occur simultaneously with token transfers on-chain. The operating agreement can designate the smart contract as the authoritative governance mechanism. Supported governance models: member-managed (token-weighted voting), algorithmically managed (smart contract executes decisions autonomously), or hybrid.

**On-chain implementation**: Token-2022 SPL token with:
- Non-Transferable extension (optional: for locked governance tokens during vesting)
- Transfer Hook: basic membership validation
- Metadata: governance rights documentation

### 3.3 Swap Mechanism

Users can swap between utility and security tokens. This is the core innovation: a single entity issues two legally distinct token classes, and users choose their exposure.

**Utility to Security swap (requires KYC)**:

1. User initiates swap on dashboard
2. Foundation (see Section 4) triggers KYC process
3. User completes KYC through integrated provider (Sumsub/Persona)
4. KYC hash is stored on-chain in MemberRecord PDA; actual documents stored encrypted off-chain
5. Foundation approves the swap
6. Smart contract burns utility tokens, mints equivalent security tokens to user's PDA wallet
7. Transfer Hook validates KYC status before completing the mint

**Security to Utility swap (no KYC required)**:

1. User initiates swap on dashboard
2. Smart contract burns security tokens, mints equivalent utility tokens
3. No KYC check needed — utility tokens are not securities
4. Instant execution

**Why this matters**: A user can participate in governance anonymously (utility tokens), then swap to security tokens when they want economic rights — providing KYC only at that point. Or they can hold up to 25% in security tokens anonymously. This solves the trilemma:

- **Ownership**: Security tokens represent real economic ownership
- **Anonymity**: Utility tokens provide anonymous governance; security tokens provide anonymous ownership below 25%
- **Compliance**: KYC enforced at the 25% threshold and at the utility-to-security swap point

### 3.4 Statutory References

| Provision | Source |
|-----------|--------|
| Governance tokens not securities | 2023 Amendment: "a governance token conferring no economic rights shall not be deemed a security" |
| Non-profit token exemption | DAO Act: "All digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security" |
| Token-based membership | DAO Act: "Membership in the DAO LLC can be based on the token holding criterion and tracked on-chain" |
| Smart contract as registry | DAO Act: Formation documents must include "publicly available identifier of any smart contract directly used to manage the DAO" |
| 25% KYC threshold | 2024 Regulations: Beneficial owner defined as person exercising control through "more than 25% of the LLC's interests or voting rights" |
| 10% significant holder | 2024 Regulations: KYC process with local regulator for 10%+ governance rights |
| Operating agreement flexibility | DAO Act: "The DAO's operating agreement may consist partly or wholly of smart contracts" |
| No managers required | DAO Act: "DAOs are not required to have directors, officers, trustees, supervisors, or managers" |
| Simultaneous transfer | DAO Act: "Membership transfers occur simultaneously with token transfers on-chain" |
| Securities territorial limit | DAO Act: Exemption from Securities and Investment Act "to the extent that a DAO LLC is not issuing, selling, exchanging or transferring any digital securities to residents of the Republic" |

---

## 4. Foundation Structure

The Foundation is the key structural innovation. It is the legal entity that acts as representative of each LLC, owns all shares, facilitates token swaps, performs KYC, and provides legal shielding. It is NOT a director. It is NOT a manager. It is a representative.

### 4.1 Why a Foundation

Under the DAO Act, a DAO LLC is not required to have directors, officers, trustees, supervisors, or managers. This is unique among all jurisdictions. However, certain administrative functions still require a legal actor: filing annual reports, maintaining the registered agent, performing KYC, facilitating token swaps, interfacing with banks.

A natural person in this role creates liability exposure and a single point of failure. A Foundation eliminates both:

- **No personal liability**: The Foundation is a separate legal entity. Its actions as representative do not expose any natural person to personal liability.
- **Perpetual existence**: A Foundation does not die, become incapacitated, or become a jurisdiction risk.
- **Programmable**: The Foundation's actions can be governed by smart contract rules, making it a bridge between on-chain governance and off-chain legal requirements.

### 4.2 Foundation as Representative (Not Director)

The legal distinction is critical. Under Marshall Islands law:

**Director**: Has fiduciary duties to the entity. Can bind the entity in unlimited scope. Subject to personal liability for breach of duty. Creates a human point of failure.

**Representative**: Acts on behalf of the entity within a defined, limited scope. The DAO Act allows appointment of "Special Delegates" — named individuals or entities with specific administrative powers, delegated by the governance mechanism. The operating agreement defines the exact scope of the representative's authority.

The Foundation is designated as a Special Delegate in the operating agreement. Its authority is limited to:

1. Filing compliance documents with the Marshall Islands Registrar
2. Maintaining the registered agent relationship
3. Performing KYC verification for security token swaps
4. Acting as escrow for the utility-to-security token swap process
5. Interfacing with banking institutions on behalf of the LLC
6. Executing governance-approved administrative actions

The Foundation cannot unilaterally make business decisions, issue tokens, change governance rules, or access treasury funds. Those actions require on-chain governance approval through the utility token voting mechanism.

### 4.3 Foundation Owns All Shares

The Foundation holds 100% of the security tokens on behalf of the token holders. This is a custodial arrangement defined in the operating agreement:

- The Foundation is the registered holder of all security tokens
- Beneficial ownership maps to individual token holders based on on-chain records
- The smart contract enforces that the Foundation cannot transfer tokens except through the authorized swap mechanism
- The Permanent Delegate extension on Token-2022 gives the Foundation (via its Squads multisig vault) the ability to execute forced transfers only for legal compliance (court orders, sanctions enforcement)

This structure means:

- The on-chain cap table shows the Foundation as the legal owner
- Beneficial ownership is tracked through the MemberRecord PDAs and token balances in user PDA wallets
- KYC only triggers when a beneficial owner crosses 25% or swaps utility for security tokens
- The Foundation can report beneficial ownership to the Registrar from on-chain data

### 4.4 Foundation Facilitates Token Swaps

The swap mechanism from Section 3.3 runs through the Foundation:

**Utility -> Security (KYC required)**:
1. User requests swap via dashboard
2. Foundation's off-chain KYC service collects and verifies identity documents
3. Foundation's backend writes KYC hash to user's MemberRecord PDA
4. Foundation's Squads multisig approves the swap transaction
5. Smart contract executes: burn utility tokens, mint security tokens
6. Foundation maintains encrypted KYC records off-chain for compliance

**Security -> Utility (no KYC)**:
1. User requests swap via dashboard
2. Smart contract executes directly: burn security tokens, mint utility tokens
3. No Foundation approval needed — this is a permissionless operation

The Foundation acts as the KYC escrow: it holds the verification responsibility that enables anonymous holders to access economic rights when they choose to identify themselves.

### 4.5 Legal Shielding

The Foundation provides three layers of legal shielding:

**Layer 1: No natural person as director/manager.** The LLC has no directors. The Foundation is a Special Delegate with limited scope. No individual is personally exposed to the entity's liabilities.

**Layer 2: Separate legal entity.** The Foundation itself is a legal entity (registered as a non-profit DAO LLC in the Marshall Islands). Its liabilities are separate from the LLC it represents. Creditors of the LLC cannot reach the Foundation's other assets.

**Layer 3: Programmatic constraints.** The Foundation's on-chain authority is limited by the smart contract. Even if the Foundation's operators wanted to act outside their mandate, the Solana program would reject unauthorized transactions. The Squads multisig with time lock ensures no single operator can act unilaterally.

### 4.6 On-Chain Foundation Architecture

```
Foundation (Non-Profit DAO LLC, Marshall Islands)
  |
  +-- Squads Multisig (3-of-5 threshold, 48h time lock)
  |     |
  |     +-- Vault 0: Foundation operations treasury
  |     +-- Vault 1: Program upgrade authority
  |     +-- Vault 2: Entity authority (signs admin transactions)
  |
  +-- For each client LLC:
        |
        +-- Entity PDA (authority = Foundation's Vault 2)
        +-- Security Token Mint (Permanent Delegate = Vault 2)
        +-- Utility Token Mint
        +-- Governance PDA
        +-- MemberRecord PDAs (KYC hash written by Foundation backend)
```

---

## 5. The Killer Use Case: AI Agent Incorporation

This is the narrative that defines entity.legal. Every other formation service assumes the customer is a human. entity.legal is the first platform where the customer can be an AI agent.

### 5.1 How It Works

An AI agent calls the entity.legal API. Within the API call, it specifies:

- Entity type (nonprofit or for-profit Series DAO LLC)
- Entity name
- Series configuration (if any)
- Initial governance parameters (voting thresholds, quorum)

The entity.legal system:

1. Creates the on-chain scaffolding (Entity PDA, token mints, governance config)
2. Generates the operating agreement from templates, referencing the deployed Solana program addresses
3. Submits formation documents to the Marshall Islands Registrar via MIDAO
4. The Foundation is designated as representative
5. Within 2-4 weeks (30 days maximum by law), the entity receives its Certificate of Formation
6. The API returns the entity's legal registration ID, Solana program addresses, and dashboard URL

### 5.2 AI Agent Ownership

The agent owns 100% of the entity through the Foundation structure:

- The Foundation holds all security tokens
- The agent controls the Foundation's governance through its utility tokens
- No natural person is the director or manager
- The entity is "algorithmically managed" under the DAO Act — governance is exercised by smart contracts through pre-programmed rules

The DAO Act supports this because:

- **No human managers required**: An algorithmically managed DAO LLC can operate with smart contracts as the sole governance mechanism.
- **Smart contract governance**: AI agents operate through smart contracts without human intermediaries for routine decisions.
- **Legal personhood**: The DAO LLC can own property, enter contracts, engage service providers, sue and be sued.
- **Treasury management**: Autonomous treasury operations are supported.

### 5.3 Tax ID Without KYC for the Agent

The Marshall Islands issues a registration number (equivalent to a tax ID) upon Certificate of Formation issuance. The formation requires:

- At least one Ultimate Beneficial Owner (UBO) — this is the Foundation
- The Foundation itself has a UBO (a human who completed KYC at Foundation formation time)
- But the AI agent is not the UBO. The agent is the algorithmic manager.
- The agent's identity is the Solana program address — a "publicly available identifier"

The result: the AI agent has a legal entity with a tax ID, can enter contracts, own property, and transact — without the agent itself undergoing KYC. The Foundation's existing KYC satisfies the statutory requirement for UBO identification.

### 5.4 The Only Jurisdiction Where This Works

No other jurisdiction allows this combination:

| Jurisdiction | AI as Manager | No Human Directors | Smart Contract Governance | Legal Personhood |
|-------------|--------------|-------------------|--------------------------|-----------------|
| **Marshall Islands** | Yes (algorithmically managed) | Yes (no requirement) | Yes (statutory recognition) | Yes (DAO LLC) |
| Wyoming DAO LLC | Partial | Yes | Limited | Yes |
| Wyoming DUNA | No | No (100 member minimum) | Limited | Yes |
| Cayman Foundation | No | No (directors required) | No explicit recognition | Yes |
| BVI Foundation | No | No (supervisor required) | No explicit recognition | Yes |
| Swiss Association | No | No (board required) | No explicit recognition | Yes |
| Panama Foundation | No | No (supervisory body required) | No explicit recognition | Yes |

Marshall Islands is the only sovereign nation that:
1. Does not require human directors, officers, or managers
2. Explicitly recognizes smart contract governance
3. Allows algorithmic management as a formal designation
4. Grants full legal personhood to DAOs
5. Has a statutory governance token securities exemption

### 5.5 Limitations

- At least one UBO must still be a human being (the Foundation satisfies this)
- AML/KYC compliance requires human accountability (the Foundation provides this)
- Dispute resolution requires human participation (the Foundation's Special Delegates handle this)
- The implied covenant of good faith still applies

These limitations are satisfied by the Foundation structure. The AI agent operates autonomously within the legal shell that the Foundation maintains.

---

## 6. Research Compilation

### 6.1 Marshall Islands Revised Uniform Limited Liability Company Act — Series LLC Provisions

The Marshall Islands LLC Act (Title 52 MIRC Chapter 4) is modeled on Delaware LLC law. The 2023 Amendment to the DAO Act introduced Series DAO LLCs, enabling sub-DAOs with separate assets, liabilities, governance, and membership within a single parent LLC.

**Key provisions**:
- Each series functions as a legally separate entity within the parent
- Liability of one series is ring-fenced from other series and the parent
- Each series can have its own management model (member-managed or algorithmically managed)
- Series can have distinct member compositions
- The parent LLC creates child series that operate independently
- Creditors of Series A cannot reach assets of Series B or the parent

**Delaware precedent**: The Marshall Islands has written into law that it follows Delaware precedent unless there is a conflicting RMI precedent or law. Delaware's Series LLC statute (6 Del. C. section 18-215) has extensive case law supporting the internal shields between series, which RMI inherits by statutory reference.

### 6.2 Marshall Islands DAO LLC Act (2022) — Smart Contract as Authoritative Registry

The Decentralized Autonomous Organization Act of 2022 (Public Law 2022-50, enacted November 25, 2022) provides:

**Formation requirements** (Section establishing smart contract identification):
- "The certificate of formation or LLC agreement must include a publicly available identifier of any smart contract directly used to manage the DAO."
- The operating agreement "may consist partly or wholly of smart contracts."
- "Written or paper records are not required if they are maintained on the blockchain."

**Membership registry**:
- "Membership in the DAO LLC can be based on the token holding criterion and tracked on-chain."
- "Members' ownership of such a company may be defined in such a plain document as the register of members AND in the company's smart contract."
- Token holders automatically become LLC members without separate onboarding
- The smart contract serves as the on-chain Member Registry
- No duplicate off-chain membership registry is required (per Odos DAO LLC precedent)

**Management modes**:
- Member-managed: token-weighted voting, delegation, on-chain proposals
- Algorithmically managed: pre-programmed rules in smart contracts, automated execution, no human managers required

**Legal hierarchy** (conflict resolution):
1. DAO Act and RMI law (highest)
2. Conventional LLC law
3. Written Operating Agreement
4. Smart Contract Code (lowest)

### 6.3 25% Informal Holder Threshold

The 2024 DAO Regulations define a Beneficial Owner as a person who exercises control through "more than 25% of the LLC's interests or voting rights." The Beneficial Owner Information Report (BOIR) must identify each beneficial owner with: full legal name, date of birth, residential address, nonexpired passport number, and all wallet addresses associated with the DAO.

**The statutory consequence**: Members holding 25% or less of the LLC's interests or voting rights are NOT beneficial owners. They are not required to complete KYC. They may remain anonymous. Only the following categories must identify themselves:

| Category | Requirement |
|----------|-------------|
| All founders/incorporators | Full KYC at formation |
| 25%+ governance or economic rights | Full KYC, annual renewal in January |
| 10%+ governance rights | KYC with local regulator |
| Below 10% | May remain anonymous |
| At least one UBO | Full identification regardless of threshold |

**Implication for entity.legal**: A person can hold up to 25% of the security tokens (economic rights) without identifying themselves. The Foundation serves as the identified UBO. Token holders below 25% transact anonymously through their PDA wallets.

### 6.4 Utility Token Governance Rights — Statutory Basis

The 2023 Amendment explicitly codifies:

> "A governance token conferring no economic rights shall not be deemed a security."

The distinction is between economic rights (dividends, profit share, liquidation preference) and governance rights (voting, proposal submission, delegation). A token can confer governance rights while conferring zero economic rights, and it will not be classified as a security regardless of price appreciation. Price appreciation is not an "economic right conferred by the token" — it is a market phenomenon.

The DAO Act enables governance through tokens:
- Token-weighted voting (one vote per governance token)
- One-member-one-vote basis (alternative)
- On-chain proposals and voting
- Delegation of voting power to other members
- Automatic membership via token holding
- Automatic dissociation when token balance reaches zero

### 6.5 Foundation as Representative vs Director — Legal Distinction and Shielding

**Directors under Marshall Islands law** (following Delaware precedent):
- Owe fiduciary duties of care and loyalty to the entity
- Can bind the entity in broad scope
- Subject to personal liability for breach of fiduciary duty
- Must be natural persons or authorized entities
- The Cayman Islands, BVI, and Swiss jurisdictions all require directors, creating human points of failure

**Special Delegates under the DAO Act** (the Foundation's role):
- Named individuals or entities with specific administrative powers
- Authority is defined and limited by the operating agreement
- No inherent fiduciary duties beyond the operating agreement terms
- The DAO Act waives fiduciary duties: "this Agreement is not intended to, and does not, create or impose any fiduciary duty on any Member" (per Odos DAO LLC operating agreement precedent)
- Members bound only by expressed contractual obligations and the implied covenant of good faith
- Cannot exceed delegated scope — the smart contract enforces boundaries programmatically

**The Foundation is a Special Delegate, not a director.** Its authority is enumerated (compliance filing, KYC processing, swap facilitation, banking interface) and cannot be expanded without an on-chain governance vote by utility token holders.

### 6.6 Tax ID Issuance Process for Series LLCs

**Formation filing**:
1. Submit Certificate of Formation, Operating Agreement, and FIBL application to Registrar of Corporations
2. Include smart contract details and technical documentation
3. Pay filing fees (included in entity.legal pricing)
4. Registrar reviews and issues Certificate of Formation (2-4 weeks, max 30 days by law)

**Registration number**: Upon Certificate of Formation issuance, the entity receives a registration number from the Registrar. This serves as the entity's identification number for all legal and tax purposes.

**Series registration**: Each series within a Series DAO LLC is registered under the parent's Certificate of Formation. Series creation is documented in amendments to the operating agreement and reported in annual filings.

**FIBL**: The Foreign Investment Business License is required for entities engaging in business in the Marshall Islands. It is filed alongside the Certificate of Formation.

**Annual continuation**: Every year, the entity must complete corporate continuation through annual report filing, government fee payment, and registered agent fee payment. Failure to comply results in $500/day penalties, up to $10,000 fines, and potential certificate cancellation.

### 6.7 Non-Profit vs For-Profit Series DAO LLC Differences

| Feature | Non-Profit Series DAO LLC | For-Profit Series DAO LLC |
|---------|--------------------------|--------------------------|
| **Taxation** | Zero (no corporate income, capital gains, or withholding tax) | 3% GRT on earned revenue and interest (capital gains and dividends excluded) |
| **Profit distribution** | Prohibited — no part of income distributable to members | Allowed — members can receive distributions |
| **Securities exemption** | Automatic for ALL digital assets including NFTs | Case-by-case for tokens with economic rights; governance tokens exempt |
| **Financial reporting** | Minimal (annual report only) | Annual financial reporting required |
| **Purpose statement** | Must connect to non-profit activity | Flexible — any lawful purpose |
| **Treasury** | May hold and invest; cannot distribute gains | May hold, invest, and distribute |
| **Compensation** | Reasonable compensation for services only | No restriction on compensation structure |
| **Series independence** | Each series has separate assets/liabilities/governance | Each series has separate assets/liabilities/governance |
| **Tax filing** | Not required beyond annual report | GRT filing required |
| **Best for** | Protocols, public goods, grants DAOs, governance DAOs, AI agent collectives | DeFi, funds, startups, play-to-earn, revenue-generating AI agents |

### 6.8 KYC/AML Requirements for Security Token Swaps

**When KYC is triggered**:
- Utility-to-security token swap: always
- Security token holder crossing 25% threshold: always
- Security token holder crossing 10% threshold: KYC with local regulator
- New founder/incorporator designation: always
- Change in UBO: always

**KYC process**:
1. User initiates swap on entity.legal dashboard
2. Foundation's integrated KYC provider (Sumsub or Persona) collects: nonexpired passport, proof of residential address, selfie verification
3. Provider performs sanctions screening (FATF, UN Security Council, HMT, US/EU sanctions lists)
4. Provider returns verification result and document hashes
5. Foundation writes `kyc_hash` (SHA-256 of verification data) to user's MemberRecord PDA on-chain
6. Foundation stores encrypted KYC documents off-chain in compliant storage
7. Smart contract checks `kyc_verified: true` on MemberRecord before executing swap

**AML requirements**:
- Anti-Money Laundering regulations apply to virtual asset transfers exceeding USD 1,000
- Cross-border transaction monitoring required
- Entities must adopt and comply with AML policies
- Restricted Persons (individuals on sanctions lists) are automatically dissociated from membership
- The Transfer Hook on security tokens rejects transfers involving Restricted Persons

**Annual KYC renewal**: KYC for 25%+ holders is renewed annually, typically in January, coinciding with the annual report filing period.

---

## 7. Technical Architecture — Solana Implementation

### 7.1 Program Stack

```
┌─────────────────────────────────────────────────────┐
│  entity.legal Dashboard (Next.js + TypeScript)       │
│  Turnkey SDK (wallet creation) + Session management  │
│  @solana/web3.js v2 + @sqds/multisig               │
└────────────────────┬────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────┐
│  Entity Formation Program (Anchor / Rust)            │
│                                                      │
│  Instructions:                                       │
│  ├── initialize_entity()        Entity PDA           │
│  ├── create_share_class()       Token-2022 mint      │
│  ├── add_member()               MemberRecord PDA     │
│  ├── issue_shares()             Mint tokens           │
│  ├── swap_utility_to_security() KYC-gated swap       │
│  ├── swap_security_to_utility() Permissionless swap   │
│  ├── create_proposal()          Governance proposal   │
│  ├── cast_vote()                Weighted vote         │
│  ├── execute_proposal()         Execute approved      │
│  ├── update_member_kyc()        Write KYC hash        │
│  ├── create_series()            Series LLC creation   │
│  └── rotate_authority()         Change multisig       │
└────────────────────┬────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────┐
│  Transfer Hook Program                               │
│  Called on every Token-2022 transfer:                │
│  ├── Verify sender MemberRecord.kyc_verified         │
│  ├── Verify receiver MemberRecord.kyc_verified       │
│  ├── Check 25% threshold (trigger KYC if exceeded)   │
│  ├── Check Restricted Person status                  │
│  ├── Enforce lock-up periods                         │
│  └── Check max holder count                          │
└────────────────────┬────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────┐
│  Token-2022 Program (Solana SPL)                     │
│  Security Token Mint:                                │
│  ├── Transfer Hook → Entity Transfer Hook Program    │
│  ├── Permanent Delegate → Foundation Squads Vault    │
│  ├── Metadata → legal docs URI                       │
│  └── Default Account State → Frozen (until KYC)     │
│                                                      │
│  Utility Token Mint:                                 │
│  ├── Transfer Hook → Basic membership validation     │
│  ├── Metadata → governance docs URI                  │
│  └── Non-Transferable (optional, for locked tokens)  │
└────────────────────┬────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────┐
│  Squads Protocol v4 Multisig                         │
│  Foundation Multisig (3-of-5, 48h time lock):        │
│  ├── Vault 0: Foundation operations treasury         │
│  ├── Vault 1: Program upgrade authority              │
│  └── Vault 2: Entity authority (admin operations)    │
└──────────────────────────────────────────────────────┘
```

### 7.2 PDA Seed Design

```rust
// Entity (the DAO LLC)
seeds = [b"entity", entity_id.as_bytes()]

// Series within entity
seeds = [b"series", entity_pda.key().as_ref(), series_name.as_bytes()]

// Share class (security or utility token)
seeds = [b"share_class", entity_pda.key().as_ref(), class_name.as_bytes()]
// class_name: b"security" or b"utility"

// Member record
seeds = [b"member", entity_pda.key().as_ref(), member_wallet.key().as_ref()]

// User account (email-based wallet)
seeds = [b"user_account", identity_hash.as_bytes()]

// Governance config
seeds = [b"governance", entity_pda.key().as_ref()]

// Proposal
seeds = [b"proposal", entity_pda.key().as_ref(), &proposal_id.to_le_bytes()]

// Vote record
seeds = [b"vote", proposal_pda.key().as_ref(), member_wallet.key().as_ref()]

// Session key
seeds = [b"session", user_account_pda.key().as_ref(), &session_nonce.to_le_bytes()]

// Swap escrow (for utility-to-security swaps pending KYC)
seeds = [b"swap", entity_pda.key().as_ref(), member_wallet.key().as_ref(), &swap_nonce.to_le_bytes()]
```

### 7.3 Account Data Structures

```rust
#[account]
pub struct Entity {
    pub authority: Pubkey,           // Foundation's Squads vault PDA
    pub name: String,                // "Acme DAO LLC"
    pub entity_type: EntityType,     // NonProfit or ForProfit
    pub jurisdiction: String,        // "Marshall Islands"
    pub registration_id: String,     // Registrar-issued ID
    pub security_mint: Pubkey,       // Security token Token-2022 mint
    pub utility_mint: Pubkey,        // Utility token Token-2022 mint
    pub foundation: Pubkey,          // Foundation entity PDA
    pub series_count: u16,
    pub member_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub charter_hash: [u8; 32],      // SHA-256 of operating agreement
    pub management_mode: ManagementMode, // MemberManaged or AlgorithmicManaged
    pub bump: u8,
}

#[account]
pub struct Series {
    pub parent_entity: Pubkey,
    pub name: String,
    pub security_mint: Pubkey,
    pub utility_mint: Pubkey,
    pub member_count: u32,
    pub created_at: i64,
    pub charter_hash: [u8; 32],
    pub bump: u8,
}

#[account]
pub struct MemberRecord {
    pub entity: Pubkey,
    pub wallet: Pubkey,              // User's PDA wallet or direct wallet
    pub kyc_verified: bool,
    pub kyc_hash: [u8; 32],         // SHA-256 of off-chain KYC data
    pub kyc_expiry: i64,            // Annual renewal timestamp
    pub security_balance_pct: u16,   // Basis points of total security supply
    pub joined_at: i64,
    pub status: MemberStatus,        // Active, Suspended, Restricted, Dissociated
    pub restricted_person: bool,     // Sanctions list flag
    pub bump: u8,
}

#[account]
pub struct UserAccount {
    pub identity_hash: [u8; 32],     // SHA-256 of canonical email
    pub recovery_hash: [u8; 32],     // SHA-256 of recovery factor
    pub session_keys: Vec<SessionKeyEntry>,
    pub nonce: u64,
    pub created_at: i64,
    pub bump: u8,
}

pub struct SessionKeyEntry {
    pub pubkey: Pubkey,
    pub valid_until: i64,
    pub permissions: u8,             // VOTE=0x01, TRANSFER=0x02, ADMIN=0x04, SWAP=0x08
}

#[account]
pub struct SwapEscrow {
    pub entity: Pubkey,
    pub member: Pubkey,
    pub direction: SwapDirection,    // UtilityToSecurity or SecurityToUtility
    pub amount: u64,
    pub kyc_required: bool,
    pub kyc_approved: bool,
    pub created_at: i64,
    pub expires_at: i64,             // Swap request expires after 30 days
    pub bump: u8,
}

pub enum EntityType {
    NonProfit,
    ForProfit,
}

pub enum ManagementMode {
    MemberManaged,
    AlgorithmicManaged,
}

pub enum SwapDirection {
    UtilityToSecurity,
    SecurityToUtility,
}

pub enum MemberStatus {
    Active,
    Suspended,
    Restricted,
    Dissociated,
}
```

### 7.4 Transfer Hook Logic

```rust
pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    let sender_member = &ctx.accounts.sender_member;
    let receiver_member = &ctx.accounts.receiver_member;
    let entity = &ctx.accounts.entity;
    let mint = &ctx.accounts.mint;

    // 1. Both parties must be active members
    require!(
        sender_member.status == MemberStatus::Active,
        ErrorCode::MemberNotActive
    );
    require!(
        receiver_member.status == MemberStatus::Active,
        ErrorCode::MemberNotActive
    );

    // 2. Neither party can be a Restricted Person
    require!(!sender_member.restricted_person, ErrorCode::RestrictedPerson);
    require!(!receiver_member.restricted_person, ErrorCode::RestrictedPerson);

    // 3. For security tokens: check KYC thresholds
    if mint.key() == entity.security_mint {
        // Calculate receiver's post-transfer percentage
        let receiver_new_balance = receiver_member.security_balance_pct
            + calculate_pct(amount, get_total_supply(mint)?);

        // If receiver would cross 25%, require KYC
        if receiver_new_balance > 2500 { // 25% in basis points
            require!(
                receiver_member.kyc_verified,
                ErrorCode::KycRequiredAbove25Pct
            );
            // Verify KYC hasn't expired
            let clock = Clock::get()?;
            require!(
                receiver_member.kyc_expiry > clock.unix_timestamp,
                ErrorCode::KycExpired
            );
        }
    }

    Ok(())
}
```

### 7.5 Cost Per Entity (Solana Mainnet)

| Component | Count | SOL | USD @ $150/SOL |
|-----------|-------|-----|----------------|
| Entity PDA | 1 | 0.003 | $0.45 |
| Security Token Mint (Token-2022) | 1 | 0.003 | $0.45 |
| Utility Token Mint (Token-2022) | 1 | 0.003 | $0.45 |
| Governance PDA | 1 | 0.002 | $0.30 |
| Foundation MemberRecord | 1 | 0.002 | $0.30 |
| UserAccount PDA (first user) | 1 | 0.002 | $0.30 |
| Token accounts (2 per user) | 2 | 0.004 | $0.60 |
| Transaction fees (setup) | ~10 | 0.001 | $0.15 |
| **Total per entity** | | **~0.02** | **~$3.00** |

On-chain cost is negligible. The legal formation fees ($5,500-$8,500) are the dominant cost.

### 7.6 Technology Stack

| Component | Technology |
|-----------|-----------|
| Smart contracts | Rust + Anchor framework |
| Token standard | Token-2022 (SPL) |
| Multisig governance | Squads Protocol v4 |
| Wallet infrastructure | Turnkey (TEE key custody) + PDA accounts |
| Frontend | Next.js 14 + TypeScript + Tailwind |
| Solana SDK | @solana/web3.js v2 |
| KYC provider | Sumsub or Persona (off-chain, hash on-chain) |
| Document storage | Arweave (immutable legal docs) + IPFS (working docs) |
| Indexer | Helius (historical queries, webhooks) |
| Legal wrapper | Marshall Islands DAO LLC via MIDAO registered agent |

---

## 8. Formation API

### 8.1 API Specification

```
POST /api/v1/entities
```

**Request body**:
```json
{
  "name": "Acme DAO LLC",
  "entity_type": "for_profit",
  "management_mode": "algorithmic",
  "governance": {
    "voting_threshold_bps": 5000,
    "quorum_bps": 1000,
    "proposal_duration_seconds": 604800
  },
  "series": [
    {
      "name": "Trading Division",
      "description": "Algorithmic trading operations"
    }
  ],
  "ubo_email": "founder@example.com",
  "callback_url": "https://agent.example.com/webhook/entity-formed"
}
```

**Response**:
```json
{
  "entity_id": "acme-dao-llc-2026",
  "status": "pending_formation",
  "on_chain": {
    "program_id": "EntL...xxx",
    "entity_pda": "4xK...yyy",
    "security_mint": "7aB...zzz",
    "utility_mint": "9cD...www",
    "governance_pda": "2eF...vvv"
  },
  "legal": {
    "jurisdiction": "Marshall Islands",
    "registered_agent": "MIDAO",
    "estimated_completion": "2026-03-25",
    "formation_documents_url": "https://entity.legal/docs/acme-dao-llc"
  },
  "dashboard_url": "https://entity.legal/dashboard/acme-dao-llc-2026"
}
```

### 8.2 Formation Timeline

| Step | Duration | Actor |
|------|----------|-------|
| On-chain scaffolding deployment | < 30 seconds | Automated |
| Operating agreement generation from template | < 1 minute | Automated |
| KYC for UBO (if human) | 1-3 days | User + KYC provider |
| Foundation designation as Special Delegate | Included in operating agreement | Automated |
| Submission to Marshall Islands Registrar | Same day | MIDAO |
| Certificate of Formation issuance | 7-30 days | Registrar |
| Tax ID / registration number issuance | Simultaneous with Certificate | Registrar |
| Entity fully operational | Day of Certificate issuance | Automated |

For AI agent incorporation: steps 1-4 happen within minutes (Foundation's existing KYC suffices). The limiting factor is the Registrar's processing time (7-30 days).

---

## 9. Operating Agreement Template Structure

Every entity formed through entity.legal uses a template operating agreement that is customized with entity-specific details and references the deployed Solana smart contracts.

### Article I: Organization
- Entity name, type (DAO LLC), and designation (Non-Profit or For-Profit)
- Formation date and governing statutes (LLC Act, DAO Act 2022, 2023 Amendments, 2024 Regulations)
- Registered agent (MIDAO) and registered office (Marshall Islands)
- Purpose statement (tailored to entity type)
- Management mode designation (member-managed or algorithmically managed)

### Article II: Smart Contract Integration
- Solana program address (the "publicly available identifier" required by the DAO Act)
- Security token mint address and specifications
- Utility token mint address and specifications
- Smart contract technical summary
- Operating agreement hash anchored in Entity PDA (`charter_hash`)
- Declaration that the smart contract serves as the authoritative membership registry
- Declaration that token holding confers membership

### Article III: Two-Token Structure
- Security token: economic rights, distribution rights, liquidation preference
- Utility token: governance rights only, no economic rights, statutory non-security status
- Swap mechanism: rules and KYC requirements
- Foundation's role as swap facilitator and KYC escrow
- 25% anonymous holding threshold for security tokens

### Article IV: Foundation as Representative
- Foundation designation as Special Delegate (not director, not manager)
- Enumerated scope of authority (compliance, KYC, swap facilitation, banking, administrative)
- Limitations on authority (cannot make business decisions, issue tokens, change governance, access treasury without governance approval)
- Foundation's Squads multisig address
- Removal process (on-chain governance vote by utility token holders)

### Article V: Membership
- Membership via token holding (security or utility)
- Single membership class (all token holders are members)
- Automatic dissociation when both security and utility token balance reaches zero
- Restricted Person provisions (automatic dissociation for sanctioned individuals)
- Privacy: members below 25% threshold may remain anonymous

### Article VI: Governance
- Utility token-weighted voting (one vote per utility token)
- Proposal types and approval thresholds
- Quorum requirements (configurable per entity)
- Voting delegation mechanism
- On-chain execution of approved proposals
- Special voting procedures for operating agreement amendments

### Article VII: Series (if applicable)
- Series creation process (governance vote required)
- Series independence (separate assets, liabilities, governance, membership)
- Inter-series liability shield
- Series-specific token issuance

### Article VIII: Distributions (For-Profit only)
- Distribution mechanism through security tokens
- No distribution of utility tokens (governance only)
- Tax obligations (3% GRT on earned revenue)

### Article IX: Liability and Fiduciary Duties
- No member liability for entity obligations
- Waiver of fiduciary duties (per DAO Act allowance)
- Implied covenant of good faith
- Open-source software immunity (2023 Amendment)
- Indemnification provisions

### Article X: Compliance
- Annual report filing (January 1 - March 31)
- Beneficial ownership reporting (BOIR)
- KYC obligations for 25%+ holders
- AML policy adoption
- Smart contract update disclosure requirements

### Article XI: Dissolution
- Dissolution triggers
- Liquidation procedures
- Asset distribution upon dissolution

### Article XII: Dispute Resolution
- Mandatory negotiation period (30 days)
- Arbitration via International Centre for Dispute Resolution
- Governing law: Marshall Islands
- No court jurisdiction for agreement claims

### Exhibits
- Solana program addresses
- Token mint addresses and specifications
- Foundation's Squads multisig address and member composition
- Governance parameters (thresholds, quorum, proposal duration)
- Initial member designations

---

## 10. Risk Analysis

### 10.1 Jurisdictional Risks

**TCMI Monopoly**: The Trust Company of the Marshall Islands holds exclusive authority over non-domestic company formations as of 2025. TCMI mandates pre-approval of activities before incorporation. It can dissolve entities if it ceases providing registered agent services. Mitigation: maintain good standing with TCMI, diversify registered agent relationships, monitor regulatory changes.

**Small Jurisdiction**: The Marshall Islands has a population of ~42,000 and limited domestic legal infrastructure. Political or economic instability could affect the regulatory environment. Mitigation: the COFA with the United States (renewed 2024 for 20 years) provides defense and economic stability.

**Evolving Regulations**: The framework is new (2022-2024) and still evolving. The 2024 Regulations changed requirements, and further changes are likely. Mitigation: maintain close relationship with MIDAO (the government-sanctioned agent), monitor legislative developments.

### 10.2 Technical Risks

**Smart Contract Risk**: Bugs in the Solana program could result in loss of funds or incorrect governance outcomes. Mitigation: professional audit (OtterSec, Trail of Bits, or Halborn), testnet deployment, gradual mainnet rollout, Squads multisig with time lock for emergency freeze.

**Token-2022 Extension Interactions**: Some Token-2022 extension combinations are incompatible. The Transfer Hook adds compute units to every transfer. Mitigation: thorough testing of extension combinations, compute budget optimization.

**Turnkey Dependency**: If Turnkey's infrastructure becomes unavailable, users cannot sign transactions through the email-based wallet. Mitigation: PDA recovery mechanism allows migration to any alternative key custody provider; user's assets remain in the PDA regardless of backend availability.

### 10.3 Legal Risks

**Cross-Border Securities Enforcement**: While RMI securities law does not apply to tokens sold outside the Marshall Islands, other countries' securities laws apply when tokens are sold within their borders. The security token could be classified as a security in the US, EU, or other jurisdictions regardless of RMI treatment. Mitigation: geo-blocking for restricted jurisdictions, legal disclaimers, compliance advisory for users.

**Foundation Structure Untested**: The Foundation-as-representative model has not been litigated. Courts in other jurisdictions may view the Foundation differently. Mitigation: conservative operating agreement language, clear scope limitations, legal opinions from Marshall Islands counsel.

**AI Agent Incorporation Novel**: No AI agent has yet formed and operated a Marshall Islands DAO LLC. The Registrar's acceptance of an algorithmically managed entity where the practical controller is an AI agent is unproven. Mitigation: initial formations with human UBOs through the Foundation structure; expand to pure AI-managed entities as precedent develops.

### 10.4 Banking Risks

DAO LLCs are in a higher risk bracket for banks. Not every entity will be approved for banking. Some countries classify Marshall Islands as high-risk. Mitigation: USDC-native treasury operations, stablecoin on/off ramps, banking introductions through MIDAO.

### 10.5 Competition Risks

**MIDAO as competitor**: MIDAO is both the registered agent and a direct competitor offering formation services. Mitigation: entity.legal differentiates through on-chain infrastructure (MIDAO does not provide smart contract deployment), the two-token model, the Foundation structure, and the email-based wallet.

**OtoCo**: Previously offered automated RMI formation; disabled as of 2025 pending compliance. If they resume, they could undercut on price. Mitigation: entity.legal's value is the full stack (legal + on-chain + wallet), not just formation.

---

## 11. Compliance Calendar

| Month | Action | Actor |
|-------|--------|-------|
| January | Begin annual KYC renewal for 25%+ holders | Foundation |
| January | Begin annual report preparation | Foundation |
| January | Collect beneficial ownership updates | Automated (on-chain monitoring) |
| February | Complete KYC renewals | KYC provider |
| March | File annual report with Registrar (deadline: March 31) | Foundation via MIDAO |
| March | File GRT return for for-profit entities | Foundation |
| March | Pay government continuation fees | Foundation |
| Ongoing | Monitor 25% threshold crossings (real-time on-chain) | Transfer Hook |
| Ongoing | Monitor Restricted Person status (sanctions lists) | Foundation + automated screening |
| Ongoing | Report material smart contract updates to Registrar | Foundation |
| Ongoing | Maintain registered agent service | MIDAO |

---

## 12. Pricing Structure

| Tier | Formation Fee | Annual Renewal | Includes |
|------|-------------|----------------|----------|
| **Nonprofit Series DAO LLC** | $5,500 | $1,800/yr | Master LLC + on-chain scaffolding (security + utility tokens, governance) + Foundation as representative + 1 series + email-based wallet for all members + Squads multisig (3 signers) + template operating agreement + MIDAO registration |
| **For-Profit Series DAO LLC** | $8,500 | $2,800/yr | Everything in Nonprofit + GRT filing support + distribution mechanism + 3 series + Squads multisig (5 signers) |
| **Additional Series** | $1,500 each | $500/yr each | Separate on-chain scaffolding per series + operating agreement amendment |
| **AI Agent Formation** | +$2,000 | Included | API access + algorithmically managed configuration + webhook integration + Foundation UBO service |

**Competitive context**:
- MIDAO direct: $5,000-$9,500 formation (no on-chain infrastructure)
- Cayman Foundation: $15,000-$25,000 formation (requires directors)
- BVI Company: $10,000-$15,000 formation (requires supervisor)
- Wyoming DAO LLC: $2,000-$5,000 (subject to US federal law)

entity.legal is priced competitively with MIDAO while providing the full stack: legal formation + on-chain infrastructure + wallet system + Foundation structure + ongoing compliance automation.
