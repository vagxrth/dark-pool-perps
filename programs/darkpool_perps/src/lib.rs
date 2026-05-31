pub mod constants;
pub mod error;
pub mod instructions;
pub mod math;
pub mod state;

use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
pub use constants::*;
pub use instructions::*;
#[allow(unused_imports)]
pub use state::*;

declare_id!("F1b3V2V3dg6YDsfPG6Rc9y769fN4uaZio96st5owXzAr");

#[arcium_program]
pub mod darkpool_perps {
    use super::*;

    pub fn init_add_together_comp_def(ctx: Context<InitAddTogetherCompDef>) -> Result<()> {
        add_together::init_add_together_comp_def_handler(ctx)
    }

    pub fn add_together(
        ctx: Context<AddTogether>,
        computation_offset: u64,
        ciphertext_0: [u8; 32],
        ciphertext_1: [u8; 32],
        pub_key: [u8; 32],
        nonce: u128,
    ) -> Result<()> {
        add_together::add_together_handler(ctx, computation_offset, ciphertext_0, ciphertext_1, pub_key, nonce)
    }

    #[arcium_callback(encrypted_ix = "add_together")]
    pub fn add_together_callback(
        ctx: Context<AddTogetherCallback>,
        output: SignedComputationOutputs<AddTogetherOutput>,
    ) -> Result<()> {
        add_together::add_together_callback_handler(ctx, output)
    }

    // ===================== Perps engine — setup & collateral (Phase 1) =====================

    #[allow(clippy::too_many_arguments)]
    pub fn init_market(
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
        instructions::init_market_handler(
            ctx,
            market_index,
            base_reserve,
            quote_reserve,
            oracle,
            oracle_source,
            maintenance_margin_bps,
            initial_margin_bps,
            liquidation_fee_bps,
        )
    }

    pub fn init_admin_oracle(
        ctx: Context<InitAdminOracle>,
        price: i128,
        conf: u64,
    ) -> Result<()> {
        instructions::init_admin_oracle_handler(ctx, price, conf)
    }

    pub fn push_admin_price(ctx: Context<PushAdminPrice>, price: i128, conf: u64) -> Result<()> {
        instructions::push_admin_price_handler(ctx, price, conf)
    }

    pub fn init_user(ctx: Context<InitUser>) -> Result<()> {
        instructions::init_user_handler(ctx)
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit_handler(ctx, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        instructions::withdraw_handler(ctx, amount)
    }

    // ===================== Perps engine — trading (Phase 1) =====================

    pub fn open_position(
        ctx: Context<Trade>,
        base_amount: u64,
        is_long: bool,
        limit_price: i128,
    ) -> Result<()> {
        instructions::open_position_handler(ctx, base_amount, is_long, limit_price)
    }

    pub fn close_position(ctx: Context<Trade>, limit_price: i128) -> Result<()> {
        instructions::close_position_handler(ctx, limit_price)
    }

    pub fn settle_funding(ctx: Context<SettleFunding>) -> Result<()> {
        instructions::settle_funding_handler(ctx)
    }

    pub fn liquidate(ctx: Context<Liquidate>) -> Result<()> {
        instructions::liquidate_handler(ctx)
    }

    // ===================== Confidential layer (Phase 2) =====================

    pub fn init_position_comp_def(ctx: Context<InitPositionCompDef>) -> Result<()> {
        instructions::init_position_comp_def_handler(ctx)
    }

    pub fn init_position(
        ctx: Context<InitPosition>,
        computation_offset: u64,
        ct_collateral: [u8; 32],
        ct_base: [u8; 32],
        ct_entry: [u8; 32],
        pub_key: [u8; 32],
        nonce: u128,
    ) -> Result<()> {
        instructions::init_position_handler(
            ctx,
            computation_offset,
            ct_collateral,
            ct_base,
            ct_entry,
            pub_key,
            nonce,
        )
    }

    #[arcium_callback(encrypted_ix = "init_position")]
    pub fn init_position_callback(
        ctx: Context<InitPositionCallback>,
        output: SignedComputationOutputs<InitPositionOutput>,
    ) -> Result<()> {
        instructions::init_position_callback_handler(ctx, output)
    }

    pub fn check_liquidation_comp_def(ctx: Context<CheckLiquidationCompDef>) -> Result<()> {
        instructions::check_liquidation_comp_def_handler(ctx)
    }

    pub fn check_liquidation(
        ctx: Context<CheckLiquidation>,
        computation_offset: u64,
    ) -> Result<()> {
        instructions::check_liquidation_handler(ctx, computation_offset)
    }

    #[arcium_callback(encrypted_ix = "check_liquidation")]
    pub fn check_liquidation_callback(
        ctx: Context<CheckLiquidationCallback>,
        output: SignedComputationOutputs<CheckLiquidationOutput>,
    ) -> Result<()> {
        instructions::check_liquidation_callback_handler(ctx, output)
    }

    pub fn update_position_comp_def(ctx: Context<UpdatePositionCompDef>) -> Result<()> {
        instructions::update_position_comp_def_handler(ctx)
    }

    pub fn update_position(
        ctx: Context<UpdatePosition>,
        computation_offset: u64,
        ct_base_delta: [u8; 32],
        ct_fill_price: [u8; 32],
        pub_key: [u8; 32],
        fill_nonce: u128,
    ) -> Result<()> {
        instructions::update_position_handler(
            ctx,
            computation_offset,
            ct_base_delta,
            ct_fill_price,
            pub_key,
            fill_nonce,
        )
    }

    #[arcium_callback(encrypted_ix = "update_position")]
    pub fn update_position_callback(
        ctx: Context<UpdatePositionCallback>,
        output: SignedComputationOutputs<UpdatePositionOutput>,
    ) -> Result<()> {
        instructions::update_position_callback_handler(ctx, output)
    }
}
