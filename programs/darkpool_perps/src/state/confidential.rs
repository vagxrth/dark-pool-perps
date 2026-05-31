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
