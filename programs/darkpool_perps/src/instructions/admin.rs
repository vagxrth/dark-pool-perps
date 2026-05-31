use crate::constants::*;
use crate::error::ErrorCode;
use crate::state::{AdminOracle, Market, OracleSource};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
#[instruction(market_index: u16)]
pub struct InitMarket<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + Market::INIT_SPACE,
        seeds = [MARKET_SEED, &market_index.to_le_bytes()],
        bump
    )]
    pub market: Account<'info, Market>,
    pub collateral_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = authority,
        seeds = [VAULT_SEED, market.key().as_ref()],
        bump,
        token::mint = collateral_mint,
        token::authority = market,
    )]
    pub vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn init_market_handler(
    ctx: Context<InitMarket>,
    market_index: u16,
    base_reserve: i128,
    quote_reserve: i128,
    oracle: Pubkey,
    oracle_source: OracleSource,
    maintenance_margin_bps: u16,
    initial_margin_bps: u16,
    liquidation_fee_bps: u16,
) -> Result<()> {
    require!(
        base_reserve > 0 && quote_reserve > 0,
        ErrorCode::InvalidAmount
    );
    let m = &mut ctx.accounts.market;
    m.authority = ctx.accounts.authority.key();
    m.collateral_mint = ctx.accounts.collateral_mint.key();
    m.vault = ctx.accounts.vault.key();
    m.oracle = oracle;
    m.oracle_source = oracle_source;
    m.base_reserve = base_reserve;
    m.quote_reserve = quote_reserve;
    m.cumulative_funding = 0;
    m.last_funding_ts = Clock::get()?.unix_timestamp;
    m.total_long_base = 0;
    m.total_short_base = 0;
    m.total_collateral = 0;
    m.maintenance_margin_bps = maintenance_margin_bps;
    m.initial_margin_bps = initial_margin_bps;
    m.liquidation_fee_bps = liquidation_fee_bps;
    m.market_index = market_index;
    m.paused = false;
    m.bump = ctx.bumps.market;
    m.vault_bump = ctx.bumps.vault;
    Ok(())
}

#[derive(Accounts)]
pub struct InitAdminOracle<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + AdminOracle::INIT_SPACE,
        seeds = [ADMIN_ORACLE_SEED, authority.key().as_ref()],
        bump
    )]
    pub admin_oracle: Account<'info, AdminOracle>,
    pub system_program: Program<'info, System>,
}

pub fn init_admin_oracle_handler(
    ctx: Context<InitAdminOracle>,
    price: i128,
    conf: u64,
) -> Result<()> {
    require!(price > 0, ErrorCode::InvalidAmount);
    let o = &mut ctx.accounts.admin_oracle;
    o.authority = ctx.accounts.authority.key();
    o.price = price;
    o.conf = conf;
    o.last_update_ts = Clock::get()?.unix_timestamp;
    o.bump = ctx.bumps.admin_oracle;
    Ok(())
}

#[derive(Accounts)]
pub struct PushAdminPrice<'info> {
    pub authority: Signer<'info>,
    #[account(mut, has_one = authority @ ErrorCode::Unauthorized)]
    pub admin_oracle: Account<'info, AdminOracle>,
}

pub fn push_admin_price_handler(
    ctx: Context<PushAdminPrice>,
    price: i128,
    conf: u64,
) -> Result<()> {
    require!(price > 0, ErrorCode::InvalidAmount);
    let o = &mut ctx.accounts.admin_oracle;
    o.price = price;
    o.conf = conf;
    o.last_update_ts = Clock::get()?.unix_timestamp;
    Ok(())
}
