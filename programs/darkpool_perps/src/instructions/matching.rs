//! Confidential dark-pool matching (Phase 3b): an encrypted order book (`Enc<Mxe,[Order;8]>`)
//! filled by `submit_order`, cleared by `crank_match` (runs the `match_batch` circuit) with the
//! net residual routed to the public vAMM. Same Arcium 0.10.4 triplet pattern as confidential.rs.

use crate::constants::*;
use crate::error::ErrorCode;
use crate::state::{Market, OrderPool};
use crate::{ArciumSignerAccount, ID, ID_CONST};
use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
use arcium_client::idl::arcium::types::CallbackAccount;

#[event]
pub struct BatchClearedEvent {
    pub market: Pubkey,
    /// Peer-to-peer volume matched at the reference price (BASE_PRECISION).
    pub matched: i128,
    /// Signed residual routed to the vAMM (+ = excess buys the vAMM sold into).
    pub net_to_vamm: i128,
}

// =========================================================================================
// init_order_pool : create an empty Enc<Mxe, [Order; 8]> book
// =========================================================================================

pub fn init_order_pool_comp_def_handler(ctx: Context<InitOrderPoolCompDef>) -> Result<()> {
    init_computation_def(ctx.accounts, None)?;
    Ok(())
}

pub fn init_order_pool_handler(ctx: Context<InitOrderPool>, computation_offset: u64) -> Result<()> {
    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
    {
        let pool = &mut ctx.accounts.order_pool;
        pool.bump = ctx.bumps.order_pool;
        pool.market = ctx.accounts.market.key();
        pool.next_slot = 0;
        pool.initialized = false;
    }
    let pool_key = ctx.accounts.order_pool.key();
    let args = ArgBuilder::new().build();
    queue_computation(
        ctx.accounts,
        computation_offset,
        args,
        vec![InitOrderPoolCallback::callback_ix(
            computation_offset,
            &ctx.accounts.mxe_account,
            &[CallbackAccount {
                pubkey: pool_key,
                is_writable: true,
            }],
        )?],
        1,
        0,
    )?;
    Ok(())
}

pub fn init_order_pool_callback_handler(
    ctx: Context<InitOrderPoolCallback>,
    output: SignedComputationOutputs<InitOrderPoolOutput>,
) -> Result<()> {
    let o = match output.verify_output(
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
    ) {
        Ok(InitOrderPoolOutput { field_0 }) => field_0,
        Err(_) => return Err(ErrorCode::AbortedComputation.into()),
    };
    let pool = &mut ctx.accounts.order_pool;
    pool.enc_orders = o.ciphertexts;
    pool.nonce = o.nonce;
    pool.next_slot = 0;
    pool.initialized = true;
    Ok(())
}

// =========================================================================================
// submit_order : insert a client Enc<Shared, Order> into the next slot of the encrypted book
// =========================================================================================

pub fn submit_order_comp_def_handler(ctx: Context<SubmitOrderCompDef>) -> Result<()> {
    init_computation_def(ctx.accounts, None)?;
    Ok(())
}

pub fn submit_order_handler(
    ctx: Context<SubmitOrder>,
    computation_offset: u64,
    ct_is_buy: [u8; 32],
    ct_price: [u8; 32],
    ct_size: [u8; 32],
    pub_key: [u8; 32],
    order_nonce: u128,
) -> Result<()> {
    require!(ctx.accounts.order_pool.initialized, ErrorCode::NoPosition);
    let slot = ctx.accounts.order_pool.next_slot;
    require!((slot as usize) < 8, ErrorCode::InvalidAmount);

    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
    let pool_nonce = ctx.accounts.order_pool.nonce;
    let pool_key = ctx.accounts.order_pool.key();

    // submit_order(pool: Enc<Mxe,[Order;8]>, order: Enc<Shared,Order>, slot: u64)
    let args = ArgBuilder::new()
        .plaintext_u128(pool_nonce)
        .account(pool_key, 8 + 1, 32 * 24)
        .x25519_pubkey(pub_key)
        .plaintext_u128(order_nonce)
        .encrypted_bool(ct_is_buy)
        .encrypted_i128(ct_price)
        .encrypted_i128(ct_size)
        .plaintext_u64(slot as u64)
        .build();

    queue_computation(
        ctx.accounts,
        computation_offset,
        args,
        vec![SubmitOrderCallback::callback_ix(
            computation_offset,
            &ctx.accounts.mxe_account,
            &[CallbackAccount {
                pubkey: pool_key,
                is_writable: true,
            }],
        )?],
        1,
        0,
    )?;
    Ok(())
}

