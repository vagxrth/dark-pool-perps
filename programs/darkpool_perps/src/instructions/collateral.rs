use crate::constants::*;
use crate::error::ErrorCode;
use crate::math;
use crate::state::oracle::load_oracle_price;
use crate::state::{Market, UserAccount};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct Deposit<'info> {
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
    #[account(mut, address = market.vault @ ErrorCode::Unauthorized)]
    pub vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn deposit_handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidAmount);
    require_keys_eq!(
        ctx.accounts.user_token_account.mint,
        ctx.accounts.market.collateral_mint,
        ErrorCode::Unauthorized
    );
    require_keys_eq!(
        ctx.accounts.user_token_account.owner,
        ctx.accounts.authority.key(),
        ErrorCode::Unauthorized
    );

    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;

    // USDC has 6 decimals == QUOTE_PRECISION, so token units map 1:1 to quote units.
    ctx.accounts.user.collateral = ctx
        .accounts
        .user
        .collateral
        .checked_add(amount as i128)
        .ok_or(ErrorCode::MathOverflow)?;
    ctx.accounts.market.total_collateral = ctx
        .accounts
        .market
        .total_collateral
        .checked_add(amount)
        .ok_or(ErrorCode::MathOverflow)?;
    Ok(())
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
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
    #[account(mut, address = market.vault @ ErrorCode::Unauthorized)]
    pub vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    /// CHECK: verified against `market.oracle` in the handler before use.
    pub oracle: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

pub fn withdraw_handler(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidAmount);
    require_keys_eq!(
        ctx.accounts.user_token_account.mint,
        ctx.accounts.market.collateral_mint,
        ErrorCode::Unauthorized
    );
    require_keys_eq!(
        ctx.accounts.user_token_account.owner,
        ctx.accounts.authority.key(),
        ErrorCode::Unauthorized
    );

    let amount_i = amount as i128;
    require!(
        ctx.accounts.user.collateral >= amount_i,
        ErrorCode::InsufficientCollateral
    );

    // With an open position, the withdrawal must keep the account above initial margin.
    if ctx.accounts.user.has_position() {
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
        let u = &ctx.accounts.user;
        let post_collateral = u
            .collateral
            .checked_sub(amount_i)
            .ok_or(ErrorCode::MathOverflow)?;
        let pnl = u.unrealized_pnl(px.price).ok_or(ErrorCode::MathOverflow)?;
        let post_value = post_collateral
            .checked_add(pnl)
            .ok_or(ErrorCode::MathOverflow)?;
        let notional = u
            .position_notional_abs(px.price)
            .ok_or(ErrorCode::MathOverflow)?;
        let ok = math::meets_initial_margin(
            post_value,
            notional,
            ctx.accounts.market.initial_margin_bps as i128,
        )
        .ok_or(ErrorCode::MathOverflow)?;
        require!(ok, ErrorCode::InsufficientMargin);
    }

    // Transfer vault -> user, signed by the market PDA (the vault authority).
    let market_index_bytes = ctx.accounts.market.market_index.to_le_bytes();
    let market_bump = ctx.accounts.market.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[MARKET_SEED, market_index_bytes.as_ref(), &[market_bump]]];
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.market.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;

    ctx.accounts.user.collateral = ctx
        .accounts
        .user
        .collateral
        .checked_sub(amount_i)
        .ok_or(ErrorCode::MathOverflow)?;
    ctx.accounts.market.total_collateral = ctx
        .accounts
        .market
        .total_collateral
        .checked_sub(amount)
        .ok_or(ErrorCode::MathOverflow)?;
    Ok(())
}
