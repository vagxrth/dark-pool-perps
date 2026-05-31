use arcis::*;

#[encrypted]
mod circuits {
    use arcis::*;

    pub struct InputValues {
        v1: u8,
        v2: u8,
    }

    #[instruction]
    pub fn add_together(input_ctxt: Enc<Shared, InputValues>) -> Enc<Shared, u16> {
        let input = input_ctxt.to_arcis();
        let sum = input.v1 as u16 + input.v2 as u16;
        input_ctxt.owner.from_arcis(sum)
    }

    // ============================ Dark-Pool Perps (Phase 2) ============================
    // Confidential position state, encrypted to the MXE cluster so it can persist on-chain
    // and be re-fed into computations. Fixed-point scales mirror the on-chain program:
    //   collateral: QUOTE_PRECISION (1e6) · base: BASE_PRECISION (1e9, signed) · entry: PRICE_PRECISION (1e6)
    pub struct Position {
        collateral: i128,
        base: i128,
        entry: i128,
    }

    /// Store a client-supplied position as MXE-encrypted state on-chain. The client encrypts
    /// the Position to the cluster (Enc<Shared>), the circuit re-encrypts it to the MXE
    /// (Enc<Mxe>) so it persists and can be re-fed into future computations.
    #[instruction]
    pub fn init_position(pos_ctxt: Enc<Shared, Position>) -> Enc<Mxe, Position> {
        let pos = pos_ctxt.to_arcis();
        Mxe::get().from_arcis(pos)
    }

    /// Confidential margin/liquidation check. The position is encrypted (only the MXE can
    /// read it); the oracle price and maintenance ratio are PUBLIC inputs. Only the boolean
    /// "is this account liquidatable?" is revealed — sizes, collateral, and PnL stay hidden.
    ///
    /// liquidatable  ⟺  account_value / notional < maintenance_bps / MARGIN_PRECISION
    /// Cross-multiplied and cleared of /BASE_PRECISION to avoid (costly) MPC division:
    ///   (collateral·BASE_PRECISION + base·(price−entry))·MARGIN_PRECISION  <  |base|·price·maintenance_bps
    /// Magnitudes stay < ~2^80, well within the field, so no overflow.
    #[instruction]
    pub fn check_liquidation(
        position: Enc<Mxe, Position>,
        price: i64,
        maintenance_bps: i64,
    ) -> bool {
        let p = position.to_arcis();
        let price = price as i128;
        let base_abs = if p.base < 0 { -p.base } else { p.base };

        // BASE_PRECISION = 1_000_000_000, MARGIN_PRECISION = 10_000
        let lhs = (p.collateral * 1_000_000_000 + p.base * (price - p.entry)) * 10_000;
        let rhs = base_abs * price * (maintenance_bps as i128);

        (lhs < rhs).reveal()
    }
}
