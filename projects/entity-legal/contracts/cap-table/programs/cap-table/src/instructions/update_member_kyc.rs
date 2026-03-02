use anchor_lang::prelude::*;

use crate::state::*;

#[derive(Accounts)]
pub struct UpdateMemberKyc<'info> {
    #[account(
        has_one = authority,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        mut,
        has_one = entity,
        seeds = [MEMBER_SEED, entity.key().as_ref(), member_record.wallet.as_ref()],
        bump = member_record.bump,
    )]
    pub member_record: Account<'info, MemberRecord>,

    /// Entity authority (Squads multisig vault). Must sign to update KYC status.
    pub authority: Signer<'info>,
}

pub fn handler(
    ctx: Context<UpdateMemberKyc>,
    kyc_verified: bool,
    kyc_hash: [u8; 32],
    accredited: bool,
) -> Result<()> {
    let member_record = &mut ctx.accounts.member_record;

    member_record.kyc_verified = kyc_verified;
    member_record.kyc_hash = kyc_hash;
    member_record.accredited = accredited;

    msg!(
        "KYC updated for member {}: verified={}, accredited={}",
        member_record.wallet,
        kyc_verified,
        accredited
    );

    Ok(())
}
