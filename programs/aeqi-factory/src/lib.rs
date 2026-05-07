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
//! `create_company` ships step 1 as a standalone helper (proves CPI). The
//! full template-driven instantiate is the staged pipeline above; templates
//! land on-chain via `register_template` and get replayed by
//! `instantiate_template` in the next iteration.

use anchor_lang::prelude::*;
use aeqi_trust::cpi::accounts::{
    Finalize as TrustFinalize, Initialize as TrustInitialize, RegisterModule as TrustRegisterModule,
};
use aeqi_trust::program::AeqiTrust;

declare_id!("7rX3fnJUy7tDSpo1EGCnUhs1XnxxbsQzXXNDCTh64v6n");

pub mod state;
pub use state::*;

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

    /// Atomic spawn — full company creation flow in one tx, skipping module
    /// init/finalize CPIs (those land once each module program implements
    /// its `init` cleanly). Steps:
    ///
    ///   1. CPI `aeqi_trust::initialize` (creates Trust PDA in creation mode).
    ///   2. For each `ModuleSpec` in `modules`, CPI `aeqi_trust::register_module`.
    ///      The matching module PDAs are passed in `remaining_accounts`,
    ///      grouped pairwise as (module_pda, system_program) per module.
    ///   3. CPI `aeqi_trust::finalize` (exits creation mode).
    ///
    /// `remaining_accounts` layout: for each module spec, push:
    ///   - the module PDA (writable, will be init'd by aeqi_trust)
    ///
    /// The caller (the `authority`) signs all CPIs as the trust authority.
    pub fn create_with_modules<'info>(
        ctx: Context<'_, '_, 'info, 'info, CreateWithModules<'info>>,
        trust_id: [u8; 32],
        modules: Vec<ModuleSpec>,
    ) -> Result<()> {
        require!(!modules.is_empty(), FactoryError::EmptyModuleSet);
        require!(modules.len() <= 16, FactoryError::TooManyModules);
        require!(
            ctx.remaining_accounts.len() == modules.len(),
            FactoryError::ModuleAccountCountMismatch
        );

        // 1. initialize
        let init_accounts = TrustInitialize {
            trust: ctx.accounts.trust.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        let init_ctx = CpiContext::new(
            ctx.accounts.aeqi_trust_program.to_account_info(),
            init_accounts,
        );
        aeqi_trust::cpi::initialize(init_ctx, trust_id)?;

        // 2. register every module
        for (spec, module_acct) in modules.iter().zip(ctx.remaining_accounts.iter()) {
            let reg_accounts = TrustRegisterModule {
                trust: ctx.accounts.trust.to_account_info(),
                module: module_acct.clone(),
                authority: ctx.accounts.authority.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
            };
            let reg_ctx = CpiContext::new(
                ctx.accounts.aeqi_trust_program.to_account_info(),
                reg_accounts,
            );
            aeqi_trust::cpi::register_module(
                reg_ctx,
                spec.module_id,
                spec.program_id,
                spec.trust_acl,
            )?;
        }

        // 3. finalize
        let fin_accounts = TrustFinalize {
            trust: ctx.accounts.trust.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let fin_ctx = CpiContext::new(
            ctx.accounts.aeqi_trust_program.to_account_info(),
            fin_accounts,
        );
        aeqi_trust::cpi::finalize(fin_ctx)?;

        emit!(CompanySpawned {
            trust: ctx.accounts.trust.key(),
            trust_id,
            authority: ctx.accounts.authority.key(),
            module_count: modules.len() as u8,
        });
        Ok(())
    }

    /// Register a template — stores the module set, ACL graph, and admin so
    /// `instantiate_template` can later replay this against a fresh TRUST.
    /// Mirrors EVM `Factory.registerTemplate` + `FactoryLibrary.Template`.
    pub fn register_template(
        ctx: Context<RegisterTemplate>,
        template_id: [u8; 32],
        modules: Vec<ModuleSpec>,
        acl_edges: Vec<AclEdgeSpec>,
    ) -> Result<()> {
        require!(!modules.is_empty(), FactoryError::EmptyModuleSet);
        require!(modules.len() <= 16, FactoryError::TooManyModules);
        require!(acl_edges.len() <= 64, FactoryError::TooManyAclEdges);

        let template = &mut ctx.accounts.template;
        template.template_id = template_id;
        template.admin = ctx.accounts.admin.key();
        template.modules = modules;
        template.acl_edges = acl_edges;
        template.bump = ctx.bumps.template;

        emit!(TemplateRegistered {
            template_id,
            admin: template.admin,
            module_count: template.modules.len() as u8,
            acl_edge_count: template.acl_edges.len() as u8,
        });
        Ok(())
    }

    /// Full instantiate — register_template-driven create flow that runs all
    /// 5 steps (initialize → register modules → wire ACLs → module finalize →
    /// trust finalize). Skeleton; lands incrementally as module init/finalize
    /// CPI surfaces stabilize across role/token/governance.
    pub fn instantiate_template(_ctx: Context<InstantiateTemplate>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(trust_id: [u8; 32])]
pub struct CreateCompany<'info> {
    /// CHECK: validated structurally by aeqi_trust::initialize, which derives
    /// the PDA from `[b"trust", trust_id]` under its own program ID.
    #[account(mut)]
    pub trust: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub aeqi_trust_program: Program<'info, AeqiTrust>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(template_id: [u8; 32], modules: Vec<ModuleSpec>, acl_edges: Vec<AclEdgeSpec>)]
pub struct RegisterTemplate<'info> {
    #[account(
        init,
        payer = admin,
        space = Template::space(modules.len(), acl_edges.len()),
        seeds = [b"template", template_id.as_ref()],
        bump,
    )]
    pub template: Account<'info, Template>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(trust_id: [u8; 32])]
pub struct CreateWithModules<'info> {
    /// CHECK: aeqi_trust::initialize derives the PDA from
    /// `[b"trust", trust_id]` under its own program ID.
    #[account(mut)]
    pub trust: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub aeqi_trust_program: Program<'info, AeqiTrust>,
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

#[event]
pub struct TemplateRegistered {
    pub template_id: [u8; 32],
    pub admin: Pubkey,
    pub module_count: u8,
    pub acl_edge_count: u8,
}

#[event]
pub struct CompanySpawned {
    pub trust: Pubkey,
    pub trust_id: [u8; 32],
    pub authority: Pubkey,
    pub module_count: u8,
}

#[error_code]
pub enum FactoryError {
    #[msg("template must declare at least one module")]
    EmptyModuleSet,
    #[msg("template module set exceeds maximum (16)")]
    TooManyModules,
    #[msg("template ACL edges exceed maximum (64)")]
    TooManyAclEdges,
    #[msg("remaining_accounts.len() must equal modules.len()")]
    ModuleAccountCountMismatch,
}
