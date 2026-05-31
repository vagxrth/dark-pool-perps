//! Confidential layer (Phase 2): store an encrypted position (`Enc<Mxe, Position>`) on-chain
//! and run the margin/liquidation decision inside MPC over it. Structure mirrors the scaffold's
//! `add_together` (Arcium 0.10.4 macro forms) and the Arcium voting example (persistent
//! `Enc<Mxe>` state via `ArgBuilder.account()` + `CallbackAccount` storage).

use crate::constants::*;
use crate::error::ErrorCode;
use crate::state::oracle::load_oracle_price;
use crate::state::{ConfidentialUser, Market};
use crate::{ArciumSignerAccount, ID, ID_CONST};
use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
use arcium_client::idl::arcium::types::CallbackAccount;

#[event]
pub struct LiquidationCheckEvent {
    pub user: Pubkey,
    pub liquidatable: bool,
}

// =========================================================================================
// init_position : store a client-supplied Enc<Shared, Position> as Enc<Mxe, Position> on-chain
// =========================================================================================

pub fn init_position_comp_def_handler(ctx: Context<InitPositionCompDef>) -> Result<()> {
    init_computation_def(ctx.accounts, None)?;
    Ok(())
}

pub fn init_position_handler(
    ctx: Context<InitPosition>,
    computation_offset: u64,
    ct_collateral: [u8; 32],
    ct_base: [u8; 32],
    ct_entry: [u8; 32],
    pub_key: [u8; 32],
    nonce: u128,
) -> Result<()> {
    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
    {
        let cu = &mut ctx.accounts.conf_user;
        cu.bump = ctx.bumps.conf_user;
        cu.authority = ctx.accounts.payer.key();
        cu.market = ctx.accounts.market.key();
        cu.initialized = false;
        cu.liquidatable = false;
    }
    let conf_user_key = ctx.accounts.conf_user.key();

    // Enc<Shared, Position{collateral, base, entry}> = pubkey + nonce + one ciphertext per field.
    let args = ArgBuilder::new()
        .x25519_pubkey(pub_key)
        .plaintext_u128(nonce)
        .encrypted_i128(ct_collateral)
        .encrypted_i128(ct_base)
        .encrypted_i128(ct_entry)
        .build();

    queue_computation(
        ctx.accounts,
        computation_offset,
        args,
        vec![InitPositionCallback::callback_ix(
            computation_offset,
            &ctx.accounts.mxe_account,
            &[CallbackAccount {
                pubkey: conf_user_key,
                is_writable: true,
            }],
        )?],
        1,
        0,
    )?;
    Ok(())
}

pub fn init_position_callback_handler(
    ctx: Context<InitPositionCallback>,
    output: SignedComputationOutputs<InitPositionOutput>,
) -> Result<()> {
    let o = match output.verify_output(
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
    ) {
        Ok(InitPositionOutput { field_0 }) => field_0,
        Err(_) => return Err(ErrorCode::AbortedComputation.into()),
    };
    let cu = &mut ctx.accounts.conf_user;
    cu.enc_position = o.ciphertexts;
    cu.nonce = o.nonce;
    cu.initialized = true;
    Ok(())
}

// =========================================================================================
// check_liquidation : MPC margin decision over the stored Enc<Mxe, Position> vs public price
// =========================================================================================

pub fn check_liquidation_comp_def_handler(ctx: Context<CheckLiquidationCompDef>) -> Result<()> {
    init_computation_def(ctx.accounts, None)?;
    Ok(())
}

pub fn check_liquidation_handler(
    ctx: Context<CheckLiquidation>,
    computation_offset: u64,
) -> Result<()> {
    require!(ctx.accounts.conf_user.initialized, ErrorCode::NoPosition);
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
    let price_i64 = i64::try_from(px.price).map_err(|_| ErrorCode::MathOverflow)?;
    let maintenance_bps = ctx.accounts.market.maintenance_margin_bps as i64;

    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
    let nonce = ctx.accounts.conf_user.nonce;
    let conf_user_key = ctx.accounts.conf_user.key();

    // Enc<Mxe, Position> = nonce + account(ciphertext bytes); then the two public plaintext args.
    let args = ArgBuilder::new()
        .plaintext_u128(nonce)
        .account(conf_user_key, 8 + 1, 32 * 3)
        .plaintext_i64(price_i64)
        .plaintext_i64(maintenance_bps)
        .build();

    queue_computation(
        ctx.accounts,
        computation_offset,
        args,
        vec![CheckLiquidationCallback::callback_ix(
            computation_offset,
            &ctx.accounts.mxe_account,
            &[CallbackAccount {
                pubkey: conf_user_key,
                is_writable: true,
            }],
        )?],
        1,
        0,
    )?;
    Ok(())
}

pub fn check_liquidation_callback_handler(
    ctx: Context<CheckLiquidationCallback>,
    output: SignedComputationOutputs<CheckLiquidationOutput>,
) -> Result<()> {
    let liquidatable = match output.verify_output(
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
    ) {
        Ok(CheckLiquidationOutput { field_0 }) => field_0,
        Err(_) => return Err(ErrorCode::AbortedComputation.into()),
    };
    ctx.accounts.conf_user.liquidatable = liquidatable;
    emit!(LiquidationCheckEvent {
        user: ctx.accounts.conf_user.key(),
        liquidatable,
    });
    Ok(())
}

// =========================================================================================
// Account contexts (arcium block copied from the scaffold add_together; custom accounts last)
// =========================================================================================

#[queue_computation_accounts("init_position", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct InitPosition<'info> {
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
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_POSITION))]
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
        space = 8 + ConfidentialUser::INIT_SPACE,
        seeds = [CONF_USER_SEED, market.key().as_ref(), payer.key().as_ref()],
        bump,
    )]
    pub conf_user: Box<Account<'info, ConfidentialUser>>,
}

#[callback_accounts("init_position")]
#[derive(Accounts)]
pub struct InitPositionCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_POSITION))]
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
    pub conf_user: Account<'info, ConfidentialUser>,
}

#[init_computation_definition_accounts("init_position", payer)]
#[derive(Accounts)]
pub struct InitPositionCompDef<'info> {
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

#[queue_computation_accounts("check_liquidation", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct CheckLiquidation<'info> {
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
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_CHECK_LIQUIDATION))]
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
    /// CHECK: verified against market.oracle in the handler.
    pub oracle: UncheckedAccount<'info>,
    #[account(mut, has_one = market @ ErrorCode::Unauthorized)]
    pub conf_user: Box<Account<'info, ConfidentialUser>>,
}

#[callback_accounts("check_liquidation")]
#[derive(Accounts)]
pub struct CheckLiquidationCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_CHECK_LIQUIDATION))]
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
    pub conf_user: Account<'info, ConfidentialUser>,
}

#[init_computation_definition_accounts("check_liquidation", payer)]
#[derive(Accounts)]
pub struct CheckLiquidationCompDef<'info> {
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
