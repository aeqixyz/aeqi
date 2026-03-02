use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    mint_to, Mint as MintAccount, MintTo, TokenAccount, TokenInterface,
};

use crate::errors::CapTableError;
use crate::state::*;

#[derive(Accounts)]
pub struct ClaimVested<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        mut,
        has_one = entity,
        constraint = share_class.mint == mint.key(),
    )]
    pub share_class: Account<'info, ShareClass>,

    #[account(
        has_one = entity,
        constraint = member_record.wallet == member.key(),
        constraint = member_record.status == MemberStatus::Active @ CapTableError::MemberNotActive,
    )]
    pub member_record: Account<'info, MemberRecord>,

    #[account(
        mut,
        has_one = entity,
        constraint = vesting_schedule.member == member.key(),
        constraint = vesting_schedule.share_class == share_class.key(),
    )]
    pub vesting_schedule: Account<'info, VestingSchedule>,

    /// The Token-2022 mint for this share class.
    #[account(mut)]
    pub mint: InterfaceAccount<'info, MintAccount>,

    /// The member's associated token account for this mint.
    #[account(
        mut,
        constraint = member_token_account.mint == mint.key(),
        constraint = member_token_account.owner == member.key(),
    )]
    pub member_token_account: InterfaceAccount<'info, TokenAccount>,

    /// The member claiming their vested tokens. Must sign.
    pub member: Signer<'info>,

    /// Entity authority — required as the mint authority for Token-2022 minting.
    /// In production, this would be the Squads vault signing via a pre-approved
    /// transaction, or the claim instruction would use a PDA as mint authority.
    /// For MVP, we require the authority signer to authorize the mint.
    pub authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<ClaimVested>) -> Result<()> {
    let vesting = &ctx.accounts.vesting_schedule;

    // Check that the vesting schedule has not been revoked.
    require!(!vesting.revoked, CapTableError::VestingRevoked);

    // Calculate claimable amount based on current timestamp.
    let clock = Clock::get()?;
    let claimable = vesting.claimable_amount(clock.unix_timestamp);
    require!(claimable > 0, CapTableError::NothingToClaim);

    // Verify this issuance won't exceed authorized shares.
    let share_class = &ctx.accounts.share_class;
    let new_total_issued = share_class
        .total_issued
        .checked_add(claimable)
        .ok_or(CapTableError::ArithmeticOverflow)?;
    require!(
        new_total_issued <= share_class.total_authorized,
        CapTableError::ExceedsAuthorizedShares
    );

    // Mint the claimable tokens to the member's token account.
    mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.member_token_account.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        claimable,
    )?;

    // Update vesting schedule.
    let vesting = &mut ctx.accounts.vesting_schedule;
    vesting.released_amount = vesting
        .released_amount
        .checked_add(claimable)
        .ok_or(CapTableError::ArithmeticOverflow)?;

    // Update share class issued count.
    let share_class = &mut ctx.accounts.share_class;
    share_class.total_issued = new_total_issued;

    // Track new holder if this is the first issuance to this member.
    if ctx.accounts.member_token_account.amount == 0 {
        share_class.current_holders = share_class
            .current_holders
            .checked_add(1)
            .ok_or(CapTableError::ArithmeticOverflow)?;
    }

    // Update entity timestamp.
    let entity = &mut ctx.accounts.entity;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Claimed {} vested tokens for {} (total released: {}/{})",
        claimable,
        ctx.accounts.member.key(),
        vesting.released_amount,
        vesting.total_amount
    );

    Ok(())
}
