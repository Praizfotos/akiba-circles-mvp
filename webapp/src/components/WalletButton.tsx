"use client";

import { useWallet } from "@/app/providers";

function short(addr: string): string {
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}

export default function WalletButton() {
  const { address, connecting, connect, disconnect } = useWallet();

  if (address) {
    return (
      <div className="flex items-center gap-3">
        <span
          className="rounded-full border border-amber-500/40 bg-amber-500/10 px-3 py-1 font-mono text-sm text-amber-200"
          title={address}
        >
          {short(address)}
        </span>
        <button
          onClick={() => disconnect()}
          className="rounded-lg border border-white/15 px-3 py-1.5 text-sm text-white/70 hover:bg-white/5"
        >
          Disconnect
        </button>
      </div>
    );
  }

  return (
    <button
      onClick={() => connect()}
      disabled={connecting}
      className="rounded-lg bg-amber-500 px-4 py-2 text-sm font-semibold text-black hover:bg-amber-400 disabled:opacity-50"
    >
      {connecting ? "Connecting…" : "Connect wallet"}
    </button>
  );
}
