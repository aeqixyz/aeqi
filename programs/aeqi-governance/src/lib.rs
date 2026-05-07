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
//! Skeleton — proposal/vote/execute instructions land in subsequent commits.

use anchor_lang::prelude::*;

declare_id!("528PTeSk8M3pKMMhc5vitbcwMGUMcHMzg6G5XpX8iVBn");

#[program]
pub mod aeqi_governance {
    use super::*;

    pub fn init(_ctx: Context<InitModule>) -> Result<()> {
        Ok(())
    }

    pub fn finalize(_ctx: Context<FinalizeModule>) -> Result<()> {
        Ok(())
    }

    pub fn propose(_ctx: Context<Propose>) -> Result<()> {
        // TODO: GovernanceConfig lookup → vote-power gate → proposal PDA init.
        Ok(())
    }

    pub fn cast_vote(_ctx: Context<CastVote>) -> Result<()> {
        // TODO: power lookup via aeqi_token or aeqi_role CPI based on
        // governance_config_id selector. Tally + early-enact check.
        Ok(())
    }

    pub fn execute_proposal(_ctx: Context<ExecuteProposal>) -> Result<()> {
        // TODO: state machine check + dispatch each call via remaining accs.
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitModule<'info> {
    /// CHECK: trust pda
    pub trust: AccountInfo<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeModule<'info> {
    /// CHECK: trust pda
    pub trust: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct Propose<'info> {
    #[account(mut)]
    pub proposer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CastVote<'info> {
    pub voter: Signer<'info>,
}

#[derive(Accounts)]
pub struct ExecuteProposal<'info> {
    pub executor: Signer<'info>,
}
