use crate::constants::*;
use crate::error::ErrorCode;
use crate::math;
use crate::state::oracle::load_oracle_price;
use crate::state::{Market, UserAccount};
use anchor_lang::prelude::*;

/// Shared account context for opening and closing a position.
#[derive(Accounts)]
pub struct Trade<'info> {
    pub authority: Signer<'info>,
    #[account(mut)]
    pub market: Account<'info, Market>,
    #[account(
        mut,
        seeds = [USER_SEED, market.key().as_ref(), authority.key().as_ref()],
        bump = user.bump,
        has_one = authority @ ErrorCode::Unauthorized,
        has_one = market @ ErrorCode::Unauthorized,
    )]
    pub user: Account<'info, UserAccount>,
    /// CHECK: verified against `market.oracle` in the handler.
    pub oracle: UncheckedAccount<'info>,
}

/// Open or increase a position against the vAMM. `base_amount` is the (positive) size in
/// BASE_PRECISION; `is_long` the direction; `limit_price` the worst acceptable fill
/// (max for a long, min for a short).
pub fn open_position_handler(
    ctx: Context<Trade>,
    base_amount: u64,
    is_long: bool,
    limit_price: i128,
) -> Result<()> {
    require!(!ctx.accounts.market.paused, ErrorCode::MarketPaused);
    require!(base_amount > 0, ErrorCode::InvalidAmount);

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

    // Settle funding before changing the position.
    let cf = ctx.accounts.market.cumulative_funding;
    ctx.accounts
        .user
        .apply_funding(cf)
        .ok_or(ErrorCode::MathOverflow)?;

    let base = base_amount as i128;

    // MVP: same-direction only; flipping requires closing first.
    if ctx.accounts.user.has_position() {
        require!(
            ctx.accounts.user.is_long() == is_long,
            ErrorCode::InvalidAmount
        );
    }

    // Execute against the vAMM and derive the fill price.
    let quote = ctx
        .accounts
        .market
        .apply_vamm_trade(base, is_long)
        .ok_or(ErrorCode::MathOverflow)?;
    let fill_price = math::exec_price(quote, base).ok_or(ErrorCode::MathOverflow)?;
    if is_long {
        require!(fill_price <= limit_price, ErrorCode::SlippageExceeded);
    } else {
        require!(fill_price >= limit_price, ErrorCode::SlippageExceeded);
    }

    // Apply the fill to the user's position (weighted-average entry on increase).
    let signed_base = if is_long { base } else { -base };
    {
        let u = &mut ctx.accounts.user;
        if u.has_position() {
            u.entry_price =
                math::weighted_entry(u.base_amount, u.entry_price, signed_base, fill_price)
                    .ok_or(ErrorCode::MathOverflow)?;
            u.base_amount = u
                .base_amount
                .checked_add(signed_base)
                .ok_or(ErrorCode::MathOverflow)?;
        } else {
            u.base_amount = signed_base;
            u.entry_price = fill_price;
        }
    }

    // Initial-margin check at the oracle (mark-to-market) price.
    let u = &ctx.accounts.user;
    let av = u.account_value(px.price).ok_or(ErrorCode::MathOverflow)?;
    let nv = u
        .position_notional_abs(px.price)
        .ok_or(ErrorCode::MathOverflow)?;
    require!(
        math::meets_initial_margin(av, nv, ctx.accounts.market.initial_margin_bps as i128)
            .ok_or(ErrorCode::MathOverflow)?,
        ErrorCode::InsufficientMargin
    );

    // Track open interest.
    let m = &mut ctx.accounts.market;
    if is_long {
        m.total_long_base = m.total_long_base.checked_add(base).ok_or(ErrorCode::MathOverflow)?;
    } else {
        m.total_short_base = m
            .total_short_base
            .checked_add(base)
            .ok_or(ErrorCode::MathOverflow)?;
    }
    Ok(())
}

/// Close the entire position against the vAMM, realizing PnL into collateral. `limit_price`
/// is the worst acceptable fill (min when closing a long / selling, max when closing a short).
pub fn close_position_handler(ctx: Context<Trade>, limit_price: i128) -> Result<()> {
    require!(ctx.accounts.user.has_position(), ErrorCode::NoPosition);

    require_keys_eq!(
        ctx.accounts.oracle.key(),
        ctx.accounts.market.oracle,
        ErrorCode::InvalidOracle
    );
    let now = Clock::get()?.unix_timestamp;
    let _px = load_oracle_price(
        &ctx.accounts.oracle.to_account_info(),
        ctx.accounts.market.oracle_source,
        now,
        ctx.program_id,
    )?;

    let cf = ctx.accounts.market.cumulative_funding;
    ctx.accounts
        .user
        .apply_funding(cf)
        .ok_or(ErrorCode::MathOverflow)?;

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
    if closing_long {
        require!(fill_price >= limit_price, ErrorCode::SlippageExceeded);
    } else {
        require!(fill_price <= limit_price, ErrorCode::SlippageExceeded);
    }

    let pnl = math::unrealized_pnl(base_signed, ctx.accounts.user.entry_price, fill_price)
        .ok_or(ErrorCode::MathOverflow)?;

    {
        let u = &mut ctx.accounts.user;
        u.collateral = u.collateral.checked_add(pnl).ok_or(ErrorCode::MathOverflow)?;
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
