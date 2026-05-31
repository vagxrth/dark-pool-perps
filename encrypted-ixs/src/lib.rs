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
    // Confidential position state, encrypted to the MXE cluster so it persists on-chain and
    // can be re-fed into computations. Cost-basis model (no MPC division):
    //   collateral   : QUOTE_PRECISION (1e6)
    //   base         : BASE_PRECISION (1e9), signed (+long / -short)
    //   quote_entry  : accumulated cost basis = sum(base_delta * fill_price), scaled to
    //                  BASE_PRECISION*PRICE_PRECISION (1e15). Avoids weighted-average division.
    //
    // Everything below works in the "scaled" domain (× BASE_PRECISION) to stay division-free:
    //   account_value_scaled = collateral*BASE_PRECISION + (base*price - quote_entry)
    //   notional_scaled      = |base| * price
    pub struct Position {
        collateral: i128,
        base: i128,
        quote_entry: i128,
    }

    /// A fill to apply to a position: signed size delta + the execution price.
    pub struct Fill {
        base_delta: i128, // BASE_PRECISION, signed
        fill_price: i128, // PRICE_PRECISION
    }

    /// Store a client-supplied position as MXE-encrypted state on-chain. The client encrypts
    /// the Position to the cluster (Enc<Shared>); the circuit re-encrypts it to the MXE
    /// (Enc<Mxe>) so it persists and can be re-fed into future computations.
    #[instruction]
    pub fn init_position(pos_ctxt: Enc<Shared, Position>) -> Enc<Mxe, Position> {
        let pos = pos_ctxt.to_arcis();
        Mxe::get().from_arcis(pos)
    }

    /// Apply a same-direction fill (open / increase) to the encrypted position, entirely in
    /// MPC. Returns the updated encrypted position plus a revealed bool: does it still meet
    /// the initial-margin requirement at the public oracle price? The on-chain callback only
    /// stores the new position when the bool is true (rejects over-leverage), so size,
    /// collateral, and cost basis stay hidden throughout.
    #[instruction]
    pub fn update_position(
        pos_ctxt: Enc<Mxe, Position>,
        fill_ctxt: Enc<Shared, Fill>,
        price: i64,
        initial_bps: i64,
    ) -> (Enc<Mxe, Position>, bool) {
        let mut p = pos_ctxt.to_arcis();
        let f = fill_ctxt.to_arcis();
        let price = price as i128;

        p.base = p.base + f.base_delta;
        p.quote_entry = p.quote_entry + f.base_delta * f.fill_price;

        let base_abs = if p.base < 0 { -p.base } else { p.base };
        // BASE_PRECISION = 1_000_000_000, MARGIN_PRECISION = 10_000
        let av_scaled = p.collateral * 1_000_000_000 + (p.base * price - p.quote_entry);
        let notional_scaled = base_abs * price;
        let meets = av_scaled * 10_000 >= notional_scaled * (initial_bps as i128);

        (pos_ctxt.owner.from_arcis(p), meets.reveal())
    }

    /// Confidential margin/liquidation check. The position is encrypted (only the MXE reads
    /// it); the oracle price and maintenance ratio are PUBLIC. Only the liquidatable bool is
    /// revealed — size, collateral, and PnL stay hidden.
    ///
    /// liquidatable ⟺ account_value / notional < maintenance_bps / MARGIN_PRECISION,
    /// cross-multiplied (division-free) in the scaled domain.
    #[instruction]
    pub fn check_liquidation(
        pos_ctxt: Enc<Mxe, Position>,
        price: i64,
        maintenance_bps: i64,
    ) -> bool {
        let p = pos_ctxt.to_arcis();
        let price = price as i128;
        let base_abs = if p.base < 0 { -p.base } else { p.base };
        let av_scaled = p.collateral * 1_000_000_000 + (p.base * price - p.quote_entry);
        let notional_scaled = base_abs * price;
        (av_scaled * 10_000 < notional_scaled * (maintenance_bps as i128)).reveal()
    }

    // ===================== Confidential dark-pool matching (Phase 3) =====================
    // A single confidential order. `is_buy` = side, `price` = limit (PRICE_PRECISION),
    // `size` = quantity (BASE_PRECISION, always positive). Padding slots use size = 0.
    pub struct Order {
        is_buy: bool,
        price: i128,
        size: i128,
    }

    /// Confidential batch-auction clearing over an encrypted order book.
    ///
    /// The whole book is encrypted to the MXE; `ref_price` (the public vAMM mark / oracle) is
    /// the clearing price. The MPC sums the size of every order that *crosses* `ref_price`
    /// (buys with limit ≥ price, sells with limit ≤ price), nets the two sides, and reveals
    /// ONLY two aggregates:
    ///   - `matched`: peer-to-peer volume crossed at `ref_price` (= min(buy, sell))
    ///   - `net`:     signed residual (buy − sell) the public vAMM absorbs (+ = vAMM sells)
    /// Every individual order's side, price, and size stays hidden — the dark pool.
    ///
    /// Fixed-shape (book of 8, zero-padded) and branchless to satisfy Arcis.
    #[instruction]
    pub fn match_batch(book_ctxt: Enc<Mxe, [Order; 8]>, ref_price: i64) -> (i128, i128) {
        let book = book_ctxt.to_arcis();
        let p = ref_price as i128;

        let mut buy_vol: i128 = 0;
        let mut sell_vol: i128 = 0;
        for i in 0..8 {
            let crosses_buy = book[i].is_buy & (book[i].price >= p);
            let crosses_sell = (!book[i].is_buy) & (book[i].price <= p);
            buy_vol += if crosses_buy { book[i].size } else { 0 };
            sell_vol += if crosses_sell { book[i].size } else { 0 };
        }

        let matched = if buy_vol < sell_vol { buy_vol } else { sell_vol };
        let net = buy_vol - sell_vol;
        (matched.reveal(), net.reveal())
    }
}
