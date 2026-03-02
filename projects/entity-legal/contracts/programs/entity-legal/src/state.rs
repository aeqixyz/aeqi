use anchor_lang::prelude::*;

pub const MAX_NAME_LEN: usize = 64;
pub const MAX_JURISDICTION_LEN: usize = 48;
pub const MAX_REGISTRATION_ID_LEN: usize = 48;
pub const MAX_SERIES_NAME_LEN: usize = 64;
pub const MAX_URI_LEN: usize = 200;

pub const ENTITY_SEED: &[u8] = b"entity";
pub const SERIES_SEED: &[u8] = b"series";
pub const SHARE_CLASS_SEED: &[u8] = b"share_class";
pub const MEMBER_SEED: &[u8] = b"member";
pub const GOVERNANCE_SEED: &[u8] = b"governance";
pub const PROPOSAL_SEED: &[u8] = b"proposal";
pub const VOTE_SEED: &[u8] = b"vote";

pub const BPS_100_PERCENT: u16 = 10_000;
pub const BPS_25_PERCENT: u16 = 2_500;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityType {
    NonProfit,
    ForProfit,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ManagementMode {
    MemberManaged,
    AlgorithmicManaged,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemberStatus {
    Active,
    Suspended,
    Restricted,
    Dissociated,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenClass {
    Security,
    Utility,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProposalStatus {
    Active,
    Passed,
    Defeated,
    Executed,
    Cancelled,
}

/// Root entity account representing a Marshall Islands DAO LLC on-chain.
///
/// PDA seeds: `[b"entity", entity_id.as_bytes()]`
#[account]
pub struct Entity {
    /// Squads multisig vault PDA governing this entity (Foundation Vault 2).
    pub authority: Pubkey,
    /// Human-readable entity name.
    pub name: String,
    /// NonProfit or ForProfit Series DAO LLC.
    pub entity_type: EntityType,
    /// Legal jurisdiction (always "Marshall Islands").
    pub jurisdiction: String,
    /// Registrar-issued registration ID.
    pub registration_id: String,
    /// Token-2022 mint address for security tokens.
    pub security_mint: Pubkey,
    /// Token-2022 mint address for utility (governance) tokens.
    pub utility_mint: Pubkey,
    /// Foundation entity PDA (the non-profit DAO LLC acting as representative).
    pub foundation: Pubkey,
    /// Number of series created under this master LLC.
    pub series_count: u16,
    /// Total members registered.
    pub member_count: u32,
    /// Unix timestamp of entity creation.
    pub created_at: i64,
    /// Unix timestamp of last modification.
    pub updated_at: i64,
    /// SHA-256 hash of the signed operating agreement.
    pub charter_hash: [u8; 32],
    /// Management mode designation.
    pub management_mode: ManagementMode,
    /// PDA canonical bump.
    pub bump: u8,
}

impl Entity {
    pub const fn space(name_len: usize, jurisdiction_len: usize, reg_id_len: usize) -> usize {
        8   // discriminator
        + 32  // authority
        + 4 + name_len
        + 1   // entity_type
        + 4 + jurisdiction_len
        + 4 + reg_id_len
        + 32  // security_mint
        + 32  // utility_mint
        + 32  // foundation
        + 2   // series_count
        + 4   // member_count
        + 8   // created_at
        + 8   // updated_at
        + 32  // charter_hash
        + 1   // management_mode
        + 1   // bump
    }

    pub fn max_space() -> usize {
        Self::space(MAX_NAME_LEN, MAX_JURISDICTION_LEN, MAX_REGISTRATION_ID_LEN)
    }
}

/// Series within a master Series DAO LLC.
///
/// PDA seeds: `[b"series", entity_pda.key().as_ref(), series_name.as_bytes()]`
#[account]
pub struct Series {
    /// Parent entity PDA.
    pub parent_entity: Pubkey,
    /// Series name (e.g., "Trading Division").
    pub name: String,
    /// Token-2022 mint for this series' security tokens.
    pub security_mint: Pubkey,
    /// Token-2022 mint for this series' utility tokens.
    pub utility_mint: Pubkey,
    /// Members registered to this series.
    pub member_count: u32,
    /// Unix timestamp of series creation.
    pub created_at: i64,
    /// SHA-256 hash of the series operating agreement amendment.
    pub charter_hash: [u8; 32],
    /// PDA canonical bump.
    pub bump: u8,
}

impl Series {
    pub const fn space(name_len: usize) -> usize {
        8   // discriminator
        + 32  // parent_entity
        + 4 + name_len
        + 32  // security_mint
        + 32  // utility_mint
        + 4   // member_count
        + 8   // created_at
        + 32  // charter_hash
        + 1   // bump
    }

    pub fn max_space() -> usize {
        Self::space(MAX_SERIES_NAME_LEN)
    }
}

/// Member record linking a wallet to an entity with KYC and compliance metadata.
///
/// PDA seeds: `[b"member", entity_pda.key().as_ref(), member_wallet.key().as_ref()]`
#[account]
pub struct MemberRecord {
    /// Entity this member belongs to.
    pub entity: Pubkey,
    /// Member's wallet address (PDA wallet or direct wallet).
    pub wallet: Pubkey,
    /// Whether KYC verification has been completed.
    pub kyc_verified: bool,
    /// SHA-256 hash of off-chain KYC data.
    pub kyc_hash: [u8; 32],
    /// Unix timestamp when KYC expires (annual renewal).
    pub kyc_expiry: i64,
    /// Member's security token balance as basis points of total supply.
    pub security_balance_bps: u16,
    /// Unix timestamp when the member joined.
    pub joined_at: i64,
    /// Current membership status.
    pub status: MemberStatus,
    /// Whether this member is on a sanctions/restricted persons list.
    pub restricted_person: bool,
    /// PDA canonical bump.
    pub bump: u8,
}

impl MemberRecord {
    pub const SPACE: usize = 8  // discriminator
        + 32  // entity
        + 32  // wallet
        + 1   // kyc_verified
        + 32  // kyc_hash
        + 8   // kyc_expiry
        + 2   // security_balance_bps
        + 8   // joined_at
        + 1   // status
        + 1   // restricted_person
        + 1;  // bump
}

/// Governance configuration for an entity.
///
/// PDA seeds: `[b"governance", entity_pda.key().as_ref()]`
#[account]
pub struct GovernanceConfig {
    /// Entity this governance config belongs to.
    pub entity: Pubkey,
    /// Voting threshold in basis points (e.g., 5000 = 50% of votes must approve).
    pub voting_threshold_bps: u16,
    /// Quorum in basis points (e.g., 1000 = 10% of utility supply must vote).
    pub quorum_bps: u16,
    /// Duration in seconds for which a proposal remains active for voting.
    pub proposal_duration_secs: i64,
    /// Running proposal counter for deterministic PDA derivation.
    pub proposal_count: u64,
    /// PDA canonical bump.
    pub bump: u8,
}

impl GovernanceConfig {
    pub const SPACE: usize = 8  // discriminator
        + 32  // entity
        + 2   // voting_threshold_bps
        + 2   // quorum_bps
        + 8   // proposal_duration_secs
        + 8   // proposal_count
        + 1;  // bump
}

/// Governance proposal.
///
/// PDA seeds: `[b"proposal", entity_pda.key().as_ref(), &proposal_id.to_le_bytes()]`
#[account]
pub struct Proposal {
    /// Entity this proposal belongs to.
    pub entity: Pubkey,
    /// Unique proposal ID within this entity.
    pub proposal_id: u64,
    /// Proposer's wallet address.
    pub proposer: Pubkey,
    /// SHA-256 hash of the proposal description (stored off-chain on Arweave/IPFS).
    pub description_hash: [u8; 32],
    /// Total votes in favor (utility token-weighted).
    pub votes_for: u64,
    /// Total votes against.
    pub votes_against: u64,
    /// Unix timestamp when voting started.
    pub started_at: i64,
    /// Unix timestamp when voting ends.
    pub ends_at: i64,
    /// Current proposal status.
    pub status: ProposalStatus,
    /// PDA canonical bump.
    pub bump: u8,
}

impl Proposal {
    pub const SPACE: usize = 8  // discriminator
        + 32  // entity
        + 8   // proposal_id
        + 32  // proposer
        + 32  // description_hash
        + 8   // votes_for
        + 8   // votes_against
        + 8   // started_at
        + 8   // ends_at
        + 1   // status
        + 1;  // bump
}

/// Vote record for a single member on a single proposal.
///
/// PDA seeds: `[b"vote", proposal_pda.key().as_ref(), member_wallet.key().as_ref()]`
#[account]
pub struct VoteRecord {
    /// Proposal this vote is cast on.
    pub proposal: Pubkey,
    /// Voter's wallet address.
    pub voter: Pubkey,
    /// Whether the vote is in favor (true) or against (false).
    pub in_favor: bool,
    /// Weight of the vote (utility token balance at time of voting).
    pub weight: u64,
    /// Unix timestamp of the vote.
    pub voted_at: i64,
    /// PDA canonical bump.
    pub bump: u8,
}

impl VoteRecord {
    pub const SPACE: usize = 8  // discriminator
        + 32  // proposal
        + 32  // voter
        + 1   // in_favor
        + 8   // weight
        + 8   // voted_at
        + 1;  // bump
}
