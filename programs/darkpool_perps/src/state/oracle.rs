use anchor_lang::prelude::*;

/// How a market's `oracle` account should be interpreted.
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace, Default,
)]
pub enum OracleSource {
    /// Admin-pushed price held in an [`AdminOracle`] account. Dev stand-in used until the
    /// manual Pyth `PriceUpdateV2` reader lands (Phase 1c); `pyth-solana-receiver-sdk` is
    /// not usable here because it pins anchor-lang 0.32.1 (see docs/ + Cargo.toml note).
    #[default]
    Admin,
    /// Pyth pull oracle `PriceUpdateV2`, parsed by manual borsh deserialization.
    Pyth,
}

/// A simple admin-controlled price feed used during development. Mirrors the fields we read
/// out of a Pyth `PriceUpdateV2` so the oracle-reading code path is identical for both sources.
#[account]
#[derive(InitSpace)]
pub struct AdminOracle {
    /// Authority allowed to push prices.
    pub authority: Pubkey,
    /// Price in `PRICE_PRECISION` (1e6).
    pub price: i128,
    /// Confidence interval in `PRICE_PRECISION`.
    pub conf: u64,
    /// Unix timestamp of the last price push (for staleness checks).
    pub last_update_ts: i64,
    pub bump: u8,
}

// ============================ Oracle reading ============================
// A normalized reader that returns (price, conf, ts) in PRICE_PRECISION from either the
// admin stand-in or a Pyth `PriceUpdateV2` account parsed by MANUAL borsh deserialization
// (we cannot depend on `pyth-solana-receiver-sdk` — it pins anchor-lang 0.32.1).

use crate::constants::{MARGIN_PRECISION, MAX_ORACLE_CONF_BPS, MAX_ORACLE_STALENESS_SECS};
use crate::error::ErrorCode;
use core::str::FromStr;

/// Base-10 digits in PRICE_PRECISION (1e6 -> 6).
const PRICE_PRECISION_EXPO: i32 = 6;

/// Pyth Solana Receiver program — owner of all `PriceUpdateV2` accounts (mainnet & devnet).
pub const PYTH_RECEIVER_PROGRAM_ID_STR: &str = "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ";

/// A validated, normalized oracle reading (all fields in PRICE_PRECISION except `ts`).
#[derive(Clone, Copy, Debug)]
pub struct OraclePrice {
    pub price: i128,
    pub conf: i128,
    pub ts: i64,
}

// --- Minimal mirror of Pyth's PriceUpdateV2 (borsh, after the 8-byte anchor discriminator) ---
#[allow(dead_code)]
#[derive(AnchorDeserialize, Clone)]
pub enum PythVerificationLevel {
    Partial { num_signatures: u8 },
    Full,
}

#[allow(dead_code)]
#[derive(AnchorDeserialize, Clone)]
pub struct PythPriceFeedMessage {
    pub feed_id: [u8; 32],
    pub price: i64,
    pub conf: u64,
    pub exponent: i32,
    pub publish_time: i64,
    pub prev_publish_time: i64,
    pub ema_price: i64,
    pub ema_conf: u64,
}

#[allow(dead_code)]
#[derive(AnchorDeserialize, Clone)]
pub struct PythPriceUpdateV2 {
    pub write_authority: Pubkey,
    pub verification_level: PythVerificationLevel,
    pub price_message: PythPriceFeedMessage,
    pub posted_slot: u64,
}

/// Scale a raw integer with base-10 `expo` into PRICE_PRECISION (1e6) units.
fn scale_to_price_precision(raw: i128, expo: i32) -> Option<i128> {
    let e = expo.checked_add(PRICE_PRECISION_EXPO)?;
    if e >= 0 {
        raw.checked_mul(10i128.checked_pow(e as u32)?)
    } else {
        raw.checked_div(10i128.checked_pow(e.unsigned_abs())?)
    }
}

/// Read and validate the price from a market's oracle account.
/// Verifies account ownership, normalizes to PRICE_PRECISION, and enforces staleness +
/// confidence guards. `now` is the current unix timestamp; `program_id` is this program.
pub fn load_oracle_price(
    oracle_ai: &AccountInfo,
    source: OracleSource,
    now: i64,
    program_id: &Pubkey,
) -> Result<OraclePrice> {
    let op = match source {
        OracleSource::Admin => {
            require_keys_eq!(*oracle_ai.owner, *program_id, ErrorCode::InvalidOracle);
            let acc = AdminOracle::try_deserialize(&mut &oracle_ai.data.borrow()[..])
                .map_err(|_| ErrorCode::InvalidOracle)?;
            OraclePrice {
                price: acc.price,
                conf: acc.conf as i128,
                ts: acc.last_update_ts,
            }
        }
        OracleSource::Pyth => {
            let pyth_owner =
                Pubkey::from_str(PYTH_RECEIVER_PROGRAM_ID_STR).map_err(|_| ErrorCode::InvalidOracle)?;
            require_keys_eq!(*oracle_ai.owner, pyth_owner, ErrorCode::InvalidOracle);
            let data = oracle_ai.data.borrow();
            require!(data.len() > 8, ErrorCode::InvalidOracle);
            let update = PythPriceUpdateV2::try_from_slice(&data[8..])
                .map_err(|_| ErrorCode::InvalidOracle)?;
            let m = &update.price_message;
            OraclePrice {
                price: scale_to_price_precision(m.price as i128, m.exponent)
                    .ok_or(ErrorCode::MathOverflow)?,
                conf: scale_to_price_precision(m.conf as i128, m.exponent)
                    .ok_or(ErrorCode::MathOverflow)?,
                ts: m.publish_time,
            }
        }
    };

    require!(op.price > 0, ErrorCode::InvalidOracle);
    require!(
        now.checked_sub(op.ts).ok_or(ErrorCode::MathOverflow)? <= MAX_ORACLE_STALENESS_SECS,
        ErrorCode::StaleOracle
    );
    // confidence as a fraction of price, in bps: conf * MARGIN_PRECISION / price
    let conf_bps = op
        .conf
        .checked_mul(MARGIN_PRECISION)
        .ok_or(ErrorCode::MathOverflow)?
        .checked_div(op.price)
        .ok_or(ErrorCode::MathOverflow)?;
    require!(
        conf_bps <= MAX_ORACLE_CONF_BPS,
        ErrorCode::OracleConfidenceTooWide
    );

    Ok(op)
}
