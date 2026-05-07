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
use anchor_spl::token_interface::{Mint, TokenInterface};

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

    /// Create the SPL Token-2022 mint for this TRUST. Mint address is a PDA
    /// seeded `[b"mint", trust]` so callers can derive it deterministically.
    /// Authority for the mint is another PDA seeded
    /// `[b"token_authority", trust]`, owned by this program — only this
    /// program can mint or freeze.
    pub fn create_mint(ctx: Context<CreateMint>, decimals: u8) -> Result<()> {
        let module = &mut ctx.accounts.module_state;
        require!(
            module.initialized == ModuleInitState::Initialized as u8,
            TokenError::NotInitialized
        );
        require!(
            module.mint == Pubkey::default(),
            TokenError::MintAlreadyCreated
        );
        module.mint = ctx.accounts.mint.key();
        emit!(MintCreated {
            trust: module.trust,
            mint: module.mint,
            decimals,
        });
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

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateMint<'info> {
    /// CHECK: trust pda — used as the seed namespace.
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"token_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
    /// CHECK: program-controlled PDA mint authority. Only this program (via
    /// signer seeds) can mint or freeze the cap-table token.
    #[account(seeds = [b"token_authority", trust.key().as_ref()], bump)]
    pub mint_authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        mint::decimals = decimals,
        mint::authority = mint_authority,
        mint::token_program = token_program,
        seeds = [b"mint", trust.key().as_ref()],
        bump,
    )]
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[event]
pub struct TokenModuleInitialized {
    pub trust: Pubkey,
    pub module_state: Pubkey,
}

#[event]
pub struct MintCreated {
    pub trust: Pubkey,
    pub mint: Pubkey,
    pub decimals: u8,
}

#[error_code]
pub enum TokenError {
    #[msg("token module not yet initialized")]
    NotInitialized,
    #[msg("mint already created for this trust")]
    MintAlreadyCreated,
}
