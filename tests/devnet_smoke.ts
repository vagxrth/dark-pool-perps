/**
 * Devnet smoke test for the deployed darkpool_perps program.
 *
 * Sends a real `init_admin_oracle` (or `push_admin_price` if it already exists) to the live
 * program on devnet and reads the value back — proving the deployed bytecode accepts and
 * executes transactions and persists state. Instructions are built by hand (Anchor
 * discriminator + borsh args) so no on-chain IDL is required.
 *
 * Run: pnpm exec ts-mocha -p ./tsconfig.json -t 1000000 tests/devnet_smoke.ts
 */
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { createHash } from "crypto";
import * as fs from "fs";
import * as os from "os";
import { expect } from "chai";

const PROGRAM_ID = new PublicKey("F1b3V2V3dg6YDsfPG6Rc9y769fN4uaZio96st5owXzAr");
const PRICE_PRECISION = 1_000_000n;

function anchorDisc(name: string): Buffer {
  return createHash("sha256").update(`global:${name}`).digest().subarray(0, 8);
}
function i128le(v: bigint): Buffer {
  const b = Buffer.alloc(16);
  const mask = (1n << 64n) - 1n;
  b.writeBigUInt64LE(v & mask, 0);
  b.writeBigUInt64LE((v >> 64n) & mask, 8);
  return b;
}
function readI128le(buf: Buffer, off: number): bigint {
  const lo = buf.readBigUInt64LE(off);
  const hi = buf.readBigInt64LE(off + 8);
  return (hi << 64n) | lo;
}
function loadKeypair(): Keypair {
  const path = `${os.homedir()}/.config/solana/id.json`;
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8"))));
}

describe("devnet smoke", () => {
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const authority = loadKeypair();
  const [adminOracle] = PublicKey.findProgramAddressSync(
    [Buffer.from("admin_oracle"), authority.publicKey.toBuffer()],
    PROGRAM_ID
  );

  it("init/update admin oracle on the deployed program", async () => {
    const price = 150n * PRICE_PRECISION;
    const conf = 0n;

    const existing = await connection.getAccountInfo(adminOracle);
    let data: Buffer;
    let keys;
    if (existing === null) {
      // init_admin_oracle(price: i128, conf: u64)
      data = Buffer.concat([anchorDisc("init_admin_oracle"), i128le(price), (() => {
        const b = Buffer.alloc(8);
        b.writeBigUInt64LE(conf);
        return b;
      })()]);
      keys = [
        { pubkey: authority.publicKey, isSigner: true, isWritable: true },
        { pubkey: adminOracle, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ];
    } else {
      // push_admin_price(price: i128, conf: u64)
      data = Buffer.concat([anchorDisc("push_admin_price"), i128le(price), (() => {
        const b = Buffer.alloc(8);
        b.writeBigUInt64LE(conf);
        return b;
      })()]);
      keys = [
        { pubkey: authority.publicKey, isSigner: true, isWritable: false },
        { pubkey: adminOracle, isSigner: false, isWritable: true },
      ];
    }

    const ix = new TransactionInstruction({ programId: PROGRAM_ID, keys, data });
    const sig = await sendAndConfirmTransaction(connection, new Transaction().add(ix), [
      authority,
    ]);
    console.log("  tx:", `https://explorer.solana.com/tx/${sig}?cluster=devnet`);
    console.log("  admin oracle:", adminOracle.toBase58());

    // Read back: 8 disc + 32 authority => price at offset 40 (i128).
    const acc = await connection.getAccountInfo(adminOracle, "confirmed");
    expect(acc).to.not.be.null;
    const onchainPrice = readI128le(acc!.data, 40);
    console.log("  on-chain price:", onchainPrice.toString());
    expect(onchainPrice).to.equal(price);
  });
});
