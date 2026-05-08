//! Bonding curve math — port of `BondingCurveMath.library.sol`.
//!
//! Two curve shapes:
//! - Linear: `price(supply) = startPrice + (endPrice - startPrice) * (supply / maxSupply)`
//! - Exponential (quadratic): `price(supply) = startPrice + (endPrice - startPrice) * progress²`
//!
//! Purchase cost over a range (avg-price integration):
//! `cost(amount) = amount * (price(start) + price(start + amount)) / 2`
//!
//! All math in `u128` with `PRECISION = 1e18` to match the EVM port. No
//! floating point. `saturating_*` semantics on overflow paths.

pub const PRECISION: u128 = 1_000_000_000_000_000_000; // 1e18

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveType {
    Linear = 0,
    Exponential = 1,
}

impl CurveType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(CurveType::Linear),
            1 => Some(CurveType::Exponential),
            _ => None,
        }
    }
}

/// Price at a given supply level. Output is "quote per 1e18 tokens" — same
/// scale as `start_price` and `end_price`.
pub fn price_at(
    curve_type: CurveType,
    start_price: u128,
    end_price: u128,
    max_supply: u128,
    current_supply: u128,
) -> u128 {
    if max_supply == 0 {
        return start_price;
    }
    // progress in [0, PRECISION]
    let progress = current_supply
        .saturating_mul(PRECISION)
        .saturating_div(max_supply)
        .min(PRECISION);

    let scale = match curve_type {
        CurveType::Linear => progress,
        CurveType::Exponential => progress
            .saturating_mul(progress)
            .saturating_div(PRECISION),
    };

    if end_price >= start_price {
        let delta = end_price - start_price;
        start_price.saturating_add(delta.saturating_mul(scale).saturating_div(PRECISION))
    } else {
        let delta = start_price - end_price;
        start_price.saturating_sub(delta.saturating_mul(scale).saturating_div(PRECISION))
    }
}

/// Cost to buy `token_amount` starting at `current_supply`. Approximates the
/// integral via the trapezoidal rule (avg of start + end prices). Direct
/// port of the EVM `_calculatePurchaseCost`.
pub fn purchase_cost(
    curve_type: CurveType,
    start_price: u128,
    end_price: u128,
    max_supply: u128,
    current_supply: u128,
    token_amount: u128,
) -> Option<u128> {
    if token_amount == 0 {
        return Some(0);
    }
    let p_start = price_at(curve_type, start_price, end_price, max_supply, current_supply);
    let p_end = price_at(
        curve_type,
        start_price,
        end_price,
        max_supply,
        current_supply.checked_add(token_amount)?,
    );
    let avg_price = p_start.checked_add(p_end)? / 2;
    // cost = amount * avg_price / PRECISION (since avg_price is per 1e18 tokens)
    token_amount.checked_mul(avg_price)?.checked_div(PRECISION)
}

/// Return (in quote) for selling `token_amount` from `current_supply` —
/// applies an optional `reserve_ratio_ppm` (parts-per-million, 1_000_000 =
/// 100% — the EVM default of 90% is `900_000`).
pub fn sale_return(
    curve_type: CurveType,
    start_price: u128,
    end_price: u128,
    max_supply: u128,
    current_supply: u128,
    token_amount: u128,
    reserve_ratio_ppm: u32,
) -> Option<u128> {
    if token_amount == 0 || token_amount > current_supply {
        return None;
    }
    let new_supply = current_supply.checked_sub(token_amount)?;
    let p_end = price_at(curve_type, start_price, end_price, max_supply, new_supply);
    let p_start = price_at(curve_type, start_price, end_price, max_supply, current_supply);
    let avg_price = p_end.checked_add(p_start)? / 2;
    let gross = token_amount.checked_mul(avg_price)?.checked_div(PRECISION)?;
    Some(
        gross
            .checked_mul(reserve_ratio_ppm as u128)?
            .checked_div(1_000_000)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_price_at_extremes() {
        // start=1e18, end=2e18, max=1000
        assert_eq!(
            price_at(CurveType::Linear, PRECISION, 2 * PRECISION, 1000, 0),
            PRECISION
        );
        assert_eq!(
            price_at(CurveType::Linear, PRECISION, 2 * PRECISION, 1000, 1000),
            2 * PRECISION
        );
        // mid-supply = avg of start + end
        assert_eq!(
            price_at(CurveType::Linear, PRECISION, 2 * PRECISION, 1000, 500),
            PRECISION + PRECISION / 2
        );
    }

    #[test]
    fn exponential_grows_slower_at_start() {
        // At progress=0.5, exponential gives delta * 0.25 (quadratic), linear gives 0.5
        let lin = price_at(CurveType::Linear, 0, PRECISION, 1000, 500);
        let exp = price_at(CurveType::Exponential, 0, PRECISION, 1000, 500);
        assert_eq!(lin, PRECISION / 2);
        assert_eq!(exp, PRECISION / 4);
    }

    #[test]
    fn purchase_cost_zero_amount_is_zero() {
        let cost = purchase_cost(CurveType::Linear, PRECISION, 2 * PRECISION, 1000, 0, 0);
        assert_eq!(cost, Some(0));
    }

    #[test]
    fn purchase_cost_full_supply_linear() {
        // Linear from 1e18 to 2e18 over 1000 supply.
        // Buying all 1000 starting at supply=0:
        // p_start = 1e18, p_end = 2e18, avg = 1.5e18
        // cost = 1000 * 1.5e18 / 1e18 = 1500
        let cost =
            purchase_cost(CurveType::Linear, PRECISION, 2 * PRECISION, 1000, 0, 1000).unwrap();
        assert_eq!(cost, 1500);
    }

    #[test]
    fn sale_return_with_90_reserve() {
        // Sell 500 from supply=1000 on linear 1e18→2e18 over 1000 max.
        // p_start (at 1000) = 2e18, p_end (at 500) = 1.5e18, avg = 1.75e18
        // gross = 500 * 1.75e18 / 1e18 = 875
        // 90% reserve = 875 * 0.9 = 787 (integer div)
        let ret = sale_return(
            CurveType::Linear,
            PRECISION,
            2 * PRECISION,
            1000,
            1000,
            500,
            900_000,
        )
        .unwrap();
        assert_eq!(ret, 787);
    }
}
