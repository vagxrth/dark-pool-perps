/**
 * One-time admin setup: create a collateral mint + SOL-PERP market (index 0) on devnet,
 * so wallet users can run init_user against the live deployed program. Manual instruction
 * encoding (no IDL needed). Run: node --import tsx scripts/setup_devnet.mts
 */
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, SYSVAR_RENT_PUBKEY,
} from "@solana/web3.js";
import { createMint, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { createHash } from "crypto";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey("F1b3V2V3dg6YDsfPG6Rc9y769fN4uaZio96st5owXzAr");
const conn = new Connection("https://api.devnet.solana.com", "confirmed");
const admin = Keypair.fromSecretKey(
  new Uint8Array(JSON.parse(fs.readFileSync(`${os.homedir()}/.config/solana/id.json`, "utf-8")))
);
const disc = (n: string) => createHash("sha256").update(`global:${n}`).digest().subarray(0, 8);
const i128le = (v: bigint) => {
  const b = Buffer.alloc(16); const m = (1n << 64n) - 1n;
  b.writeBigUInt64LE(v & m, 0); b.writeBigUInt64LE((v >> 64n) & m, 8); return b;
};
const u16le = (v: number) => { const b = Buffer.alloc(2); b.writeUInt16LE(v); return b; };

const idx = new Uint8Array(new Uint16Array([0]).buffer);
const [market] = PublicKey.findProgramAddressSync([Buffer.from("market"), idx], PROGRAM_ID);
const [vault] = PublicKey.findProgramAddressSync([Buffer.from("vault"), market.toBuffer()], PROGRAM_ID);
const [oracle] = PublicKey.findProgramAddressSync([Buffer.from("admin_oracle"), admin.publicKey.toBuffer()], PROGRAM_ID);

if (await conn.getAccountInfo(market)) { console.log("market already exists:", market.toBase58()); process.exit(0); }

console.log("creating collateral mint…");
const mint = await createMint(conn, admin, admin.publicKey, null, 6);
console.log("  mint:", mint.toBase58());

const data = Buffer.concat([
  disc("init_market"),
  u16le(0),                              // market_index
  i128le(10_000n * 1_000_000_000n),      // base_reserve  (1e9)
  i128le(1_500_000n * 1_000_000n),       // quote_reserve (1e6) -> $150 mark
  oracle.toBuffer(),                     // oracle
  Buffer.from([0]),                      // OracleSource::Admin
  u16le(500), u16le(1000), u16le(250),   // maintenance / initial / liq-fee bps
]);
const keys = [
  { pubkey: admin.publicKey, isSigner: true, isWritable: true },
  { pubkey: market, isSigner: false, isWritable: true },
  { pubkey: mint, isSigner: false, isWritable: false },
  { pubkey: vault, isSigner: false, isWritable: true },
  { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
];
console.log("sending init_market…");
const sig = await sendAndConfirmTransaction(
  conn, new Transaction().add(new TransactionInstruction({ programId: PROGRAM_ID, keys, data })),
  [admin], { commitment: "confirmed" }
);
console.log("  ✓ market:", market.toBase58());
console.log("  ✓ vault :", vault.toBase58());
console.log("  ✓ tx    :", sig);
