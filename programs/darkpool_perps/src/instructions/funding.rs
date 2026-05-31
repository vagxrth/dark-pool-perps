use crate::constants::*;
use crate::error::ErrorCode;
use crate::math;
use crate::state::oracle::load_oracle_price;
use crate::state::Market;
use anchor_lang::prelude::*;

/// Permissionless funding crank: advances the market's cumulative funding index by the
/// mark-vs-oracle premium accrued since the last update.
#[derive(Accounts)]
pub struct SettleFunding<'info> {
    #[account(mut)]
    pub market: Account<'info, Market>,
    /// CHECK: verified against `market.oracle` in the handler.
    pub oracle: UncheckedAccount<'info>,
}

pub fn settle_funding_handler(ctx: Context<SettleFunding>) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.oracle.key(),
        ctx.accounts.market.oracle,
        ErrorCode::InvalidOracle
    );
    let now = Clock::get()?.unix_timestamp;
    let px = load_oracle_price(
        &ctx.accounts.oracle.to_account_info(),
        ctx.accounts.market.oracle_source,
        now,
        ctx.program_id,
    )?;

    let m = &mut ctx.accounts.market;
    let elapsed = now.checked_sub(m.last_funding_ts).ok_or(ErrorCode::MathOverflow)?;
    if elapsed <= 0 {
        return Ok(());
    }

    let mark = m.mark_price().ok_or(ErrorCode::MathOverflow)?;
    // premium (quote-per-base) longs pay over one funding interval; pro-rate by elapsed time.
    let premium = mark.checked_sub(px.price).ok_or(ErrorCode::MathOverflow)?;
    let delta = math::checked_mul_div(premium, elapsed as i128, FUNDING_INTERVAL_SECS)
        .ok_or(ErrorCode::MathOverflow)?;

    m.cumulative_funding = m
        .cumulative_funding
        .checked_add(delta)
        .ok_or(ErrorCode::MathOverflow)?;
    m.last_funding_ts = now;
    Ok(())
}
