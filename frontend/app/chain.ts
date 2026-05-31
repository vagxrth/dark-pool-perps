/**
 * On-chain wiring for the LIVE (public) path — talks to the deployed
 * darkpool_perps program on Solana devnet. Instructions are hand-encoded
 * (Anchor discriminator + borsh) so the browser needs no IDL/Anchor runtime.
 *
 * The confidential MPC path is NOT here — it needs a live Arcium cluster and
 * stays as the labeled visual demo. This module is only the public engine:
 * read the live oracle/market, and let a connected wallet create its user PDA.
 */
import {
  Connection,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";

export const DEVNET_RPC = "https://api.devnet.solana.com";
export const EXPLORER = (sig: string) =>
  `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
export const EXPLORER_ADDR = (addr: string) =>
  `https://explorer.solana.com/address/${addr}?cluster=devnet`;

export const PROGRAM_ID = new PublicKey(
  "F1b3V2V3dg6YDsfPG6Rc9y769fN4uaZio96st5owXzAr"
);
/** SOL-PERP market (index 0), created on devnet by scripts/setup_devnet.mts. */
export const MARKET = new PublicKey(
  "8xZbR3KxHsVs2LFgSAsQ5DMPEzYbMGJjZ5enDB9TasL1"
);
/** Admin-oracle stand-in (live on devnet). */
export const ADMIN_ORACLE = new PublicKey(
  "B6QJnZdBJx127gkDk45NDT1QQb85AMgQSvBXUNjFxyaC"
);

const INIT_USER_DISC = Uint8Array.from([14, 51, 68, 159, 237, 78, 158, 102]);
const PRICE_PRECISION = 1_000_000n;

/** The connected wallet's public User PDA: ["user", market, authority]. */
export function userPda(authority: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("user"), MARKET.toBuffer(), authority.toBuffer()],
    PROGRAM_ID
  )[0];
}

/** Read a signed i128 (little-endian) from a buffer. */
function readI128LE(buf: Uint8Array, off: number): bigint {
  let lo = 0n;
  for (let i = 0; i < 8; i++) lo |= BigInt(buf[off + i]) << (8n * BigInt(i));
  let hi = 0n;
  for (let i = 0; i < 8; i++) hi |= BigInt(buf[off + 8 + i]) << (8n * BigInt(i));
  // hi is signed
  if (hi >= 1n << 63n) hi -= 1n << 64n;
  return (hi << 64n) + lo;
}

/**
 * Fetch the live mark price (USD) from the admin-oracle account.
 * Layout: disc(8) + authority(32) + price:i128(16) ... → price at offset 40.
 */
export async function fetchOraclePrice(conn: Connection): Promise<number | null> {
  const ai = await conn.getAccountInfo(ADMIN_ORACLE);
  if (!ai) return null;
  const price = readI128LE(ai.data, 8 + 32);
  return Number(price) / Number(PRICE_PRECISION);
}

/** True if the wallet already has a User account on this market. */
export async function userExists(
  conn: Connection,
  authority: PublicKey
): Promise<boolean> {
  return (await conn.getAccountInfo(userPda(authority))) !== null;
}

/**
 * Build the real `init_user` instruction — creates the connected wallet's
 * public User PDA on the deployed program. No tokens, no args.
 */
export function buildInitUserIx(authority: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: MARKET, isSigner: false, isWritable: false },
      { pubkey: userPda(authority), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from(INIT_USER_DISC),
  });
}
