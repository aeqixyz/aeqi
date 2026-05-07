//! aeqi_factory — on-chain DAO factory.
//!
//! Ports `core/Factory.sol`. Template registry + multi-sig approval gate +
//! instantiate flow. Templates declare a set of modules + ACL graph + value
//! configs. `instantiate_template` mirrors EVM `_createTRUST`:
//!
//! 1. Create the TRUST PDA (CPI into `aeqi_trust::initialize`).
//! 2. For each module in the template, register it on TRUST + CPI the module
//!    program's `init`.
//! 3. Wire ACL edges between modules.
//! 4. CPI each module's `finalize` so it loads its config.
//! 5. CPI `aeqi_trust::finalize` to exit creation mode.
//!
//! Skeleton — full implementation lands in WS-S6.

use anchor_lang::prelude::*;

declare_id!("7rX3fnJUy7tDSpo1EGCnUhs1XnxxbsQzXXNDCTh64v6n");

#[program]
pub mod aeqi_factory {
    use super::*;

    /// Register a new template — name + module set + ACL graph + default
    /// value configs. Approved-creator gate enforced by signature multisig
    /// from the factory admin set.
    pub fn register_template(_ctx: Context<RegisterTemplate>) -> Result<()> {
        // TODO: implement template registration.
        Ok(())
    }

    /// Instantiate a TRUST from a registered template. Multi-sig approval gate
    /// mirrors EVM `Factory.registerTRUST` + `approveTRUST` flow.
    pub fn instantiate_template(_ctx: Context<InstantiateTemplate>) -> Result<()> {
        // TODO: orchestrate the 5-step create flow.
        Ok(())
    }
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
