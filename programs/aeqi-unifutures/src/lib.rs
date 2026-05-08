//! aeqi_unifutures — bonding curves, commitment sales, exits.
//!
//! Ports `modules/Unifutures.module.sol` + position managers. Three
//! primitives in the EVM original; this crate ships them incrementally:
//!
//! - **BondingCurve** ← this iteration: state PDA + math + create_curve.
//!   Buy/sell ixes follow.
//! - CommitmentSale (fixed-price pre-sale w/ countdown) — pending
//! - Exit (pro-rata redemption) — pending
//!
//! Curve math is in `curve.rs` with unit tests covering linear +
//! exponential price, trapezoidal-rule purchase cost, and reserve-ratio
//! sale return.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

pub mod curve;
pub use curve::CurveType;

declare_id!("2AqvqotDRhQj67YGn3MaZPUoYFBUEbnEbvbLD8Q2mF4s");

#[program]
pub mod aeqi_unifutures {
    use super::*;

    /// Module init — creates UnifuturesModuleState PDA bound to a trust.
    pub fn init(ctx: Context<InitUnifutures>) -> Result<()> {
        let m = &mut ctx.accounts.module_state;
        m.trust = ctx.accounts.trust.key();
        m.curve_count = 0;
        m.bump = ctx.bumps.module_state;
        Ok(())
    }

    /// Create a bonding curve. Curve config is immutable after creation.
    /// Validates `start_price < end_price` (rising curves are the typical
    /// case) is NOT enforced — falling curves are allowed; the math handles
    /// either direction. `max_supply > 0` is enforced.
    pub fn create_curve(
        ctx: Context<CreateCurve>,
        curve_id: [u8; 32],
        curve_type: u8,
        start_price: u128,
        end_price: u128,
        max_supply: u64,
        reserve_ratio_ppm: u32,
    ) -> Result<()> {
        require!(max_supply > 0, UnifuturesError::ZeroMaxSupply);
        require!(
            reserve_ratio_ppm <= 1_000_000,
            UnifuturesError::InvalidReserveRatio
        );
        let _ct = CurveType::from_u8(curve_type)
            .ok_or_else(|| error!(UnifuturesError::InvalidCurveType))?;

        let c = &mut ctx.accounts.curve;
        c.trust = ctx.accounts.trust.key();
        c.curve_id = curve_id;
        c.creator = ctx.accounts.creator.key();
        c.curve_type = curve_type;
        c.start_price = start_price;
        c.end_price = end_price;
        c.max_supply = max_supply;
        c.current_supply = 0;
        c.reserve_balance = 0;
        c.reserve_ratio_ppm = reserve_ratio_ppm;
        c.proceeds_collected = 0;
        c.bump = ctx.bumps.curve;

        let m = &mut ctx.accounts.module_state;
        m.curve_count = m.curve_count.checked_add(1).unwrap();

        emit!(CurveCreated {
            trust: c.trust,
            curve_id,
            creator: c.creator,
            curve_type,
            start_price,
            end_price,
            max_supply,
        });
        Ok(())
    }

    /// Buy `token_amount` of asset from the curve. Buyer pays `cost` of
    /// quote tokens (computed from the curve), receives `token_amount` of
    /// asset tokens from the program-controlled curve_asset_vault.
    /// `max_cost` is slippage protection — reverts if cost exceeds it.
    pub fn buy_from_curve(
        ctx: Context<BuyFromCurve>,
        token_amount: u64,
        max_cost: u64,
    ) -> Result<()> {
        require!(token_amount > 0, UnifuturesError::ZeroAmount);

        let c = &mut ctx.accounts.curve;
        let ct = CurveType::from_u8(c.curve_type)
            .ok_or_else(|| error!(UnifuturesError::InvalidCurveType))?;

        require!(
            c.current_supply.checked_add(token_amount).unwrap() <= c.max_supply,
            UnifuturesError::ExceedsMaxSupply
        );

        let cost_u128 = curve::purchase_cost(
            ct,
            c.start_price,
            c.end_price,
            c.max_supply as u128,
            c.current_supply as u128,
            token_amount as u128,
        )
        .ok_or_else(|| error!(UnifuturesError::MathOverflow))?;
        let cost: u64 = cost_u128.try_into().map_err(|_| error!(UnifuturesError::MathOverflow))?;
        require!(cost <= max_cost, UnifuturesError::SlippageExceeded);

        // 1. buyer pays quote → curve_quote_vault (buyer signs)
        let cpi_in = TransferChecked {
            from: ctx.accounts.buyer_quote_ta.to_account_info(),
            mint: ctx.accounts.quote_mint.to_account_info(),
            to: ctx.accounts.curve_quote_vault.to_account_info(),
            authority: ctx.accounts.buyer.to_account_info(),
        };
        transfer_checked(
            CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_in),
            cost,
            ctx.accounts.quote_mint.decimals,
        )?;

