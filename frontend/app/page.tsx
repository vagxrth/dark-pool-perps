"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

/* ----------------------------------------------------------------
   The trader's PLAINTEXT position (held client-side, never sent raw).
   Matches the verified e2e flow: $80 collateral, 5-unit long @ $150.
   Liquidation (cost-basis, 5% maintenance):
     equity      = collateral + base * (price - entry)
     maintenance = 0.05 * base * price
     liquidatable  ⇔  equity < maintenance
   Solving 80 + 5(p-150) < 0.25p  ⇒  liq price ≈ $141.05
   ---------------------------------------------------------------- */
const COLLATERAL = 80;
const BASE = 5;
const ENTRY = 150;
const MAINT_BPS = 0.05;

const equity = (p: number) => COLLATERAL + BASE * (p - ENTRY);
const maintenance = (p: number) => MAINT_BPS * BASE * p;
const isLiquidatable = (p: number) => equity(p) < maintenance(p);
const LIQ_PRICE = (MAINT_BPS * BASE * 0 + (BASE * ENTRY - COLLATERAL)) / (BASE - MAINT_BPS * BASE); // ≈141.05

const HEXCHARS = "0123456789abcdef";
const randHex = (n: number) =>
  Array.from({ length: n }, () => HEXCHARS[Math.floor(Math.random() * 16)]).join("");

type Phase = "idle" | "encrypting" | "computing" | "live";

