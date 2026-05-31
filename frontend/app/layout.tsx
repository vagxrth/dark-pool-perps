import type { Metadata } from "next";
import { Anton, JetBrains_Mono, Instrument_Serif, Archivo } from "next/font/google";
import "./globals.css";
import { Providers } from "./providers";

const anton = Anton({
  weight: "400",
  subsets: ["latin"],
  variable: "--font-display",
});
const archivo = Archivo({
  subsets: ["latin"],
  variable: "--font-ui",
});
const mono = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-mono",
});
const serif = Instrument_Serif({
  weight: "400",
  style: ["italic", "normal"],
  subsets: ["latin"],
  variable: "--font-serif",
});

export const metadata: Metadata = {
  title: "DARK POOL — confidential perps on Solana",
  description:
    "A confidential perpetual-futures DEX. Positions are encrypted on-chain via Arcium MPC; margin and liquidation are decided inside the MPC. The order book is dark.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body
        className={`${anton.variable} ${archivo.variable} ${mono.variable} ${serif.variable}`}
      >
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
