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
use aeqi_role::cpi::accounts::{FinalizeModule as RoleFinalize, InitModule as RoleInit};
use aeqi_role::program::AeqiRole;
use aeqi_token::cpi::accounts::{FinalizeToken, InitToken};
use aeqi_token::program::AeqiToken;
use aeqi_governance::cpi::accounts::{FinalizeGovernance, InitGovernance};
use aeqi_governance::program::AeqiGovernance;

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

    /// Full atomic spawn — closes the gap where the EVM Factory orchestrates
    /// everything in one tx. Runs all 5 steps of `Factory._createTRUST`
    /// for the canonical 3-module configuration (role + token + governance):
    ///
    ///   1. CPI `aeqi_trust::initialize` (creates trust PDA, creation mode)
    ///   2. CPI `aeqi_trust::register_module` ×3 (one per module slot)
    ///   3. CPI each module's `init` (creates its module-state PDA bound
    ///      to the trust)
    ///   4. CPI `aeqi_trust::finalize` (exits creation mode)
    ///
    /// Module finalize CPIs (config-bytes decode) are NOT yet called here —
    /// that requires the BytesConfig dispatch flow which follows.
    /// Tx size: ~13 accounts; should fit comfortably in 1232 bytes.
    pub fn create_company_full(
        ctx: Context<CreateCompanyFull>,
        trust_id: [u8; 32],
        role_module_id: [u8; 32],
        token_module_id: [u8; 32],
        gov_module_id: [u8; 32],
        role_acl: u64,
        token_acl: u64,
        gov_acl: u64,
    ) -> Result<()> {
        // 1. initialize trust
        let init_accs = TrustInitialize {
            trust: ctx.accounts.trust.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        aeqi_trust::cpi::initialize(
            CpiContext::new(ctx.accounts.aeqi_trust_program.to_account_info(), init_accs),
            trust_id,
        )?;

        // 2. register the 3 modules on trust (one CPI each)
        aeqi_trust::cpi::register_module(
            CpiContext::new(
                ctx.accounts.aeqi_trust_program.to_account_info(),
                TrustRegisterModule {
                    trust: ctx.accounts.trust.to_account_info(),
                    module: ctx.accounts.role_module.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                },
            ),
            role_module_id,
            ctx.accounts.aeqi_role_program.key(),
            role_acl,
        )?;
        aeqi_trust::cpi::register_module(
            CpiContext::new(
                ctx.accounts.aeqi_trust_program.to_account_info(),
                TrustRegisterModule {
                    trust: ctx.accounts.trust.to_account_info(),
                    module: ctx.accounts.token_module.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                },
            ),
            token_module_id,
            ctx.accounts.aeqi_token_program.key(),
            token_acl,
        )?;
        aeqi_trust::cpi::register_module(
            CpiContext::new(
                ctx.accounts.aeqi_trust_program.to_account_info(),
                TrustRegisterModule {
                    trust: ctx.accounts.trust.to_account_info(),
                    module: ctx.accounts.gov_module.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                },
            ),
            gov_module_id,
            ctx.accounts.aeqi_governance_program.key(),
            gov_acl,
        )?;

        // 3. CPI each module's init — creates the module-state PDA
        aeqi_role::cpi::init(CpiContext::new(
            ctx.accounts.aeqi_role_program.to_account_info(),
            RoleInit {
                trust: ctx.accounts.trust.to_account_info(),
                module_state: ctx.accounts.role_module_state.to_account_info(),
                payer: ctx.accounts.authority.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
            },
        ))?;
        aeqi_token::cpi::init(CpiContext::new(
            ctx.accounts.aeqi_token_program.to_account_info(),
            InitToken {
                trust: ctx.accounts.trust.to_account_info(),
                module_state: ctx.accounts.token_module_state.to_account_info(),
                payer: ctx.accounts.authority.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
            },
        ))?;
        aeqi_governance::cpi::init(CpiContext::new(
            ctx.accounts.aeqi_governance_program.to_account_info(),
            InitGovernance {
                trust: ctx.accounts.trust.to_account_info(),
                module_state: ctx.accounts.gov_module_state.to_account_info(),
                payer: ctx.accounts.authority.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
            },
        ))?;

        // 4. finalize each module — transitions Initialized → Finalized.
        // (Module config-bytes decode flow follows when BytesConfig dispatch
        // ships; for now finalize is a state-machine transition.)
        aeqi_role::cpi::finalize(CpiContext::new(
            ctx.accounts.aeqi_role_program.to_account_info(),
            RoleFinalize {
                trust: ctx.accounts.trust.to_account_info(),
                module_state: ctx.accounts.role_module_state.to_account_info(),
            },
        ))?;
        aeqi_token::cpi::finalize(CpiContext::new(
            ctx.accounts.aeqi_token_program.to_account_info(),
            FinalizeToken {
                trust: ctx.accounts.trust.to_account_info(),
                module_state: ctx.accounts.token_module_state.to_account_info(),
            },
        ))?;
        aeqi_governance::cpi::finalize(CpiContext::new(
            ctx.accounts.aeqi_governance_program.to_account_info(),
            FinalizeGovernance {
                trust: ctx.accounts.trust.to_account_info(),
            },
        ))?;

        // 5. finalize trust
        aeqi_trust::cpi::finalize(CpiContext::new(
            ctx.accounts.aeqi_trust_program.to_account_info(),
            TrustFinalize {
                trust: ctx.accounts.trust.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ))?;

        emit!(CompanyFullySpawned {
            trust: ctx.accounts.trust.key(),
            trust_id,
            authority: ctx.accounts.authority.key(),
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

    /// Full template-driven create flow: reads a registered Template PDA and
    /// replays its module set against a fresh TRUST. Mirrors EVM
    /// `Factory._createTRUST` reading from `Factory.templates[templateId]`.
    ///
    /// `remaining_accounts` layout: one Module PDA per module in the
    /// template, in declaration order. The aeqi_trust program will init them
    /// pairwise during CPI.
    ///
    /// Steps run atomically:
    ///   1. CPI aeqi_trust::initialize (creates trust, enters creation mode)
    ///   2. For each ModuleSpec in template.modules: CPI register_module
    ///   3. CPI aeqi_trust::finalize (exits creation mode)
    ///
    /// Module init/finalize CPIs (loading per-module config) land separately
    /// once each module's surface stabilizes. ACL-edge CPIs likewise.
    pub fn instantiate_template<'info>(
        ctx: Context<'_, '_, 'info, 'info, InstantiateTemplate<'info>>,
        trust_id: [u8; 32],
    ) -> Result<()> {
        let template = &ctx.accounts.template;
        require!(
            !template.modules.is_empty(),
            FactoryError::EmptyModuleSet
        );
        require!(
            ctx.remaining_accounts.len() == template.modules.len(),
            FactoryError::ModuleAccountCountMismatch
        );

        // 1. initialize trust
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

        // 2. register each module from the template's spec
        for (spec, module_acct) in template.modules.iter().zip(ctx.remaining_accounts.iter()) {
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

        // 3. finalize trust
        let fin_accounts = TrustFinalize {
            trust: ctx.accounts.trust.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let fin_ctx = CpiContext::new(
            ctx.accounts.aeqi_trust_program.to_account_info(),
            fin_accounts,
        );
        aeqi_trust::cpi::finalize(fin_ctx)?;

        emit!(TemplateInstantiated {
            trust: ctx.accounts.trust.key(),
            trust_id,
            template_id: template.template_id,
            module_count: template.modules.len() as u8,
        });
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
#[instruction(
    trust_id: [u8; 32],
    role_module_id: [u8; 32],
    token_module_id: [u8; 32],
    gov_module_id: [u8; 32],
)]
pub struct CreateCompanyFull<'info> {
    /// CHECK: aeqi_trust::initialize derives + creates the PDA.
    #[account(mut)]
    pub trust: UncheckedAccount<'info>,
    /// CHECK: aeqi_trust::register_module creates this PDA.
    #[account(mut)]
    pub role_module: UncheckedAccount<'info>,
    /// CHECK: aeqi_trust::register_module creates this PDA.
    #[account(mut)]
    pub token_module: UncheckedAccount<'info>,
    /// CHECK: aeqi_trust::register_module creates this PDA.
    #[account(mut)]
    pub gov_module: UncheckedAccount<'info>,
    /// CHECK: aeqi_role::init creates this PDA.
    #[account(mut)]
    pub role_module_state: UncheckedAccount<'info>,
    /// CHECK: aeqi_token::init creates this PDA.
    #[account(mut)]
    pub token_module_state: UncheckedAccount<'info>,
    /// CHECK: aeqi_governance::init creates this PDA.
    #[account(mut)]
    pub gov_module_state: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub aeqi_trust_program: Program<'info, AeqiTrust>,
    pub aeqi_role_program: Program<'info, AeqiRole>,
    pub aeqi_token_program: Program<'info, AeqiToken>,
    pub aeqi_governance_program: Program<'info, AeqiGovernance>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(trust_id: [u8; 32])]
pub struct InstantiateTemplate<'info> {
    #[account(seeds = [b"template", template.template_id.as_ref()], bump = template.bump)]
    pub template: Account<'info, Template>,
    /// CHECK: aeqi_trust::initialize derives the PDA from
    /// `[b"trust", trust_id]` under its own program ID.
    #[account(mut)]
    pub trust: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub aeqi_trust_program: Program<'info, AeqiTrust>,
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
pub struct CompanyFullySpawned {
    pub trust: Pubkey,
    pub trust_id: [u8; 32],
    pub authority: Pubkey,
}

#[event]
pub struct TemplateInstantiated {
    pub trust: Pubkey,
    pub trust_id: [u8; 32],
    pub template_id: [u8; 32],
    pub module_count: u8,
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
