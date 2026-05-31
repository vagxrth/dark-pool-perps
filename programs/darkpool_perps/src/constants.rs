use anchor_lang::prelude::*;
use arcium_anchor::comp_def_offset;

#[constant]
pub const SEED: &str = "arcium";

pub const COMP_DEF_OFFSET_ADD_TOGETHER: u32 = comp_def_offset("add_together");

// ===================== Perps fixed-point precision =====================
// All on-chain perps math uses i128 integer fixed-point with these scales.

/// Oracle / mark price precision (1e6). e.g. $150.25 -> 150_250_000.
pub const PRICE_PRECISION: i128 = 1_000_000;
/// Quote (USDC) precision (1e6) — USDC has 6 decimals.
pub const QUOTE_PRECISION: i128 = 1_000_000;
/// Base asset size precision (1e9). e.g. 1.5 SOL -> 1_500_000_000.
pub const BASE_PRECISION: i128 = 1_000_000_000;
/// vAMM peg multiplier precision (1e6).
pub const PEG_PRECISION: i128 = 1_000_000;
/// Margin-ratio precision in basis points (10_000 = 100%).
pub const MARGIN_PRECISION: i128 = 10_000;
/// Funding-rate precision (1e9).
pub const FUNDING_PRECISION: i128 = 1_000_000_000;

// ===================== Default risk parameters =====================
// (Per-market configurable later; sane defaults for a single SOL-PERP market.)

/// Maintenance margin: 5% (500 bps). Below this an account is liquidatable.
pub const DEFAULT_MAINTENANCE_MARGIN_BPS: i128 = 500;
/// Initial margin: 10% (1000 bps) -> 10x max leverage at open.
pub const DEFAULT_INITIAL_MARGIN_BPS: i128 = 1_000;
/// Liquidation fee: 2.5% (250 bps) of notional, paid to liquidator/insurance.
pub const DEFAULT_LIQUIDATION_FEE_BPS: i128 = 250;
/// Funding accrues over this interval (1 hour, in seconds).
pub const FUNDING_INTERVAL_SECS: i128 = 3_600;

// ===================== Oracle guards =====================
/// Max age (seconds) of an oracle price before it is considered stale.
pub const MAX_ORACLE_STALENESS_SECS: i64 = 60;
/// Max confidence interval as a fraction of price, in bps (e.g. 200 = 2%).
pub const MAX_ORACLE_CONF_BPS: i128 = 200;

// ===================== PDA seed prefixes =====================
pub const MARKET_SEED: &[u8] = b"market";
pub const USER_SEED: &[u8] = b"user";
pub const VAULT_SEED: &[u8] = b"vault";
pub const ADMIN_ORACLE_SEED: &[u8] = b"admin_oracle";
