use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenAccount;

use crate::errors::EntityLegalError;
use crate::state::*;

/// Casts a governance vote on an active proposal using utility tokens.
///
/// Under the Marshall Islands DAO Act, governance is exercised through
/// token-weighted voting. Each utility token equals one vote. Only utility
/// token holders can vote (security tokens confer economic rights, not
/// governance rights). The vote weight is the voter's utility token balance
/// at the time of voting.
///
/// Vote records are stored as PDAs to prevent double-voting. Once the
/// voting period ends, anyone can finalize the proposal by calling a
/// separate execute instruction.
#[derive(Accounts)]
pub struct Vote<'info> {
    #[account(
        constraint = entity.utility_mint == voter_utility_ata.mint @ EntityLegalError::UtilityMintMismatch,
    )]
    pub entity: Account<'info, Entity>,

    pub governance: Account<'info, GovernanceConfig>,

    #[account(
        mut,
        has_one = entity,
        constraint = proposal.status == ProposalStatus::Active @ EntityLegalError::ProposalNotActive,
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        init,
        payer = payer,
        space = VoteRecord::SPACE,
        seeds = [VOTE_SEED, proposal.key().as_ref(), voter.key().as_ref()],
        bump,
    )]
    pub vote_record: Account<'info, VoteRecord>,

    #[account(
        has_one = entity,
        constraint = member_record.wallet == voter.key(),
        constraint = member_record.status == MemberStatus::Active @ EntityLegalError::MemberNotActive,
    )]
    pub member_record: Account<'info, MemberRecord>,

    /// Voter's utility token account. Balance determines vote weight.
    #[account(
        constraint = voter_utility_ata.owner == voter.key(),
    )]
    pub voter_utility_ata: InterfaceAccount<'info, TokenAccount>,

    /// The voting member. Must sign to cast vote.
    pub voter: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Vote>, in_favor: bool) -> Result<()> {
    let clock = Clock::get()?;
    let proposal = &ctx.accounts.proposal;

    require!(
        clock.unix_timestamp <= proposal.ends_at,
        EntityLegalError::VotingPeriodEnded
    );

    let weight = ctx.accounts.voter_utility_ata.amount;
    require!(weight > 0, EntityLegalError::NoVotingPower);

    let vote_record = &mut ctx.accounts.vote_record;
    vote_record.proposal = ctx.accounts.proposal.key();
    vote_record.voter = ctx.accounts.voter.key();
    vote_record.in_favor = in_favor;
    vote_record.weight = weight;
    vote_record.voted_at = clock.unix_timestamp;
    vote_record.bump = ctx.bumps.vote_record;

    let proposal = &mut ctx.accounts.proposal;
    if in_favor {
        proposal.votes_for = proposal
            .votes_for
            .checked_add(weight)
            .ok_or(EntityLegalError::ArithmeticOverflow)?;
    } else {
        proposal.votes_against = proposal
            .votes_against
            .checked_add(weight)
            .ok_or(EntityLegalError::ArithmeticOverflow)?;
    }

    msg!(
        "Vote cast on proposal {}: {} with weight {} by {}",
        proposal.proposal_id,
        if in_favor { "FOR" } else { "AGAINST" },
        weight,
        ctx.accounts.voter.key()
    );

    Ok(())
}
