use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    mint_to, Mint as MintAccount, MintTo, TokenAccount, TokenInterface,
};

use crate::errors::EntityLegalError;
use crate::state::*;

/// Mints security tokens (economic ownership) to a member.
///
/// Security tokens represent economic ownership in the LLC and confer rights
/// to profit distributions, liquidation proceeds, and economic value. The
/// Token-2022 Transfer Hook enforces the 25% KYC threshold on subsequent
/// transfers. The Permanent Delegate extension gives the Foundation forced
/// transfer capability for legal compliance.
///
/// Only the entity authority (Foundation Squads Vault 2) can mint.
#[derive(Accounts)]
pub struct MintSecurityToken<'info> {
    #[account(
        mut,
        has_one = authority,
        constraint = entity.security_mint == mint.key() @ EntityLegalError::SecurityMintMismatch,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        has_one = entity,
        constraint = member_record.wallet == recipient.key(),
        constraint = member_record.status == MemberStatus::Active @ EntityLegalError::MemberNotActive,
    )]
    pub member_record: Account<'info, MemberRecord>,

    #[account(mut)]
    pub mint: InterfaceAccount<'info, MintAccount>,

    /// Recipient's Token-2022 token account for the security mint.
    #[account(
        mut,
        constraint = recipient_token_account.mint == mint.key(),
        constraint = recipient_token_account.owner == recipient.key(),
    )]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Validated via member_record constraint.
    pub recipient: UncheckedAccount<'info>,

    /// Entity authority (Foundation Squads Vault 2). Also the mint authority.
    pub authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<MintSecurityToken>, amount: u64) -> Result<()> {
    require!(amount > 0, EntityLegalError::ZeroMintAmount);

    mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.recipient_token_account.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;

    let clock = Clock::get()?;
    let entity = &mut ctx.accounts.entity;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Minted {} security tokens to {} for entity {}",
        amount,
        ctx.accounts.recipient.key(),
        entity.name
    );

    Ok(())
}
