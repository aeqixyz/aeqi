//! aeqi_treasury — USDC vault for an AEQI company.
//!
//! Ports `modules/Fund.module.sol` + treasury bits of the EVM framework. A
//! treasury holds USDC (or other SPL tokens) in a program-controlled vault
//! ATA. Deposits are permissionless. Withdrawals are gated by either the
//! TRUST authority (creation mode) or a successful governance proposal CPI
//! (live mode) — for now this skeleton accepts the trust authority signing
//! directly; full governance gating lands once `aeqi_governance.execute_proposal`
//! grows ix dispatch.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

declare_id!("CQ7TGZFmkoZh61xgKnbjcj9Uomht38LqeihMNsY4p9KC");

#[program]
pub mod aeqi_treasury {
    use super::*;

    pub fn init(ctx: Context<InitTreasury>, treasury_authority: Pubkey) -> Result<()> {
        let m = &mut ctx.accounts.module_state;
        m.trust = ctx.accounts.trust.key();
        m.treasury_authority = treasury_authority;
        m.bump = ctx.bumps.module_state;
        Ok(())
    }

    /// Deposit `amount` into the treasury vault. Permissionless — anyone
    /// can fund the treasury. Wraps the SPL transfer so the indexer gets a
    /// typed `TreasuryDeposited` event instead of having to filter raw
    /// Token-2022 transfers.
    pub fn deposit(ctx: Context<TreasuryDeposit>, amount: u64) -> Result<()> {
        let cpi = TransferChecked {
            from: ctx.accounts.depositor_ta.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.depositor.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi);
        transfer_checked(cpi_ctx, amount, ctx.accounts.mint.decimals)?;

        emit!(TreasuryDeposited {
            trust: ctx.accounts.module_state.trust,
            depositor_ta: ctx.accounts.depositor_ta.key(),
            amount,
        });
        Ok(())
    }

    /// Withdraw `amount` from the treasury vault to `recipient_ta`. The
    /// vault is owned by the program-controlled PDA
    /// `[b"treasury_vault_authority", trust]`; we sign via PDA seeds.
    /// Authority gate: caller must equal `module_state.treasury_authority`.
    pub fn withdraw(ctx: Context<TreasuryWithdraw>, amount: u64) -> Result<()> {
        let m = &ctx.accounts.module_state;
        require_keys_eq!(
            ctx.accounts.authority.key(),
            m.treasury_authority,
            TreasuryError::Unauthorized
        );

        let trust_key = ctx.accounts.trust.key();
        let bump = ctx.bumps.vault_authority;
        let seeds: &[&[&[u8]]] = &[&[b"treasury_vault_authority", trust_key.as_ref(), &[bump]]];

        let cpi = TransferChecked {
            from: ctx.accounts.vault.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.recipient_ta.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi,
            seeds,
        );
        transfer_checked(cpi_ctx, amount, ctx.accounts.mint.decimals)?;

        emit!(TreasuryWithdrew {
            trust: m.trust,
            recipient_ta: ctx.accounts.recipient_ta.key(),
            amount,
        });
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct TreasuryModuleState {
    pub trust: Pubkey,
    /// The single account allowed to authorize withdrawals. In creation mode
    /// the factory sets this to the trust authority; in live mode it gets
    /// rewritten to a governance-signer PDA so withdrawals require an executed
    /// proposal.
    pub treasury_authority: Pubkey,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct InitTreasury<'info> {
    /// CHECK: trust pda — used as seed namespace.
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + TreasuryModuleState::INIT_SPACE,
        seeds = [b"treasury_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, TreasuryModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TreasuryDeposit<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        seeds = [b"treasury_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TreasuryModuleState>,
    /// CHECK: vault authority PDA — used as the seed namespace for the vault.
    /// Doesn't sign the deposit (depositor signs).
    #[account(seeds = [b"treasury_vault_authority", trust.key().as_ref()], bump)]
    pub vault_authority: UncheckedAccount<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = vault_authority)]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint)]
    pub depositor_ta: InterfaceAccount<'info, TokenAccount>,
    pub depositor: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct TreasuryWithdraw<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        seeds = [b"treasury_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TreasuryModuleState>,
    /// CHECK: program-controlled vault authority PDA. Signed via signer seeds.
    #[account(seeds = [b"treasury_vault_authority", trust.key().as_ref()], bump)]
    pub vault_authority: UncheckedAccount<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = vault_authority)]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint)]
    pub recipient_ta: InterfaceAccount<'info, TokenAccount>,
    pub authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[event]
pub struct TreasuryWithdrew {
    pub trust: Pubkey,
    pub recipient_ta: Pubkey,
    pub amount: u64,
}

#[event]
pub struct TreasuryDeposited {
    pub trust: Pubkey,
    pub depositor_ta: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum TreasuryError {
    #[msg("caller is not the configured treasury authority")]
    Unauthorized,
}
