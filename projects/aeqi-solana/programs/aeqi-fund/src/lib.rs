//! aeqi_fund — NAV-based fund accounting, LP shares, deposits, redeems.
//!
//! Ports `modules/Fund.module.sol`. Each Fund accepts deposits in a quote
//! asset (USDC), issues LP shares (1:1 at first deposit, NAV-based after),
//! and tracks gross NAV across positions for redemption pricing.
//!
//! Skeleton scope (this iteration):
//! - Fund + LpShare PDAs
//! - create_fund (manager defines quote_mint + share_price unit)
//! - deposit (LP transfers quote → fund vault, receives shares pro-rata
//!   to current NAV — at first deposit, share_price = 1)
//! - redeem (LP burns shares, receives quote pro-rata to current NAV)
//!
//! Full EVM Fund.module features still pending in subsequent iterations:
//! - NAV-batch processing (`_processNAVAndCarryBatched`)
//! - Position-manager `markPosition()` CPI iteration
//! - Carry calculation on outperformance vs high-water mark
//! - Director carry-vesting via aeqi_vesting CPI
//! - Flow-request settlement (deposit/redeem at next checkpoint)

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

declare_id!("4QJQsnRYUyXo9EFxAayL79zAkFejjdWeKhoTXeMVK7Nv");

const PRECISION: u128 = 1_000_000_000_000_000_000; // 1e18 — same as unifutures

#[program]
pub mod aeqi_fund {
    use super::*;

    pub fn init(ctx: Context<InitFund>) -> Result<()> {
        let m = &mut ctx.accounts.module_state;
        m.trust = ctx.accounts.trust.key();
        m.fund_count = 0;
        m.bump = ctx.bumps.module_state;
        Ok(())
    }

    /// Create a fund. The manager defines the quote_mint (USDC etc.). The
    /// fund starts with NAV=0, total_shares=0; share price is 1:1 at first
    /// deposit and adjusts based on NAV thereafter.
    pub fn create_fund(
        ctx: Context<CreateFund>,
        fund_id: [u8; 32],
        carry_bps: u16,
    ) -> Result<()> {
        require!(carry_bps <= 10_000, FundError::InvalidBps);
        let f = &mut ctx.accounts.fund;
        f.trust = ctx.accounts.trust.key();
        f.fund_id = fund_id;
        f.manager = ctx.accounts.manager.key();
        f.quote_mint = ctx.accounts.quote_mint.key();
        f.gross_nav = 0;
        f.total_shares = 0;
        f.high_water_mark = 0;
        f.carry_bps = carry_bps;
        f.accrued_carry = 0;
        f.bump = ctx.bumps.fund;

        let m = &mut ctx.accounts.module_state;
        m.fund_count = m.fund_count.checked_add(1).unwrap();

        emit!(FundCreated {
            trust: f.trust,
            fund_id,
            manager: f.manager,
            quote_mint: f.quote_mint,
            carry_bps,
        });
        Ok(())
    }

    /// LP deposits `amount` of quote into the fund. Receives shares
    /// proportional to current NAV: shares = amount * total_shares / gross_nav
    /// (1:1 at first deposit when gross_nav == 0). The actual share token
    /// is recorded in an LpShare PDA per LP — no separate SPL mint.
    pub fn deposit(ctx: Context<FundDeposit>, amount: u64) -> Result<()> {
        require!(amount > 0, FundError::ZeroAmount);
        let f = &mut ctx.accounts.fund;

        // Transfer quote: lp_quote_ta → fund_quote_vault (LP signs)
        let cpi = TransferChecked {
            from: ctx.accounts.lp_quote_ta.to_account_info(),
            mint: ctx.accounts.quote_mint.to_account_info(),
            to: ctx.accounts.fund_quote_vault.to_account_info(),
            authority: ctx.accounts.lp.to_account_info(),
        };
        transfer_checked(
            CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi),
            amount,
            ctx.accounts.quote_mint.decimals,
        )?;

        // Compute shares to issue
        let shares = if f.gross_nav == 0 || f.total_shares == 0 {
            amount as u128
        } else {
            (amount as u128)
                .checked_mul(f.total_shares as u128)
                .ok_or(error!(FundError::MathOverflow))?
                .checked_div(f.gross_nav as u128)
                .ok_or(error!(FundError::MathOverflow))?
        };
        let shares_u64: u64 = shares
            .try_into()
            .map_err(|_| error!(FundError::MathOverflow))?;
        require!(shares_u64 > 0, FundError::ShareTooSmall);

