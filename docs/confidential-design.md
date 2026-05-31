# Confidential design (Phase 2+)

How Dark-Pool Perps makes positions private using Arcium MPC, while the Phase 1 engine keeps
working. Based on the verified Arcium 0.10.4 API (`arcis` 0.10.4 / `arcium-anchor` 0.10.4).

## Encryption model (from `arcis::enc`)
- **`Enc<Mxe, T>`** — encrypted to the MPC cluster's persistent key. Can be **stored on-chain**
  and re-fed into later computations. This is how confidential position state persists.
- **`Enc<Shared, T>`** — encrypted to a client's x25519 key (per-tx nonce). For client inputs
  and for results only that client can decrypt.
- Inside a circuit: `x.to_arcis()` decrypts to secret-shares; `owner.from_arcis(v)` /
  `Mxe::get().from_arcis(v)` re-encrypts; `v.reveal()` makes a value public.

## Data model
- **`ConfidentialUser`** account (replaces the public fields of `UserAccount` for the private
  path): `authority`, `market`, **`enc_position`** = the `Enc<Mxe, Position>` ciphertext bytes
  (collateral + base + entry + nonce), `last_cumulative_funding`, `bump`.
  `Position { collateral: i128 /*QUOTE 1e6*/, base: i128 /*BASE 1e9, signed*/, quote_entry: i128 /*cost basis = Σ base_delta·fill_price*/ }`.
  (Cost-basis model, not weighted-avg price: MPC has no division by encrypted values, so we accumulate cost and keep all margin math multiply/add/compare.)
- **Collateral backing:** real USDC stays in the existing pooled `Market.vault`; the *per-user
  amount* is confidential (lives inside `enc_position`). No dependency on C-SPL.
- The public `Market` (vAMM reserves, funding, OI, oracle) is unchanged — only per-user state
  becomes private.

## Circuits (`encrypted-ixs/src/lib.rs`)
- ✅ **`check_liquidation(position: Enc<Mxe,Position>, price: i64, maintenance_bps: i64) -> bool`**
  — margin check in MPC; price/maint are PUBLIC inputs; only the liquidatable bool is revealed.
  Division-free: `(collateral·1e9 + base·(price−entry))·1e4 < |base|·price·maint`. *Compiles.*
- ✅ **`update_position(position: Enc<Mxe,Position>, fill: Enc<Shared,Fill>, price: i64, initial_bps: i64) -> (Enc<Mxe,Position>, bool)`**
  — apply an encrypted fill to the stored position in MPC; returns the new encrypted state +
  a revealed `meets_initial_margin` bool. Callback stores the new position only if margin is met
  (rejects over-leverage), so the trade size/collateral stay hidden. *Compiles + builds.*
- (Phase 3) **`match_batch([EncOrder;32]) -> [EncFill;32]`** — confidential batch auction.

## MPC flow (mirrors the validated `add_together` pattern)
Each confidential op = 3 program pieces:
1. **`init_*_comp_def`** (once) — register the circuit's computation definition + upload `.arcis`.
2. **queue instruction** — CPI `queue_computation` with args built via `ArgBuilder`:
   - stored encrypted position → **`.account(conf_user_pubkey, offset, length)`** (MPC reads the
     ciphertext straight from the account — no client re-supply),
   - public price/maint → `.plaintext_i64(...)`,
   - client inputs (for `update_position`) → `.x25519_pubkey(pk).plaintext_u128(nonce).encrypted_*(ct)`.
3. **`#[arcium_callback]`** — `output.verify_output(cluster, computation)`; for
   `check_liquidation` the revealed `bool` drives an on-chain liquidation (close vs vAMM + fee);
   for `update_position` the new `Enc<Mxe,Position>` ciphertext is written back to the account.

## What's hidden vs revealed
- **Hidden:** collateral, position size/direction, entry price, PnL.
- **Revealed:** that a liquidation occurred (the bool), market-level vAMM/OI/funding (public by design),
  the oracle price (public input — Arcium can't fetch oracles anyway).

## Constraints baked into the design
- **~1–2 s MPC latency** → liquidation is a crank, not real-time; UX shows pending state.
- **Fixed-shape circuits** (no `Vec`/`while`/early-return) → batch auction uses fixed 32-slot arrays.
- **Division-free math** in circuits (MPC division is costly) → cross-multiplied comparisons.
- **feed-in via `account()`** keeps the per-call payload small and avoids client re-encryption of state.

## Status
Phase 2a (research) ✅ · 2b (design) ✅ · 2c circuit ✅ (compiles) → wiring next ·
2d (update_position + storage) · 2e (client x25519/RescueCipher + localnet MPC tests).