pub fn submit_order_callback_handler(
    ctx: Context<SubmitOrderCallback>,
    output: SignedComputationOutputs<SubmitOrderOutput>,
) -> Result<()> {
    let o = match output.verify_output(
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
    ) {
        Ok(SubmitOrderOutput { field_0 }) => field_0,
        Err(_) => return Err(ErrorCode::AbortedComputation.into()),
    };
    let pool = &mut ctx.accounts.order_pool;
    pool.enc_orders = o.ciphertexts;
    pool.nonce = o.nonce;
    pool.next_slot = pool.next_slot.saturating_add(1);
    Ok(())
}

// =========================================================================================
// crank_match : run match_batch over the encrypted book; route the net residual to the vAMM
// =========================================================================================

pub fn match_batch_comp_def_handler(ctx: Context<MatchBatchCompDef>) -> Result<()> {
    init_computation_def(ctx.accounts, None)?;
    Ok(())
}

pub fn crank_match_handler(ctx: Context<MatchBatch>, computation_offset: u64) -> Result<()> {
    require!(ctx.accounts.order_pool.initialized, ErrorCode::NoPosition);
    // Clear at the public vAMM mark price.
    let mark = ctx
        .accounts
        .market
        .mark_price()
        .ok_or(ErrorCode::MathOverflow)?;
    let ref_price = i64::try_from(mark).map_err(|_| ErrorCode::MathOverflow)?;

    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
    let pool_nonce = ctx.accounts.order_pool.nonce;
    let pool_key = ctx.accounts.order_pool.key();
    let market_key = ctx.accounts.market.key();

    // match_batch(book: Enc<Mxe,[Order;8]>, ref_price: i64)
    let args = ArgBuilder::new()
        .plaintext_u128(pool_nonce)
        .account(pool_key, 8 + 1, 32 * 24)
        .plaintext_i64(ref_price)
        .build();

    queue_computation(
        ctx.accounts,
        computation_offset,
        args,
        vec![MatchBatchCallback::callback_ix(
            computation_offset,
            &ctx.accounts.mxe_account,
            &[CallbackAccount {
                pubkey: market_key,
                is_writable: true,
            }],
        )?],
        1,
        0,
    )?;
    Ok(())
}

pub fn crank_match_callback_handler(
    ctx: Context<MatchBatchCallback>,
    output: SignedComputationOutputs<MatchBatchOutput>,
) -> Result<()> {
    // match_batch returns the tuple (matched, net): field_0.field_0 / field_0.field_1.
    let (matched, net) = match output.verify_output(
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
    ) {
        Ok(MatchBatchOutput { field_0 }) => (field_0.field_0, field_0.field_1),
        Err(_) => return Err(ErrorCode::AbortedComputation.into()),
    };

    // Route the net residual to the public vAMM (it takes the opposite of the imbalance).
    if net != 0 {
        let base_delta = net.checked_abs().ok_or(ErrorCode::MathOverflow)?;
        let is_long = net > 0; // excess buys -> vAMM sells base into the pool
        ctx.accounts
            .market
            .apply_vamm_trade(base_delta, is_long)
            .ok_or(ErrorCode::MathOverflow)?;
    }

    emit!(BatchClearedEvent {
        market: ctx.accounts.market.key(),
        matched,
        net_to_vamm: net,
    });
    Ok(())
}

