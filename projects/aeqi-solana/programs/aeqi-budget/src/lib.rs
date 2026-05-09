//! aeqi_budget — role-bound treasury allocations + spend tracking.
//!
//! Ports `modules/Budget.module.sol`. Each Budget allocates an `amount` to a
//! `target_role_id`; spends decrement the budget's `spent` counter against
//! the cap. Budgets can be frozen/unfrozen by their grantor, and have an
//! optional expiry. Authorization to spend is gated by role authority —
//! caller must hold a role that is the target_role itself or an ancestor
//! (verified off-chain via the role DAG walk; this module just records
//! the gate, not the walk).
//!
//! Settlement of actual funds is delegated: a Budget records the *intent*
//! to spend; the corresponding token transfer happens via `aeqi_treasury`
//! or another module that respects the budget's allocation as a quota.

use anchor_lang::prelude::*;

declare_id!("2XVZqURv6hVL7EEMd4BL1zyJhngSiAEV2q4yCgbQjASA");

#[program]
pub mod aeqi_budget {
    use super::*;

    pub fn init(ctx: Context<InitBudget>) -> Result<()> {
        let m = &mut ctx.accounts.module_state;
        m.trust = ctx.accounts.trust.key();
        m.budget_count = 0;
        m.bump = ctx.bumps.module_state;
        Ok(())
    }

    /// Create a budget allocation for a role. The grantor (typically a
    /// treasury authority or governance signer) signs to lock the
    /// allocation. Budget can be sourced from TRUST (no parent) or
    /// from a parent budget (which the grantor must control).
    pub fn create_budget(
        ctx: Context<CreateBudget>,
        budget_id: [u8; 32],
        target_role_id: [u8; 32],
        amount: u64,
        expiry: i64,
        parent_budget_id: Option<[u8; 32]>,
    ) -> Result<()> {
        require!(amount > 0, BudgetError::ZeroAmount);
        let now = Clock::get()?.unix_timestamp;
        require!(expiry == 0 || expiry > now, BudgetError::InvalidExpiry);

        let b = &mut ctx.accounts.budget;
        b.trust = ctx.accounts.trust.key();
        b.budget_id = budget_id;
        b.grantor = ctx.accounts.grantor.key();
        b.target_role_id = target_role_id;
        b.parent_budget_id = parent_budget_id.unwrap_or([0u8; 32]);
        b.amount = amount;
        b.spent = 0;
        b.expiry = expiry;
        b.frozen = false;
        b.bump = ctx.bumps.budget;

        let m = &mut ctx.accounts.module_state;
        m.budget_count = m.budget_count.checked_add(1).unwrap();

        emit!(BudgetCreated {
            trust: b.trust,
            budget_id,
            grantor: b.grantor,
            target_role_id,
            amount,
            expiry,
        });
        Ok(())
    }

    /// Record a spend against the budget. Caller must be a role-authorized
    /// signer (verification of `caller` against role.account is the role
    /// module's job and is enforced upstream by the calling module —
    /// budget just enforces the cap + expiry + frozen flag).
    pub fn record_spend(ctx: Context<RecordSpend>, amount: u64) -> Result<()> {
        require!(amount > 0, BudgetError::ZeroAmount);
        let b = &mut ctx.accounts.budget;
        require!(!b.frozen, BudgetError::BudgetFrozen);
        if b.expiry != 0 {
            let now = Clock::get()?.unix_timestamp;
            require!(now < b.expiry, BudgetError::BudgetExpired);
        }
        let new_spent = b.spent.checked_add(amount).ok_or(error!(BudgetError::MathOverflow))?;
        require!(new_spent <= b.amount, BudgetError::ExceedsAllocation);
        b.spent = new_spent;

        emit!(BudgetSpent {
            trust: b.trust,
            budget_id: b.budget_id,
            amount,
            total_spent: b.spent,
        });
        Ok(())
    }

    /// Freeze a budget — blocks further spends. Grantor signs.
    pub fn freeze(ctx: Context<Freeze>) -> Result<()> {
        let b = &mut ctx.accounts.budget;
        require_keys_eq!(
            ctx.accounts.grantor.key(),
            b.grantor,
            BudgetError::Unauthorized
        );
        b.frozen = true;
        emit!(BudgetFrozen {
            trust: b.trust,
            budget_id: b.budget_id,
        });
        Ok(())
    }

    /// Unfreeze. Grantor signs.
    pub fn unfreeze(ctx: Context<Freeze>) -> Result<()> {
        let b = &mut ctx.accounts.budget;
        require_keys_eq!(
            ctx.accounts.grantor.key(),
            b.grantor,
            BudgetError::Unauthorized
        );
        b.frozen = false;
        emit!(BudgetUnfrozen {
            trust: b.trust,
            budget_id: b.budget_id,
        });
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct BudgetModuleState {
    pub trust: Pubkey,
    pub budget_count: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Budget {
    pub trust: Pubkey,
    pub budget_id: [u8; 32],
    pub grantor: Pubkey,
    pub target_role_id: [u8; 32],
    /// Parent budget if hierarchical; [0u8; 32] if sourced from TRUST directly.
    pub parent_budget_id: [u8; 32],
    pub amount: u64,
    pub spent: u64,
    pub expiry: i64,
    pub frozen: bool,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct InitBudget<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + BudgetModuleState::INIT_SPACE,
        seeds = [b"budget_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, BudgetModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(budget_id: [u8; 32])]
pub struct CreateBudget<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"budget_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, BudgetModuleState>,
    #[account(
        init,
        payer = grantor,
        space = 8 + Budget::INIT_SPACE,
        seeds = [b"budget", trust.key().as_ref(), budget_id.as_ref()],
        bump,
    )]
    pub budget: Account<'info, Budget>,
    #[account(mut)]
    pub grantor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RecordSpend<'info> {
    #[account(
        mut,
        seeds = [b"budget", budget.trust.as_ref(), budget.budget_id.as_ref()],
        bump = budget.bump,
    )]
    pub budget: Account<'info, Budget>,
    pub spender: Signer<'info>,
}

#[derive(Accounts)]
pub struct Freeze<'info> {
    #[account(
        mut,
        seeds = [b"budget", budget.trust.as_ref(), budget.budget_id.as_ref()],
        bump = budget.bump,
    )]
    pub budget: Account<'info, Budget>,
    pub grantor: Signer<'info>,
}

#[event]
pub struct BudgetCreated {
    pub trust: Pubkey,
    pub budget_id: [u8; 32],
    pub grantor: Pubkey,
    pub target_role_id: [u8; 32],
    pub amount: u64,
    pub expiry: i64,
}

#[event]
pub struct BudgetSpent {
    pub trust: Pubkey,
    pub budget_id: [u8; 32],
    pub amount: u64,
    pub total_spent: u64,
}

#[event]
pub struct BudgetFrozen {
    pub trust: Pubkey,
    pub budget_id: [u8; 32],
}

#[event]
pub struct BudgetUnfrozen {
    pub trust: Pubkey,
    pub budget_id: [u8; 32],
}

#[error_code]
pub enum BudgetError {
    #[msg("amount must be > 0")]
    ZeroAmount,
    #[msg("expiry must be 0 (no expiry) or in the future")]
    InvalidExpiry,
    #[msg("budget is frozen")]
    BudgetFrozen,
    #[msg("budget has expired")]
    BudgetExpired,
    #[msg("spend would exceed budget.amount")]
    ExceedsAllocation,
    #[msg("math overflow")]
    MathOverflow,
    #[msg("caller is not the budget's grantor")]
    Unauthorized,
}