export default function Page() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [price, setPrice] = useState(150);
  const [cipher, setCipher] = useState<string[]>(["", "", ""]);
  const [status, setStatus] = useState("awaiting confidential order");
  const [recompute, setRecompute] = useState(false);
  const scrambleRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const liquidatable = isLiquidatable(price);

  // scramble the on-chain ciphertext during the encrypt phase
  useEffect(() => {
    if (phase === "encrypting") {
      scrambleRef.current = setInterval(() => {
        setCipher([randHex(64), randHex(64), randHex(64)]);
      }, 60);
    } else if (scrambleRef.current) {
      clearInterval(scrambleRef.current);
      scrambleRef.current = null;
    }
    return () => {
      if (scrambleRef.current) clearInterval(scrambleRef.current);
    };
  }, [phase]);

  const open = useCallback(async () => {
    setPhase("encrypting");
    setStatus("x25519 handshake · RescueCipher · encrypting position locally");
    await wait(1300);
    // freeze the ciphertext — this is what lives on-chain from now on
    setCipher([randHex(64), randHex(64), randHex(64)]);
    setPhase("computing");
    setStatus("queue_computation → Arcium MPC · proving margin over ciphertext");
    await wait(1700);
    setPhase("live");
    setStatus("position open · order book is dark");
  }, []);

  // brief "recomputing in MPC" flash whenever the price (and thus verdict) moves
  const onPrice = (p: number) => {
    setPrice(p);
    if (phase === "live") {
      setRecompute(true);
      window.clearTimeout((onPrice as any)._t);
      (onPrice as any)._t = window.setTimeout(() => setRecompute(false), 280);
    }
  };

  const reset = () => {
    setPhase("idle");
    setPrice(150);
    setCipher(["", "", ""]);
    setStatus("awaiting confidential order");
  };

  const showPlain = phase === "live";
  const sliderPct = ((price - 100) / (200 - 100)) * 100;

  return (
    <>
      <div className="depth-grid" />
      <div className="shell">
        {/* top bar */}
        <header className="topbar">
          <div className="brand">
            <span className="dot" />
            DARKPOOL&nbsp;//&nbsp;SOL-PERP
          </div>
          <nav>
            <span>CONFIDENTIAL</span>
            <span>ARCIUM&nbsp;MPC</span>
            <span>vAMM</span>
            <span>SOLANA</span>
          </nav>
        </header>

        {/* hero */}
        <section className="hero">
          <div className="eyebrow fade-up">Confidential perpetual futures</div>
          <h1>
            <span className="fade-up" style={{ animationDelay: "0.05s" }}>
              DARK
            </span>
            <span className="pool fade-up" style={{ animationDelay: "0.15s" }}>
              POOL
            </span>
          </h1>
          <p className="lede fade-up" style={{ animationDelay: "0.3s" }}>
            Your size, collateral and entry are <em>encrypted on-chain</em>. Margin and
            liquidation are decided <em>inside</em> a multi-party computation — never revealed.
            No more liquidation-hunting. No more copy-trading. The book is dark.
          </p>

          <div className="metrics">
            <Metric k="Position privacy" v="100%" cyan />
            <Metric k="MPC nodes" v="2-of-2" />
            <Metric k="On-chain leak" v="1 bool" cyan />
            <Metric k="Settlement" v="vAMM" />
          </div>
        </section>

        {/* the reveal */}
        <div className="section-tag">Proof of confidentiality — same position, two views</div>
        <div className="reveal">
          {/* what the chain sees */}
          <div className="panel chain">
            <div className="panel-head">
              <span className="title">On-chain · Enc&lt;Mxe&gt;</span>
              <span className="sub">what any explorer sees</span>
            </div>
            <CipherField label="collateral" hex={cipher[0]} phase={phase} />
            <CipherField label="base size" hex={cipher[1]} phase={phase} />
            <CipherField label="quote entry" hex={cipher[2]} phase={phase} />
          </div>

          {/* what you see */}
          <div className="panel you">
            <div className="panel-head">
              <span className="title">Your view · decrypted</span>
              <span className="sub">viewing key, client-side</span>
            </div>
            <PlainField label="collateral">
              {showPlain ? (
                <span className="plain">
                  ${COLLATERAL}
                  <span className="unit">USDC</span>
                </span>
              ) : (
                <span className="locked">— sealed —</span>
              )}
            </PlainField>
            <PlainField label="position">
              {showPlain ? (
                <span className="plain long">
                  +{BASE}
                  <span className="unit">SOL · long</span>
                </span>
              ) : (
                <span className="locked">— sealed —</span>
              )}
            </PlainField>
            <PlainField label="entry / cost basis">
              {showPlain ? (
                <span className="plain">
                  ${ENTRY}
                  <span className="unit">${BASE * ENTRY} basis</span>
                </span>
              ) : (
                <span className="locked">— sealed —</span>
              )}
            </PlainField>
          </div>
        </div>

        {/* console */}
        <div className="console">
          {/* left: oracle control */}
          <div className="control">
            <div className="label">
              <span>Mark price (oracle)</span>
              <span className="price">${price.toFixed(2)}</span>
            </div>
            <input
              type="range"
              min={100}
              max={200}
              step={1}
              value={price}
              disabled={!showPlain}
              onChange={(e) => onPrice(Number(e.target.value))}
              style={{
                background: `linear-gradient(90deg, var(--phosphor) ${sliderPct}%, var(--line) ${sliderPct}%)`,
              }}
            />
            <div className="scale">
              <span>$100</span>
              <span>$150 entry</span>
              <span>$200</span>
            </div>
            <div className="liq-marker">
              ▲ liquidation price ≈ ${LIQ_PRICE.toFixed(2)} — drag below it to trip the MPC check
            </div>

            <div className="cta">
              {phase === "live" ? (
                <button className="ghost" onClick={reset}>
                  Reset
                </button>
              ) : (
                <button
                  className="act"
                  onClick={open}
                  disabled={phase === "encrypting" || phase === "computing"}
                >
                  {phase === "idle"
                    ? "Open confidential position"
                    : phase === "encrypting"
                    ? "Encrypting…"
                    : "Computing in MPC…"}
                </button>
              )}
            </div>

            <div className="status-line">
              {phase === "encrypting" || phase === "computing" ? (
                <>
                  {status} <span className="blink">█</span>
                </>
              ) : (
                <>›&nbsp;{status}</>
              )}
            </div>
          </div>

          {/* right: MPC verdict */}
          <div
            className={`verdict ${
              !showPlain ? "" : recompute ? "computing" : liquidatable ? "danger" : "safe"
            }`}
          >
            <div className="v-label">MPC margin verdict</div>
            <div className="v-state">
              {!showPlain
                ? "— idle —"
                : recompute
                ? "computing…"
                : liquidatable
                ? "Liquidate"
                : "Healthy"}
            </div>
            <div className="v-note">
              {!showPlain
                ? "Open a position to run check_liquidation in MPC."
                : liquidatable
                ? `equity $${equity(price).toFixed(0)} < maintenance $${maintenance(price).toFixed(
                    0
                  )} — decided without revealing the position.`
                : `equity $${equity(price).toFixed(0)} ≥ maintenance $${maintenance(price).toFixed(
                    0
                  )} — only this boolean ever leaves the MPC.`}
            </div>
          </div>
        </div>

        {/* how it works */}
        <div className="section-tag">How the dark pool works</div>
        <div className="steps">
          <Step
            n="01"
            title="Encrypt at the edge"
            body={
              <>
                The client runs an x25519 handshake with the MXE cluster and encrypts{" "}
                <code>{"{size, price, collateral}"}</code> with RescueCipher. Ciphertext goes
                on-chain as <code>Enc&lt;Mxe&gt;</code>.
              </>
            }
          />
          <Step
            n="02"
            title="Match & settle in MPC"
            body={
              <>
                A branchless uniform-price batch auction (<code>match_batch</code>) clears
                encrypted orders; residual flow hits a public, Pyth-anchored{" "}
                <code>vAMM</code>. Sizes never decrypt.
              </>
            }
          />
          <Step
            n="03"
            title="Liquidate blind"
            body={
              <>
                <code>check_liquidation</code> compares equity to maintenance margin{" "}
                <em>inside</em> the MPC. Only a single boolean is revealed — the position
                stays sealed.
              </>
            }
          />
        </div>

        {/* footer */}
        <footer className="foot">
          <span>
            DARK-POOL PERPS · Arcium 0.10.4 · Anchor 1.0.2 · Solana · Pyth
          </span>
          <a href="https://github.com/vagxrth/dark-pool-perps" target="_blank" rel="noreferrer">
            github.com/vagxrth/dark-pool-perps ↗
          </a>
        </footer>
        <p className="disclaim">
          Interactive visualization of the confidential flow that is verified end-to-end on a
          live 2-node Arcium MPC cluster (init_position → update_position → check_liquidation).
          Ciphertext and MPC latency are reproduced here for the demo; the cryptographic model —
          encrypted at rest, MPC-only reveal — is faithful to the on-chain program.
        </p>
      </div>
    </>
  );
}

/* ------------------------------- bits ------------------------------- */

function Metric({ k, v, cyan }: { k: string; v: string; cyan?: boolean }) {
  return (
    <div className="metric fade-up" style={{ animationDelay: "0.45s" }}>
      <div className="k">{k}</div>
      <div className={`v ${cyan ? "cyan" : ""}`}>{v}</div>
    </div>
  );
}

function CipherField({ label, hex, phase }: { label: string; hex: string; phase: Phase }) {
  const display = hex || "·".repeat(64);
  const grouped = display.replace(/(.{4})/g, "$1 ").trim();
  return (
    <div className="field">
      <div className="label">{label}</div>
      <div className={`cipher ${phase === "encrypting" ? "scrambling" : ""}`}>
        0x{grouped}
      </div>
    </div>
  );
}

function PlainField({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="field">
      <div className="label">{label}</div>
      {children}
    </div>
  );
}

function Step({ n, title, body }: { n: string; title: string; body: React.ReactNode }) {
  return (
    <div className="step">
      <div className="n">{n}</div>
      <h3>{title}</h3>
      <p>{body}</p>
    </div>
  );
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
