/**
 * Dark-Pool Perps — "Proof of Confidentiality" demo (Phase 4)
 *
 * The capstone money-shot, as a script. It drives the confidential flow through
 * the SDK and prints, side by side:
 *
 *   • what the TRADER knows locally (the plaintext position), versus
 *   • what the CHAIN stores (raw ciphertext bytes — what any explorer/RPC sees),
 *   • plus the only thing the MPC ever reveals: the liquidation verdict.
 *
 * It then crashes the oracle and re-checks, showing the verdict flip — all while
 * collateral / size / entry remain encrypted at rest.
 *
 * Run inside the MPC localnet (which injects ARCIUM_CLUSTER_OFFSET):
 *   arcium test --test-name demo            # (symlink/point a test at this), or
 *   node --import tsx app/demo.ts           # against an already-running localnet
 */
import * as anchor from "@anchor-lang/core";
import { createMint } from "@solana/spl-token";
import { DarkpoolClient, PRICE_PRECISION, QUOTE_PRECISION, BASE_PRECISION } from "./sdk";

const hex = (bytes: number[]) =>
  bytes.map((b) => b.toString(16).padStart(2, "0")).join("");

async function main() {
  console.log("\n🌑  DARK-POOL PERPS — proof of confidentiality\n" + "═".repeat(52));

  const client = DarkpoolClient.fromEnv();
  const market = client.marketPda(0);

  // ---- one-time MPC computation-definition setup ----
  console.log("• uploading confidential circuits (init/update/check)…");
  await client.initCompDef("init_position", () => client.program.methods.initPositionCompDef());
  await client.initCompDef("update_position", () => client.program.methods.updatePositionCompDef());
  await client.initCompDef("check_liquidation", () => client.program.methods.checkLiquidationCompDef());

  // ---- market + admin oracle @ $150 ----
  console.log("• creating SOL-PERP market + oracle @ $150…");
  const mint = await createMint(
    client.provider.connection,
    (client.provider.wallet as anchor.Wallet).payer,
    client.authority,
    null,
    6
  );
  await client.program.methods
    .initAdminOracle(new anchor.BN((150n * PRICE_PRECISION).toString()), new anchor.BN(0))
    .accountsPartial({ authority: client.authority, adminOracle: client.adminOraclePda() })
    .rpc({ commitment: "confirmed" });
  await client.program.methods
    .initMarket(
      0,
      new anchor.BN((10_000n * BASE_PRECISION).toString()),
      new anchor.BN((1_500_000n * QUOTE_PRECISION).toString()),
      client.adminOraclePda(),
      { admin: {} },
      500,
      1000,
      250
    )
    .accountsPartial({
      authority: client.authority,
      market,
      collateralMint: mint,
      vault: client.vaultPda(market),
    })
    .rpc({ commitment: "confirmed" });

  // ---- encryption handshake with the MXE cluster ----
  console.log("• x25519 handshake + RescueCipher with the MXE cluster…\n");
  await client.connectEncryption();

  // ---- the trader's PLAINTEXT intent (never sent in the clear) ----
  const plain = { collateral: 80, base: 0, quoteEntry: 0 }; // start flat, $80 margin
  await client.initConfidentialPosition(plain);

  const trade = { baseDelta: 5, fillPrice: 150 }; // 5-unit long @ $150 (~9.4x)
  await client.openConfidentialTrade(trade.baseDelta, trade.fillPrice);
  const finalPlain = {
    collateral: plain.collateral,
    base: trade.baseDelta,
    quoteEntry: trade.baseDelta * trade.fillPrice,
  };

  // ---- the side-by-side proof ----
  const ct = await client.fetchOnChainCiphertext();
  console.log("┌─ WHAT THE TRADER KNOWS (client-side plaintext) " + "─".repeat(4));
  console.log(`│   collateral : $${finalPlain.collateral}`);
  console.log(`│   position   : ${finalPlain.base > 0 ? "+" : ""}${finalPlain.base} SOL (long)`);
  console.log(`│   cost basis : $${finalPlain.quoteEntry}`);
  console.log("├─ WHAT THE CHAIN STORES (Enc<Mxe> ciphertext) " + "─".repeat(6));
  console.log(`│   collateral : 0x${hex(ct[0]).slice(0, 48)}…`);
  console.log(`│   base       : 0x${hex(ct[1]).slice(0, 48)}…`);
  console.log(`│   quote_entry: 0x${hex(ct[2]).slice(0, 48)}…`);
  console.log("└" + "─".repeat(51));

  // ---- liquidation decided in MPC, at two prices ----
  const safe = await client.checkLiquidation();
  console.log(`\n• MPC margin check @ $150 → liquidatable = ${safe}  (expected false)`);

  console.log("• oracle crashes to $124…");
  await client.program.methods
    .pushAdminPrice(new anchor.BN((124n * PRICE_PRECISION).toString()), new anchor.BN(0))
    .accountsPartial({ authority: client.authority, adminOracle: client.adminOraclePda() })
    .rpc({ commitment: "confirmed" });

  const danger = await client.checkLiquidation();
  console.log(`• MPC margin check @ $124 → liquidatable = ${danger}  (expected true)`);

  console.log(
    "\n✅  The position stayed ciphertext on-chain the whole time; only a single\n" +
      "    boolean ever left the MPC. That's a dark pool.\n"
  );
}

main().then(
  () => process.exit(0),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
