use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;
use state::*;

declare_id!("ELeg111111111111111111111111111111111111111");

#[program]
pub mod entity_legal {
    use super::*;

    /// Initialize a new Marshall Islands DAO LLC on-chain.
    ///
    /// Creates the Entity PDA and both Token-2022 mints (security + utility)
    /// with Transfer Hook, Permanent Delegate (security only), and Metadata
    /// Pointer extensions. The Foundation's Squads vault is set as the entity
    /// authority and mint authority for both token types.
    pub fn create_entity(
        ctx: Context<CreateEntity>,
        entity_id: String,
        name: String,
        entity_type: EntityType,
        jurisdiction: String,
        registration_id: String,
        charter_hash: [u8; 32],
        management_mode: ManagementMode,
        security_decimals: u8,
        utility_decimals: u8,
    ) -> Result<()> {
        instructions::create_entity::handler(
            ctx,
            entity_id,
            name,
            entity_type,
            jurisdiction,
            registration_id,
            charter_hash,
            management_mode,
            security_decimals,
            utility_decimals,
        )
    }

    /// Create a new series under a master Series DAO LLC.
    ///
    /// Each series is a legally independent sub-entity with its own assets,
    /// liabilities, governance, and membership. Gets its own security and
    /// utility Token-2022 mints with the same extension configuration.
    pub fn create_series(
        ctx: Context<CreateSeries>,
        series_name: String,
        charter_hash: [u8; 32],
        security_decimals: u8,
        utility_decimals: u8,
    ) -> Result<()> {
        instructions::create_series::handler(
            ctx,
            series_name,
            charter_hash,
            security_decimals,
            utility_decimals,
        )
    }

    /// Mint security tokens (economic ownership) to a registered member.
    ///
    /// Security tokens represent economic ownership in the LLC. The Token-2022
    /// Transfer Hook enforces the 25% KYC threshold on subsequent transfers.
    /// The Permanent Delegate extension gives the Foundation forced transfer
    /// capability for legal compliance (court orders, sanctions enforcement).
    pub fn mint_security_token(ctx: Context<MintSecurityToken>, amount: u64) -> Result<()> {
        instructions::mint_security_token::handler(ctx, amount)
    }

    /// Mint utility tokens (governance rights) to a registered member.
    ///
    /// Utility tokens confer governance rights but no economic rights. Under
    /// the 2023 Amendment, governance tokens are explicitly not securities.
    /// One utility token equals one governance vote.
    pub fn mint_utility_token(ctx: Context<MintUtilityToken>, amount: u64) -> Result<()> {
        instructions::mint_utility_token::handler(ctx, amount)
    }

    /// Swap utility tokens to security tokens. Requires KYC verification.
    ///
    /// Burns the member's utility tokens and mints an equivalent amount of
    /// security tokens. The member must have completed KYC (kyc_verified=true
    /// on their MemberRecord) and KYC must not be expired. Both the member
    /// (to authorize the burn) and the Foundation authority (to authorize the
    /// mint) must sign.
    pub fn swap_utility_to_security(
        ctx: Context<SwapUtilityToSecurity>,
        amount: u64,
    ) -> Result<()> {
        instructions::swap_utility_to_security::handler(ctx, amount)
    }

    /// Swap security tokens to utility tokens. No KYC required.
    ///
    /// Burns the member's security tokens and mints an equivalent amount of
    /// utility tokens. This is permissionless because utility tokens are not
    /// securities. Both the member and authority must sign (member for the
    /// burn, authority for the mint).
    pub fn swap_security_to_utility(
        ctx: Context<SwapSecurityToUtility>,
        amount: u64,
    ) -> Result<()> {
        instructions::swap_security_to_utility::handler(ctx, amount)
    }

    /// Register a new member to the entity's on-chain cap table.
    ///
    /// Creates a MemberRecord PDA. Under the DAO Act, token holders
    /// automatically become LLC members. KYC starts as unverified and
    /// is updated separately when required.
    pub fn register_member(ctx: Context<RegisterMember>) -> Result<()> {
        instructions::register_member::handler(ctx)
    }

    /// Cast a governance vote on an active proposal.
    ///
    /// Uses utility token-weighted voting (one vote per utility token).
    /// Vote weight is determined by the voter's utility token balance at
    /// the time of voting. A VoteRecord PDA prevents double-voting.
    pub fn vote(ctx: Context<Vote>, in_favor: bool) -> Result<()> {
        instructions::vote::handler(ctx, in_favor)
    }
}
