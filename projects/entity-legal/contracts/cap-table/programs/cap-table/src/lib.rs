use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;
use state::*;

declare_id!("Et3Wov1N1NG7FLPNJh6hj76pkKLvkAYprHpDxoDNbGZT");

#[program]
pub mod cap_table {
    use super::*;

    /// Initialize a new entity (DAO LLC) on-chain.
    ///
    /// Creates the root Entity PDA that all other accounts reference.
    /// The authority is set to the provided Squads multisig vault, which
    /// will govern all privileged operations on this entity.
    ///
    /// # Arguments
    /// * `entity_id` - Unique string identifier for PDA derivation (e.g. "acme-dao")
    /// * `name` - Human-readable entity name
    /// * `jurisdiction` - Legal jurisdiction (e.g. "Marshall Islands")
    /// * `registration_id` - Registrar-assigned ID (e.g. MIDAO number)
    /// * `charter_hash` - SHA-256 hash of the signed operating agreement
    pub fn initialize_entity(
        ctx: Context<InitializeEntity>,
        entity_id: String,
        name: String,
        jurisdiction: String,
        registration_id: String,
        charter_hash: [u8; 32],
    ) -> Result<()> {
        instructions::initialize_entity::handler(
            ctx,
            entity_id,
            name,
            jurisdiction,
            registration_id,
            charter_hash,
        )
    }

    /// Create a new share class under an existing entity.
    ///
    /// This creates both a ShareClass PDA (metadata + restrictions) and a
    /// Token-2022 mint with the following extensions:
    /// - **Transfer Hook**: Enforces KYC, accreditation, and lock-up checks
    /// - **Permanent Delegate**: Allows entity authority to force-transfer/burn
    /// - **Metadata Pointer**: Links share class metadata to the mint
    ///
    /// Requires entity authority signature.
    ///
    /// # Arguments
    /// * `class_name` - Name of the share class (e.g. "Common", "Series A Preferred")
    /// * `total_authorized` - Maximum number of shares that can be issued
    /// * `par_value_lamports` - Par value per share in lamports (0 for no-par)
    /// * `voting_weight` - Voting power in basis points (10000 = 1x)
    /// * `is_transferable` - Whether shares can be transferred between members
    /// * `transfer_restriction` - Transfer restriction policy
    /// * `liquidation_preference` - Liquidation preference in basis points
    /// * `requires_accreditation` - Whether receivers must be accredited investors
    /// * `lockup_end` - Unix timestamp when lock-up expires (0 = no lock-up)
    /// * `max_holders` - Maximum distinct holders (0 = unlimited)
    /// * `decimals` - Token decimal places (typically 0 for equity shares)
    pub fn create_share_class(
        ctx: Context<CreateShareClass>,
        class_name: String,
        total_authorized: u64,
        par_value_lamports: u64,
        voting_weight: u16,
        is_transferable: bool,
        transfer_restriction: TransferRestriction,
        liquidation_preference: u64,
        requires_accreditation: bool,
        lockup_end: i64,
        max_holders: u32,
        decimals: u8,
    ) -> Result<()> {
        instructions::create_share_class::handler(
            ctx,
            class_name,
            total_authorized,
            par_value_lamports,
            voting_weight,
            is_transferable,
            transfer_restriction,
            liquidation_preference,
            requires_accreditation,
            lockup_end,
            max_holders,
            decimals,
        )
    }

    /// Add a new member to an entity.
    ///
    /// Creates a MemberRecord PDA linking a wallet address to the entity with
    /// KYC and accreditation metadata. The member starts with `kyc_verified = false`;
    /// use `update_member_kyc` to verify after off-chain KYC is completed.
    ///
    /// Requires entity authority signature.
    ///
    /// # Arguments
    /// * `kyc_hash` - SHA-256 hash of off-chain KYC data
    /// * `accredited` - Whether the member is an accredited investor
    pub fn add_member(
        ctx: Context<AddMember>,
        kyc_hash: [u8; 32],
        accredited: bool,
    ) -> Result<()> {
        instructions::add_member::handler(ctx, kyc_hash, accredited)
    }

    /// Issue (mint) shares to a member's token account.
    ///
    /// Mints Token-2022 tokens representing equity shares to the specified member.
    /// The member must be active, and the issuance must not exceed the total
    /// authorized shares for the class.
    ///
    /// Requires entity authority signature (which is also the mint authority).
    ///
    /// # Arguments
    /// * `amount` - Number of shares to issue
    pub fn issue_shares(ctx: Context<IssueShares>, amount: u64) -> Result<()> {
        instructions::issue_shares::handler(ctx, amount)
    }

    /// Update a member's KYC verification status.
    ///
    /// Called after off-chain KYC verification is completed by the KYC provider.
    /// Updates the on-chain record with the new verification status and KYC hash.
    ///
    /// Requires entity authority signature.
    ///
    /// # Arguments
    /// * `kyc_verified` - New KYC verification status
    /// * `kyc_hash` - Updated SHA-256 hash of KYC data
    /// * `accredited` - Updated accredited investor status
    pub fn update_member_kyc(
        ctx: Context<UpdateMemberKyc>,
        kyc_verified: bool,
        kyc_hash: [u8; 32],
        accredited: bool,
    ) -> Result<()> {
        instructions::update_member_kyc::handler(ctx, kyc_verified, kyc_hash, accredited)
    }

    /// Create a vesting schedule for a member's shares.
    ///
    /// Creates a VestingSchedule PDA that tracks time-locked token release.
    /// Tokens are not pre-minted; they are minted on-demand when the member
    /// calls `claim_vested`. This avoids tying up authorized share capacity
    /// and keeps the mint authority in control.
    ///
    /// Requires entity authority signature.
    ///
    /// # Arguments
    /// * `total_amount` - Total number of tokens to vest
    /// * `start_time` - Unix timestamp when vesting accrual begins
    /// * `cliff_time` - Unix timestamp of the cliff (no tokens before this)
    /// * `end_time` - Unix timestamp when 100% of tokens are vested
    /// * `schedule_type` - Vesting curve type (Linear, Graded, or Cliff)
    /// * `revocable` - Whether the entity can revoke unvested tokens
    pub fn create_vesting(
        ctx: Context<CreateVesting>,
        total_amount: u64,
        start_time: i64,
        cliff_time: i64,
        end_time: i64,
        schedule_type: VestingType,
        revocable: bool,
    ) -> Result<()> {
        instructions::create_vesting::handler(
            ctx,
            total_amount,
            start_time,
            cliff_time,
            end_time,
            schedule_type,
            revocable,
        )
    }

    /// Claim vested tokens.
    ///
    /// Calculates the number of tokens that have vested based on the current
    /// timestamp and the vesting schedule, then mints the claimable amount
    /// to the member's token account.
    ///
    /// The member must sign this transaction. The entity authority must also
    /// sign as the mint authority.
    pub fn claim_vested(ctx: Context<ClaimVested>) -> Result<()> {
        instructions::claim_vested::handler(ctx)
    }
}
