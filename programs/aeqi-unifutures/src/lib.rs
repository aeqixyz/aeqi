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
}
