"use client";

import { useCallback, useEffect, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import { LAMPORTS_PER_SOL, Transaction } from "@solana/web3.js";
import {
  EXPLORER,
  EXPLORER_ADDR,
  MARKET,
  buildInitUserIx,
  fetchOraclePrice,
  userExists,
  userPda,
} from "./chain";

type TxState = "idle" | "sending" | "done" | "error";

export function LiveDevnet() {
  const { connection } = useConnection();
  const { publicKey, sendTransaction } = useWallet();
  const [balance, setBalance] = useState<number | null>(null);
  const [oracle, setOracle] = useState<number | null>(null);
  const [hasUser, setHasUser] = useState<boolean | null>(null);
  const [tx, setTx] = useState<TxState>("idle");
  const [sig, setSig] = useState<string>("");
  const [err, setErr] = useState<string>("");
  const [airdropping, setAirdropping] = useState(false);
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  // live reads
  const refresh = useCallback(async () => {
    setOracle(await fetchOraclePrice(connection).catch(() => null));
    if (publicKey) {
      setBalance(
        (await connection.getBalance(publicKey).catch(() => 0)) / LAMPORTS_PER_SOL
      );
      setHasUser(await userExists(connection, publicKey).catch(() => null));
    }
  }, [connection, publicKey]);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 12000);
    return () => clearInterval(t);
  }, [refresh]);

  const airdrop = async () => {
    if (!publicKey) return;
    setAirdropping(true);
    try {
      const s = await connection.requestAirdrop(publicKey, LAMPORTS_PER_SOL);
      await connection.confirmTransaction(s, "confirmed");
      await refresh();
    } catch (e: any) {
      setErr(e?.message ?? "airdrop failed (devnet faucet may be rate-limited)");
    } finally {
      setAirdropping(false);
    }
  };

  const initUser = async () => {
    if (!publicKey) return;
    setTx("sending");
    setErr("");
    try {
      const ix = buildInitUserIx(publicKey);
      const signature = await sendTransaction(new Transaction().add(ix), connection);
      await connection.confirmTransaction(signature, "confirmed");
      setSig(signature);
      setTx("done");
      setHasUser(true);
    } catch (e: any) {
      setErr(e?.message ?? "transaction failed");
      setTx("error");
    }
  };

  return (
    <div className="live">
      <div className="live-head">
        <div>
          <div className="live-kicker">
            <span className="live-dot" /> LIVE · SOLANA DEVNET
          </div>
          <h2 className="live-title">Trade the public engine — for real</h2>
          <p className="live-sub">
            The vAMM, oracle and accounts below are the actual deployed program. Connect a
            devnet wallet and create your on-chain account with a real, signed transaction.
          </p>
        </div>
        {mounted && <WalletMultiButton />}
      </div>

      <div className="live-grid">
        <Stat label="Mark price (oracle)" value={oracle !== null ? `$${oracle.toFixed(2)}` : "—"} live />
        <Stat
          label="Your balance"
          value={publicKey ? (balance !== null ? `${balance.toFixed(3)} SOL` : "…") : "connect"}
        />
        <Stat
          label="User account"
          value={
            !publicKey ? "—" : hasUser === null ? "…" : hasUser ? "created ✓" : "not created"
          }
          cyan={hasUser === true}
        />
        <Stat label="Market" value="SOL-PERP" sub="index 0" />
      </div>

      {publicKey && (
        <div className="live-actions">
          <button className="ghost" onClick={airdrop} disabled={airdropping}>
            {airdropping ? "Airdropping…" : "Airdrop 1 SOL"}
          </button>
          <button
            className="act"
            onClick={initUser}
            disabled={tx === "sending" || hasUser === true}
          >
            {hasUser === true
              ? "User account ready"
              : tx === "sending"
              ? "Confirming on devnet…"
              : "Create user account (real tx)"}
          </button>
          <a
            className="link-out"
            href={EXPLORER_ADDR(userPda(publicKey).toBase58())}
            target="_blank"
            rel="noreferrer"
          >
            view your PDA ↗
          </a>
        </div>
      )}

      <div className="live-status">
        {tx === "done" && sig && (
          <>
            ✓ confirmed —{" "}
            <a href={EXPLORER(sig)} target="_blank" rel="noreferrer">
              {sig.slice(0, 8)}…{sig.slice(-8)} ↗
            </a>
          </>
        )}
        {err && <span className="live-err">⚠ {err}</span>}
        {!publicKey && (
          <span>
            Program{" "}
            <a href={EXPLORER_ADDR(MARKET.toBase58())} target="_blank" rel="noreferrer">
              8xZbR3Kx…TasL1 ↗
            </a>{" "}
            · pick Phantom or Solflare, set it to <b>Devnet</b>.
          </span>
        )}
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  sub,
  live,
  cyan,
}: {
  label: string;
  value: string;
  sub?: string;
  live?: boolean;
  cyan?: boolean;
}) {
  return (
    <div className="live-stat">
      <div className="ls-label">
        {live && <span className="live-dot sm" />}
        {label}
      </div>
      <div className={`ls-value ${cyan ? "cyan" : ""}`}>{value}</div>
      {sub && <div className="ls-sub">{sub}</div>}
    </div>
  );
}
