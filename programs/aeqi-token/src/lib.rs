//! aeqi_token — cap-table token, SPL Token-2022 mint authority.
//!
//! Ports `modules/Token.module.sol`. Each TRUST gets one Token-2022 mint
//! whose authority is a PDA of this program seeded
//! `[b"token_authority", trust]`. Module finalize decodes
//! `(name, symbol, decimals, max_supply, allocations[])` from
//! TRUST `BytesConfig` slot `TOKEN_TRUST_CONFIG_KEY` and creates the mint +
//! initial allocation accounts.
//!
//! This iteration: `init` stores the TokenModuleState PDA. Mint creation via
//! Token-2022 CPI lands as `create_mint` in the next iteration.

use anchor_lang::prelude::*;

declare_id!("V9WiXaeayA8KTyVAEEG1rAuPQ28G6NEwzSCmzZNZv6z");

#[program]
pub mod aeqi_token {
    use super::*;

    /// Module init — called by the factory (or directly by the user during
    /// company spawn). Creates the TokenModuleState PDA that anchors all
    /// subsequent token operations to this trust.
    pub fn init(ctx: Context<InitToken>) -> Result<()> {
        let module = &mut ctx.accounts.module_state;
        module.trust = ctx.accounts.trust.key();
        module.mint = Pubkey::default(); // set by create_mint
        module.initialized = ModuleInitState::Initialized as u8;
        module.bump = ctx.bumps.module_state;
        emit!(TokenModuleInitialized {
            trust: module.trust,
            module_state: ctx.accounts.module_state.key(),
        });
        Ok(())
    }

    /// Module finalize — decodes the config bytes the factory wrote into the
    /// trust's BytesConfig slot under `TOKEN_TRUST_CONFIG_KEY`. Mirrors EVM
    /// `Token.module.finalizeModule`. Skeleton; full decode + mint init
    /// lands in the next iteration.
    pub fn finalize(ctx: Context<FinalizeToken>) -> Result<()> {
        let module = &mut ctx.accounts.module_state;
        require!(
            module.initialized == ModuleInitState::Initialized as u8,
            TokenError::NotInitialized
        );
        module.initialized = ModuleInitState::Finalized as u8;
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct TokenModuleState {
    pub trust: Pubkey,
    pub mint: Pubkey,
    pub initialized: u8,
    pub bump: u8,
}

#[repr(u8)]
pub enum ModuleInitState {
    Pending = 0,
    Initialized = 1,
    Finalized = 2,
}

#[derive(Accounts)]
pub struct InitToken<'info> {
    /// CHECK: structurally validated by the parent trust PDA derivation; this
    /// module just records the trust key so subsequent ix can authorize against
    /// it.
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + TokenModuleState::INIT_SPACE,
        seeds = [b"token_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeToken<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"token_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
}

#[event]
pub struct TokenModuleInitialized {
    pub trust: Pubkey,
    pub module_state: Pubkey,
}

#[error_code]
pub enum TokenError {
    #[msg("token module not yet initialized")]
    NotInitialized,
}
