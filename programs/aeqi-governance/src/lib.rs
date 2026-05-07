//! aeqi_governance — proposal lifecycle + voting.
//!
//! Ports `modules/Governance.module.sol`. Two voting modes selected per
//! proposal via `governance_config_id`:
//!
//! - `governance_config_id == [0u8; 32]` → token-weighted voting (CPI into
//!   `aeqi_token` for vote power at proposal start slot).
//! - `governance_config_id == role_type_id` → per-role multisig (CPI into
//!   `aeqi_role::get_past_role_votes`).
//!
//! Proposal state machine: Pending → Active → (Defeated | Succeeded) →
//! Queued → Executed.
//!
//! This iteration: GovernanceConfig + Proposal PDAs + register_config + propose
//! ixes. cast_vote and execute land in subsequent iterations.

use anchor_lang::prelude::*;

declare_id!("528PTeSk8M3pKMMhc5vitbcwMGUMcHMzg6G5XpX8iVBn");

#[program]
pub mod aeqi_governance {
    use super::*;

    /// Module init — creates GovernanceModuleState PDA bound to a trust.
    pub fn init(ctx: Context<InitGovernance>) -> Result<()> {
        let m = &mut ctx.accounts.module_state;
        m.trust = ctx.accounts.trust.key();
        m.proposal_count = 0;
        m.config_count = 0;
        m.bump = ctx.bumps.module_state;
        Ok(())
    }

    pub fn finalize(_ctx: Context<FinalizeGovernance>) -> Result<()> {
        Ok(())
    }

    /// Register a governance config (one per voting mode the trust supports).
    /// Mirrors EVM `Governance.module.registerGovernanceConfig`.
    pub fn register_config(
        ctx: Context<RegisterConfig>,
        governance_config_id: [u8; 32],
        config: GovernanceConfigInput,
    ) -> Result<()> {
        require!(
            config.quorum_bps <= 10_000,
            GovernanceError::InvalidBpsValue
        );
        require!(
            config.support_bps <= 10_000,
            GovernanceError::InvalidBpsValue
        );
        require!(config.voting_period > 0, GovernanceError::ZeroVotingPeriod);

        let g = &mut ctx.accounts.governance_config;
        g.trust = ctx.accounts.trust.key();
        g.governance_config_id = governance_config_id;
        g.proposal_threshold = config.proposal_threshold;
        g.quorum_bps = config.quorum_bps;
        g.support_bps = config.support_bps;
        g.voting_period = config.voting_period;
        g.execution_delay = config.execution_delay;
        g.allow_early_enact = config.allow_early_enact;
        g.bump = ctx.bumps.governance_config;

        let m = &mut ctx.accounts.module_state;
        m.config_count = m.config_count.checked_add(1).unwrap();

        emit!(ConfigRegistered {
            trust: g.trust,
            governance_config_id,
            quorum_bps: g.quorum_bps,
            support_bps: g.support_bps,
        });
        Ok(())
    }

    /// Cast a vote on a proposal. Records a `VoteRecord` PDA so the same voter
    /// can't double-vote, and bumps the proposal's tally. Vote power is
    /// passed in for now; the next iteration replaces this with a CPI to
    /// `aeqi_token::get_past_votes` (token mode) or
    /// `aeqi_role::get_past_role_votes` (per-role multisig).
    pub fn cast_vote(
        ctx: Context<CastVote>,
        choice: u8,
        weight: u128,
    ) -> Result<()> {
        require!(choice <= 2, GovernanceError::InvalidVoteChoice);
        require!(weight > 0, GovernanceError::ZeroWeight);

        let p = &mut ctx.accounts.proposal;
        let now = Clock::get()?.unix_timestamp;
        require!(!p.executed, GovernanceError::ProposalAlreadyExecuted);
        require!(!p.canceled, GovernanceError::ProposalCanceled);
        require!(now >= p.vote_start, GovernanceError::VotingNotStarted);
        require!(
            now < p.vote_start.checked_add(p.vote_duration).unwrap(),
            GovernanceError::VotingClosed
        );

        // Record vote — VoteRecord PDA init enforces uniqueness per voter.
        let v = &mut ctx.accounts.vote;
        v.trust = p.trust;
        v.proposal_id = p.proposal_id;
        v.voter = ctx.accounts.voter.key();
        v.choice = choice;
        v.weight = weight;
        v.bump = ctx.bumps.vote;

        match choice {
            0 => p.against_votes = p.against_votes.checked_add(weight).unwrap(),
            1 => p.for_votes = p.for_votes.checked_add(weight).unwrap(),
            2 => p.abstain_votes = p.abstain_votes.checked_add(weight).unwrap(),
            _ => unreachable!(),
        }

        emit!(VoteCast {
            trust: p.trust,
            proposal_id: p.proposal_id,
            voter: v.voter,
            choice,
            weight,
        });
        Ok(())
    }

