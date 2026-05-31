//! Pure fixed-point perps math (no Anchor/Solana types) so it is trivially unit-testable
//! with `cargo test --lib` and reusable by keepers / the Arcis confidential side.
//!
//! Conventions (see `constants.rs`):
//! - base_amount: i128 in `BASE_PRECISION` (1e9), signed (+long / -short)
//! - price:       i128 in `PRICE_PRECISION` (1e6)
//! - quote/PnL:   i128 in `QUOTE_PRECISION` (1e6)
//!
//! Every function uses checked arithmetic and returns `None` on overflow / invalid
//! input. On-chain callers map `None` to a program error; tests assert on the `Option`.

use crate::constants::{BASE_PRECISION, MARGIN_PRECISION, PRICE_PRECISION, QUOTE_PRECISION};

/// `a * b / denom` with overflow checks (i128 intermediate). `None` if `denom == 0`.
#[inline]
pub fn checked_mul_div(a: i128, b: i128, denom: i128) -> Option<i128> {
    if denom == 0 {
        return None;
    }
    a.checked_mul(b)?.checked_div(denom)
}

/// Notional (quote, signed) of a base position at `price`.
/// `notional = base_amount * price / BASE_PRECISION`.
#[inline]
pub fn notional(base_amount: i128, price: i128) -> Option<i128> {
    checked_mul_div(base_amount, price, BASE_PRECISION)
}

/// Absolute notional (quote) — used for margin/leverage denominators.
#[inline]
pub fn abs_notional(base_amount: i128, price: i128) -> Option<i128> {
    notional(base_amount.checked_abs()?, price)
}

/// Unrealized PnL (quote) for a position.
/// `pnl = base_amount * (current_price - entry_price) / BASE_PRECISION`.
#[inline]
pub fn unrealized_pnl(base_amount: i128, entry_price: i128, current_price: i128) -> Option<i128> {
    let diff = current_price.checked_sub(entry_price)?;
    checked_mul_div(base_amount, diff, BASE_PRECISION)
}

/// Account value (quote) = collateral + unrealized PnL.
#[inline]
pub fn account_value(
    collateral: i128,
    base_amount: i128,
    entry_price: i128,
    current_price: i128,
) -> Option<i128> {
    unrealized_pnl(base_amount, entry_price, current_price)?.checked_add(collateral)
}

/// Margin ratio in bps = `account_value / |position_notional| * MARGIN_PRECISION`.
/// `None` when there is no open position (notional <= 0).
#[inline]
pub fn margin_ratio_bps(account_value: i128, position_notional_abs: i128) -> Option<i128> {
    if position_notional_abs <= 0 {
        return None;
    }
    checked_mul_div(account_value, MARGIN_PRECISION, position_notional_abs)
}

/// True when the account's margin ratio has fallen below `maintenance_bps`.
/// No position (or non-positive notional) is never liquidatable.
#[inline]
pub fn is_liquidatable(
    account_value: i128,
    position_notional_abs: i128,
    maintenance_bps: i128,
) -> bool {
    match margin_ratio_bps(account_value, position_notional_abs) {
        Some(r) => r < maintenance_bps,
        None => false,
    }
}

/// Does `account_value` meet the initial-margin requirement for a position of the given
/// notional? Used to gate opening/increasing positions.
#[inline]
pub fn meets_initial_margin(
    account_value: i128,
    position_notional_abs: i128,
    initial_bps: i128,
) -> Option<bool> {
    let required = checked_mul_div(position_notional_abs, initial_bps, MARGIN_PRECISION)?;
    Some(account_value >= required)
}

/// Constant-product vAMM quote for trading `base_delta` (BASE_PRECISION, positive).
/// `is_long` (buy base, drains base reserve) -> returns quote **cost** (positive paid in).
/// short (sell base, grows base reserve) -> returns quote **proceeds** (positive received).
/// `None` on overflow or if a long would drain the entire base reserve.
pub fn vamm_quote_for_base(
    base_reserve: i128,
    quote_reserve: i128,
    base_delta: i128,
    is_long: bool,
) -> Option<i128> {
    if base_delta <= 0 || base_reserve <= 0 || quote_reserve <= 0 {
        return None;
    }
    let k = base_reserve.checked_mul(quote_reserve)?;
    if is_long {
        let new_base = base_reserve.checked_sub(base_delta)?;
        if new_base <= 0 {
            return None; // cannot drain the reserve
        }
        let new_quote = k.checked_div(new_base)?;
        new_quote.checked_sub(quote_reserve) // cost
    } else {
        let new_base = base_reserve.checked_add(base_delta)?;
        let new_quote = k.checked_div(new_base)?;
        quote_reserve.checked_sub(new_quote) // proceeds
    }
}

