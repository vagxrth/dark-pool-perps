use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid computation")]
    InvalidComputation,
    #[msg("Invalid callback")]
    InvalidCallback,
    #[msg("Custom error message")]
    CustomError,
    #[msg("The computation was aborted")]
    AbortedComputation,

    // ---- Perps engine ----
    #[msg("Arithmetic overflow / underflow")]
    MathOverflow,
    #[msg("Invalid amount (must be > 0)")]
    InvalidAmount,
    #[msg("Market is paused")]
    MarketPaused,
    #[msg("Oracle account does not match the market")]
    InvalidOracle,
    #[msg("Oracle price is stale")]
    StaleOracle,
    #[msg("Oracle confidence interval too wide")]
    OracleConfidenceTooWide,
    #[msg("Insufficient collateral for withdrawal")]
    InsufficientCollateral,
    #[msg("Order would breach initial margin requirement")]
    InsufficientMargin,
    #[msg("No open position")]
    NoPosition,
    #[msg("Position is not liquidatable")]
    PositionNotLiquidatable,
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Unauthorized")]
    Unauthorized,
}
