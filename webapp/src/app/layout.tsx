import type { Metadata } from "next";
import Link from "next/link";
import "./globals.css";
import { WalletProvider } from "./providers";
import WalletButton from "@/components/WalletButton";

export const metadata: Metadata = {
  title: "Akiba Circles — on-chain rotating savings",
  description:
    "Rotating savings circles (esusu / tontine) on Stellar Soroban. Contributions held by code, not a collector.",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className="dark">
      <body className="min-h-screen bg-[color:var(--b1)] text-[color:var(--fg)] antialiased">
        <WalletProvider>
          <header className="border-y border-white/10">
            <div className="mx-auto max-w-5xl flex items-center justify-between px-4 py-3">
              <Link href="/" className="text-lg font-bold tracking-tight">
                Akiba <span className="text-amber-400">Circles</span>
              </Link>
              <nav className="flex items-center gap-6 text-sm text-white/70">
                <Link href="/circles" className="hover:text-white">Circles</Link>
                <Link href="/circles/new" className="hover:text-white">Create</Link>
                <WalletButton />
              </nav>
            </div>
          </header>
          <main className="mx-auto max-w-5xl px-4 py-8">{children}</main>
          <footer className="border-t border-white/10 py-6 text-center text-xs text-white/40">
            Akiba Circles — built on Stellar Soroban. Not affiliated with or endorsed by SDF. Testnet by default.
          </footer>
        </WalletProvider>
      </body>
    </html>
  );
}