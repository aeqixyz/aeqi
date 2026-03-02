use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    mint_to, Mint as MintAccount, MintTo, TokenAccount, TokenInterface,
};

use crate::errors::CapTableError;
use crate::state::*;

#[derive(Accounts)]
pub struct IssueShares<'info> {
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
        constraint = member_record.wallet == recipient.key(),
        constraint = member_record.status == MemberStatus::Active @ CapTableError::MemberNotActive,
    )]
    pub member_record: Account<'info, MemberRecord>,

    /// The Token-2022 mint for this share class.
    #[account(mut)]
    pub mint: InterfaceAccount<'info, MintAccount>,

    /// The member's associated token account for this mint.
    #[account(
        mut,
        constraint = recipient_token_account.mint == mint.key(),
        constraint = recipient_token_account.owner == recipient.key(),
    )]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    /// The member's wallet address (token account owner).
    /// CHECK: Validated via member_record constraint above.
    pub recipient: UncheckedAccount<'info>,

    /// Entity authority (Squads multisig vault). Must sign to issue shares.
    /// This is also the mint authority for the Token-2022 mint.
    pub authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<IssueShares>, amount: u64) -> Result<()> {
    require!(amount > 0, CapTableError::ZeroShareIssuance);

    let share_class = &ctx.accounts.share_class;

    // Check that issuance does not exceed authorized shares.
    let new_total_issued = share_class
        .total_issued
        .checked_add(amount)
        .ok_or(CapTableError::ArithmeticOverflow)?;
    require!(
        new_total_issued <= share_class.total_authorized,
        CapTableError::ExceedsAuthorizedShares
    );

    // Check max holder count if configured.
    if share_class.max_holders > 0
        && ctx.accounts.recipient_token_account.amount == 0
    {
        // This recipient currently holds 0 tokens, so this issuance creates a new holder.
        require!(
            share_class.current_holders < share_class.max_holders,
            CapTableError::MaxHoldersReached
        );
    }

    // Mint Token-2022 tokens to the recipient's token account.
    // The authority (Squads vault) is the mint authority.
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

    // Update share class state.
    let share_class = &mut ctx.accounts.share_class;
    share_class.total_issued = new_total_issued;

    // Track new holder if recipient previously had zero balance.
    if ctx.accounts.recipient_token_account.amount == 0 {
        share_class.current_holders = share_class
            .current_holders
            .checked_add(1)
            .ok_or(CapTableError::ArithmeticOverflow)?;
    }

    // Update entity timestamp.
    let clock = Clock::get()?;
    let entity = &mut ctx.accounts.entity;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Issued {} shares of class {} to {} (total issued: {}/{})",
        amount,
        share_class.name,
        ctx.accounts.recipient.key(),
        share_class.total_issued,
        share_class.total_authorized
    );

    Ok(())
}
