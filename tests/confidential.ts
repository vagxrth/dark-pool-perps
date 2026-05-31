/**
 * Phase 2e — end-to-end confidential liquidation check on the local MPC cluster.
 *
 * Flow: create market + admin oracle ($150) → client encrypts a leveraged Position and stores
 * it as Enc<Mxe> via `init_position` (MPC) → `check_liquidation` (MPC) at $150 reveals
 * NOT-liquidatable → drop the oracle to $124 → `check_liquidation` reveals LIQUIDATABLE.
 * The position (collateral/size/entry) is never exposed on-chain — only the boolean leaks.
 *
 * Run via `arcium test` (spins up the 2-node MPC localnet + validator).
 */
import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { Connection, PublicKey } from "@solana/web3.js";
import { DarkpoolPerps } from "../target/types/darkpool_perps";
import { randomBytes } from "crypto";
import {
  awaitComputationFinalization,
  getArciumEnv,
  getCompDefAccOffset,
  getArciumAccountBaseSeed,
  getArciumProgramId,
  getArciumProgram,
  uploadCircuit,
  RescueCipher,
  deserializeLE,
  getMXEPublicKey,
  getMXEAccAddress,
  getMempoolAccAddress,
  getCompDefAccAddress,
  getExecutingPoolAccAddress,
  getComputationAccAddress,
  getClusterAccAddress,
  getLookupTableAddress,
  x25519,
} from "@arcium-hq/client";
import { createMint } from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";
import { expect } from "chai";

const PRICE = 1_000_000n; // PRICE_PRECISION
const BASE = 1_000_000_000n; // BASE_PRECISION
const QUOTE = 1_000_000n; // QUOTE_PRECISION

