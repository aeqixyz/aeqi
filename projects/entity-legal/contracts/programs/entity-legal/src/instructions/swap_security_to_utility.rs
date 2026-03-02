use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    burn, mint_to, Burn, Mint as MintAccount, MintTo, TokenAccount, TokenInterface,
};

use crate::errors::EntityLegalError;
use crate::state::*;

/// Swaps security tokens to utility tokens. No KYC required.
///
/// This is a permissionless operation. A member burns security tokens
/// (economic ownership) and receives equivalent utility tokens (governance
/// only, not securities). No Foundation approval or KYC is needed because
/// utility tokens are explicitly not securities under Marshall Islands law.
///
/// The member signs to authorize the burn of their security tokens.
/// The authority signs to authorize the mint of utility tokens.
/// This dual-signature ensures the swap goes through the program.
#[derive(Accounts)]
pub struct SwapSecurityToUtility<'info> {
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
        constraint = !member_record.restricted_person @ EntityLegalError::RestrictedPerson,
    )]
    pub member_record: Account<'info, MemberRecord>,

    /// Security token mint — tokens burned from here.
    #[account(mut)]
    pub security_mint: InterfaceAccount<'info, MintAccount>,

    /// Utility token mint — tokens minted here.
    #[account(mut)]
    pub utility_mint: InterfaceAccount<'info, MintAccount>,

    /// Member's security token account (tokens burned from here).
    #[account(
        mut,
        constraint = member_security_ata.mint == security_mint.key(),
        constraint = member_security_ata.owner == member.key(),
    )]
    pub member_security_ata: InterfaceAccount<'info, TokenAccount>,

    /// Member's utility token account (receives minted tokens).
    #[account(
        mut,
        constraint = member_utility_ata.mint == utility_mint.key(),
        constraint = member_utility_ata.owner == member.key(),
    )]
    pub member_utility_ata: InterfaceAccount<'info, TokenAccount>,

    /// The member performing the swap. Must sign to authorize the burn.
    pub member: Signer<'info>,

    /// Entity authority (Foundation Squads Vault 2). Signs to authorize utility mint.
    pub authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<SwapSecurityToUtility>, amount: u64) -> Result<()> {
    require!(amount > 0, EntityLegalError::ZeroSwapAmount);

    burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.security_mint.to_account_info(),
                from: ctx.accounts.member_security_ata.to_account_info(),
                authority: ctx.accounts.member.to_account_info(),
            },
        ),
        amount,
    )?;

    mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.utility_mint.to_account_info(),
                to: ctx.accounts.member_utility_ata.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;

    let clock = Clock::get()?;
    let entity = &mut ctx.accounts.entity;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Swapped {} security -> utility for member {} (entity: {})",
        amount,
        ctx.accounts.member.key(),
        entity.name
    );

    Ok(())
}
