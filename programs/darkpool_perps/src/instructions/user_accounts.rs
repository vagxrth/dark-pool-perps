use crate::constants::*;
use crate::state::{Market, UserAccount};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct InitUser<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub market: Account<'info, Market>,
    #[account(
        init,
        payer = authority,
        space = 8 + UserAccount::INIT_SPACE,
        seeds = [USER_SEED, market.key().as_ref(), authority.key().as_ref()],
        bump
    )]
    pub user: Account<'info, UserAccount>,
    pub system_program: Program<'info, System>,
}

pub fn init_user_handler(ctx: Context<InitUser>) -> Result<()> {
    let u = &mut ctx.accounts.user;
    u.authority = ctx.accounts.authority.key();
    u.market = ctx.accounts.market.key();
    u.collateral = 0;
    u.base_amount = 0;
    u.entry_price = 0;
    u.last_cumulative_funding = ctx.accounts.market.cumulative_funding;
    u.bump = ctx.bumps.user;
    Ok(())
}