    /// Create a proposal under a registered governance config. Per-proposal
    /// mode selection via `governance_config_id`. Mirrors EVM
    /// `Governance.module.propose`.
    pub fn propose(
        ctx: Context<Propose>,
        proposal_id: [u8; 32],
        governance_config_id: [u8; 32],
        ipfs_cid: [u8; 64],
    ) -> Result<()> {
        let cfg = &ctx.accounts.governance_config;
        require!(
            cfg.governance_config_id == governance_config_id,
            GovernanceError::ConfigMismatch
        );

        let now = Clock::get()?.unix_timestamp;
        let p = &mut ctx.accounts.proposal;
        p.trust = ctx.accounts.trust.key();
        p.proposal_id = proposal_id;
        p.governance_config_id = governance_config_id;
        p.proposer = ctx.accounts.proposer.key();
        p.ipfs_cid = ipfs_cid;
        p.vote_start = now;
        p.vote_duration = cfg.voting_period;
        p.execution_delay = cfg.execution_delay;
        p.for_votes = 0;
        p.against_votes = 0;
        p.abstain_votes = 0;
        p.executed = false;
        p.canceled = false;
        p.succeeded_at = 0;
        p.bump = ctx.bumps.proposal;

        let m = &mut ctx.accounts.module_state;
        m.proposal_count = m.proposal_count.checked_add(1).unwrap();

        emit!(ProposalCreated {
            trust: p.trust,
            proposal_id,
            governance_config_id,
            proposer: p.proposer,
            vote_start: p.vote_start,
            vote_duration: p.vote_duration,
        });
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// State
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct GovernanceModuleState {
    pub trust: Pubkey,
    pub proposal_count: u64,
    pub config_count: u32,
    pub bump: u8,
}

/// One per voting mode. Mirrors EVM `GovernanceConfig`.
#[account]
#[derive(InitSpace)]
pub struct GovernanceConfig {
    pub trust: Pubkey,
    pub governance_config_id: [u8; 32],
    pub proposal_threshold: u128,
    pub quorum_bps: u16,
    pub support_bps: u16,
    pub voting_period: i64,
    pub execution_delay: i64,
    pub allow_early_enact: bool,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct GovernanceConfigInput {
    pub proposal_threshold: u128,
    pub quorum_bps: u16,
    pub support_bps: u16,
    pub voting_period: i64,
    pub execution_delay: i64,
    pub allow_early_enact: bool,
}

/// One per (proposal, voter) pair — init enforces single-vote-per-voter.
#[account]
#[derive(InitSpace)]
pub struct VoteRecord {
    pub trust: Pubkey,
    pub proposal_id: [u8; 32],
    pub voter: Pubkey,
    pub choice: u8, // 0 = against, 1 = for, 2 = abstain
    pub weight: u128,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Proposal {
    pub trust: Pubkey,
    pub proposal_id: [u8; 32],
    pub governance_config_id: [u8; 32],
    pub proposer: Pubkey,
    pub ipfs_cid: [u8; 64],
    pub vote_start: i64,
    pub vote_duration: i64,
    pub execution_delay: i64,
    pub for_votes: u128,
    pub against_votes: u128,
    pub abstain_votes: u128,
    pub executed: bool,
    pub canceled: bool,
    pub succeeded_at: i64,
    pub bump: u8,
}

// -----------------------------------------------------------------------------
// Account contexts
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitGovernance<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + GovernanceModuleState::INIT_SPACE,
        seeds = [b"gov_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, GovernanceModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeGovernance<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
}

#[derive(Accounts)]
#[instruction(governance_config_id: [u8; 32])]
pub struct RegisterConfig<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"gov_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, GovernanceModuleState>,
    #[account(
        init,
        payer = payer,
        space = 8 + GovernanceConfig::INIT_SPACE,
        seeds = [b"gov_config", trust.key().as_ref(), governance_config_id.as_ref()],
        bump,
    )]
    pub governance_config: Account<'info, GovernanceConfig>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(proposal_id: [u8; 32], governance_config_id: [u8; 32])]
pub struct Propose<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"gov_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, GovernanceModuleState>,
    #[account(
        seeds = [b"gov_config", trust.key().as_ref(), governance_config_id.as_ref()],
        bump = governance_config.bump,
    )]
    pub governance_config: Account<'info, GovernanceConfig>,
    #[account(
        init,
        payer = proposer,
        space = 8 + Proposal::INIT_SPACE,
        seeds = [b"proposal", trust.key().as_ref(), proposal_id.as_ref()],
        bump,
    )]
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub proposer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CastVote<'info> {
    #[account(
        mut,
        seeds = [b"proposal", proposal.trust.as_ref(), proposal.proposal_id.as_ref()],
        bump = proposal.bump,
    )]
    pub proposal: Account<'info, Proposal>,
    #[account(
        init,
        payer = voter,
        space = 8 + VoteRecord::INIT_SPACE,
        seeds = [b"vote", proposal.trust.as_ref(), proposal.proposal_id.as_ref(), voter.key().as_ref()],
        bump,
    )]
    pub vote: Account<'info, VoteRecord>,
    #[account(mut)]
    pub voter: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------

#[event]
pub struct ConfigRegistered {
    pub trust: Pubkey,
    pub governance_config_id: [u8; 32],
    pub quorum_bps: u16,
    pub support_bps: u16,
}

#[event]
pub struct ProposalCreated {
    pub trust: Pubkey,
    pub proposal_id: [u8; 32],
    pub governance_config_id: [u8; 32],
    pub proposer: Pubkey,
    pub vote_start: i64,
    pub vote_duration: i64,
}

#[event]
pub struct VoteCast {
    pub trust: Pubkey,
    pub proposal_id: [u8; 32],
    pub voter: Pubkey,
    pub choice: u8,
    pub weight: u128,
}

#[error_code]
pub enum GovernanceError {
    #[msg("bps value must be ≤ 10000 (100.00%)")]
    InvalidBpsValue,
    #[msg("voting_period must be > 0")]
    ZeroVotingPeriod,
    #[msg("governance_config_id mismatch — config PDA doesn't match the id passed")]
    ConfigMismatch,
    #[msg("vote choice must be 0 (against), 1 (for), or 2 (abstain)")]
    InvalidVoteChoice,
    #[msg("vote weight must be > 0")]
    ZeroWeight,
    #[msg("proposal has already been executed")]
    ProposalAlreadyExecuted,
    #[msg("proposal was canceled")]
    ProposalCanceled,
    #[msg("voting has not yet started for this proposal")]
    VotingNotStarted,
    #[msg("voting has closed for this proposal")]
    VotingClosed,
}
