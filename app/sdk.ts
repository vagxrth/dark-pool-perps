/**
 * Dark-Pool Perps — TypeScript SDK (Phase 4)
 *
 * A thin, typed client over the confidential perps program. It bundles the three
 * things a confidential dApp needs and that are otherwise easy to get wrong:
 *
 *   1. Arcium client encryption  — x25519 key exchange with the MXE cluster +
 *      RescueCipher, so order/position inputs are ciphertext before they ever
 *      leave the browser.
 *   2. PDA derivation            — market / vault / admin-oracle / conf-user /
 *      order-pool addresses.
 *   3. Computation orchestration — queue an MPC computation and await its
 *      on-chain finalization (the ~1–2s Arcium round-trip).
 *
 * The flow here is the exact one verified end-to-end on the local MPC cluster in
 * `tests/confidential.ts` ("1 passing"); this just packages it for reuse by the
 * demo script and a frontend.
 */
import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { Connection, PublicKey, Keypair } from "@solana/web3.js";
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
import type { DarkpoolPerps } from "../target/types/darkpool_perps";
import * as fs from "fs";

// ---- fixed-point precision (mirrors programs/.../constants.rs) ----
export const PRICE_PRECISION = 1_000_000n;
export const BASE_PRECISION = 1_000_000_000n;
export const QUOTE_PRECISION = 1_000_000n;

/** A trader's confidential position, in human units, as held client-side. */
export interface PlainPosition {
  /** Collateral in USDC. */
  collateral: number;
  /** Signed base size (+long / -short), in base asset units. */
  base: number;
  /** Quote cost-basis (Σ base_delta · fill_price), in USDC. */
  quoteEntry: number;
}

export interface DarkpoolClientOpts {
  /** RPC endpoint. Defaults to ANCHOR_PROVIDER_URL or localnet. */
  url?: string;
  /** Signer. Defaults to the local solana CLI keypair at ~/.config/solana/id.json. */
  wallet?: Keypair;
  /** Program id. Defaults to the address baked into the IDL. */
  programId?: PublicKey;
}

/**
 * High-level client for the confidential perps program.
 *
 * Usage:
 *   const client = DarkpoolClient.fromEnv();
 *   await client.connectEncryption();          // x25519 handshake with the MXE
 *   await client.openConfidentialTrade(5, 150); // 5-unit long @ $150, encrypted
 *   const verdict = await client.checkLiquidation();
 */
export class DarkpoolClient {
  readonly provider: anchor.AnchorProvider;
  readonly program: Program<DarkpoolPerps>;
  private readonly arciumProgram: ReturnType<typeof getArciumProgram>;
  private readonly arciumEnv: ReturnType<typeof getArciumEnv>;
  private readonly clusterAccount: PublicKey;

  // encryption identity (populated by connectEncryption())
  private x25519Priv?: Uint8Array;
  private x25519Pub?: Uint8Array;
  private cipher?: RescueCipher;

  constructor(
    provider: anchor.AnchorProvider,
    program: Program<DarkpoolPerps>
  ) {
    this.provider = provider;
    this.program = program;
    this.arciumProgram = getArciumProgram(provider);
    this.arciumEnv = getArciumEnv();
    this.clusterAccount = getClusterAccAddress(this.arciumEnv.arciumClusterOffset);
  }

  /** Construct from env/local keypair, loading the IDL from target/. */
  static fromEnv(opts: DarkpoolClientOpts = {}): DarkpoolClient {
    const wallet =
      opts.wallet ??
      Keypair.fromSecretKey(
        new Uint8Array(
          JSON.parse(
            fs.readFileSync(`${process.env.HOME}/.config/solana/id.json`, "utf-8")
          )
        )
      );
    const url = opts.url ?? process.env.ANCHOR_PROVIDER_URL ?? "http://127.0.0.1:8899";
    const provider = new anchor.AnchorProvider(
      new Connection(url, "confirmed"),
      new anchor.Wallet(wallet),
      { commitment: "confirmed", preflightCommitment: "confirmed" }
    );
    anchor.setProvider(provider);
    const program = anchor.workspace.DarkpoolPerps as Program<DarkpoolPerps>;
    return new DarkpoolClient(provider, program);
  }

  get authority(): PublicKey {
    return this.provider.wallet.publicKey;
  }

  // ===================== PDAs =====================

