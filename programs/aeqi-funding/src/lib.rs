//! aeqi_funding — capital raise orchestration.
//!
//! Ports `modules/Funding.module.sol`. A FundingRequest declares the *intent*
//! to raise capital via one of the three Unifutures primitives:
//!   - CommitmentSale (fixed-price pre-sale)
//!   - BondingCurve (continuous-curve issuance)
//!   - Exit (pro-rata redemption)
//!
//! Lifecycle (full EVM model — implemented incrementally):
//!   1. `create_funding_request` — declares the intent, references a Budget
//!      for the asset allocation
//!   2. `activate` — draws from Budget, creates the corresponding Unifutures
//!      primitive (CPIs into aeqi_unifutures) [pending]
//!   3. `on_tokens_claimed` — hook fired when Unifutures tokens are claimed,
//!      creates vesting roles for buyers via aeqi_role + aeqi_vesting CPIs
//!      [pending]
//!   4. `finalize` — closes the funding round, returns excess to Budget
//!      [pending]
//!
//! This iteration ships state + create only. The CPI-orchestrated lifecycle
//! follows once the inter-module CPI surfaces stabilize.

use anchor_lang::prelude::*;

declare_id!("8EAVY6uosAatbwhemj1gsPB47WwwmDLzi2t7yo2b8CWV");

#[program]
pub mod aeqi_funding {
    use super::*;

    pub fn init(ctx: Context<InitFunding>) -> Result<()> {
        let m = &mut ctx.accounts.module_state;
        m.trust = ctx.accounts.trust.key();
        m.request_count = 0;
        m.bump = ctx.bumps.module_state;
        Ok(())
    }

    /// Declare a funding request. Records the intent without activating.
    /// `kind` is 0 (CommitmentSale), 1 (BondingCurve), or 2 (Exit).
    pub fn create_funding_request(
        ctx: Context<CreateFundingRequest>,
        request_id: [u8; 32],
        kind: u8,
        budget_id: [u8; 32],
        asset_amount: u64,
        target_quote: u64,
    ) -> Result<()> {
        require!(kind <= 2, FundingError::InvalidKind);
        require!(asset_amount > 0, FundingError::ZeroAmount);
        require!(target_quote > 0, FundingError::ZeroAmount);

        let now = Clock::get()?.unix_timestamp;
        let r = &mut ctx.accounts.request;
        r.trust = ctx.accounts.trust.key();
        r.request_id = request_id;
        r.creator = ctx.accounts.creator.key();
        r.kind = kind;
        r.budget_id = budget_id;
        r.asset_amount = asset_amount;
        r.target_quote = target_quote;
        r.status = RequestStatus::Pending as u8;
        r.created_at = now;
        r.primitive_id = [0u8; 32]; // set on activation
        r.bump = ctx.bumps.request;

        let m = &mut ctx.accounts.module_state;
        m.request_count = m.request_count.checked_add(1).unwrap();

        emit!(FundingRequestCreated {
            trust: r.trust,
            request_id,
            creator: r.creator,
            kind,
            budget_id,
            asset_amount,
            target_quote,
        });
        Ok(())
    }

    /// Cancel a pending funding request. Only the creator can cancel.
    pub fn cancel_funding_request(ctx: Context<CancelFundingRequest>) -> Result<()> {
        let r = &mut ctx.accounts.request;
        require_keys_eq!(
            ctx.accounts.creator.key(),
            r.creator,
            FundingError::Unauthorized
        );
        require!(
            r.status == RequestStatus::Pending as u8,
            FundingError::CannotCancel
        );
        r.status = RequestStatus::Cancelled as u8;
        emit!(FundingRequestCancelled {
            trust: r.trust,
            request_id: r.request_id,
        });
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct FundingModuleState {
    pub trust: Pubkey,
    pub request_count: u64,
    pub bump: u8,
}

#[repr(u8)]
pub enum RequestStatus {
    Pending = 0,
    Activated = 1,
    Finalized = 2,
    Cancelled = 3,
}

#[account]
#[derive(InitSpace)]
pub struct FundingRequest {
    pub trust: Pubkey,
    pub request_id: [u8; 32],
    pub creator: Pubkey,
    pub kind: u8, // 0=CommitmentSale 1=BondingCurve 2=Exit
    pub budget_id: [u8; 32],
    pub asset_amount: u64,
    pub target_quote: u64,
    pub status: u8,
    pub created_at: i64,
    /// Set on activation to the underlying Unifutures primitive's id
    /// (sale_id / curve_id / exit_id depending on kind).
    pub primitive_id: [u8; 32],
    pub bump: u8,
}

#[derive(Accounts)]
pub struct InitFunding<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + FundingModuleState::INIT_SPACE,
        seeds = [b"funding_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, FundingModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(request_id: [u8; 32])]
pub struct CreateFundingRequest<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"funding_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, FundingModuleState>,
    #[account(
        init,
        payer = creator,
        space = 8 + FundingRequest::INIT_SPACE,
        seeds = [b"funding_request", trust.key().as_ref(), request_id.as_ref()],
        bump,
    )]
    pub request: Account<'info, FundingRequest>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CancelFundingRequest<'info> {
    #[account(
        mut,
        seeds = [b"funding_request", request.trust.as_ref(), request.request_id.as_ref()],
        bump = request.bump,
    )]
    pub request: Account<'info, FundingRequest>,
    pub creator: Signer<'info>,
}

#[event]
pub struct FundingRequestCreated {
    pub trust: Pubkey,
    pub request_id: [u8; 32],
    pub creator: Pubkey,
    pub kind: u8,
    pub budget_id: [u8; 32],
    pub asset_amount: u64,
    pub target_quote: u64,
}

#[event]
pub struct FundingRequestCancelled {
    pub trust: Pubkey,
    pub request_id: [u8; 32],
}

#[error_code]
pub enum FundingError {
    #[msg("kind must be 0 (CommitmentSale), 1 (BondingCurve), or 2 (Exit)")]
    InvalidKind,
    #[msg("amount must be > 0")]
    ZeroAmount,
    #[msg("only creator can cancel a request")]
    Unauthorized,
    #[msg("request is not in Pending status — can't cancel")]
    CannotCancel,
}
