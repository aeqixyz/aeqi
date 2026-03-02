use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum length for entity name (UTF-8 bytes).
pub const MAX_NAME_LEN: usize = 64;
/// Maximum length for jurisdiction string.
pub const MAX_JURISDICTION_LEN: usize = 48;
/// Maximum length for registration ID string.
pub const MAX_REGISTRATION_ID_LEN: usize = 48;
/// Maximum length for share class name.
pub const MAX_CLASS_NAME_LEN: usize = 32;

// ---------------------------------------------------------------------------
// PDA Seeds (exported for use in instruction contexts)
// ---------------------------------------------------------------------------

pub const ENTITY_SEED: &[u8] = b"entity";
pub const SHARE_CLASS_SEED: &[u8] = b"share_class";
pub const MEMBER_SEED: &[u8] = b"member";
pub const VESTING_SEED: &[u8] = b"vesting";

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransferRestriction {
    /// No restrictions — freely transferable.
    None,
    /// Only KYC-verified members may receive transfers.
    KycOnly,
    /// KYC + accredited investor verification required.
    AccreditedOnly,
    /// Transfers locked until a specific Unix timestamp.
    LockedUntil { unlock_ts: i64 },
    /// Right of first refusal — transfers require entity approval.
    Rofr,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemberStatus {
    /// Member is in good standing.
    Active,
    /// Member is temporarily suspended (e.g., compliance hold).
    Suspended,
    /// Member has been removed from the entity.
    Removed,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum VestingType {
    /// Tokens vest linearly from start_time to end_time after the cliff.
    Linear,
    /// Tokens vest in discrete tranches (graded schedule stored off-chain,
    /// on-chain calculation uses linear approximation between checkpoints).
    Graded,
    /// All tokens vest at cliff_time (single cliff, no linear component).
    Cliff,
}

// ---------------------------------------------------------------------------
// Account Structures
// ---------------------------------------------------------------------------

/// Root entity account — represents a single DAO LLC on-chain.
///
/// PDA seeds: `[b"entity", entity_id.as_bytes()]`
#[account]
pub struct Entity {
    /// The Squads multisig vault that governs this entity.
    pub authority: Pubkey,
    /// Human-readable entity name (e.g. "Acme DAO LLC").
    pub name: String,
    /// Legal jurisdiction (e.g. "Marshall Islands").
    pub jurisdiction: String,
    /// Registration identifier from the jurisdiction registrar (e.g. MIDAO number).
    pub registration_id: String,
    /// Number of share classes created under this entity.
    pub share_class_count: u8,
    /// Total number of members registered.
    pub member_count: u32,
    /// Unix timestamp of entity creation.
    pub created_at: i64,
    /// Unix timestamp of last modification.
    pub updated_at: i64,
    /// SHA-256 hash of the signed operating agreement document.
    pub charter_hash: [u8; 32],
    /// Whether right-of-first-refusal is active for this entity.
    pub rofr_active: bool,
    /// PDA canonical bump.
    pub bump: u8,
}

impl Entity {
    /// Discriminator (8) + pubkey (32) + string prefix (4) + name + string prefix (4) + jurisdiction
    /// + string prefix (4) + registration_id + u8 + u32 + i64 + i64 + [u8;32] + bool + u8
    pub const fn space(name_len: usize, jurisdiction_len: usize, reg_id_len: usize) -> usize {
        8  // discriminator
        + 32 // authority
        + 4 + name_len
        + 4 + jurisdiction_len
        + 4 + reg_id_len
        + 1  // share_class_count
        + 4  // member_count
        + 8  // created_at
        + 8  // updated_at
        + 32 // charter_hash
        + 1  // rofr_active
        + 1  // bump
    }

    pub fn max_space() -> usize {
        Self::space(MAX_NAME_LEN, MAX_JURISDICTION_LEN, MAX_REGISTRATION_ID_LEN)
    }
}

/// Share class definition — each class maps 1:1 to a Token-2022 mint.
///
/// PDA seeds: `[b"share_class", entity.key().as_ref(), class_name.as_bytes()]`
#[account]
pub struct ShareClass {
    /// The entity this share class belongs to.
    pub entity: Pubkey,
    /// Share class name (e.g. "Common", "Series A Preferred").
    pub name: String,
    /// Token-2022 mint address for this share class.
    pub mint: Pubkey,
    /// Maximum number of shares that can be issued (authorized shares).
    pub total_authorized: u64,
    /// Number of shares currently issued and outstanding.
    pub total_issued: u64,
    /// Par value in lamports (0 for no-par shares).
    pub par_value_lamports: u64,
    /// Voting weight in basis points (10000 = 1x, 20000 = 2x, 0 = non-voting).
    pub voting_weight: u16,
    /// Whether shares of this class can be transferred between members.
    pub is_transferable: bool,
    /// Transfer restriction policy applied to this share class.
    pub transfer_restriction: TransferRestriction,
    /// Liquidation preference in basis points (0 = none, 10000 = 1x).
    pub liquidation_preference: u64,
    /// Whether this share class requires accredited investor status.
    pub requires_accreditation: bool,
    /// Lock-up end timestamp (0 = no lock-up). Transfers blocked until this time.
    pub lockup_end: i64,
    /// Maximum number of distinct holders for this share class (0 = unlimited).
    /// Useful for SEC Rule 12g compliance.
    pub max_holders: u32,
    /// Current number of distinct holders.
    pub current_holders: u32,
    /// Unix timestamp of creation.
    pub created_at: i64,
    /// PDA canonical bump.
    pub bump: u8,
}

impl ShareClass {
    pub const fn space(name_len: usize) -> usize {
        8   // discriminator
        + 32  // entity
        + 4 + name_len // name
        + 32  // mint
        + 8   // total_authorized
        + 8   // total_issued
        + 8   // par_value_lamports
        + 2   // voting_weight
        + 1   // is_transferable
        + 1 + 8  // transfer_restriction (enum discriminant + largest variant)
        + 8   // liquidation_preference
        + 1   // requires_accreditation
        + 8   // lockup_end
        + 4   // max_holders
        + 4   // current_holders
        + 8   // created_at
        + 1   // bump
    }

    pub fn max_space() -> usize {
        Self::space(MAX_CLASS_NAME_LEN)
    }
}

/// Member record — links a wallet to an entity with KYC/compliance metadata.
///
/// PDA seeds: `[b"member", entity.key().as_ref(), wallet.key().as_ref()]`
#[account]
pub struct MemberRecord {
    /// The entity this member belongs to.
    pub entity: Pubkey,
    /// The member's wallet address.
    pub wallet: Pubkey,
    /// Whether KYC verification has been completed.
    pub kyc_verified: bool,
    /// SHA-256 hash of off-chain KYC data (passport, identity docs).
    pub kyc_hash: [u8; 32],
    /// Whether the member is an accredited investor.
    pub accredited: bool,
    /// Unix timestamp of when the member joined.
    pub joined_at: i64,
    /// Current membership status.
    pub status: MemberStatus,
    /// PDA canonical bump.
    pub bump: u8,
}

impl MemberRecord {
    pub const SPACE: usize = 8  // discriminator
        + 32  // entity
        + 32  // wallet
        + 1   // kyc_verified
        + 32  // kyc_hash
        + 1   // accredited
        + 8   // joined_at
        + 1   // status (enum)
        + 1;  // bump
}

/// Vesting schedule — time-locked token release attached to a member+share_class pair.
///
/// PDA seeds: `[b"vesting", entity.key().as_ref(), member.key().as_ref(), share_class.key().as_ref()]`
#[account]
pub struct VestingSchedule {
    /// Entity this vesting schedule belongs to.
    pub entity: Pubkey,
    /// Member wallet this vesting applies to.
    pub member: Pubkey,
    /// Share class mint this vesting is for.
    pub share_class: Pubkey,
    /// Total number of tokens to be vested.
    pub total_amount: u64,
    /// Number of tokens already released/claimed.
    pub released_amount: u64,
    /// Unix timestamp when vesting begins accruing.
    pub start_time: i64,
    /// Unix timestamp of the cliff. No tokens vest before this time.
    pub cliff_time: i64,
    /// Unix timestamp when 100% of tokens are vested.
    pub end_time: i64,
    /// Type of vesting schedule.
    pub schedule_type: VestingType,
    /// Whether the entity authority can revoke unvested tokens.
    pub revocable: bool,
    /// Whether this schedule has been revoked.
    pub revoked: bool,
    /// PDA canonical bump.
    pub bump: u8,
}

impl VestingSchedule {
    pub const SPACE: usize = 8  // discriminator
        + 32  // entity
        + 32  // member
        + 32  // share_class
        + 8   // total_amount
        + 8   // released_amount
        + 8   // start_time
        + 8   // cliff_time
        + 8   // end_time
        + 1   // schedule_type
        + 1   // revocable
        + 1   // revoked
        + 1;  // bump

    /// Calculate the number of tokens that have vested as of `now`.
    /// This does NOT subtract already-released tokens — caller must do that.
    pub fn vested_amount(&self, now: i64) -> u64 {
        if self.revoked {
            return self.released_amount;
        }
        if now < self.cliff_time {
            return 0;
        }
        if now >= self.end_time {
            return self.total_amount;
        }

        match self.schedule_type {
            VestingType::Cliff => {
                // All tokens vest at cliff_time.
                self.total_amount
            }
            VestingType::Linear | VestingType::Graded => {
                // Linear interpolation from start_time to end_time.
                let elapsed = (now - self.start_time) as u128;
                let total_duration = (self.end_time - self.start_time) as u128;
                ((self.total_amount as u128 * elapsed) / total_duration) as u64
            }
        }
    }

    /// Calculate the number of tokens that can be claimed right now.
    pub fn claimable_amount(&self, now: i64) -> u64 {
        let vested = self.vested_amount(now);
        vested.saturating_sub(self.released_amount)
    }
}