        f.gross_nav = f.gross_nav.checked_add(amount).unwrap();
        f.total_shares = f.total_shares.checked_add(shares_u64).unwrap();

        let s = &mut ctx.accounts.lp_share;
        s.trust = f.trust;
        s.fund_id = f.fund_id;
        s.lp = ctx.accounts.lp.key();
        s.shares = s.shares.checked_add(shares_u64).unwrap();
        s.bump = ctx.bumps.lp_share;

        emit!(FundDeposited {
            trust: f.trust,
            fund_id: f.fund_id,
            lp: s.lp,
            quote_in: amount,
            shares_issued: shares_u64,
        });
        Ok(())
    }

    /// Manager-only mark-to-market. Recompute LP-attributable NAV; if it
    /// crosses the prior HWM, accrue carry on the increase and reset HWM
    /// to the post-carry NAV. Down-marks just reduce gross_nav (no carry
    /// clawback — high-water-mark semantics).
    ///
    /// `new_gross_nav` is the manager's reported portfolio mark including
    /// any unclaimed carry already sitting in the vault — i.e. the full
    /// vault valuation, not LP-attributable. Carry is split off here.
    pub fn update_nav(ctx: Context<UpdateNav>, new_gross_nav: u64) -> Result<()> {
        let f = &mut ctx.accounts.fund;
        require_keys_eq!(
            ctx.accounts.manager.key(),
            f.manager,
            FundError::NotManager
        );

        // Subtract already-accrued carry so we're working in LP terms.
        let lp_nav = new_gross_nav
            .checked_sub(f.accrued_carry)
            .ok_or(error!(FundError::MathOverflow))?;

        if lp_nav > f.high_water_mark {
            let increase = lp_nav - f.high_water_mark;
            let carry = (increase as u128)
                .checked_mul(f.carry_bps as u128)
                .ok_or(error!(FundError::MathOverflow))?
                / 10_000u128;
            let carry_u64: u64 = carry
                .try_into()
                .map_err(|_| error!(FundError::MathOverflow))?;
            f.accrued_carry = f.accrued_carry.checked_add(carry_u64).unwrap();
            f.gross_nav = lp_nav.checked_sub(carry_u64).unwrap();
            f.high_water_mark = f.gross_nav;
        } else {
            f.gross_nav = lp_nav;
            // HWM unchanged on down-marks.
        }

        emit!(NavUpdated {
            trust: f.trust,
            fund_id: f.fund_id,
            gross_nav: f.gross_nav,
            high_water_mark: f.high_water_mark,
            accrued_carry: f.accrued_carry,
        });
        Ok(())
    }

    /// Manager claims accrued carry from the fund vault. Resets
    /// `accrued_carry` to zero. Vault → manager TA, PDA-signed.
    pub fn claim_carry(ctx: Context<ClaimCarry>) -> Result<()> {
        let f = &mut ctx.accounts.fund;
        require_keys_eq!(
            ctx.accounts.manager.key(),
            f.manager,
            FundError::NotManager
        );
        let carry = f.accrued_carry;
        require!(carry > 0, FundError::NoCarry);

        let trust_key = f.trust;
        let fund_id_bytes = f.fund_id;
        let bump = ctx.bumps.fund_authority;
        let seeds: &[&[&[u8]]] = &[&[
            b"fund_authority",
            trust_key.as_ref(),
            fund_id_bytes.as_ref(),
            &[bump],
        ]];
        let cpi = TransferChecked {
            from: ctx.accounts.fund_quote_vault.to_account_info(),
            mint: ctx.accounts.quote_mint.to_account_info(),
            to: ctx.accounts.manager_quote_ta.to_account_info(),
            authority: ctx.accounts.fund_authority.to_account_info(),
        };
        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi,
                seeds,
            ),
            carry,
            ctx.accounts.quote_mint.decimals,
        )?;

        f.accrued_carry = 0;

        emit!(CarryClaimed {
            trust: f.trust,
            fund_id: f.fund_id,
            manager: f.manager,
            amount: carry,
        });
        Ok(())
    }

    /// LP burns `shares` to receive quote pro-rata to NAV. Reverses
    /// deposit: quote_out = shares * gross_nav / total_shares.
    pub fn redeem(ctx: Context<FundRedeem>, shares: u64) -> Result<()> {
        require!(shares > 0, FundError::ZeroAmount);
        let f = &mut ctx.accounts.fund;
        require!(f.total_shares > 0, FundError::EmptyFund);

        let s = &mut ctx.accounts.lp_share;
        require_keys_eq!(
            ctx.accounts.lp.key(),
            s.lp,
            FundError::Unauthorized
        );
        require!(shares <= s.shares, FundError::InsufficientShares);

        let quote_out_u128 = (shares as u128)
            .checked_mul(f.gross_nav as u128)
            .ok_or(error!(FundError::MathOverflow))?
            .checked_div(f.total_shares as u128)
            .ok_or(error!(FundError::MathOverflow))?;
        let quote_out: u64 = quote_out_u128
            .try_into()
            .map_err(|_| error!(FundError::MathOverflow))?;
        require!(quote_out > 0, FundError::ShareTooSmall);

        // PDA-signed transfer fund_quote_vault → lp_quote_ta
        let trust_key = f.trust;
        let fund_id_bytes = f.fund_id;
        let bump = ctx.bumps.fund_authority;
        let seeds: &[&[&[u8]]] = &[&[
            b"fund_authority",
            trust_key.as_ref(),
            fund_id_bytes.as_ref(),
            &[bump],
        ]];
        let cpi = TransferChecked {
            from: ctx.accounts.fund_quote_vault.to_account_info(),
            mint: ctx.accounts.quote_mint.to_account_info(),
            to: ctx.accounts.lp_quote_ta.to_account_info(),
            authority: ctx.accounts.fund_authority.to_account_info(),
        };
        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi,
                seeds,
            ),
            quote_out,
            ctx.accounts.quote_mint.decimals,
        )?;

        s.shares = s.shares.checked_sub(shares).unwrap();
        f.total_shares = f.total_shares.checked_sub(shares).unwrap();
        f.gross_nav = f.gross_nav.checked_sub(quote_out).unwrap();

        emit!(FundRedeemed {
            trust: f.trust,
            fund_id: f.fund_id,
            lp: s.lp,
            shares_burned: shares,
            quote_out,
        });
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// State
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct FundModuleState {
    pub trust: Pubkey,
    pub fund_count: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Fund {
    pub trust: Pubkey,
    pub fund_id: [u8; 32],
    pub manager: Pubkey,
    pub quote_mint: Pubkey,
    /// LP-attributable NAV. Excludes `accrued_carry` so deposit/redeem
    /// share-price math remains LP-fair.
    pub gross_nav: u64,
    pub total_shares: u64,
    pub high_water_mark: u64,
    pub carry_bps: u16,
    /// Carry the manager has earned via NAV-up updates beyond HWM. Sits
    /// in the fund vault until `claim_carry` transfers it out to the
    /// manager. Counted separately from `gross_nav` so the share price
    /// LPs see is exactly what they're owed.
    pub accrued_carry: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct LpShare {
    pub trust: Pubkey,
    pub fund_id: [u8; 32],
    pub lp: Pubkey,
    pub shares: u64,
    pub bump: u8,
}

// -----------------------------------------------------------------------------
// Account contexts
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitFund<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + FundModuleState::INIT_SPACE,
        seeds = [b"fund_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, FundModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(fund_id: [u8; 32])]
pub struct CreateFund<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"fund_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, FundModuleState>,
    #[account(
        init,
        payer = manager,
        space = 8 + Fund::INIT_SPACE,
        seeds = [b"fund", trust.key().as_ref(), fund_id.as_ref()],
        bump,
    )]
    pub fund: Account<'info, Fund>,
    pub quote_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub manager: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FundDeposit<'info> {
    #[account(
        mut,
        seeds = [b"fund", fund.trust.as_ref(), fund.fund_id.as_ref()],
        bump = fund.bump,
    )]
    pub fund: Box<Account<'info, Fund>>,
    /// CHECK: PDA — owns the quote vault
    #[account(seeds = [b"fund_authority", fund.trust.as_ref(), fund.fund_id.as_ref()], bump)]
    pub fund_authority: UncheckedAccount<'info>,
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = quote_mint, token::authority = fund_authority)]
    pub fund_quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = quote_mint, token::authority = lp)]
    pub lp_quote_ta: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = lp,
        space = 8 + LpShare::INIT_SPACE,
        seeds = [b"lp_share", fund.trust.as_ref(), fund.fund_id.as_ref(), lp.key().as_ref()],
        bump,
    )]
    pub lp_share: Box<Account<'info, LpShare>>,
    #[account(mut)]
    pub lp: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FundRedeem<'info> {
    #[account(
        mut,
        seeds = [b"fund", fund.trust.as_ref(), fund.fund_id.as_ref()],
        bump = fund.bump,
    )]
    pub fund: Box<Account<'info, Fund>>,
    /// CHECK: PDA — signs the quote out-transfer
    #[account(seeds = [b"fund_authority", fund.trust.as_ref(), fund.fund_id.as_ref()], bump)]
    pub fund_authority: UncheckedAccount<'info>,
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = quote_mint, token::authority = fund_authority)]
    pub fund_quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = quote_mint)]
    pub lp_quote_ta: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [b"lp_share", fund.trust.as_ref(), fund.fund_id.as_ref(), lp.key().as_ref()],
        bump = lp_share.bump,
    )]
    pub lp_share: Box<Account<'info, LpShare>>,
    pub lp: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct UpdateNav<'info> {
    #[account(
        mut,
        seeds = [b"fund", fund.trust.as_ref(), fund.fund_id.as_ref()],
        bump = fund.bump,
    )]
    pub fund: Box<Account<'info, Fund>>,
    pub manager: Signer<'info>,
}

