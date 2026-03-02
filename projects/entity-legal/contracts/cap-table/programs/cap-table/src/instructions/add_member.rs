use anchor_lang::prelude::*;

use crate::errors::CapTableError;
use crate::state::*;

#[derive(Accounts)]
pub struct AddMember<'info> {
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
    /// CHECK: This is the member's wallet. It does not need to sign — the entity
    /// authority adds members. The wallet is stored for future lookups.
    pub member_wallet: UncheckedAccount<'info>,

    /// Entity authority (Squads multisig vault). Must sign to add members.
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<AddMember>,
    kyc_hash: [u8; 32],
    accredited: bool,
) -> Result<()> {
    let clock = Clock::get()?;

    let member_record = &mut ctx.accounts.member_record;
    member_record.entity = ctx.accounts.entity.key();
    member_record.wallet = ctx.accounts.member_wallet.key();
    member_record.kyc_verified = false; // KYC must be explicitly verified later.
    member_record.kyc_hash = kyc_hash;
    member_record.accredited = accredited;
    member_record.joined_at = clock.unix_timestamp;
    member_record.status = MemberStatus::Active;
    member_record.bump = ctx.bumps.member_record;

    // Update entity member count.
    let entity = &mut ctx.accounts.entity;
    entity.member_count = entity
        .member_count
        .checked_add(1)
        .ok_or(CapTableError::ArithmeticOverflow)?;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Member added: {} to entity {} (member #{})",
        member_record.wallet,
        entity.name,
        entity.member_count
    );

    Ok(())
}
