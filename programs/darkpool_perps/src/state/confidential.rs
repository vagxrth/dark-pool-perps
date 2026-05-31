use anchor_lang::prelude::*;

/// Confidential per-user position: the on-chain ciphertext of the circuit's
/// `Position { collateral, base, entry }`, encrypted to the MXE cluster (`Enc<Mxe, Position>`).
///
/// `bump` is declared FIRST so the ciphertext array sits at a fixed offset (8 discriminator +
/// 1 bump = 9), which `ArgBuilder.account(pubkey, 9, 32*3)` reads to feed the stored state
/// back into MPC computations (same layout trick as the Arcium voting example).
#[account]
#[derive(InitSpace)]
pub struct ConfidentialUser {
    pub bump: u8,
    /// Enc<Mxe, Position> ciphertexts in field order: [collateral, base, entry], 32 bytes each.
    pub enc_position: [[u8; 32]; 3],
    /// MXE nonce for the encrypted position.
    pub nonce: u128,
    pub authority: Pubkey,
    pub market: Pubkey,
    /// Result of the most recent confidential margin check (set by the check_liquidation callback).
    pub liquidatable: bool,
    /// True once the encrypted position has been stored by the init_position callback.
    pub initialized: bool,
}

/// Confidential dark-pool order book: the on-chain ciphertext of `Enc<Mxe, [Order; 8]>`
/// (8 orders × 3 fields = 24 ciphertexts). `bump` first so the ciphertext array is at offset
/// 9 for `ArgBuilder.account(pubkey, 9, 32*24)` feed-in.
#[account]
#[derive(InitSpace)]
pub struct OrderPool {
    pub bump: u8,
    /// Enc<Mxe, [Order; 8]> ciphertexts (order-major: [is_buy, price, size] × 8).
    pub enc_orders: [[u8; 32]; 24],
    pub nonce: u128,
    pub market: Pubkey,
    /// Next free slot to write (0..8); wraps for the demo.
    pub next_slot: u8,
    pub initialized: bool,
}