describe("Confidential perps (Phase 2)", () => {
  // Build the provider with commitment "confirmed" — AnchorProvider.env() defaults to
  // "processed", whose getLatestBlockhash is rejected by localnet preflight ("Blockhash
  // not found"). "confirmed" matches what the working CLI uses.
  const owner = readKpJson(`${os.homedir()}/.config/solana/id.json`);
  const url = process.env.ANCHOR_PROVIDER_URL || "http://127.0.0.1:8899";
  const provider = new anchor.AnchorProvider(
    new Connection(url, "confirmed"),
    new anchor.Wallet(owner),
    { commitment: "confirmed", preflightCommitment: "confirmed" }
  );
  anchor.setProvider(provider);
  const program = anchor.workspace.DarkpoolPerps as Program<DarkpoolPerps>;
  const arciumProgram = getArciumProgram(provider);
  const arciumEnv = getArciumEnv();
  const clusterAccount = getClusterAccAddress(arciumEnv.arciumClusterOffset);

  const marketIndex = 0;
  const [marketPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("market"), new Uint8Array(new Uint16Array([marketIndex]).buffer)],
    program.programId
  );
  const [vaultPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), marketPda.toBuffer()],
    program.programId
  );
  const [adminOraclePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("admin_oracle"), owner.publicKey.toBuffer()],
    program.programId
  );
  const [confUserPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("conf_user"), marketPda.toBuffer(), owner.publicKey.toBuffer()],
    program.programId
  );

  it("hides a position and decides liquidation in MPC", async () => {
    // ---- comp def setup + circuit upload ----
    await initCompDef("init_position", () => program.methods.initPositionCompDef());
    await initCompDef("check_liquidation", () => program.methods.checkLiquidationCompDef());

    // ---- market + admin oracle @ $150 ----
    const mint = await createMint(provider.connection, owner, owner.publicKey, null, 6);

    await program.methods
      .initAdminOracle(new anchor.BN((150n * PRICE).toString()), new anchor.BN(0))
      .accounts({ authority: owner.publicKey, adminOracle: adminOraclePda })
      .rpc({ commitment: "confirmed" });

    await program.methods
      .initMarket(
        marketIndex,
        new anchor.BN((10_000n * BASE).toString()), // base_reserve
        new anchor.BN((1_500_000n * QUOTE).toString()), // quote_reserve
        adminOraclePda, // oracle
        { admin: {} }, // OracleSource::Admin
        500, // maintenance bps
        1000, // initial bps
        250 // liquidation fee bps
      )
      .accounts({
        authority: owner.publicKey,
        market: marketPda,
        collateralMint: mint,
        vault: vaultPda,
      })
      .rpc({ commitment: "confirmed" });

    // ---- encrypt a leveraged long: $80 collateral, 5 SOL @ $150 (~9.4x) ----
    const mxePublicKey = await getMXEPublicKeyWithRetry(provider, program.programId);
    const priv = x25519.utils.randomSecretKey();
    const pub = x25519.getPublicKey(priv);
    const shared = x25519.getSharedSecret(priv, mxePublicKey);
    const cipher = new RescueCipher(shared);

    const collateral = 80n * QUOTE;
    const base = 5n * BASE;
    const entry = 150n * PRICE;
    const nonce = randomBytes(16);
    const ct = cipher.encrypt([collateral, base, entry], nonce);

    // ---- init_position (store Enc<Mxe, Position>) ----
    await queueAndFinalize((computationOffset) =>
      program.methods
        .initPosition(
          computationOffset,
          Array.from(ct[0]),
          Array.from(ct[1]),
          Array.from(ct[2]),
          Array.from(pub),
          new anchor.BN(deserializeLE(nonce).toString())
        )
        .accountsPartial({
          payer: owner.publicKey,
          ...arciumAccounts(computationOffset, "init_position"),
          market: marketPda,
          confUser: confUserPda,
        })
    );

    let cu = await program.account.confidentialUser.fetch(confUserPda);
    expect(cu.initialized).to.equal(true);
    console.log("  position stored as ciphertext (enc_position[0..8]):", cu.encPosition[0].slice(0, 8));

    // ---- check_liquidation at $150 → NOT liquidatable ----
    await queueAndFinalize((computationOffset) =>
      program.methods.checkLiquidation(computationOffset).accountsPartial({
        payer: owner.publicKey,
        ...arciumAccounts(computationOffset, "check_liquidation"),
        market: marketPda,
        oracle: adminOraclePda,
        confUser: confUserPda,
      })
    );
    cu = await program.account.confidentialUser.fetch(confUserPda);
    console.log("  liquidatable @ $150:", cu.liquidatable);
    expect(cu.liquidatable).to.equal(false);

    // ---- crash the oracle to $124, re-check → LIQUIDATABLE ----
    await program.methods
      .pushAdminPrice(new anchor.BN((124n * PRICE).toString()), new anchor.BN(0))
      .accounts({ authority: owner.publicKey, adminOracle: adminOraclePda })
      .rpc({ commitment: "confirmed" });

    await queueAndFinalize((computationOffset) =>
      program.methods.checkLiquidation(computationOffset).accountsPartial({
        payer: owner.publicKey,
        ...arciumAccounts(computationOffset, "check_liquidation"),
        market: marketPda,
        oracle: adminOraclePda,
        confUser: confUserPda,
      })
    );
    cu = await program.account.confidentialUser.fetch(confUserPda);
    console.log("  liquidatable @ $124:", cu.liquidatable);
    expect(cu.liquidatable).to.equal(true);
  });

  // ---- helpers ----
  function arciumAccounts(computationOffset: anchor.BN, circuit: string) {
    return {
      computationAccount: getComputationAccAddress(arciumEnv.arciumClusterOffset, computationOffset),
      clusterAccount,
      mxeAccount: getMXEAccAddress(program.programId),
      mempoolAccount: getMempoolAccAddress(arciumEnv.arciumClusterOffset),
      executingPool: getExecutingPoolAccAddress(arciumEnv.arciumClusterOffset),
      compDefAccount: getCompDefAccAddress(
        program.programId,
        Buffer.from(getCompDefAccOffset(circuit)).readUInt32LE()
      ),
    };
  }

  async function queueAndFinalize(build: (offset: anchor.BN) => any) {
    const computationOffset = new anchor.BN(randomBytes(8), "hex");
    await build(computationOffset).rpc({ skipPreflight: true, commitment: "confirmed" });
    await awaitComputationFinalization(provider, computationOffset, program.programId, "confirmed");
  }

  async function initCompDef(circuit: string, methodBuilder: () => any): Promise<void> {
    const offset = getCompDefAccOffset(circuit);
    const compDefPDA = PublicKey.findProgramAddressSync(
      [getArciumAccountBaseSeed("ComputationDefinitionAccount"), program.programId.toBuffer(), offset],
      getArciumProgramId()
    )[0];
    const mxeAccount = getMXEAccAddress(program.programId);
    const mxeAcc = await arciumProgram.account.mxeAccount.fetch(mxeAccount);
    const lutAddress = getLookupTableAddress(program.programId, mxeAcc.lutOffsetSlot);
    await methodBuilder()
      .accounts({ compDefAccount: compDefPDA, payer: owner.publicKey, mxeAccount, addressLookupTable: lutAddress })
      .signers([owner])
      .rpc({ commitment: "confirmed" });
    const rawCircuit = fs.readFileSync(`build/${circuit}.arcis`);
    await uploadCircuit(provider, circuit, program.programId, rawCircuit, true, 500, {
      skipPreflight: true,
      preflightCommitment: "confirmed",
      commitment: "confirmed",
    });
  }
});

async function getMXEPublicKeyWithRetry(
  provider: anchor.AnchorProvider,
  programId: PublicKey,
  maxRetries = 150
): Promise<Uint8Array> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      const k = await getMXEPublicKey(provider, programId);
      if (k) return k;
    } catch (_) {}
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error("Failed to fetch MXE public key");
}

function readKpJson(path: string): anchor.web3.Keypair {
  return anchor.web3.Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(path, "utf-8"))));
}