// =========================================================================================
// Account contexts
// =========================================================================================

#[queue_computation_accounts("init_order_pool", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct InitOrderPool<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init_if_needed,
        space = 9,
        payer = payer,
        seeds = [&SIGN_PDA_SEED],
        bump,
        address = derive_sign_pda!(),
    )]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool_account, checked by the arcium program.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: executing_pool, checked by the arcium program.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation_account, checked by the arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_ORDER_POOL))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // ---- custom ----
    pub market: Box<Account<'info, Market>>,
    #[account(
        init,
        payer = payer,
        space = 8 + OrderPool::INIT_SPACE,
        seeds = [ORDER_POOL_SEED, market.key().as_ref()],
        bump,
    )]
    pub order_pool: Box<Account<'info, OrderPool>>,
}

#[callback_accounts("init_order_pool")]
#[derive(Accounts)]
pub struct InitOrderPoolCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_ORDER_POOL))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Account<'info, MXEAccount>,
    /// CHECK: computation_account, checked by arcium program via the callback context.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions_sysvar, checked by the account constraint.
    pub instructions_sysvar: UncheckedAccount<'info>,
    #[account(mut)]
    pub order_pool: Account<'info, OrderPool>,
}

#[init_computation_definition_accounts("init_order_pool", payer)]
#[derive(Accounts)]
pub struct InitOrderPoolCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account, checked by arcium program.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table, checked by arcium program.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program is the Address Lookup Table program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[queue_computation_accounts("submit_order", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct SubmitOrder<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init_if_needed,
        space = 9,
        payer = payer,
        seeds = [&SIGN_PDA_SEED],
        bump,
        address = derive_sign_pda!(),
    )]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool_account, checked by the arcium program.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: executing_pool, checked by the arcium program.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation_account, checked by the arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_SUBMIT_ORDER))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // ---- custom ----
    #[account(mut)]
    pub order_pool: Box<Account<'info, OrderPool>>,
}

#[callback_accounts("submit_order")]
#[derive(Accounts)]
pub struct SubmitOrderCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_SUBMIT_ORDER))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Account<'info, MXEAccount>,
    /// CHECK: computation_account, checked by arcium program via the callback context.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions_sysvar, checked by the account constraint.
    pub instructions_sysvar: UncheckedAccount<'info>,
    #[account(mut)]
    pub order_pool: Account<'info, OrderPool>,
}

#[init_computation_definition_accounts("submit_order", payer)]
#[derive(Accounts)]
pub struct SubmitOrderCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account, checked by arcium program.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table, checked by arcium program.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program is the Address Lookup Table program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[queue_computation_accounts("match_batch", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct MatchBatch<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init_if_needed,
        space = 9,
        payer = payer,
        seeds = [&SIGN_PDA_SEED],
        bump,
        address = derive_sign_pda!(),
    )]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool_account, checked by the arcium program.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: executing_pool, checked by the arcium program.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation_account, checked by the arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_MATCH_BATCH))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // ---- custom ----
    #[account(mut)]
    pub market: Box<Account<'info, Market>>,
    #[account(has_one = market @ ErrorCode::Unauthorized)]
    pub order_pool: Box<Account<'info, OrderPool>>,
}

#[callback_accounts("match_batch")]
#[derive(Accounts)]
pub struct MatchBatchCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_MATCH_BATCH))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Account<'info, MXEAccount>,
    /// CHECK: computation_account, checked by arcium program via the callback context.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions_sysvar, checked by the account constraint.
    pub instructions_sysvar: UncheckedAccount<'info>,
    #[account(mut)]
    pub market: Account<'info, Market>,
}

#[init_computation_definition_accounts("match_batch", payer)]
#[derive(Accounts)]
pub struct MatchBatchCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account, checked by arcium program.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table, checked by arcium program.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program is the Address Lookup Table program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}