#[derive(Accounts)]
pub struct ClaimCarry<'info> {
    #[account(
        mut,
        seeds = [b"fund", fund.trust.as_ref(), fund.fund_id.as_ref()],
        bump = fund.bump,
    )]
    pub fund: Box<Account<'info, Fund>>,
    /// CHECK: PDA — signs the carry out-transfer
    #[account(seeds = [b"fund_authority", fund.trust.as_ref(), fund.fund_id.as_ref()], bump)]
    pub fund_authority: UncheckedAccount<'info>,
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = quote_mint, token::authority = fund_authority)]
    pub fund_quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = quote_mint)]
    pub manager_quote_ta: Box<InterfaceAccount<'info, TokenAccount>>,
    pub manager: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------

#[event]
pub struct FundCreated {
    pub trust: Pubkey,
    pub fund_id: [u8; 32],
    pub manager: Pubkey,
    pub quote_mint: Pubkey,
    pub carry_bps: u16,
}

#[event]
pub struct FundDeposited {
    pub trust: Pubkey,
    pub fund_id: [u8; 32],
    pub lp: Pubkey,
    pub quote_in: u64,
    pub shares_issued: u64,
}

#[event]
pub struct FundRedeemed {
    pub trust: Pubkey,
    pub fund_id: [u8; 32],
    pub lp: Pubkey,
    pub shares_burned: u64,
    pub quote_out: u64,
}

#[event]
pub struct NavUpdated {
    pub trust: Pubkey,
    pub fund_id: [u8; 32],
    pub gross_nav: u64,
    pub high_water_mark: u64,
    pub accrued_carry: u64,
}

#[event]
pub struct CarryClaimed {
    pub trust: Pubkey,
    pub fund_id: [u8; 32],
    pub manager: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum FundError {
    #[msg("amount must be > 0")]
    ZeroAmount,
    #[msg("carry_bps must be ≤ 10000 (100%)")]
    InvalidBps,
    #[msg("math overflow")]
    MathOverflow,
    #[msg("computed shares or quote_out rounded to zero")]
    ShareTooSmall,
    #[msg("fund has no shares — no LPs to redeem to")]
    EmptyFund,
    #[msg("caller is not the LP recorded on this share account")]
    Unauthorized,
    #[msg("LP doesn't have enough shares")]
    InsufficientShares,
    #[msg("only the fund manager can call this ix")]
    NotManager,
    #[msg("no accrued carry to claim")]
    NoCarry,
}
