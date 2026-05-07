//! aeqi_factory — on-chain DAO factory.
//!
//! Ports `core/Factory.sol`. Template registry + instantiate flow. The
//! canonical create flow mirrors EVM `_createTRUST`:
//!
//! 1. Create the TRUST PDA (CPI into `aeqi_trust::initialize`).
//! 2. For each module in the template, register it on TRUST + CPI the module
//!    program's `init`.
//! 3. Wire ACL edges between modules.
//! 4. CPI each module's `finalize` so it loads its config.
//! 5. CPI `aeqi_trust::finalize` to exit creation mode.
//!
//! This file ships step 1 as `create_company` so cross-program CPI is proven
//! end-to-end. The full template-driven instantiate flow lands as
//! `instantiate_template` in WS-S6 (next iteration).

use anchor_lang::prelude::*;
use aeqi_trust::cpi::accounts::Initialize as TrustInitialize;
use aeqi_trust::program::AeqiTrust;

declare_id!("7rX3fnJUy7tDSpo1EGCnUhs1XnxxbsQzXXNDCTh64v6n");

#[program]
pub mod aeqi_factory {
    use super::*;

    /// Skeleton create flow — initializes a fresh TRUST PDA via CPI into
    /// `aeqi_trust::initialize`. The caller becomes the trust authority.
    /// Module registration / finalize / etc. follow in `instantiate_template`.
    pub fn create_company(ctx: Context<CreateCompany>, trust_id: [u8; 32]) -> Result<()> {
        let cpi_accounts = TrustInitialize {
            trust: ctx.accounts.trust.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.aeqi_trust_program.to_account_info(), cpi_accounts);
        aeqi_trust::cpi::initialize(cpi_ctx, trust_id)?;

        emit!(CompanyCreated {
            trust: ctx.accounts.trust.key(),
            trust_id,
            authority: ctx.accounts.authority.key(),
        });
        Ok(())
    }

    /// Register a template — stores config + module set + ACL graph + initial
    /// values keyed by `template_id`. Skeleton; full impl in WS-S6.
    pub fn register_template(_ctx: Context<RegisterTemplate>) -> Result<()> {
        Ok(())
    }

    /// Full instantiate — register_template-driven create flow that runs all
    /// 5 steps above. Skeleton.
    pub fn instantiate_template(_ctx: Context<InstantiateTemplate>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(trust_id: [u8; 32])]
pub struct CreateCompany<'info> {
    /// CHECK: validated structurally by aeqi_trust::initialize, which
    /// derives the PDA from `[b"trust", trust_id]` under its own program ID.
    #[account(mut)]
    pub trust: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub aeqi_trust_program: Program<'info, AeqiTrust>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterTemplate<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InstantiateTemplate<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[event]
pub struct CompanyCreated {
    pub trust: Pubkey,
    pub trust_id: [u8; 32],
    pub authority: Pubkey,
}