/// vAMM mark price (PRICE_PRECISION) implied by current virtual reserves.
/// `price = quote_reserve * BASE_PRECISION * PRICE_PRECISION / (base_reserve * QUOTE_PRECISION)`.
pub fn vamm_mark_price(base_reserve: i128, quote_reserve: i128) -> Option<i128> {
    if base_reserve <= 0 {
        return None;
    }
    let num = quote_reserve
        .checked_mul(BASE_PRECISION)?
        .checked_mul(PRICE_PRECISION)?;
    let den = base_reserve.checked_mul(QUOTE_PRECISION)?;
    num.checked_div(den)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;

    // Helpers to write human numbers in fixed-point.
    fn usd(n: i128) -> i128 {
        n * PRICE_PRECISION
    } // price
    fn sol(n: i128) -> i128 {
        n * BASE_PRECISION
    } // base size
    fn quote(n: i128) -> i128 {
        n * QUOTE_PRECISION
    } // quote/collateral

    #[test]
    fn notional_long_and_short() {
        // 1.5 SOL @ $150 = $225
        let base = 3 * BASE_PRECISION / 2; // 1.5
        assert_eq!(notional(base, usd(150)), Some(quote(225)));
        assert_eq!(notional(-base, usd(150)), Some(-quote(225)));
        assert_eq!(abs_notional(-base, usd(150)), Some(quote(225)));
    }

    #[test]
    fn pnl_directions() {
        let base = 3 * BASE_PRECISION / 2; // 1.5 SOL
                                           // long: price up $10 -> +$15
        assert_eq!(unrealized_pnl(base, usd(150), usd(160)), Some(quote(15)));
        // long: price down $10 -> -$15
        assert_eq!(unrealized_pnl(base, usd(150), usd(140)), Some(-quote(15)));
        // short: price up $10 -> -$15
        assert_eq!(unrealized_pnl(-base, usd(150), usd(160)), Some(-quote(15)));
    }

    #[test]
    fn account_value_combines_collateral_and_pnl() {
        let base = sol(1); // 1 SOL long
                           // $30 collateral, price up $10 -> 30 + 10 = $40
        assert_eq!(
            account_value(quote(30), base, usd(150), usd(160)),
            Some(quote(40))
        );
    }

    #[test]
    fn margin_ratio_and_liquidation() {
        // 1 SOL long @ $150 notional = $150. collateral $30, no pnl -> ratio = 30/150 = 20% = 2000 bps
        let nv = abs_notional(sol(1), usd(150)).unwrap();
        let av = account_value(quote(30), sol(1), usd(150), usd(150)).unwrap();
        assert_eq!(margin_ratio_bps(av, nv), Some(2000));
        assert!(!is_liquidatable(av, nv, DEFAULT_MAINTENANCE_MARGIN_BPS));

        // price drops to $128 -> pnl = -$22 -> av = $8 -> ratio = 8/128 = 6.25% (>5%, not liq)
        let av2 = account_value(quote(30), sol(1), usd(150), usd(128)).unwrap();
        let nv2 = abs_notional(sol(1), usd(128)).unwrap();
        assert!(!is_liquidatable(av2, nv2, DEFAULT_MAINTENANCE_MARGIN_BPS));

        // price drops to $124 -> pnl = -$26 -> av = $4 -> ratio = 4/124 = 3.2% (<5%, liquidatable)
        let av3 = account_value(quote(30), sol(1), usd(150), usd(124)).unwrap();
        let nv3 = abs_notional(sol(1), usd(124)).unwrap();
        assert!(is_liquidatable(av3, nv3, DEFAULT_MAINTENANCE_MARGIN_BPS));
    }

    #[test]
    fn initial_margin_gate() {
        // $150 notional needs 10% = $15 initial margin.
        let nv = abs_notional(sol(1), usd(150)).unwrap();
        assert_eq!(
            meets_initial_margin(quote(15), nv, DEFAULT_INITIAL_MARGIN_BPS),
            Some(true)
        );
        assert_eq!(
            meets_initial_margin(quote(14), nv, DEFAULT_INITIAL_MARGIN_BPS),
            Some(false)
        );
    }

    #[test]
    fn no_position_is_not_liquidatable() {
        assert!(!is_liquidatable(quote(0), 0, DEFAULT_MAINTENANCE_MARGIN_BPS));
        assert_eq!(margin_ratio_bps(quote(10), 0), None);
    }

    #[test]
    fn vamm_mark_price_matches_reserves() {
        // base_reserve = 10_000 SOL, quote_reserve = $1.5M -> mark = $150
        let base_reserve = sol(10_000);
        let quote_reserve = quote(1_500_000);
        assert_eq!(vamm_mark_price(base_reserve, quote_reserve), Some(usd(150)));
    }

    #[test]
    fn vamm_long_costs_more_than_spot_due_to_slippage() {
        let base_reserve = sol(10_000);
        let quote_reserve = quote(1_500_000);
        let delta = sol(10); // buy 10 SOL
        let cost = vamm_quote_for_base(base_reserve, quote_reserve, delta, true).unwrap();
        // spot value = 10 * 150 = $1500; long cost should exceed it (price impact)
        assert!(cost > quote(1500), "cost {} should exceed 1500 spot", cost);
        // and be close-ish (within ~1%) for a small trade vs deep reserves
        assert!(cost < quote(1520));
    }

    #[test]
    fn vamm_short_receives_less_than_spot() {
        let base_reserve = sol(10_000);
        let quote_reserve = quote(1_500_000);
        let delta = sol(10);
        let proceeds = vamm_quote_for_base(base_reserve, quote_reserve, delta, false).unwrap();
        assert!(proceeds < quote(1500), "proceeds {} should be < 1500", proceeds);
        assert!(proceeds > quote(1480));
    }

    #[test]
    fn vamm_rejects_draining_reserve() {
        let base_reserve = sol(100);
        let quote_reserve = quote(15_000);
        // try to buy the entire base reserve -> None
        assert_eq!(
            vamm_quote_for_base(base_reserve, quote_reserve, sol(100), true),
            None
        );
    }

    #[test]
    fn checked_mul_div_guards_zero_denominator() {
        assert_eq!(checked_mul_div(10, 10, 0), None);
        assert_eq!(checked_mul_div(10, 10, 5), Some(20));
    }
}
