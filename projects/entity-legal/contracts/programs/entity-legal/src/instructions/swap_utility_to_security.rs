use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    burn, mint_to, Burn, Mint as MintAccount, MintTo, TokenAccount, TokenInterface,
};

use crate::errors::EntityLegalError;
use crate::state::*;

/// Swaps utility tokens to security tokens. Requires KYC.
///
/// This is the core mechanism enabling the ownership/anonymity/compliance
/// trilemma solution. A member burns utility tokens (governance-only, not
/// securities) and receives an equivalent amount of security tokens
/// (economic ownership). The Foundation must approve via KYC verification
/// before the swap executes.
///
/// Flow:
/// 1. Member initiates swap on dashboard
/// 2. Foundation triggers KYC process off-chain
/// 3. Foundation writes kyc_hash to MemberRecord and sets kyc_verified=true
/// 4. Foundation's Squads multisig approves the swap transaction
/// 5. This instruction burns utility tokens and mints security tokens
///
/// The Transfer Hook on the security mint validates KYC status.
#[derive(Accounts)]
pub struct SwapUtilityToSecurity<'info> {
    #[account(
        mut,
        has_one = authority,
        constraint = entity.security_mint == security_mint.key() @ EntityLegalError::SecurityMintMismatch,
        constraint = entity.utility_mint == utility_mint.key() @ EntityLegalError::UtilityMintMismatch,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        has_one = entity,
        constraint = member_record.wallet == member.key(),
        constraint = member_record.status == MemberStatus::Active @ EntityLegalError::MemberNotActive,
        constraint = member_record.kyc_verified @ EntityLegalError::SwapRequiresKyc,
        constraint = !member_record.restricted_person @ EntityLegalError::RestrictedPerson,
    )]
    pub member_record: Account<'info, MemberRecord>,

    /// Security token mint — tokens will be minted here.
    #[account(mut)]
    pub security_mint: InterfaceAccount<'info, MintAccount>,

    /// Utility token mint — tokens will be burned from here.
    #[account(mut)]
    pub utility_mint: InterfaceAccount<'info, MintAccount>,

    /// Member's security token account (receives minted tokens).
    #[account(
        mut,
        constraint = member_security_ata.mint == security_mint.key(),
        constraint = member_security_ata.owner == member.key(),
    )]
    pub member_security_ata: InterfaceAccount<'info, TokenAccount>,

    /// Member's utility token account (tokens burned from here).
    #[account(
        mut,
        constraint = member_utility_ata.mint == utility_mint.key(),
        constraint = member_utility_ata.owner == member.key(),
    )]
    pub member_utility_ata: InterfaceAccount<'info, TokenAccount>,

    /// The member performing the swap. Must sign to authorize the burn.
    pub member: Signer<'info>,

    /// Entity authority (Foundation Squads Vault 2). Must co-sign to mint security tokens.
    pub authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<SwapUtilityToSecurity>, amount: u64) -> Result<()> {
    require!(amount > 0, EntityLegalError::ZeroSwapAmount);

    let member_record = &ctx.accounts.member_record;
    let clock = Clock::get()?;
    require!(
        member_record.kyc_expiry > clock.unix_timestamp,
        EntityLegalError::KycExpired
    );

    burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.utility_mint.to_account_info(),
                from: ctx.accounts.member_utility_ata.to_account_info(),
                authority: ctx.accounts.member.to_account_info(),
            },
        ),
        amount,
    )?;

    mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.security_mint.to_account_info(),
                to: ctx.accounts.member_security_ata.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;

    let entity = &mut ctx.accounts.entity;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Swapped {} utility -> security for member {} (entity: {})",
        amount,
        ctx.accounts.member.key(),
        entity.name
    );

    Ok(())
}
