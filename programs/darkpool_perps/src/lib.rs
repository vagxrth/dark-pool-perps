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
}
