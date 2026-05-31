use crate::math;
use anchor_lang::prelude::*;

/// A trader's account in a single market. Phase 1 is PUBLIC (size/collateral in the clear);
/// Phase 2 replaces `base_amount`/`collateral`/`entry_price` with Arcium ciphertext.
#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    /// Owner.
    pub authority: Pubkey,
    /// Market this account trades.
    pub market: Pubkey,
    /// Collateral in `QUOTE_PRECISION`; reduced by realized losses, funding, and fees.
    pub collateral: i128,
    /// Position size, signed (+long / -short), `BASE_PRECISION`. 0 == flat.
    pub base_amount: i128,
    /// Volume-weighted entry price, `PRICE_PRECISION`. Meaningless when `base_amount == 0`.
    pub entry_price: i128,
    /// Cumulative-funding index snapshot at last settlement.
    pub last_cumulative_funding: i128,
    pub bump: u8,
}

impl UserAccount {
    pub fn has_position(&self) -> bool {
        self.base_amount != 0
    }

    pub fn is_long(&self) -> bool {
        self.base_amount > 0
    }

    /// Unrealized PnL (quote) at `price` (`PRICE_PRECISION`).
    pub fn unrealized_pnl(&self, price: i128) -> Option<i128> {
        math::unrealized_pnl(self.base_amount, self.entry_price, price)
    }

    /// Account value (quote) = collateral + unrealized PnL.
    pub fn account_value(&self, price: i128) -> Option<i128> {
        math::account_value(self.collateral, self.base_amount, self.entry_price, price)
    }

    /// Absolute position notional (quote) at `price`.
    pub fn position_notional_abs(&self, price: i128) -> Option<i128> {
        math::abs_notional(self.base_amount, price)
    }

    /// True when this account is below the maintenance-margin threshold at `price`.
    pub fn is_liquidatable(&self, price: i128, maintenance_bps: i128) -> bool {
        if !self.has_position() {
            return false;
        }
        match (self.account_value(price), self.position_notional_abs(price)) {
            (Some(av), Some(nv)) => math::is_liquidatable(av, nv, maintenance_bps),
            _ => false,
        }
    }

    /// Settle accrued funding against collateral and snapshot the market's cumulative index.
    /// Returns `None` on overflow. Longs pay when the index rises; shorts receive.
    pub fn apply_funding(&mut self, market_cumulative_funding: i128) -> Option<()> {
        if self.base_amount != 0 {
            let payment = math::funding_payment(
                self.base_amount,
                market_cumulative_funding,
                self.last_cumulative_funding,
            )?;
            self.collateral = self.collateral.checked_sub(payment)?;
        }
        self.last_cumulative_funding = market_cumulative_funding;
        Some(())
    }
}
