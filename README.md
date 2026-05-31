# Dark-Pool Perps 🌑

A **confidential ("dark-pool") perpetual-futures DEX** on Solana. Position sizes,
collateral, entry prices, and orders are **encrypted on-chain** via [Arcium](https://arcium.com)
MPC; margin and liquidation are decided **inside the MPC** over that encrypted state.
A public, Pyth-anchored vAMM absorbs residual flow.

> On every Solana perps DEX today your position and liquidation price are public, so
> whales get liquidation-hunted and traders get copy-traded. This keeps them private.

Built as a Solana India Fellowship capstone.

## What works today

- **Public perps engine** — vAMM (constant-product), cost-basis positions, funding,
  margin/liquidation, Pyth oracle (manual `PriceUpdateV2` parse). Unit + integration tested; devnet-deployed.
- **Confidential layer (Arcium MPC)** — `init_position`, `update_position`,
  `check_liquidation` over `Enc<Mxe>` state. **Verified end-to-end on a live MPC cluster**
  (position is ciphertext on-chain; only the liquidation boolean is revealed).
- **Confidential dark-pool matching** — encrypted `OrderPool`, `submit_order`, and a
  branchless uniform-price batch auction (`match_batch`) routing net flow to the vAMM.
- **TypeScript SDK + demo** — `app/sdk.ts` (`DarkpoolClient`) and a proof-of-confidentiality
  demo (`app/demo.ts`).
- **Next.js frontend** — `frontend/`: an interactive "proof of confidentiality" UI that shows
  the on-chain `Enc<Mxe>` ciphertext beside the trader's decrypted view, with a live MPC
  liquidation verdict that flips as the oracle crosses the liquidation price. `cd frontend && npm i && npm run dev`.
  It also has a **Live on Solana devnet** panel wired to the *deployed* program via Solana
  wallet-adapter: connect Phantom/Solflare (set to **Devnet**), read the live oracle mark
  price, and create your on-chain user account with a real signed `init_user` transaction.
  (The confidential MPC panel stays a labeled visual demo — it needs a live Arcium cluster.)

## Live on devnet

| Account | Address |
|---|---|
| Program | `F1b3V2V3dg6YDsfPG6Rc9y769fN4uaZio96st5owXzAr` |
| SOL-PERP market (index 0) | `8xZbR3KxHsVs2LFgSAsQ5DMPEzYbMGJjZ5enDB9TasL1` |
| Admin oracle | `B6QJnZdBJx127gkDk45NDT1QQb85AMgQSvBXUNjFxyaC` |

The market was created with `node --import tsx scripts/setup_devnet.mts` (one-time admin setup).

## Quickstart

```bash
arcium build          # compile circuits + program + IDL
arcium test           # spin up the 2-node MPC localnet and run the confidential e2e
pnpm run typecheck    # type-check the SDK + demo + tests
```

If `arcium test` reports `Failed to fetch MXE public key`, the persisted localnet
node keys are stale — move them aside so a fresh keyset is generated:
`mv artifacts/localnet artifacts/private_shares_node_* artifacts/public_inputs_node_* /tmp/`.
After one good run, `arcium test --skip-keygen` reuses the cached keys.

## SDK usage

```ts
import { DarkpoolClient } from "./app/sdk";

const client = DarkpoolClient.fromEnv();
await client.connectEncryption();              // x25519 + RescueCipher with the MXE
await client.initConfidentialPosition({ collateral: 80, base: 0, quoteEntry: 0 });
await client.openConfidentialTrade(5, 150);    // 5-unit long @ $150, encrypted
const liquidatable = await client.checkLiquidation();   // decided in MPC
const ciphertext  = await client.fetchOnChainCiphertext(); // what the chain sees
```

## Layout

| Path | Purpose |
|------|---------|
| `programs/darkpool_perps/` | Anchor program: public engine + Arcium queue/callbacks |
| `encrypted-ixs/` | Arcis confidential circuits (positions, liquidation, matching) |
| `app/sdk.ts` | `DarkpoolClient` — encryption + PDAs + MPC orchestration |
| `app/demo.ts` | "Proof of confidentiality" demo (ciphertext vs plaintext) |
| `tests/confidential.ts` | End-to-end MPC test (the verified flow) |
| `tests/integration.rs` | LiteSVM tests for the public engine |
| `docs/confidential-design.md` | Confidential architecture + threat model |
| `Arcium.toml` | Localnet / cluster configuration |

## Docs

- Arcium developers: <https://docs.arcium.com/developers>
