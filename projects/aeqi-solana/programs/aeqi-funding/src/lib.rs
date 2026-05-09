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
use aeqi_unifutures::cpi::accounts::{CreateCommitmentSale, CreateCurve, CreateExit};
use aeqi_unifutures::program::AeqiUnifutures;

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
        // CommitmentSale needs concrete amounts at request time; BondingCurve
        // and Exit carry their parameters in the activation call (price curve
        // / exit_quote are kind-specific and meaningless here), so the zero
        // gate is kind=0 only.
        if kind == 0 {
            require!(asset_amount > 0, FundingError::ZeroAmount);
            require!(target_quote > 0, FundingError::ZeroAmount);
        }

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

    /// Activate a CommitmentSale-kind funding request — CPIs into
    /// `aeqi_unifutures::create_commitment_sale` with the request's params.
    /// Sets status = Activated, primitive_id = the new sale's id.
    /// (BondingCurve + Exit activation follow the same shape; this iteration
    /// covers kind=0 only.)
    pub fn activate_commitment_sale<'info>(
        ctx: Context<'_, '_, 'info, 'info, ActivateCommitmentSale<'info>>,
        sale_id: [u8; 32],
        overflow_quote: u64,
        duration_secs: i64,
    ) -> Result<()> {
        let r = &mut ctx.accounts.request;
        require!(
            r.status == RequestStatus::Pending as u8,
            FundingError::CannotActivate
        );
        require!(r.kind == 0, FundingError::WrongKind);

        let cpi = CreateCommitmentSale {
            trust: ctx.accounts.trust.to_account_info(),
            module_state: ctx.accounts.unifutures_module_state.to_account_info(),
            sale: ctx.accounts.sale.to_account_info(),
            creator: ctx.accounts.creator.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        aeqi_unifutures::cpi::create_commitment_sale(
            CpiContext::new(ctx.accounts.aeqi_unifutures_program.to_account_info(), cpi),
            sale_id,
            r.asset_amount,
            r.target_quote,
            overflow_quote,
            duration_secs,
        )?;

        r.status = RequestStatus::Activated as u8;
        r.primitive_id = sale_id;

        emit!(FundingRequestActivated {
            trust: r.trust,
            request_id: r.request_id,
            kind: r.kind,
            primitive_id: sale_id,
        });
        Ok(())
    }

    /// Activate a BondingCurve-kind funding request — CPIs into
    /// `aeqi_unifutures::create_curve`.
    pub fn activate_bonding_curve<'info>(
        ctx: Context<'_, '_, 'info, 'info, ActivateBondingCurve<'info>>,
        curve_id: [u8; 32],
        curve_type: u8,
        start_price: u128,
        end_price: u128,
        max_supply: u64,
        reserve_ratio_ppm: u32,
    ) -> Result<()> {
        let r = &mut ctx.accounts.request;
        require!(
            r.status == RequestStatus::Pending as u8,
            FundingError::CannotActivate
        );
        require!(r.kind == 1, FundingError::WrongKind);

        let cpi = CreateCurve {
            trust: ctx.accounts.trust.to_account_info(),
            module_state: ctx.accounts.unifutures_module_state.to_account_info(),
            curve: ctx.accounts.curve.to_account_info(),
            creator: ctx.accounts.creator.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        aeqi_unifutures::cpi::create_curve(
            CpiContext::new(ctx.accounts.aeqi_unifutures_program.to_account_info(), cpi),
            curve_id,
            curve_type,
            start_price,
            end_price,
            max_supply,
            reserve_ratio_ppm,
        )?;

        r.status = RequestStatus::Activated as u8;
        r.primitive_id = curve_id;

        emit!(FundingRequestActivated {
            trust: r.trust,
            request_id: r.request_id,
            kind: r.kind,
            primitive_id: curve_id,
        });
        Ok(())
    }

    /// Activate an Exit-kind funding request — CPIs into
    /// `aeqi_unifutures::create_exit`.
    pub fn activate_exit<'info>(
        ctx: Context<'_, '_, 'info, 'info, ActivateExit<'info>>,
        exit_id: [u8; 32],
        exit_quote: u64,
        total_supply_snapshot: u64,
        duration_secs: i64,
    ) -> Result<()> {
        let r = &mut ctx.accounts.request;
        require!(
            r.status == RequestStatus::Pending as u8,
            FundingError::CannotActivate
        );
        require!(r.kind == 2, FundingError::WrongKind);

        let cpi = CreateExit {
            trust: ctx.accounts.trust.to_account_info(),
            module_state: ctx.accounts.unifutures_module_state.to_account_info(),
            exit: ctx.accounts.exit.to_account_info(),
            creator: ctx.accounts.creator.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        aeqi_unifutures::cpi::create_exit(
            CpiContext::new(ctx.accounts.aeqi_unifutures_program.to_account_info(), cpi),
            exit_id,
            exit_quote,
            total_supply_snapshot,
            duration_secs,
        )?;

        r.status = RequestStatus::Activated as u8;
        r.primitive_id = exit_id;

        emit!(FundingRequestActivated {
            trust: r.trust,
            request_id: r.request_id,
            kind: r.kind,
            primitive_id: exit_id,
        });
        Ok(())
    }

    /// Finalize an Activated funding request — closes the lifecycle once
    /// the underlying Unifutures primitive has settled. Caller is the
    /// creator (they own request lifecycle), and finalize is permanent;
    /// downstream excess-budget refund / vesting role hooks will read
    /// `status == Finalized` as their gate.
    pub fn finalize_funding_request(ctx: Context<FinalizeFundingRequest>) -> Result<()> {
        let r = &mut ctx.accounts.request;
        require_keys_eq!(
            ctx.accounts.creator.key(),
            r.creator,
            FundingError::Unauthorized
        );
        require!(
            r.status == RequestStatus::Activated as u8,
            FundingError::CannotFinalize
        );
        r.status = RequestStatus::Finalized as u8;
        emit!(FundingRequestFinalized {
            trust: r.trust,
            request_id: r.request_id,
            kind: r.kind,
            primitive_id: r.primitive_id,
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
pub struct ActivateCommitmentSale<'info> {
    #[account(
        mut,
        seeds = [b"funding_request", request.trust.as_ref(), request.request_id.as_ref()],
        bump = request.bump,
    )]
    pub request: Account<'info, FundingRequest>,
    /// CHECK: trust pda — passed through to aeqi_unifutures CPI
    pub trust: UncheckedAccount<'info>,
    /// CHECK: aeqi_unifutures' module_state PDA — validated by the CPI
    #[account(mut)]
    pub unifutures_module_state: UncheckedAccount<'info>,
    /// CHECK: aeqi_unifutures will init the CommitmentSale PDA
    #[account(mut)]
    pub sale: UncheckedAccount<'info>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub aeqi_unifutures_program: Program<'info, AeqiUnifutures>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ActivateBondingCurve<'info> {
    #[account(
        mut,
        seeds = [b"funding_request", request.trust.as_ref(), request.request_id.as_ref()],
        bump = request.bump,
    )]
    pub request: Account<'info, FundingRequest>,
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    /// CHECK: unifutures module_state
    #[account(mut)]
    pub unifutures_module_state: UncheckedAccount<'info>,
    /// CHECK: aeqi_unifutures inits the BondingCurve PDA
    #[account(mut)]
    pub curve: UncheckedAccount<'info>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub aeqi_unifutures_program: Program<'info, AeqiUnifutures>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ActivateExit<'info> {
    #[account(
        mut,
        seeds = [b"funding_request", request.trust.as_ref(), request.request_id.as_ref()],
        bump = request.bump,
    )]
    pub request: Account<'info, FundingRequest>,
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    /// CHECK: unifutures module_state
    #[account(mut)]
    pub unifutures_module_state: UncheckedAccount<'info>,
    /// CHECK: aeqi_unifutures inits the Exit PDA
    #[account(mut)]
    pub exit: UncheckedAccount<'info>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub aeqi_unifutures_program: Program<'info, AeqiUnifutures>,
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

#[derive(Accounts)]
pub struct FinalizeFundingRequest<'info> {
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

#[event]
pub struct FundingRequestActivated {
    pub trust: Pubkey,
    pub request_id: [u8; 32],
    pub kind: u8,
    pub primitive_id: [u8; 32],
}

#[event]
pub struct FundingRequestFinalized {
    pub trust: Pubkey,
    pub request_id: [u8; 32],
    pub kind: u8,
    pub primitive_id: [u8; 32],
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
    #[msg("request is not in Pending status — can't activate")]
    CannotActivate,
    #[msg("request is not in Activated status — can't finalize")]
    CannotFinalize,
    #[msg("request kind doesn't match this activation ix (kind=0 for CommitmentSale)")]
    WrongKind,
}
