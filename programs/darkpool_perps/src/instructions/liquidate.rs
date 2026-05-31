use crate::constants::*;
use crate::error::ErrorCode;
use crate::math;
use crate::state::oracle::load_oracle_price;
use crate::state::{Market, UserAccount};
use anchor_lang::prelude::*;

/// Permissionless liquidation of an under-margined account. The position is closed against
/// the vAMM, PnL is realized, and a liquidation fee is taken from the victim's collateral.
///
/// MVP NOTE: the fee is retained in the vault as an insurance surplus; a direct token reward
/// transfer to the liquidator is a Phase 4 follow-up.
#[derive(Accounts)]
pub struct Liquidate<'info> {
    pub liquidator: Signer<'info>,
    #[account(mut)]
    pub market: Account<'info, Market>,
    #[account(
        mut,
        seeds = [USER_SEED, market.key().as_ref(), user.authority.as_ref()],
        bump = user.bump,
        has_one = market @ ErrorCode::Unauthorized,
    )]
    pub user: Account<'info, UserAccount>,
    /// CHECK: verified against `market.oracle` in the handler.
    pub oracle: UncheckedAccount<'info>,
}

pub fn liquidate_handler(ctx: Context<Liquidate>) -> Result<()> {
    require!(ctx.accounts.user.has_position(), ErrorCode::NoPosition);

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

    // Settle funding, then confirm the account is actually liquidatable.
    let cf = ctx.accounts.market.cumulative_funding;
    ctx.accounts
        .user
        .apply_funding(cf)
        .ok_or(ErrorCode::MathOverflow)?;
    require!(
        ctx.accounts
            .user
            .is_liquidatable(px.price, ctx.accounts.market.maintenance_margin_bps as i128),
        ErrorCode::PositionNotLiquidatable
    );

    // Close the position against the vAMM.
    let base_signed = ctx.accounts.user.base_amount;
    let base_abs = base_signed.checked_abs().ok_or(ErrorCode::MathOverflow)?;
    let closing_long = ctx.accounts.user.is_long();
    let is_long_trade = !closing_long;

    let quote = ctx
        .accounts
        .market
        .apply_vamm_trade(base_abs, is_long_trade)
        .ok_or(ErrorCode::MathOverflow)?;
    let fill_price = math::exec_price(quote, base_abs).ok_or(ErrorCode::MathOverflow)?;

    let pnl = math::unrealized_pnl(base_signed, ctx.accounts.user.entry_price, fill_price)
        .ok_or(ErrorCode::MathOverflow)?;
    let notional = math::abs_notional(base_signed, fill_price).ok_or(ErrorCode::MathOverflow)?;
    let fee = math::checked_mul_div(
        notional,
        ctx.accounts.market.liquidation_fee_bps as i128,
        MARGIN_PRECISION,
    )
    .ok_or(ErrorCode::MathOverflow)?;

    {
        let u = &mut ctx.accounts.user;
        u.collateral = u.collateral.checked_add(pnl).ok_or(ErrorCode::MathOverflow)?;
        u.collateral = u.collateral.checked_sub(fee).ok_or(ErrorCode::MathOverflow)?;
        u.base_amount = 0;
        u.entry_price = 0;
    }

    let m = &mut ctx.accounts.market;
    if closing_long {
        m.total_long_base = m.total_long_base.checked_sub(base_abs).ok_or(ErrorCode::MathOverflow)?;
    } else {
        m.total_short_base = m
            .total_short_base
            .checked_sub(base_abs)
            .ok_or(ErrorCode::MathOverflow)?;
    }
    Ok(())
}
