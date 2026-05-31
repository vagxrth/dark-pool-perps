use crate::math;
use crate::state::oracle::OracleSource;
use anchor_lang::prelude::*;

/// A single perpetual-futures market (e.g. SOL-PERP). Phase 1 ships one market with a
/// constant-product virtual AMM; all fields here are PUBLIC (confidentiality is layered on
/// in Phase 2+). Amounts use the fixed-point scales defined in `constants.rs`.
#[account]
#[derive(InitSpace)]
pub struct Market {
    /// Admin: can update params, push admin-oracle prices, and pause the market.
    pub authority: Pubkey,
    /// Collateral mint (USDC).
    pub collateral_mint: Pubkey,
    /// Pooled collateral vault (token account PDA owned by the market).
    pub vault: Pubkey,
    /// Oracle account (Pyth `PriceUpdateV2` or [`super::oracle::AdminOracle`]).
    pub oracle: Pubkey,
    /// How to interpret `oracle`.
    pub oracle_source: OracleSource,

    // ---- vAMM virtual reserves (constant product: base_reserve * quote_reserve = k) ----
    /// Virtual base reserve, `BASE_PRECISION`.
    pub base_reserve: i128,
    /// Virtual quote reserve, `QUOTE_PRECISION`.
    pub quote_reserve: i128,

    // ---- funding ----
    /// Cumulative funding index (quote owed per 1.0 base for a long, scaled by
    /// `FUNDING_PRECISION`); a short owes the negation.
    pub cumulative_funding: i128,
    /// Unix timestamp of the last funding update.
    pub last_funding_ts: i64,

    // ---- open interest / accounting ----
    /// Sum of all long base size (`BASE_PRECISION`).
    pub total_long_base: i128,
    /// Sum of all |short| base size (`BASE_PRECISION`).
    pub total_short_base: i128,
    /// Net deposited collateral in token units (invariant check vs the vault balance).
    pub total_collateral: u64,

    // ---- risk parameters (basis points) ----
    pub maintenance_margin_bps: u16,
    pub initial_margin_bps: u16,
    pub liquidation_fee_bps: u16,

    pub market_index: u16,
    pub paused: bool,
    pub bump: u8,
    pub vault_bump: u8,
}

impl Market {
    /// vAMM mark price (`PRICE_PRECISION`) implied by the current virtual reserves.
    pub fn mark_price(&self) -> Option<i128> {
        math::vamm_mark_price(self.base_reserve, self.quote_reserve)
    }

    /// Net base position across the book (longs positive, shorts negative).
    pub fn net_base(&self) -> i128 {
        self.total_long_base
            .saturating_sub(self.total_short_base)
    }

    /// Apply a vAMM trade to the virtual reserves. `base_delta` is positive; `is_long`
    /// drains base (buy), otherwise grows it (sell). Returns the quote amount moved.
    pub fn apply_vamm_trade(&mut self, base_delta: i128, is_long: bool) -> Option<i128> {
        let quote = math::vamm_quote_for_base(
            self.base_reserve,
            self.quote_reserve,
            base_delta,
            is_long,
        )?;
        if is_long {
            self.base_reserve = self.base_reserve.checked_sub(base_delta)?;
            self.quote_reserve = self.quote_reserve.checked_add(quote)?;
        } else {
            self.base_reserve = self.base_reserve.checked_add(base_delta)?;
            self.quote_reserve = self.quote_reserve.checked_sub(quote)?;
        }
        Some(quote)
    }
}
