// @solana/web3.js expects a global Buffer; the browser has none. Polyfill it
// before any web3 code (PublicKey, etc.) evaluates. Imported first in providers.tsx.
import { Buffer } from "buffer";

if (typeof globalThis.Buffer === "undefined") {
  (globalThis as unknown as { Buffer: typeof Buffer }).Buffer = Buffer;
}
