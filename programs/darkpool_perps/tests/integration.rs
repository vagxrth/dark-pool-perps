//! LiteSVM integration tests for the public perps engine (Phase 1e).
//!
//! We seed `Market` / `UserAccount` / `AdminOracle` directly with `set_account` (so we don't
//! need to load the SPL-token program or set up a mint/vault) and exercise the pure on-chain
//! trading / margin / liquidation logic, which moves no tokens. The token flow
//! (init_market / deposit / withdraw) is validated separately via the devnet deploy.

use anchor_lang::prelude::Pubkey;
use anchor_lang::{
    AccountDeserialize, AnchorSerialize, Discriminator, InstructionData, ToAccountMetas,
};
use darkpool_perps::constants::*;
use darkpool_perps::state::{AdminOracle, Market, OracleSource, UserAccount};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;

fn pid() -> Pubkey {
    darkpool_perps::ID
}

fn load_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let so = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/deploy/darkpool_perps.so"
    );
    svm.add_program_from_file(pid(), so).unwrap();
    svm
}

/// Write an Anchor account (8-byte discriminator + borsh body) at `addr`, owned by our program.
fn seed<T: AnchorSerialize + Discriminator>(svm: &mut LiteSVM, addr: Pubkey, acct: &T) {
    let mut data = T::DISCRIMINATOR.to_vec();
    acct.serialize(&mut data).unwrap();
    let lamports = svm.minimum_balance_for_rent_exemption(data.len());
    svm.set_account(
        addr,
        Account {
            lamports,
            data,
            owner: pid(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> bool {
    // Advance the blockhash so otherwise-identical txs (e.g. two liquidate attempts) get
    // distinct signatures and aren't rejected as AlreadyProcessed.
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[payer], bh);
    match svm.send_transaction(tx) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("tx failed: {:?}", e.err);
            for l in &e.meta.logs {
                eprintln!("  log: {l}");
            }
            false
        }
    }
}

fn read_user(svm: &LiteSVM, addr: Pubkey) -> UserAccount {
    UserAccount::try_deserialize(&mut svm.get_account(&addr).unwrap().data.as_slice()).unwrap()
}
fn read_market(svm: &LiteSVM, addr: Pubkey) -> Market {
    Market::try_deserialize(&mut svm.get_account(&addr).unwrap().data.as_slice()).unwrap()
}

/// Far-future timestamp so the oracle/funding staleness check always passes regardless of the
/// litesvm clock (`now - ts <= MAX_ORACLE_STALENESS_SECS` holds when ts is in the future).
const FUTURE_TS: i64 = 4_000_000_000;

struct Env {
    svm: LiteSVM,
    market: Pubkey,
    oracle: Pubkey,
    trader: Keypair,
    user: Pubkey,
}

/// Build a market (SOL-PERP) anchored at $150 with 10k SOL / $1.5M virtual reserves, a funded
/// trader with `collateral_usd` of collateral and a flat position.
fn make_env(collateral_usd: i128) -> Env {
    let mut svm = load_svm();
    let admin = Keypair::new();
    let trader = Keypair::new();
    svm.airdrop(&trader.pubkey(), 10_000_000_000).unwrap();

    let oracle = Pubkey::new_unique();
    seed(
        &mut svm,
        oracle,
        &AdminOracle {
            authority: admin.pubkey(),
            price: 150 * PRICE_PRECISION,
            conf: 0,
            last_update_ts: FUTURE_TS,
            bump: 0,
        },
    );

    let market_index: u16 = 0;
    let (market, mbump) =
        Pubkey::find_program_address(&[MARKET_SEED, &market_index.to_le_bytes()], &pid());
    seed(
        &mut svm,
        market,
        &Market {
            authority: admin.pubkey(),
            collateral_mint: Pubkey::new_unique(),
            vault: Pubkey::new_unique(),
            oracle,
            oracle_source: OracleSource::Admin,
            base_reserve: 10_000 * BASE_PRECISION,
            quote_reserve: 1_500_000 * QUOTE_PRECISION,
            cumulative_funding: 0,
            last_funding_ts: FUTURE_TS,
            total_long_base: 0,
            total_short_base: 0,
            total_collateral: 0,
            maintenance_margin_bps: DEFAULT_MAINTENANCE_MARGIN_BPS as u16,
            initial_margin_bps: DEFAULT_INITIAL_MARGIN_BPS as u16,
            liquidation_fee_bps: DEFAULT_LIQUIDATION_FEE_BPS as u16,
            market_index,
            paused: false,
            bump: mbump,
            vault_bump: 0,
        },
    );

    let (user, ubump) = Pubkey::find_program_address(
        &[USER_SEED, market.as_ref(), trader.pubkey().as_ref()],
        &pid(),
    );
    seed(
        &mut svm,
        user,
        &UserAccount {
            authority: trader.pubkey(),
            market,
            collateral: collateral_usd * QUOTE_PRECISION,
            base_amount: 0,
            entry_price: 0,
            last_cumulative_funding: 0,
            bump: ubump,
        },
    );

    Env {
        svm,
        market,
        oracle,
        trader,
        user,
    }
}

fn open_ix(e: &Env, base_amount: u64, is_long: bool, limit_price: i128) -> Instruction {
    Instruction {
        program_id: pid(),
        accounts: darkpool_perps::accounts::Trade {
            authority: e.trader.pubkey(),
            market: e.market,
            user: e.user,
            oracle: e.oracle,
        }
        .to_account_metas(None),
        data: darkpool_perps::instruction::OpenPosition {
            base_amount,
            is_long,
            limit_price,
        }
        .data(),
    }
}

fn close_ix(e: &Env, limit_price: i128) -> Instruction {
    Instruction {
        program_id: pid(),
        accounts: darkpool_perps::accounts::Trade {
            authority: e.trader.pubkey(),
            market: e.market,
            user: e.user,
            oracle: e.oracle,
        }
        .to_account_metas(None),
        data: darkpool_perps::instruction::ClosePosition { limit_price }.data(),
    }
}

#[test]
fn open_long_updates_state() {
    let mut e = make_env(1_000); // $1000 collateral
    let ix = open_ix(&e, 5 * BASE_PRECISION as u64, true, 160 * PRICE_PRECISION);
    assert!(send(&mut e.svm, &e.trader, ix), "open should succeed");

    let u = read_user(&e.svm, e.user);
    assert_eq!(u.base_amount, 5 * BASE_PRECISION, "5 SOL long");
    // entry ~ $150 (tiny slippage above)
    assert!(
        u.entry_price >= 150 * PRICE_PRECISION && u.entry_price < 151 * PRICE_PRECISION,
        "entry {} near $150",
        u.entry_price
    );
    let m = read_market(&e.svm, e.market);
    assert_eq!(m.total_long_base, 5 * BASE_PRECISION);
    assert_eq!(m.base_reserve, 10_000 * BASE_PRECISION - 5 * BASE_PRECISION);
}

#[test]
fn open_then_close_resets_and_settles() {
    let mut e = make_env(1_000);
    let initial = read_user(&e.svm, e.user).collateral;

    let oix = open_ix(&e, 5 * BASE_PRECISION as u64, true, 160 * PRICE_PRECISION);
    assert!(send(&mut e.svm, &e.trader, oix));
    // close the long (selling): require fill >= $140
    let cix = close_ix(&e, 140 * PRICE_PRECISION);
    assert!(send(&mut e.svm, &e.trader, cix));

    let u = read_user(&e.svm, e.user);
    assert_eq!(u.base_amount, 0, "flat after close");
    assert_eq!(u.entry_price, 0);
    let m = read_market(&e.svm, e.market);
    assert_eq!(m.total_long_base, 0, "OI cleared");
    // round-trip on the vAMM costs a little slippage; collateral within ~$10 of start
    let delta = (u.collateral - initial).abs();
    assert!(delta < 10 * QUOTE_PRECISION, "round-trip delta {} small", delta);
    assert!(u.collateral <= initial, "round trip should not be profitable");
}

#[test]
fn initial_margin_blocks_overleverage() {
    // $60 collateral, try to open $750 notional (5 SOL @ $150). Initial margin 10% = $75 > $60.
    let mut e = make_env(60);
    let oix = open_ix(&e, 5 * BASE_PRECISION as u64, true, 160 * PRICE_PRECISION);
    let ok = send(&mut e.svm, &e.trader, oix);
    assert!(!ok, "open must fail initial-margin check");
    assert_eq!(read_user(&e.svm, e.user).base_amount, 0);
}

#[test]
fn liquidation_triggers_on_oracle_drop() {
    // $80 collateral, open 5 SOL long (~$750 notional, ~9.4x). Meets initial margin ($75).
    let mut e = make_env(80);
    let oix = open_ix(&e, 5 * BASE_PRECISION as u64, true, 160 * PRICE_PRECISION);
    assert!(send(&mut e.svm, &e.trader, oix));

    let liquidator = Keypair::new();
    e.svm.airdrop(&liquidator.pubkey(), 1_000_000_000).unwrap();
    let liq_ix = |e: &Env| Instruction {
        program_id: pid(),
        accounts: darkpool_perps::accounts::Liquidate {
            liquidator: liquidator.pubkey(),
            market: e.market,
            user: e.user,
            oracle: e.oracle,
        }
        .to_account_metas(None),
        data: darkpool_perps::instruction::Liquidate {}.data(),
    };

    // Healthy at $150 -> liquidation must fail.
    let lix = liq_ix(&e);
    assert!(
        !send(&mut e.svm, &liquidator, lix),
        "should not liquidate a healthy account"
    );

    // Drop the oracle to $124: pnl = 5*(124-150) = -$130 -> account value negative -> liquidatable.
    let admin = Keypair::new();
    seed(
        &mut e.svm,
        e.oracle,
        &AdminOracle {
            authority: admin.pubkey(),
            price: 124 * PRICE_PRECISION,
            conf: 0,
            last_update_ts: FUTURE_TS,
            bump: 0,
        },
    );
    let lix2 = liq_ix(&e);
    assert!(
        send(&mut e.svm, &liquidator, lix2),
        "should liquidate an underwater account"
    );

    let u = read_user(&e.svm, e.user);
    assert_eq!(u.base_amount, 0, "position closed by liquidation");
    let m = read_market(&e.svm, e.market);
    assert_eq!(m.total_long_base, 0, "OI cleared after liquidation");
}
