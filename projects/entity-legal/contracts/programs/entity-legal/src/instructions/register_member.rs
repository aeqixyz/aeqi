use anchor_lang::prelude::*;

use crate::errors::EntityLegalError;
use crate::state::*;

/// Registers a new member to the entity's on-chain cap table.
///
/// Creates a MemberRecord PDA linking a wallet address to the entity.
/// Under Marshall Islands DAO Act, token holders automatically become
/// LLC members. This instruction creates the on-chain membership record
/// that the Transfer Hook program uses for compliance validation.
///
/// Members below 25% of economic or governance rights may remain anonymous.
/// KYC is set to false initially and must be explicitly verified via a
/// separate instruction when the member requests a utility-to-security swap
/// or crosses the 25% threshold.
#[derive(Accounts)]
pub struct RegisterMember<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        init,
        payer = payer,
        space = MemberRecord::SPACE,
        seeds = [MEMBER_SEED, entity.key().as_ref(), member_wallet.key().as_ref()],
        bump,
    )]
    pub member_record: Account<'info, MemberRecord>,

    /// The wallet address of the new member.
    /// CHECK: Stored for future lookups. Does not need to sign.
    pub member_wallet: UncheckedAccount<'info>,

    /// Entity authority (Foundation Squads Vault 2).
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RegisterMember>) -> Result<()> {
    let clock = Clock::get()?;

    let member_record = &mut ctx.accounts.member_record;
    member_record.entity = ctx.accounts.entity.key();
    member_record.wallet = ctx.accounts.member_wallet.key();
    member_record.kyc_verified = false;
    member_record.kyc_hash = [0u8; 32];
    member_record.kyc_expiry = 0;
    member_record.security_balance_bps = 0;
    member_record.joined_at = clock.unix_timestamp;
    member_record.status = MemberStatus::Active;
    member_record.restricted_person = false;
    member_record.bump = ctx.bumps.member_record;

    let entity = &mut ctx.accounts.entity;
    entity.member_count = entity
        .member_count
        .checked_add(1)
        .ok_or(EntityLegalError::ArithmeticOverflow)?;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Member registered: {} to entity {} (member #{})",
        member_record.wallet,
        entity.name,
        entity.member_count
    );

    Ok(())
}
