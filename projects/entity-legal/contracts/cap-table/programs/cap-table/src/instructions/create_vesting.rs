use anchor_lang::prelude::*;

use crate::errors::CapTableError;
use crate::state::*;

#[derive(Accounts)]
pub struct CreateVesting<'info> {
    #[account(
        has_one = authority,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        has_one = entity,
    )]
    pub share_class: Account<'info, ShareClass>,

    #[account(
        has_one = entity,
        constraint = member_record.status == MemberStatus::Active @ CapTableError::MemberNotActive,
    )]
    pub member_record: Account<'info, MemberRecord>,

    #[account(
        init,
        payer = payer,
        space = VestingSchedule::SPACE,
        seeds = [
            VESTING_SEED,
            entity.key().as_ref(),
            member_record.wallet.as_ref(),
            share_class.key().as_ref(),
        ],
        bump,
    )]
    pub vesting_schedule: Account<'info, VestingSchedule>,

    /// Entity authority (Squads multisig vault). Must sign to create vesting schedules.
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<CreateVesting>,
    total_amount: u64,
    start_time: i64,
    cliff_time: i64,
    end_time: i64,
    schedule_type: VestingType,
    revocable: bool,
) -> Result<()> {
    // Validate inputs.
    require!(total_amount > 0, CapTableError::ZeroVestingAmount);
    require!(start_time < end_time, CapTableError::InvalidVestingPeriod);
    require!(
        cliff_time >= start_time && cliff_time <= end_time,
        CapTableError::InvalidCliffTime
    );

    let vesting = &mut ctx.accounts.vesting_schedule;

    vesting.entity = ctx.accounts.entity.key();
    vesting.member = ctx.accounts.member_record.wallet;
    vesting.share_class = ctx.accounts.share_class.key();
    vesting.total_amount = total_amount;
    vesting.released_amount = 0;
    vesting.start_time = start_time;
    vesting.cliff_time = cliff_time;
    vesting.end_time = end_time;
    vesting.schedule_type = schedule_type;
    vesting.revocable = revocable;
    vesting.revoked = false;
    vesting.bump = ctx.bumps.vesting_schedule;

    msg!(
        "Vesting schedule created: {} tokens for {} (cliff: {}, end: {}, type: {:?})",
        total_amount,
        vesting.member,
        cliff_time,
        end_time,
        schedule_type
    );

    Ok(())
}