        // 2. curve_asset_vault → buyer_asset_ta (curve_authority PDA signs)
        let trust_key = c.trust;
        let curve_id_bytes = c.curve_id;
        let bump = ctx.bumps.curve_authority;
        let seeds: &[&[&[u8]]] = &[&[
            b"curve_authority",
            trust_key.as_ref(),
            curve_id_bytes.as_ref(),
            &[bump],
        ]];
        let cpi_out = TransferChecked {
            from: ctx.accounts.curve_asset_vault.to_account_info(),
            mint: ctx.accounts.asset_mint.to_account_info(),
            to: ctx.accounts.buyer_asset_ta.to_account_info(),
            authority: ctx.accounts.curve_authority.to_account_info(),
        };
        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi_out,
                seeds,
            ),
            token_amount,
            ctx.accounts.asset_mint.decimals,
        )?;

        c.current_supply = c.current_supply.checked_add(token_amount).unwrap();
        c.reserve_balance = c.reserve_balance.checked_add(cost as u128).unwrap();
        c.proceeds_collected = c.proceeds_collected.checked_add(cost as u128).unwrap();

        emit!(CurveBuy {
            trust: c.trust,
            curve_id: c.curve_id,
            buyer: ctx.accounts.buyer.key(),
            token_amount,
            cost,
        });
        Ok(())
    }

    /// Read-only — quotes the cost to buy `token_amount` at the curve's
    /// current state. Useful for client-side previews; on-chain just
    /// returns the value via the program's logged return.
    pub fn quote_buy(ctx: Context<QuoteBuy>, token_amount: u64) -> Result<u128> {
        let c = &ctx.accounts.curve;
        let ct = CurveType::from_u8(c.curve_type)
            .ok_or_else(|| error!(UnifuturesError::InvalidCurveType))?;
        let cost = curve::purchase_cost(
            ct,
            c.start_price,
            c.end_price,
            c.max_supply as u128,
            c.current_supply as u128,
            token_amount as u128,
        )
        .ok_or_else(|| error!(UnifuturesError::MathOverflow))?;
        Ok(cost)
    }
}

// -----------------------------------------------------------------------------
// State
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct UnifuturesModuleState {
    pub trust: Pubkey,
    pub curve_count: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct BondingCurve {
    pub trust: Pubkey,
    pub curve_id: [u8; 32],
    pub creator: Pubkey,
    pub curve_type: u8, // 0=linear, 1=exponential
    pub start_price: u128,
    pub end_price: u128,
    pub max_supply: u64,
    pub current_supply: u64,
    pub reserve_balance: u128,
    pub reserve_ratio_ppm: u32,
    pub proceeds_collected: u128,
    pub bump: u8,
}

// -----------------------------------------------------------------------------
// Account contexts
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitUnifutures<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + UnifuturesModuleState::INIT_SPACE,
        seeds = [b"unifutures_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, UnifuturesModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(curve_id: [u8; 32])]
pub struct CreateCurve<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"unifutures_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, UnifuturesModuleState>,
    #[account(
        init,
        payer = creator,
        space = 8 + BondingCurve::INIT_SPACE,
        seeds = [b"curve", trust.key().as_ref(), curve_id.as_ref()],
        bump,
    )]
    pub curve: Account<'info, BondingCurve>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct QuoteBuy<'info> {
    pub curve: Account<'info, BondingCurve>,
}

#[derive(Accounts)]
pub struct BuyFromCurve<'info> {
    #[account(
        mut,
        seeds = [b"curve", curve.trust.as_ref(), curve.curve_id.as_ref()],
        bump = curve.bump,
    )]
    pub curve: Box<Account<'info, BondingCurve>>,
    /// CHECK: program-controlled vault authority — signs the asset out-transfer.
    #[account(seeds = [b"curve_authority", curve.trust.as_ref(), curve.curve_id.as_ref()], bump)]
    pub curve_authority: UncheckedAccount<'info>,
    pub asset_mint: Box<InterfaceAccount<'info, Mint>>,
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = asset_mint, token::authority = curve_authority)]
    pub curve_asset_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = quote_mint, token::authority = curve_authority)]
    pub curve_quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = asset_mint)]
    pub buyer_asset_ta: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = quote_mint, token::authority = buyer)]
    pub buyer_quote_ta: Box<InterfaceAccount<'info, TokenAccount>>,
    pub buyer: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[event]
pub struct CurveCreated {
    pub trust: Pubkey,
    pub curve_id: [u8; 32],
    pub creator: Pubkey,
    pub curve_type: u8,
    pub start_price: u128,
    pub end_price: u128,
    pub max_supply: u64,
}

#[event]
pub struct CurveBuy {
    pub trust: Pubkey,
    pub curve_id: [u8; 32],
    pub buyer: Pubkey,
    pub token_amount: u64,
    pub cost: u64,
}

#[error_code]
pub enum UnifuturesError {
    #[msg("max_supply must be > 0")]
    ZeroMaxSupply,
    #[msg("reserve_ratio_ppm must be ≤ 1_000_000 (100%)")]
    InvalidReserveRatio,
    #[msg("curve_type must be 0 (linear) or 1 (exponential)")]
    InvalidCurveType,
    #[msg("math overflow in curve calculation")]
    MathOverflow,
    #[msg("amount must be > 0")]
    ZeroAmount,
    #[msg("buy would exceed curve's max_supply")]
    ExceedsMaxSupply,
    #[msg("cost exceeded max_cost (slippage protection)")]
    SlippageExceeded,
}
