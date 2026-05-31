# Dark-Pool Perps 🌑

> A **confidential ("dark-pool") perpetual-futures DEX** on Solana. Position sizes,
> collateral, entry prices, and orders are **encrypted on-chain** via [Arcium](https://arcium.com)
> MPC; margin and liquidation are decided **inside** the multi-party computation, never revealed.
> A public, Pyth-anchored vAMM absorbs residual flow. **The order book is dark.**

Solana India Fellowship capstone.

| | |
|---|---|
| **Repository** | https://github.com/vagxrth/dark-pool-perps |
| **Network** | Solana **devnet** |
| **Program ID** | `F1b3V2V3dg6YDsfPG6Rc9y769fN4uaZio96st5owXzAr` |
| **Confidential compute** | Arcium MPC (Arcis circuits + MXE) |

---

## 1. The problem & the idea

On every Solana perps DEX today (Drift, Jupiter, Zeta), your **position size, collateral, and
liquidation price are fully public**. The consequences are real and well-documented:

- **Whales get liquidation-hunted** — anyone can see your liquidation price and push the market into it.
- **Traders get copy-traded and front-run** — your size and entry are an open book.

**Dark-Pool Perps** fixes this. It is a perpetual-futures DEX where:

- **Positions, collateral, entry prices, and orders are encrypted on-chain** as Arcium `Enc<Mxe>` ciphertext.
- **Order matching** runs as a **confidential, branchless, uniform-price batch auction inside MPC**.
- **Margin & liquidation checks run inside the MPC** over the encrypted state — only a single
  *liquidatable: bool* is ever revealed; the position itself stays sealed.
- A **public vAMM** (constant-product, Pyth/oracle-anchored) absorbs residual / AMM-fallback flow.

The result: a perps DEX where **your exposure and the order book are genuinely private**, while
settlement remains verifiable on Solana.

---

## 2. Live on devnet

The program is deployed and a market is live on devnet:

| Account | Address |
|---|---|
| Program | `F1b3V2V3dg6YDsfPG6Rc9y769fN4uaZio96st5owXzAr` |
| SOL-PERP market (index 0) | `8xZbR3KxHsVs2LFgSAsQ5DMPEzYbMGJjZ5enDB9TasL1` |
| Admin oracle | `B6QJnZdBJx127gkDk45NDT1QQb85AMgQSvBXUNjFxyaC` |

The Next.js frontend has a **Live on Solana devnet** panel wired to this deployment via Solana
wallet-adapter — connect Phantom/Solflare on devnet, read the live oracle mark price, and create
your on-chain account with a **real signed transaction**.

> _Live hosted demo link + walkthrough video: coming next (hosting/recording step)._

---

## 3. What's implemented

| Layer | Status | Notes |
|---|---|---|
| **Public perps engine** | ✅ Built, tested, **devnet-deployed** | vAMM (constant-product), cost-basis positions, funding, margin/liquidation, manual Pyth `PriceUpdateV2` parse + admin-oracle stand-in |
| **Confidential positions (MPC)** | ✅ **Runtime-verified e2e** on a live 2-node Arcium cluster | `init_position` → `update_position` → `check_liquidation`; position is ciphertext on-chain, only a boolean leaks |
| **Confidential dark-pool matching** | ✅ Built (circuits + program) | encrypted `OrderPool`, `submit_order`, branchless uniform-price batch auction `match_batch` routing net flow to the vAMM |
| **TypeScript SDK** | ✅ | `app/sdk.ts` — `DarkpoolClient` (encryption + PDAs + MPC orchestration) |
| **Proof-of-confidentiality demo** | ✅ | `app/demo.ts` (CLI) + the Next.js frontend |
| **Next.js frontend** | ✅ | Interactive ciphertext-vs-plaintext UI + live devnet wallet panel |
| MagicBlock ephemeral rollups | ⛔ Stretch (not done) | Real-time public path — deferred |

**Arcis circuits** (`encrypted-ixs/src/lib.rs`): `init_position`, `update_position`,
`check_liquidation`, `match_batch`, `init_order_pool`, `submit_order` (+ `add_together` example).

---

## 4. Architecture

```
          ┌──────────────────────── CLIENT (browser / SDK) ────────────────────────┐
          │  x25519 key-exchange with MXE · RescueCipher encrypt(order/size/collat) │
          │  client-side decrypt of own position · wallet-adapter signing           │
          └───────────────┬─────────────────────────────────────────────────────────┘
                          │ submit ENCRYPTED order / position
                          ▼
   ┌──────────────────── ANCHOR PROGRAM (Solana devnet) ─────────────────────┐
   │  PUBLIC state:   Market (vAMM reserves, funding, OI), pooled USDC vault   │
   │  CIPHERTEXT:     ConfidentialUser (Enc<Mxe> position), OrderPool (orders) │
   │  ixs: init_market, deposit/withdraw, open/close (vAMM), settle_funding,   │
   │       liquidate, init_position, update_position, check_liquidation,       │
   │       init_order_pool, submit_order, crank_match  + Arcium callbacks      │
   └───────────┬─────────────────────────────────────────────┬────────────────┘
        queue_computation (CPI)                        read oracle (Pyth/admin)
               │                                               ▲
               ▼                                               │
   ┌──────────────── ARCIUM MXE (MPC, ~1–2s) ───────────┐      │     ┌── ORACLE ──┐
   │  Arcis circuits (fixed-shape, branchless):          │      └─────│ Pyth /     │
   │   • match_batch     (uniform-price batch auction)   │            │ admin push │
   │   • update_position (apply fill to enc state)       │            └────────────┘
   │   • check_liquidation (margin vs oracle, in MPC)    │
   └───────────────────────┬──────────────────────────────┘
              signed callback → program applies fills / liquidations,
                               routes residual to the vAMM
```

**Confidential trade lifecycle:** client encrypts `{side, price, size}` → `submit_order` stores
ciphertext → keeper `crank_match` → MPC runs the branchless batch auction → callback applies
matched fills to encrypted positions and routes residual to the vAMM. A periodic
`check_liquidation` crank runs the margin check in MPC; only the verdict is revealed.

---

## 5. Tech stack (pinned, verified)

| Layer | Choice | Version |
|---|---|---|
| Confidential compute | **Arcium** (Arcis eDSL + MXE) | CLI/arcup `0.10.4` |
| On-chain framework | **Anchor** | `1.0.2` |
| Validator / CLI | **Agave / Solana CLI** | `3.1.10` |
| Rust | toolchain | `1.89.0` (pinned in `rust-toolchain.toml`) |
| Oracle | **Pyth** `PriceUpdateV2` | manual borsh parse (+ admin stand-in for dev) |
| Container runtime | **Docker Desktop** | required for the Arcium MPC localnet |
| Frontend | **Next.js / React** | Next `16`, React `19` |
| Wallet | Solana **wallet-adapter** | Phantom / Solflare |
| Node / pkg mgr | Node `22`, **pnpm** `9` | |

> ⚠️ **Why no Pyth crate:** `pyth-solana-receiver-sdk` pins `anchor-lang 0.32.1`, incompatible
> with Anchor 1.0.2 / Solana 3.x. We deserialize `PriceUpdateV2` ourselves (owner-checked, with
> staleness + confidence guards) and use an admin-pushed oracle as a dev stand-in.

---

## 6. Repository structure

```
dark-pool-perps/
├─ programs/darkpool_perps/      # Anchor program (#[arcium_program])
│  └─ src/
│     ├─ lib.rs                  # all instruction entrypoints + Arcium callbacks
│     ├─ state/                  # market, user, oracle, confidential (Enc<Mxe>) accounts
│     ├─ instructions/           # admin, collateral, trade, funding, liquidate,
│     │                          #   confidential (init/update/check), matching
│     ├─ math.rs                 # i128 fixed-point: vAMM, PnL, margin (division-free)
│     ├─ constants.rs · error.rs
│     └─ tests/integration.rs    # LiteSVM tests for the public engine
├─ encrypted-ixs/src/lib.rs      # Arcis confidential circuits (compile to .arcis)
├─ app/
│  ├─ sdk.ts                     # DarkpoolClient — encryption + PDAs + MPC orchestration
│  └─ demo.ts                    # CLI "proof of confidentiality" demo
├─ frontend/                     # Next.js app (interactive UI + live devnet wallet panel)
│  └─ app/{page,layout,globals.css,providers,LiveDevnet,chain,polyfill}.*
├─ tests/confidential.ts         # end-to-end MPC test (the verified flow)
├─ scripts/setup_devnet.mts      # one-time admin: create mint + market on devnet
├─ docs/confidential-design.md   # confidential architecture + threat model
├─ Anchor.toml · Arcium.toml · Cargo.toml · rust-toolchain.toml
```

---

## 7. Local setup & how to run

### 7.1 Prerequisites (install once)

```bash
# Rust (the repo pins 1.89.0 via rust-toolchain.toml)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup install 1.89.0 && rustup default 1.89.0
rustup update stable        # anchor idl build uses the `stable` toolchain; must be >=1.89

# Solana CLI 3.1.10 (pin explicitly — the `stable` channel installs 4.x, too new)
sh -c "$(curl -sSfL https://release.anza.xyz/v3.1.10/install)"
# add to PATH: ~/.local/share/solana/install/active_release/bin

# Anchor 1.0.2 (via avm built from the matching tag)
cargo install --git https://github.com/coral-xyz/anchor --tag v1.0.2 avm --force --locked
avm install 1.0.2 && avm use 1.0.2

# Arcium toolchain (installs arcup, the arcium CLI, and pulls Docker images)
curl https://install.arcium.com | bash
arcup install            # installs arcium 0.10.4 + arx-node/trusted-dealer images

# Node 22 + pnpm 9, and Docker Desktop (running) for the MPC localnet
```

Verify: `solana --version` → 3.1.10 · `anchor --version` → 1.0.2 · `arcium --version` → 0.10.4 · `rustc --version` → 1.89.0.

### 7.2 Build everything

```bash
git clone https://github.com/vagxrth/dark-pool-perps.git
cd dark-pool-perps
pnpm install
arcium build        # compiles Arcis circuits + the Anchor program + IDL + TS types
```

### 7.3 Run the public-engine unit tests (fast, no Docker)

```bash
cargo test --manifest-path programs/darkpool_perps/Cargo.toml   # LiteSVM: vAMM, margin, funding, liquidation
```

### 7.4 Run the confidential end-to-end test on the MPC localnet (Docker required)

```bash
# Make sure Docker Desktop is running, then:
arcium test         # spins up a 2-node Arcium MPC localnet + validator and runs tests/confidential.ts
```

Expected output:

```
Confidential perps (Phase 2)
  confidential 5-SOL long applied; on-chain is ciphertext: [134, 46, 1, 226, ...]
  liquidatable @ $150: false
  liquidatable @ $124: true
    ✔ hides a position and decides liquidation in MPC
  1 passing
```

> **If you see `Failed to fetch MXE public key`:** stale localnet node keys are blocking MXE key
> agreement. Move them aside so a fresh keyset is generated, then re-run:
> ```bash
> mv artifacts/localnet artifacts/private_shares_node_* artifacts/public_inputs_node_* /tmp/ 2>/dev/null
> arcium test
> ```
> After one good run, `arcium test --skip-keygen` reuses the cached keys (much faster).

### 7.5 Run the frontend

```bash
cd frontend
npm install
npm run dev         # http://localhost:3000
```

The UI works standalone (the confidential reveal is a faithful visual demo). For the **Live on
Solana devnet** panel, connect **Phantom** or **Solflare** set to **Devnet**, use the in-app
*Airdrop 1 SOL* button if needed, then *Create user account* to send a real `init_user` transaction.

### 7.6 (Admin, optional) recreate the devnet market

```bash
node --import tsx scripts/setup_devnet.mts   # creates a collateral mint + SOL-PERP market (index 0)
```

---

## 8. Using the SDK

```ts
import { DarkpoolClient } from "./app/sdk";

const client = DarkpoolClient.fromEnv();
await client.connectEncryption();                                   // x25519 + RescueCipher with the MXE
await client.initConfidentialPosition({ collateral: 80, base: 0, quoteEntry: 0 });
await client.openConfidentialTrade(5, 150);                         // 5-unit long @ $150, encrypted
const liquidatable = await client.checkLiquidation();              // decided in MPC
const ciphertext   = await client.fetchOnChainCiphertext();        // what the chain sees
```

---

## 9. How confidentiality works (the model)

- **Encrypted at the edge:** the client runs an x25519 handshake with the MXE cluster and encrypts
  `{size, price, collateral}` with RescueCipher. Ciphertext is stored on-chain as `Enc<Mxe>`.
- **Computed without decrypting:** `match_batch`, `update_position`, and `check_liquidation` are
  Arcis circuits — fixed-shape and **branchless** (no `Vec`/`while`/early-return; both `if`
  branches execute; no division by encrypted values). They run inside the MPC over the ciphertext.
- **Minimal leakage:** liquidation reveals only a single boolean; matching reveals only aggregate
  cleared volume to the vAMM. Sizes, collateral and entry never decrypt on-chain.

See [`docs/confidential-design.md`](docs/confidential-design.md) for the full design + threat model.

---

## 10. Honest scope & limitations

- **Confidential MPC runs on the Arcium localnet**, where it is verified end-to-end. A browser
  cannot reach a local MPC cluster, and Arcium devnet is permissioned alpha — so the frontend's
  confidential panel is a **labeled visual demo**, while its **Live on devnet** panel performs
  **real** wallet-signed transactions against the deployed public engine.
- `open_position` from the browser needs deposited SPL collateral (a faucet-mint flow); the live
  panel currently does account creation + reads. The public engine fully supports open/close — see
  `tests/integration.rs`.
- MagicBlock ephemeral-rollup integration (real-time public path) is a documented stretch goal, not implemented.

---

## 11. Verification summary

- **Public engine:** `cargo test` (LiteSVM) for vAMM / margin / funding / liquidation; deployed and exercised on devnet.
- **Confidential layer:** `arcium test` runs the full `init_position → update_position →
  check_liquidation` flow on a live 2-node MPC cluster — **1 passing**, with the on-chain position
  confirmed to be ciphertext and the liquidation decided inside the MPC.
- **Frontend:** production build passes (TypeScript strict); the live panel reads `$150.00` from
  the deployed program in-browser and sends real devnet transactions.

---

## License

ISC. Built for the Solana India Fellowship.