  marketPda(marketIndex = 0): PublicKey {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("market"), new Uint8Array(new Uint16Array([marketIndex]).buffer)],
      this.program.programId
    )[0];
  }
  vaultPda(market: PublicKey): PublicKey {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), market.toBuffer()],
      this.program.programId
    )[0];
  }
  adminOraclePda(authority = this.authority): PublicKey {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("admin_oracle"), authority.toBuffer()],
      this.program.programId
    )[0];
  }
  confUserPda(market: PublicKey, authority = this.authority): PublicKey {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("conf_user"), market.toBuffer(), authority.toBuffer()],
      this.program.programId
    )[0];
  }
  orderPoolPda(market: PublicKey): PublicKey {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("order_pool"), market.toBuffer()],
      this.program.programId
    )[0];
  }

  // ===================== Encryption handshake =====================

  /**
   * Perform the x25519 key exchange with the MXE cluster and build the
   * RescueCipher used to encrypt all confidential inputs. Retries while the
   * cluster publishes its public key (localnet keygen can take a few seconds).
   */
  async connectEncryption(maxRetries = 150): Promise<void> {
    const mxePublicKey = await this.getMXEPublicKeyWithRetry(maxRetries);
    this.x25519Priv = x25519.utils.randomSecretKey();
    this.x25519Pub = x25519.getPublicKey(this.x25519Priv);
    const shared = x25519.getSharedSecret(this.x25519Priv, mxePublicKey);
    this.cipher = new RescueCipher(shared);
  }

  private requireCipher(): { cipher: RescueCipher; pub: Uint8Array } {
    if (!this.cipher || !this.x25519Pub) {
      throw new Error("call connectEncryption() before submitting confidential inputs");
    }
    return { cipher: this.cipher, pub: this.x25519Pub };
  }

  // ===================== Confidential operations =====================

  /**
   * Encrypt and store the initial confidential position (Enc<Mxe>). Pass a flat
   * position (e.g. just collateral) and open exposure later via openConfidentialTrade.
   */
  async initConfidentialPosition(pos: PlainPosition, marketIndex = 0): Promise<void> {
    const { cipher, pub } = this.requireCipher();
    const market = this.marketPda(marketIndex);
    const nonce = randomBytes(16);
    const ct = cipher.encrypt(
      [
        BigInt(Math.round(pos.collateral * Number(QUOTE_PRECISION))),
        BigInt(Math.round(pos.base * Number(BASE_PRECISION))),
        BigInt(Math.round(pos.quoteEntry * Number(QUOTE_PRECISION))),
      ],
      nonce
    );
    await this.queueAndFinalize((offset) =>
      this.program.methods
        .initPosition(
          offset,
          Array.from(ct[0]),
          Array.from(ct[1]),
          Array.from(ct[2]),
          Array.from(pub),
          new anchor.BN(deserializeLE(nonce).toString())
        )
        .accountsPartial({
          payer: this.authority,
          ...this.arciumAccounts(offset, "init_position"),
          market,
          confUser: this.confUserPda(market),
        })
    );
  }

  /**
   * Open/modify exposure confidentially: encrypts {baseDelta, fillPrice} and runs
   * update_position in MPC (which enforces initial margin over encrypted state).
   * `baseDelta` is signed (+long / -short) in base units; `fillPrice` in USD.
   */
  async openConfidentialTrade(
    baseDelta: number,
    fillPrice: number,
    marketIndex = 0
  ): Promise<void> {
    const { cipher, pub } = this.requireCipher();
    const market = this.marketPda(marketIndex);
    const nonce = randomBytes(16);
    const ct = cipher.encrypt(
      [
        BigInt(Math.round(baseDelta * Number(BASE_PRECISION))),
        BigInt(Math.round(fillPrice * Number(PRICE_PRECISION))),
      ],
      nonce
    );
    await this.queueAndFinalize((offset) =>
      this.program.methods
        .updatePosition(
          offset,
          Array.from(ct[0]),
          Array.from(ct[1]),
          Array.from(pub),
          new anchor.BN(deserializeLE(nonce).toString())
        )
        .accountsPartial({
          payer: this.authority,
          ...this.arciumAccounts(offset, "update_position"),
          market,
          oracle: this.adminOraclePda(),
          confUser: this.confUserPda(market),
        })
    );
  }

  /**
   * Run the confidential margin check in MPC and return the resulting verdict.
   * Only the boolean leaves the MPC — collateral/size/entry stay encrypted.
   */
  async checkLiquidation(marketIndex = 0): Promise<boolean> {
    const market = this.marketPda(marketIndex);
    await this.queueAndFinalize((offset) =>
      this.program.methods.checkLiquidation(offset).accountsPartial({
        payer: this.authority,
        ...this.arciumAccounts(offset, "check_liquidation"),
        market,
        oracle: this.adminOraclePda(),
        confUser: this.confUserPda(market),
      })
    );
    const cu = await this.program.account.confidentialUser.fetch(
      this.confUserPda(market)
    );
    return cu.liquidatable;
  }

  /**
   * Read the raw on-chain ciphertext of the position (Enc<Mxe>). This is exactly
   * what any explorer/RPC sees — proof that the position is encrypted at rest.
   * Returns the three 32-byte field ciphertexts [collateral, base, quote_entry].
   */
  async fetchOnChainCiphertext(marketIndex = 0): Promise<number[][]> {
    const cu = await this.program.account.confidentialUser.fetch(
      this.confUserPda(this.marketPda(marketIndex))
    );
    return cu.encPosition.map((c: number[]) => Array.from(c));
  }

  // ===================== Internals =====================

  /** Build the Arcium account set for a queued computation of `circuit`. */
  private arciumAccounts(computationOffset: anchor.BN, circuit: string) {
    return {
      computationAccount: getComputationAccAddress(
        this.arciumEnv.arciumClusterOffset,
        computationOffset
      ),
      clusterAccount: this.clusterAccount,
      mxeAccount: getMXEAccAddress(this.program.programId),
      mempoolAccount: getMempoolAccAddress(this.arciumEnv.arciumClusterOffset),
      executingPool: getExecutingPoolAccAddress(this.arciumEnv.arciumClusterOffset),
      compDefAccount: getCompDefAccAddress(
        this.program.programId,
        Buffer.from(getCompDefAccOffset(circuit)).readUInt32LE()
      ),
    };
  }

  /** Queue a computation and block until Arcium finalizes it on-chain. */
  private async queueAndFinalize(build: (offset: anchor.BN) => any): Promise<void> {
    const offset = new anchor.BN(randomBytes(8), "hex");
    await build(offset).rpc({ skipPreflight: true, commitment: "confirmed" });
    await awaitComputationFinalization(
      this.provider,
      offset,
      this.program.programId,
      "confirmed"
    );
  }

  /**
   * Initialize the computation-definition account for `circuit` and upload its
   * compiled `.arcis` bytecode. Idempotent setup, run once per circuit per MXE.
   */
  async initCompDef(circuit: string, methodBuilder: () => any): Promise<void> {
    const offset = getCompDefAccOffset(circuit);
    const compDefPDA = PublicKey.findProgramAddressSync(
      [
        getArciumAccountBaseSeed("ComputationDefinitionAccount"),
        this.program.programId.toBuffer(),
        offset,
      ],
      getArciumProgramId()
    )[0];
    const mxeAccount = getMXEAccAddress(this.program.programId);
    const mxeAcc = await this.arciumProgram.account.mxeAccount.fetch(mxeAccount);
    const lutAddress = getLookupTableAddress(
      this.program.programId,
      mxeAcc.lutOffsetSlot
    );
    await methodBuilder()
      .accounts({
        compDefAccount: compDefPDA,
        payer: this.authority,
        mxeAccount,
        addressLookupTable: lutAddress,
      })
      .rpc({ commitment: "confirmed" });
    const rawCircuit = fs.readFileSync(`build/${circuit}.arcis`);
    await uploadCircuit(this.provider, circuit, this.program.programId, rawCircuit, true, 500, {
      skipPreflight: true,
      preflightCommitment: "confirmed",
      commitment: "confirmed",
    });
  }

  private async getMXEPublicKeyWithRetry(maxRetries: number): Promise<Uint8Array> {
    for (let i = 0; i < maxRetries; i++) {
      try {
        const k = await getMXEPublicKey(this.provider, this.program.programId);
        if (k) return k;
      } catch (_) {}
      await new Promise((r) => setTimeout(r, 1000));
    }
    throw new Error("Failed to fetch MXE public key");
  }
}
