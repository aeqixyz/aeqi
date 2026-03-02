# Solana DAO Cap Table Architecture Research

**Date**: 2026-02-23
**Status**: Initial Research
**Domain**: entity-legal

---

## Table of Contents

1. [PDAs for Entity Membership & Equity Ownership](#1-pdas-for-entity-membership--equity-ownership)
2. [Squads Protocol v4 — Multisig Governance](#2-squads-protocol-v4--multisig-governance)
3. [Account Abstraction on Solana](#3-account-abstraction-on-solana)
4. [Cap Table Smart Contract Design](#4-cap-table-smart-contract-design)
5. [SPL Token vs Custom Program](#5-spl-token-vs-custom-program)
6. [Upgrade Safety](#6-upgrade-safety)
7. [Legal Binding — Marshall Islands DAO LLC](#7-legal-binding--marshall-islands-dao-llc)
8. [Existing Projects & Protocols](#8-existing-projects--protocols)
9. [Account Abstraction Without Privy](#9-account-abstraction-without-privy)
10. [Cost Analysis](#10-cost-analysis)
11. [Recommended Architecture](#11-recommended-architecture)

---

## 1. PDAs for Entity Membership & Equity Ownership

### How PDAs Work

Program Derived Addresses (PDAs) are deterministic addresses created from a program ID and a set of seeds. They are "off-curve" on Ed25519, meaning no private key exists for them — only the owning program can sign on their behalf. This makes them ideal for on-chain data storage controlled by program logic rather than individual key holders.

**Key properties:**
- Deterministic: same seeds always produce the same address
- No private key: only the program can authorize actions
- Canonical bump: the first valid bump seed that produces an off-curve address
- Independently accessible: can be read/written in parallel (Solana's parallelism advantage)

### PDA Seed Design for Cap Table

The seed design is the most critical architectural decision. For a DAO cap table, the following PDA hierarchy is recommended:

```
// Entity (the DAO LLC itself)
seeds = [b"entity", entity_id.as_bytes()]

// Share class definition
seeds = [b"share_class", entity_pda.key().as_ref(), class_name.as_bytes()]
// e.g., [b"share_class", entity_key, b"common"]
// e.g., [b"share_class", entity_key, b"series_a_preferred"]

// Member record (membership + equity position)
seeds = [b"member", entity_pda.key().as_ref(), member_wallet.key().as_ref()]

// Equity position per share class per member
seeds = [b"position", entity_pda.key().as_ref(), member_wallet.key().as_ref(), share_class_pda.key().as_ref()]

// Vesting schedule attached to a position
seeds = [b"vesting", position_pda.key().as_ref()]

// Transfer restriction rule
seeds = [b"restriction", entity_pda.key().as_ref(), share_class_pda.key().as_ref()]

// Governance config
seeds = [b"governance", entity_pda.key().as_ref()]

// Proposal
seeds = [b"proposal", entity_pda.key().as_ref(), &proposal_id.to_le_bytes()]

// Vote record
seeds = [b"vote", proposal_pda.key().as_ref(), member_wallet.key().as_ref()]
```

### Architecture Pattern: Hybrid Token + PDA

The recommended pattern is a **hybrid approach**:
- Use **Token-2022 SPL tokens** with extensions for each share class (the transferable, fungible representation)
- Use **PDAs** for metadata, restrictions, vesting schedules, and governance rules (the non-fungible control layer)

This gives you the composability of SPL tokens (wallets, DEXs, explorers all understand them) while maintaining the fine-grained control needed for regulated securities.

### Account Data Layout (Anchor)

```rust
#[account]
pub struct Entity {
    pub authority: Pubkey,          // Squads multisig vault
    pub name: String,               // "Acme DAO LLC"
    pub jurisdiction: String,       // "Marshall Islands"
    pub registration_id: String,    // MIDAO registration number
    pub share_class_count: u8,
    pub member_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub charter_hash: [u8; 32],     // SHA-256 of operating agreement
    pub bump: u8,
}

#[account]
pub struct ShareClass {
    pub entity: Pubkey,
    pub name: String,               // "Series A Preferred"
    pub mint: Pubkey,               // Token-2022 mint address
    pub total_authorized: u64,      // Max shares authorized
    pub total_issued: u64,          // Shares currently issued
    pub par_value_lamports: u64,    // Par value (0 for no-par)
    pub voting_weight: u16,         // Basis points (10000 = 1x, 20000 = 2x)
    pub is_transferable: bool,
    pub transfer_restriction: TransferRestriction,
    pub liquidation_preference: u64, // In basis points
    pub created_at: i64,
    pub bump: u8,
}

#[account]
pub struct MemberRecord {
    pub entity: Pubkey,
    pub wallet: Pubkey,
    pub kyc_verified: bool,
    pub kyc_hash: [u8; 32],        // Hash of off-chain KYC data
    pub accredited: bool,
    pub joined_at: i64,
    pub status: MemberStatus,       // Active, Suspended, Removed
    pub bump: u8,
}

#[account]
pub struct VestingSchedule {
    pub position: Pubkey,
    pub total_amount: u64,
    pub released_amount: u64,
    pub start_time: i64,
    pub cliff_time: i64,
    pub end_time: i64,
    pub schedule_type: VestingType, // Linear, Graded, Cliff
    pub revocable: bool,
    pub revoked: bool,
    pub bump: u8,
}
```

### Security Considerations

- Always validate canonical bump in Anchor constraints: `bump = entity.bump`
- Use `has_one` constraints to enforce relationship integrity
- Never allow PDA reinitialization without explicit close + recreate
- Store `bump` in account data to avoid recomputation

---

## 2. Squads Protocol v4 — Multisig Governance

### Overview

Squads Protocol is the dominant multisig standard on Solana, securing over $10B in value. The v4 release (current version) adds critical features for DAO governance:

- **Time locks**: Mandatory delay between proposal approval and execution
- **Spending limits**: Per-member withdrawal caps without full multisig approval
- **Roles**: Granular permission assignments (proposer, voter, executor)
- **Sub-accounts (Vaults)**: Isolated fund pools within a single multisig
- **Address Lookup Tables**: Support for complex transactions with many accounts

### Architecture

```
Squads Multisig
├── Multisig Config PDA (threshold, members, settings)
├── Vault PDA (index 0 — default treasury)
├── Vault PDA (index 1 — operations fund)
├── Vault PDA (index 2 — vesting escrow)
├── Transaction PDAs (proposals awaiting approval)
└── Spending Limit PDAs (per-member allowances)
```

The **Vault PDA** is the key concept. It is a PDA derived from the multisig account and a vault index. This vault PDA becomes the **authority** for your cap table program — it signs transactions on behalf of the multisig.

### Using Squads as Program Upgrade Authority

This is the recommended pattern for the cap table program:

1. **Deploy program** with a temporary keypair as upgrade authority
2. **Create Squads multisig** with the founding team as members (e.g., 3-of-5 threshold)
3. **Transfer upgrade authority** from the temporary keypair to the Squads Vault PDA
4. **All future upgrades** require multisig approval:
   - Developer writes new buffer with `solana program write-buffer`
   - Creates upgrade proposal in Squads
   - Required threshold of signers approve
   - Squads executes the BPF Loader upgrade instruction

**GitHub Action automation**: Squads provides a GitHub Action (`Squads-Protocol/squads-v4-program-upgrade`) that automates the CI/CD pipeline:
- Builds the program
- Deploys to a buffer account
- Creates an upgrade proposal in the Squads multisig
- Team reviews and approves via Squads UI

### Integration with Cap Table Program

The Squads multisig vault PDA becomes the `authority` field on your Entity account:

```rust
#[derive(Accounts)]
pub struct IssueShares<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub entity: Account<'info, Entity>,

    // This is the Squads vault PDA — the multisig must have approved this tx
    pub authority: Signer<'info>,

    // ... other accounts
}
```

For critical operations (issuing shares, modifying vesting, changing transfer restrictions), the instruction requires the Squads vault as signer, which means the multisig must have approved the transaction.

### Smart Account Program (Latest)

Squads has released the **Smart Account Program** (beyond v4), which provides:
- Rent-free wallet creation (as low as 0.0000025 SOL per wallet)
- Atomic policy enforcement
- Passkey support (coming Q2 2025, likely live by now)
- Programmable policies for customized wallet behavior

This could be used for member wallets in the DAO — each member gets a Squads Smart Account instead of managing raw keypairs.

### SDK

- **TypeScript**: `@sqds/multisig` — full SDK for creating multisigs, proposals, and executing transactions
- **Rust**: `squads-multisig` crate — for CPI from your Anchor program
- **Docs**: https://docs.squads.so

---

## 3. Account Abstraction on Solana

### Why Solana is Different from EVM

Solana does not have EVM-style account abstraction (ERC-4337) because its account model is fundamentally different:

- **All Solana accounts are already "abstracted"** — programs (smart contracts) own accounts, not EOAs
- **Programs can sign via PDAs** — no private key needed for program-controlled accounts
- **No EOA vs contract account distinction** — every account is just data owned by a program

However, Solana still has the UX problem: users need a keypair (wallet) to sign transactions. The "account abstraction" challenge on Solana is about **making wallet management invisible**.

### Equivalent Patterns

#### 1. PDA-Based Smart Accounts
The program itself acts as the "account abstraction layer":

```rust
// A user's smart account is a PDA owned by your program
seeds = [b"user_account", user_identity.as_bytes()]
```

The program validates identity through alternative mechanisms (social login, email hash, passkey) and then uses the PDA to interact with the cap table on the user's behalf.

#### 2. Session Keys
Temporary keypairs authorized to act on behalf of a user for a limited scope:

```rust
#[account]
pub struct SessionKey {
    pub owner: Pubkey,           // The real owner's wallet
    pub delegate: Pubkey,        // The temporary session key
    pub valid_until: i64,        // Expiry timestamp
    pub max_amount: u64,         // Spending cap
    pub allowed_programs: Vec<Pubkey>, // Which programs can be called
    pub bump: u8,
}
```

The program checks if the signer is either the owner OR a valid session key for the owner.

#### 3. Delegated Signers
A more permanent form of session keys — useful for employees or agents acting on behalf of a member:

```rust
#[account]
pub struct DelegatedSigner {
    pub owner: Pubkey,
    pub delegate: Pubkey,
    pub permissions: DelegatePermissions,  // Bitflags for allowed actions
    pub created_at: i64,
    pub revoked: bool,
    pub bump: u8,
}
```

#### 4. Durable Nonces for Offline/Async Signing
Solana's durable nonce mechanism allows transactions to be constructed, signed offline, and submitted later:

- Create a nonce account with `SystemProgram::CreateNonceAccount`
- Use the nonce value instead of `recent_blockhash`
- Transaction remains valid indefinitely until the nonce is advanced
- Useful for multi-party signing workflows where signers are not online simultaneously
- Nonce authority can be delegated to a different keypair

This is critical for multisig governance where members sign at different times.

#### 5. Swig Smart Wallets (Anagram)
Swig is the emerging standard for smart wallets on Solana:

- **Role-based permissions**: Junior, Manager, Admin roles with different capabilities
- **Session-based access**: Users approve once for a time window instead of per-transaction
- **Non-custodial**: Wallet lives on-chain in a Solana program
- **Pull payments**: Authorized parties can initiate withdrawals within limits
- **Multi-chain identity**: Can use ETH keys or Bitcoin keys to sign Solana transactions
- **Open source**: https://build.onswig.com/

---

## 4. Cap Table Smart Contract Design

### Share Classes

Each share class is represented as a **Token-2022 mint** with extensions, plus a **PDA metadata account**:

| Share Class | Token Mint | Extensions | Metadata PDA |
|-------------|-----------|------------|--------------|
| Common | `mint_common` | Transfer Hook, Metadata | Voting weight: 1x, No liquidation pref |
| Series A Preferred | `mint_series_a` | Transfer Hook, Metadata, Permanent Delegate | Voting weight: 2x, 1x liquidation pref |
| Series B Preferred | `mint_series_b` | Transfer Hook, Metadata, Permanent Delegate | Voting weight: 2x, 2x liquidation pref |
| Founder Common | `mint_founder` | Transfer Hook, Metadata | Voting weight: 10x, Subject to vesting |

### Vesting Schedules

Vesting is implemented as a **PDA-based escrow** with time-locked release:

```
[VestingSchedule PDA]
├── total_amount: 1,000,000 shares
├── released_amount: 250,000
├── start_time: 2026-01-01
├── cliff_time: 2027-01-01 (1 year cliff)
├── end_time: 2030-01-01 (4 year total)
├── schedule_type: Linear
├── revocable: true
└── revoked: false
```

**Vesting claim instruction flow:**
1. User calls `claim_vested()`
2. Program calculates vested amount based on `clock.unix_timestamp`
3. If past cliff, calculates linear/graded release
4. Mints or transfers tokens from escrow to user's token account
5. Updates `released_amount`

```rust
pub fn calculate_vested_amount(schedule: &VestingSchedule, now: i64) -> u64 {
    if now < schedule.cliff_time {
        return 0;
    }
    if now >= schedule.end_time {
        return schedule.total_amount;
    }
    let elapsed = (now - schedule.start_time) as u64;
    let total_duration = (schedule.end_time - schedule.start_time) as u64;
    (schedule.total_amount * elapsed) / total_duration
}
```

### Transfer Restrictions

Token-2022's **Transfer Hook** extension is the key mechanism:

```rust
// Transfer Hook Program — called on every token transfer
pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    let sender_member = &ctx.accounts.sender_member;
    let receiver_member = &ctx.accounts.receiver_member;
    let share_class = &ctx.accounts.share_class;
    let entity = &ctx.accounts.entity;

    // 1. Both parties must be KYC-verified members
    require!(sender_member.kyc_verified, ErrorCode::KycRequired);
    require!(receiver_member.kyc_verified, ErrorCode::KycRequired);

    // 2. Check accredited investor status if required
    if share_class.requires_accreditation {
        require!(receiver_member.accredited, ErrorCode::AccreditationRequired);
    }

    // 3. Check lock-up period
    if let Some(lockup_end) = share_class.lockup_end {
        let clock = Clock::get()?;
        require!(clock.unix_timestamp > lockup_end, ErrorCode::LockupActive);
    }

    // 4. Check right of first refusal (ROFR)
    // If ROFR is active, transfer must go through a proposal process
    if entity.rofr_active {
        let transfer_approval = &ctx.accounts.transfer_approval;
        require!(transfer_approval.approved, ErrorCode::RofrNotCleared);
    }

    // 5. Check maximum holder count (SEC Rule 12g)
    if share_class.max_holders > 0 {
        require!(
            share_class.current_holders < share_class.max_holders,
            ErrorCode::MaxHoldersReached
        );
    }

    Ok(())
}
```

### Voting Rights

Voting power is calculated from token balances weighted by share class:

```rust
pub fn calculate_voting_power(
    member: &Pubkey,
    entity: &Entity,
    share_classes: &[ShareClass],
    token_accounts: &[TokenAccount],
) -> u64 {
    let mut total_power: u64 = 0;
    for (class, account) in share_classes.iter().zip(token_accounts.iter()) {
        // voting_weight is in basis points (10000 = 1x)
        let weighted = (account.amount as u128 * class.voting_weight as u128) / 10000;
        total_power += weighted as u64;
    }
    total_power
}
```

### Token-Gated Governance

Proposals and voting are gated by token ownership:

```rust
#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(
        constraint = member_token_account.amount >= entity.min_proposal_tokens
            @ ErrorCode::InsufficientTokens
    )]
    pub member_token_account: Account<'info, TokenAccount>,

    #[account(
        has_one = entity,
        constraint = member_record.status == MemberStatus::Active
    )]
    pub member_record: Account<'info, MemberRecord>,

    // ... proposal creation accounts
}
```

---

## 5. SPL Token vs Custom Program

### Recommendation: Token-2022 (SPL) with Custom Anchor Program

This is not an either/or decision. The optimal architecture uses **both**:

| Layer | Technology | Purpose |
|-------|-----------|---------|
| Token representation | Token-2022 SPL tokens | Each share class = one mint. Standard wallets understand these. |
| Business logic | Custom Anchor program | Vesting, restrictions, governance, member management |
| Transfer control | Token-2022 Transfer Hook | Compliance checks on every transfer |
| Authority | Squads multisig vault | Program upgrade + entity admin operations |

### Why Token-2022 (Not Legacy SPL Token)

Token-2022 is the **only viable choice** for a cap table because of these extensions:

| Extension | Cap Table Use |
|-----------|--------------|
| **Transfer Hook** | KYC/AML compliance, accreditation checks, ROFR enforcement, lock-up periods |
| **Permanent Delegate** | Entity can burn/transfer shares for legal compliance (court orders, clawback) |
| **Metadata** | On-chain share class name, symbol, URI pointing to legal docs |
| **Confidential Transfers** | Hide balances/amounts for privacy (available post-Agave 2.0) |
| **Transfer Fee** | Automatic fee collection on secondary transfers |
| **Non-Transferable** | For non-transferable membership tokens (voting-only, no economic rights) |
| **Default Account State** | New token accounts start frozen until KYC is verified |
| **Interest-Bearing** | Represent dividend-accruing preferred shares |

### Pros of Token-2022 SPL Approach

1. **Wallet compatibility**: Phantom, Backpack, Solflare all display Token-2022 tokens
2. **Explorer visibility**: Solscan, Solana FM show token balances
3. **Composability**: Can integrate with DeFi protocols, lending, etc. (when appropriate)
4. **Audited**: The Token-2022 program is audited and battle-tested
5. **Standard tooling**: SDKs, CLIs, and libraries all support Token-2022
6. **Parallel processing**: Token accounts are independent, enabling Solana's parallelism

### Cons / Risks of Token-2022

1. **Extension interactions**: Some extension combinations are incompatible (e.g., NonTransferable + TransferHook)
2. **Complexity**: Transfer hooks add latency and compute units to every transfer
3. **Permanent Delegate risk**: If the delegate key is compromised, attacker controls all tokens of that mint
4. **No retroactive extensions**: Cannot add extensions to an existing mint — must be set at creation

### Pure Custom Program (Not Recommended)

A fully custom program that tracks ownership in PDAs without SPL tokens:

**Pros**: Total control, no token standard constraints
**Cons**: No wallet display, no explorer support, not composable, must build all token logic from scratch, harder to audit

---

## 6. Upgrade Safety

### Program Upgrade Mechanism on Solana

Solana programs are upgradeable by default. The **upgrade authority** is a single public key that can replace the program's executable code at any time. This is both a feature (bug fixes, new features) and a risk (malicious upgrades, compromised keys).

### Squads Multisig as Upgrade Authority

**Setup:**
```bash
# 1. Create Squads multisig (3-of-5)
# 2. Get vault PDA address
# 3. Transfer authority
solana program set-upgrade-authority <PROGRAM_ID> \
  --new-upgrade-authority <SQUADS_VAULT_PDA>
```

**Upgrade flow:**
1. Developer deploys new buffer: `solana program write-buffer <program.so>`
2. Developer creates proposal in Squads to execute `BpfLoaderUpgradeable::Upgrade`
3. 3 of 5 members approve the proposal
4. Any member executes the proposal
5. Program code is replaced atomically

### Time Lock Protection

Squads v4 supports **time locks** on transactions. For program upgrades, configure a mandatory delay (e.g., 48-72 hours) between approval and execution. This gives the community time to:
- Review the proposed changes
- Verify the buffer matches a known build (verifiable builds)
- Object or signal concern

### What If Upgrade Authority Is Compromised?

**Scenario 1: Single signer compromise (below threshold)**
- No impact — attacker cannot unilaterally upgrade
- Remaining signers should rotate the compromised key immediately via Squads member management

**Scenario 2: Threshold compromise (e.g., 3 of 5 keys)**
- Attacker can upgrade the program to malicious code
- **Mitigation**: Time lock gives community time to detect and respond
- **Response**: Remaining honest signers can create a counter-proposal to freeze operations
- If the program has a freeze instruction, honest signers can freeze before malicious upgrade executes

**Scenario 3: All keys compromised**
- Complete loss of control. Program can be upgraded to anything.
- **Final failsafe**: If the program has been made **immutable** (upgrade authority set to `None`), no upgrades are possible
- Trade-off: immutability means no bug fixes

### Best Practices

1. **Verifiable builds**: Use `solana-verify` or Squads' verify PDA to prove on-chain code matches public source
2. **Time locks**: 48-72 hour minimum for program upgrades
3. **Multi-jurisdiction signers**: Distribute keys across legal jurisdictions to prevent single-point seizure
4. **Hardware wallets**: All multisig members use Ledger/Trezor
5. **Gradual immutability**: Start upgradeable, set immutable once the program is mature
6. **Emergency freeze**: Include an instruction that freezes all operations, callable by multisig with shorter time lock
7. **On-chain governance vote**: For major upgrades, require an on-chain governance vote from token holders before multisig can execute

### Upgrade Authority Rotation

```rust
// Include an instruction to rotate upgrade authority if needed
pub fn rotate_authority(ctx: Context<RotateAuthority>) -> Result<()> {
    // Only current authority (Squads vault) can call this
    // Changes the entity authority to a new Squads vault
    ctx.accounts.entity.authority = ctx.accounts.new_authority.key();
    Ok(())
}
```

---

## 7. Legal Binding — Marshall Islands DAO LLC

### Marshall Islands DAO Act (2022, Amended 2023)

The Republic of the Marshall Islands (RMI) enacted the **Decentralized Autonomous Organization Act** in 2022, making it the first sovereign nation to recognize DAOs as legal entities. Key provisions:

- **Legal personhood**: DAO LLCs can own property, contract, sue and be sued
- **Smart contract governance**: On-chain governance is legally recognized and enforceable
- **No managers required**: Unlike Cayman foundations, no directors or managers needed
- **Token governance**: Governance tokens are explicitly not securities "if they don't confer any economic rights"
- **KYC requirement**: Members with 25%+ governance rights must complete KYC (name, address, passport)
- **AML/CFT monitoring**: On-chain activity is monitored for compliance
- **Registration**: Through MIDAO, 30-day maximum processing time

### Making On-Chain Records Legally Authoritative

The operating agreement is the key legal document. Under RMI law, it can explicitly point to smart contracts as the authoritative governance system. Here is what the Pyth DAO LLC does: the operating agreement simply references the Pyth governance smart contracts.

**Required on-chain data:**

| Data | On-Chain Storage | Legal Purpose |
|------|-----------------|---------------|
| Operating Agreement Hash | `entity.charter_hash: [u8; 32]` | SHA-256 of the signed operating agreement. Proves the on-chain entity maps to a specific legal document. |
| Member Registry | `MemberRecord` PDAs | Authoritative record of who is a member. KYC hash stored on-chain, actual KYC data off-chain. |
| Share Ownership | Token-2022 balances | Authoritative cap table. Token balance = share ownership. |
| Governance Actions | `Proposal` + `Vote` PDAs | Authoritative record of DAO decisions. Proposal text hash + vote tallies. |
| Transfer History | Solana transaction log | Immutable audit trail of all share transfers. |
| Entity Metadata | `Entity` PDA | Registration ID, jurisdiction, name. |

**Required off-chain data (referenced by on-chain hashes):**

| Data | Storage | Reference |
|------|---------|-----------|
| Full operating agreement text | IPFS/Arweave | CID stored in Entity PDA or metadata |
| KYC documents | Encrypted off-chain (compliant provider) | Hash stored in MemberRecord |
| Board resolutions | IPFS/Arweave | Hash stored in Proposal PDA |
| Financial statements | Off-chain (accountant) | Hash stored periodically on-chain |

### Minimum Viable On-Chain Representation

At minimum, the on-chain system must provide:

1. **Entity identity**: Name, jurisdiction, registration number
2. **Member registry**: Wallet addresses mapped to verified identities (via KYC hash)
3. **Ownership ledger**: Token balances per share class per member
4. **Governance mechanism**: Proposal + voting system with recorded outcomes
5. **Operating agreement anchor**: Hash of the legal document that references the smart contracts
6. **Transfer audit trail**: Immutable record of all ownership changes (inherent in Solana's ledger)

### MIDAO Registration Process

- **Cost**: Starting at $5,999 for DAO LLC formation (per MIDAO pricing)
- **Requirements**: Operating agreement, registered agent, KYC for 25%+ members
- **Timeline**: Up to 30 days
- **Ongoing**: Annual renewal, compliance monitoring
- **Entity types**: Member-managed, algorithmically-managed, or hybrid

### Legal Architecture Pattern

```
Operating Agreement (off-chain, signed PDF)
├── References: Smart contract program ID on Solana mainnet
├── Defines: "The Cap Table shall be maintained exclusively on-chain"
├── Defines: "Governance decisions shall be executed through on-chain voting"
├── Defines: Share classes and their rights/restrictions
├── Defines: Amendment procedures (require on-chain governance vote)
└── Stored: IPFS/Arweave with CID anchored in Entity PDA

Smart Contract (on-chain)
├── Entity PDA: charter_hash = SHA-256 of operating agreement
├── ShareClass PDAs: Encode the share classes defined in the agreement
├── MemberRecord PDAs: Link wallets to verified identities
├── Token-2022 Mints: Authoritative ownership record
└── Governance PDAs: Proposal/voting system matching agreement terms
```

---

## 8. Existing Projects & Protocols

### Realms / SPL Governance

**What it is**: The standard governance framework on Solana, built by Solana Labs as part of the Solana Program Library.

**Features:**
- Token-weighted voting
- Multi-signature governance
- Treasury management
- Proposal lifecycle (draft, voting, executing, completed/defeated)
- Plugin system for custom voting logic

**Used by**: Pyth, Mango, Metaplex, Jupiter, MonkeDAO, and hundreds of other DAOs

**Cap table relevance**: Realms handles governance (proposals, voting, execution) but does NOT handle:
- Share class management
- Vesting schedules
- Transfer restrictions
- KYC/accreditation
- Legal metadata

**Recommendation**: Do NOT build on Realms directly. It is a governance framework, not a cap table system. Instead, build a custom program that can optionally integrate with Realms for the governance layer, or build governance natively.

### Magna

**What it is**: "Carta for Web3" — token cap table management and distribution platform.

**Features:**
- On-chain vesting through audited smart contracts
- Cap table visualization and management
- Airdrop execution
- Tax deduction tracking
- Integration with Squads multisig on Solana
- $2.4B+ TVL in audited contracts
- Audited by OtterSec, Trail of Bits, Zellic, Guardian Audits

**Cap table relevance**: Magna is the closest existing solution. However:
- It is a SaaS platform, not open-source infrastructure
- You depend on Magna's contracts and platform availability
- May not support the specific legal metadata needs for Marshall Islands DAO LLC
- Good reference architecture for what works

### Streamflow

**What it is**: Token distribution and vesting platform on Solana.

**Features:**
- Customizable vesting schedules
- Airdrops
- Staking
- Audited by FYEO and OPCODES

**Cap table relevance**: Useful for the vesting component only. Does not handle share classes, governance, or legal compliance.

### Bonfida Token Vesting

**What it is**: Open-source Solana program for token vesting.

**Features:**
- Deposit SPL tokens with unlock schedule
- Unix timestamp-based unlock
- Simple and auditable

**Cap table relevance**: Legacy (uses original SPL Token, not Token-2022). Good reference for vesting logic but needs modernization.

### Other Notable Projects

| Project | Relevance |
|---------|-----------|
| **Metaplex** | NFT standard — could use for non-fungible membership tokens |
| **Dialect** | On-chain messaging — could use for governance communications |
| **Clockwork** (deprecated) | Was used for automated cron-like execution on Solana |
| **Pyth Governance** | Example of a major DAO LLC using on-chain governance |

### Build vs. Buy Assessment

| Component | Build | Buy/Integrate |
|-----------|-------|---------------|
| Share class management | Build (custom) | N/A — nothing fits |
| Token minting | Use Token-2022 | Standard library |
| Vesting | Build (custom) | Could use Streamflow, but custom gives more control |
| Transfer restrictions | Build (Transfer Hook) | N/A — must be custom |
| Governance | Build or integrate Realms | Realms if basic voting is sufficient |
| Multisig authority | Integrate Squads | Squads v4 SDK |
| Member management | Build (custom) | N/A |
| Legal metadata | Build (custom) | N/A |
| Wallet abstraction | Integrate (Turnkey/Swig) | See section 9 |

---

## 9. Account Abstraction Without Privy

### Context

Privy was acquired by Stripe in June 2025 and is now part of Stripe's infrastructure. While this adds legitimacy, it also means vendor lock-in to Stripe's ecosystem. Here are alternatives:

### 1. Turnkey

**Architecture**: TEE-based key management using AWS Nitro Enclaves

**Pros:**
- Built by the team that created Coinbase Custody
- Fastest signing (50-100ms in TEE)
- Private keys never leave the enclave — not even Turnkey can see them
- Ed25519 support (native Solana compatibility)
- Policy-controlled API access — scope what each API key can do
- Non-custodial by design
- Millions of wallets in production
- Embedded Wallet Kit for rapid integration

**Cons:**
- Centralized infrastructure (AWS dependency)
- Pricing can scale with volume
- Less decentralized than Lit Protocol

**Best for**: Production-grade applications requiring high throughput and institutional security

### 2. Web3Auth (Now MetaMask/Consensys)

**Architecture**: MPC-based social login with threshold key splitting

**Pros:**
- Social login (Google, Twitter, email) — best consumer UX
- Blockchain-agnostic (supports Solana Ed25519)
- 20M+ monthly active users
- Non-custodial (MPC key shares distributed)
- Customizable login flows

**Cons:**
- Acquired by MetaMask/Consensys — potential Ethereum bias
- MPC ceremony adds latency
- Key reconstruction requires coordination between shares
- Complex infrastructure

**Best for**: Consumer-facing apps where social login is critical

### 3. Lit Protocol

**Architecture**: Decentralized key management using threshold cryptography + TEEs across a distributed node network

**Pros:**
- Truly decentralized (no single point of failure)
- Programmable Key Pairs (PKPs) — conditions for signing defined in code (Lit Actions)
- Wrapped Keys for instant non-custodial wallet creation on Solana
- 24M+ cryptographic requests processed
- Can gate signing on arbitrary conditions (KYC status, token ownership, time windows)
- Cross-chain by design

**Cons:**
- Higher latency (network of nodes must coordinate)
- More complex integration
- Newer, less battle-tested than Turnkey
- PKPs use ECDSA (secp256k1) not Ed25519 — Wrapped Keys needed for Solana

**Best for**: Decentralized applications requiring programmable, condition-based signing

### 4. Swig Smart Wallets

**Architecture**: On-chain program (Solana-native smart wallet)

**Pros:**
- Fully on-chain — no external infrastructure
- Role-based permissions (Junior/Manager/Admin)
- Session keys for UX
- Pull payments for subscriptions
- Multi-chain identity (ETH/BTC keys can sign Solana txs)
- Open source
- No vendor dependency

**Cons:**
- Newer project (Anagram)
- Solana-only (not cross-chain infrastructure)
- Requires users to have *some* initial Solana interaction
- Less mature ecosystem

**Best for**: Solana-native applications wanting maximum decentralization and programmability

### 5. Custom PDA-Based Account System

**Architecture**: Your own program manages identity via PDAs

```rust
#[account]
pub struct UserAccount {
    pub identity_hash: [u8; 32],  // Hash of email/social ID
    pub recovery_hash: [u8; 32],  // Hash of recovery factor
    pub session_keys: Vec<SessionKeyEntry>,
    pub nonce: u64,               // Replay protection
    pub created_at: i64,
    pub bump: u8,
}

pub struct SessionKeyEntry {
    pub pubkey: Pubkey,
    pub valid_until: i64,
    pub permissions: u8,
}
```

**Pros:**
- Zero external dependencies
- Complete control over identity and authentication
- Can implement any auth mechanism
- No vendor lock-in
- Cheapest long-term

**Cons:**
- Must build everything from scratch
- Security burden falls entirely on you
- No social login without additional infrastructure
- Need your own key custody solution

**Best for**: Maximum control, but requires significant engineering investment

### Recommendation

For a DAO cap table system:

**Primary**: **Turnkey** for wallet infrastructure + **Swig** for on-chain session keys

- Turnkey handles key generation, custody, and basic signing
- Swig handles on-chain permissions, session keys, and delegated signing
- Custom PDAs handle member identity and cap table logic

**Fallback**: **Lit Protocol** if decentralization is a hard requirement

This avoids Privy/Stripe lock-in while providing production-grade infrastructure.

---

## 10. Cost Analysis

### Rent Costs (Account Creation)

Rent is a refundable deposit based on account data size. The formula:

```
rent = (128 + data_size_bytes) * 3480 * 2  lamports
     = (128 + data_size_bytes) * 6960      lamports
```

Where 128 bytes is the account storage overhead, 3480 is lamports per byte per year, and 2 is years for rent exemption.

| Account Type | Data Size (est.) | Rent (lamports) | Rent (SOL) | Rent (USD @ $150/SOL) |
|-------------|-----------------|-----------------|------------|----------------------|
| Entity PDA | ~300 bytes | 2,978,880 | 0.00298 | $0.45 |
| ShareClass PDA | ~250 bytes | 2,630,880 | 0.00263 | $0.39 |
| MemberRecord PDA | ~150 bytes | 1,934,880 | 0.00193 | $0.29 |
| VestingSchedule PDA | ~120 bytes | 1,725,120 | 0.00173 | $0.26 |
| Proposal PDA | ~200 bytes | 2,282,880 | 0.00228 | $0.34 |
| Vote PDA | ~80 bytes | 1,446,720 | 0.00145 | $0.22 |
| Token-2022 Mint | ~250 bytes | 2,630,880 | 0.00263 | $0.39 |
| Token Account | 165 bytes | 2,039,280 | 0.00204 | $0.31 |
| Session Key PDA | ~100 bytes | 1,586,880 | 0.00159 | $0.24 |

**Note**: Rent is refundable when accounts are closed.

### Transaction Costs

| Operation | Base Fee | Priority Fee (est.) | Total (SOL) | Total (USD) |
|-----------|---------|-------------------|-------------|-------------|
| Base transaction | 0.000005 | 0.00001-0.0001 | ~0.000015-0.000105 | $0.002-$0.016 |
| Issue shares (mint tokens) | 0.000005 | 0.00005 | ~0.000055 | $0.008 |
| Transfer shares | 0.000005 | 0.00005 | ~0.000055 | $0.008 |
| Create proposal | 0.000005 | 0.00002 | ~0.000025 | $0.004 |
| Cast vote | 0.000005 | 0.00002 | ~0.000025 | $0.004 |
| Claim vested tokens | 0.000005 | 0.00005 | ~0.000055 | $0.008 |
| Add member | 0.000005 | 0.00002 | ~0.000025 | $0.004 |

### Scenario: DAO with 100 Members, 3 Share Classes

**One-time setup costs:**

| Item | Count | SOL | USD |
|------|-------|-----|-----|
| Entity PDA | 1 | 0.00298 | $0.45 |
| ShareClass PDAs | 3 | 0.00789 | $1.18 |
| Token-2022 Mints | 3 | 0.00789 | $1.18 |
| MemberRecord PDAs | 100 | 0.19300 | $28.95 |
| Token Accounts (100 members x 3 classes) | 300 | 0.61200 | $91.80 |
| VestingSchedule PDAs (50% have vesting) | 50 | 0.08650 | $12.98 |
| Governance PDA | 1 | 0.00228 | $0.34 |
| **Total setup** | | **~0.91** | **~$137** |

**Monthly operational costs (assuming 20 proposals/month, 2 transfers/member/month):**

| Item | Count/month | SOL | USD |
|------|------------|-----|-----|
| Proposals | 20 | 0.00050 | $0.08 |
| Votes (avg 60 voters per proposal) | 1,200 | 0.03000 | $4.50 |
| Share transfers | 200 | 0.01100 | $1.65 |
| Vesting claims | 50 | 0.00275 | $0.41 |
| Member additions | 5 | 0.01090 | $1.64 |
| **Total monthly** | | **~0.055** | **~$8.28** |

### Comparison with Ethereum

| Metric | Solana | Ethereum |
|--------|--------|----------|
| Account creation (100 members, 3 classes) | ~$137 | ~$15,000-50,000 (storage + gas) |
| Monthly operations (same scenario) | ~$8 | ~$500-2,000 |
| Single token transfer | ~$0.008 | ~$2-15 |
| Program deployment | ~$5-20 | ~$500-5,000 |

Solana is approximately **100-500x cheaper** than Ethereum for cap table operations.

---

## 11. Recommended Architecture

### System Overview

```
┌─────────────────────────────────────────────────────┐
│                    Frontend (Web App)                 │
│    Next.js + @solana/web3.js + @sqds/multisig        │
│    Turnkey SDK (wallet creation) + Swig SDK          │
└───────────────────┬─────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────┐
│              Cap Table Anchor Program                 │
│                                                       │
│  Instructions:                                        │
│  ├── initialize_entity()                              │
│  ├── create_share_class()  → creates Token-2022 mint │
│  ├── add_member()          → creates MemberRecord PDA │
│  ├── issue_shares()        → mints tokens             │
│  ├── create_vesting()      → creates VestingSchedule  │
│  ├── claim_vested()        → releases vested tokens   │
│  ├── create_proposal()     → governance proposal      │
│  ├── cast_vote()           → weighted vote            │
│  ├── execute_proposal()    → execute approved action  │
│  ├── update_member_kyc()   → update KYC status        │
│  └── transfer_approval()   → ROFR approval            │
│                                                       │
│  PDAs:                                                │
│  ├── Entity, ShareClass, MemberRecord                 │
│  ├── VestingSchedule, Proposal, Vote                  │
│  └── TransferApproval, GovernanceConfig               │
└───────────────────┬─────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────┐
│           Transfer Hook Program                       │
│  Called on every Token-2022 transfer:                 │
│  ├── Verify sender KYC                               │
│  ├── Verify receiver KYC                             │
│  ├── Check accreditation                             │
│  ├── Enforce lock-up periods                         │
│  ├── Enforce ROFR                                    │
│  └── Check max holder count                          │
└─────────────────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────┐
│              Token-2022 Program                       │
│  Mints (one per share class):                        │
│  ├── Common shares mint                              │
│  ├── Series A Preferred mint                         │
│  └── [Additional share class mints]                  │
│                                                       │
│  Extensions per mint:                                │
│  ├── Transfer Hook → Cap Table Transfer Hook Program │
│  ├── Permanent Delegate → Squads vault (clawback)    │
│  ├── Metadata → share class info + legal URI         │
│  └── Default Account State → Frozen (until KYC)     │
└─────────────────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────┐
│           Squads v4 Multisig                          │
│  Members: Founding team (e.g., 3-of-5)               │
│  Vaults:                                              │
│  ├── Vault 0: Treasury                               │
│  ├── Vault 1: Program upgrade authority              │
│  └── Vault 2: Cap table entity authority             │
│                                                       │
│  Controls:                                            │
│  ├── Program upgrades (with 72h time lock)           │
│  ├── Share issuance / class creation                 │
│  ├── Emergency freeze                                │
│  └── Member management overrides                     │
└─────────────────────────────────────────────────────┘
```

### Technology Stack

| Component | Technology |
|-----------|-----------|
| Smart contract language | Rust + Anchor |
| Token standard | Token-2022 (SPL) |
| Multisig | Squads Protocol v4 |
| Wallet abstraction | Turnkey (infrastructure) + Swig (on-chain sessions) |
| Frontend | Next.js + TypeScript |
| Solana SDK | @solana/web3.js v2 |
| Legal wrapper | Marshall Islands DAO LLC via MIDAO |
| Off-chain storage | Arweave (immutable docs) + IPFS (working docs) |
| KYC provider | Synaps, Sumsub, or Persona (off-chain, hash on-chain) |
| Indexer | Helius or Triton (for historical queries) |

### Development Phases

**Phase 1: Core Cap Table (MVP)**
- Entity + ShareClass + MemberRecord PDAs
- Token-2022 mints with Transfer Hook + Permanent Delegate
- Basic share issuance and transfer
- KYC hash storage
- Squads multisig as authority

**Phase 2: Vesting + Governance**
- Vesting schedule creation and claiming
- Proposal creation and voting
- Vote weight calculation from token balances
- Proposal execution

**Phase 3: Compliance + Legal**
- Transfer Hook with full restriction logic
- Operating agreement hash anchoring
- ROFR workflow
- Accredited investor checks
- Max holder enforcement

**Phase 4: UX + Account Abstraction**
- Turnkey wallet integration
- Swig session keys
- Social login flow
- Mobile-friendly signing

**Phase 5: Advanced Features**
- Confidential transfers (post-Agave 2.0)
- Dividend distribution
- Secondary market with restriction enforcement
- Multi-entity management
- Cross-chain bridging

---

## Sources

### PDAs
- [Helius: What are Solana PDAs?](https://www.helius.dev/blog/solana-pda)
- [Solana Docs: Program-Derived Address](https://solana.com/docs/core/pda)
- [Solana Docs: PDAs with Anchor](https://solana.com/docs/programs/anchor/pda)
- [QuickNode: How to Use PDAs in Anchor](https://www.quicknode.com/guides/solana-development/anchor/how-to-use-program-derived-addresses)

### Squads Protocol
- [Squads Protocol v4 GitHub](https://github.com/Squads-Protocol/v4)
- [Squads Smart Account Program GitHub](https://github.com/Squads-Protocol/smart-account-program)
- [Squads v4 Program Upgrade GitHub Action](https://github.com/Squads-Protocol/squads-v4-program-upgrade)
- [Squads Blog: Managing Program Upgrades with Multisig](https://squads.xyz/blog/solana-multisig-program-upgrades-management)
- [Squads Blog: Smart Account Program on Mainnet](https://squads.xyz/blog/squads-smart-account-program-live-on-mainnet)
- [Squads Docs](https://docs.squads.so)

### Account Abstraction
- [Helius: What are Solana Smart Wallets?](https://www.helius.dev/blog/solana-smart-wallets)
- [Solana Docs: ERC-4337 Equivalent on Solana](https://solana.com/developers/evm-to-svm/erc4337)
- [Squads Blog: Account Abstraction Use Cases](https://squads.xyz/blog/account-abstraction-use-cases)
- [Anagram: Introducing Swig](https://blog.anagram.xyz/introducing-swig-unlocking-the-future-of-smart-wallets-on-solana-and-ethereum/)
- [Swig Docs](https://build.onswig.com/)
- [Solana Docs: Durable Nonces](https://solana.com/developers/guides/advanced/introduction-to-durable-nonces)

### Token-2022
- [Solana: Token Extensions](https://solana.com/solutions/token-extensions)
- [RareSkills: Token 2022 Specification](https://rareskills.io/post/token-2022)
- [Solana Docs: Transfer Hook Extension](https://solana.com/developers/guides/token-extensions/transfer-hook)
- [Solana Docs: Permanent Delegate](https://solana.com/docs/tokens/extensions/permanent-delegate)
- [QuickNode: Confidential Transfers Guide](https://www.quicknode.com/guides/solana-development/spl-tokens/token-2022/confidential)

### Cap Table & Vesting
- [Magna](https://www.magna.so/)
- [Streamflow](https://streamflow.finance/)
- [Bonfida Token Vesting](https://github.com/Bonfida/token-vesting)

### Governance
- [Realms](https://realms.today/)
- [SPL Governance Docs](https://docs.realms.today/spl-governance)
- [Solana: DAO Development](https://solana.com/developers/dao)

### Legal
- [MIDAO](https://midao.org/)
- [MIDAO Docs](https://docs.midao.org/)
- [LegalNodes: Marshall Islands LLC as DAO Wrapper](https://www.legalnodes.com/article/marshall-islands-llc-as-a-dao-legal-wrapper)
- [DAObox: Marshall Islands DAO LLC Guide](https://docs.daobox.io/educational/marshall-islands-dao-llc-as-a-dao-legal-wrapper-comprehensive-guide)
- [Global Law Experts: Marshall Islands DAO for 2026](https://globallawexperts.com/marshall-islands-dao-the-golden-standard-for-defi-startups-for-2026/)

### Wallet Infrastructure
- [Turnkey](https://www.turnkey.com/)
- [Turnkey: Best Solana Wallets for dApp Devs](https://www.turnkey.com/blog/best-solana-wallets-dapp-developers)
- [Web3Auth](https://web3auth.io/)
- [Lit Protocol: Solana Integration](https://spark.litprotocol.com/solana/)
- [Lit Protocol: PKP Overview](https://developer.litprotocol.com/user-wallets/pkps/overview)
- [Openfort: Top Privy Alternatives 2026](https://www.openfort.io/blog/privy-alternatives)
- [Openfort: Top 10 Embedded Wallets 2026](https://www.openfort.io/blog/top-10-embedded-wallets)

### Costs
- [Solana Docs: Transaction Fees](https://solana.com/docs/core/fees)
- [QuickNode: Understanding Rent on Solana](https://www.quicknode.com/guides/solana-development/getting-started/understanding-rent-on-solana)
- [Solana Cookbook: Calculate Rent](https://solana.com/developers/cookbook/accounts/calculate-rent)
- [RareSkills: Cost of Storage on Solana](https://rareskills.io/post/solana-account-rent)
