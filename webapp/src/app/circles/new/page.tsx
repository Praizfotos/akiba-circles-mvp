"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { useWallet } from "@/app/providers";
import { createCircle, joinCircle, parseAmount } from "@/lib/circles";

const CYCLE_PRESETS = [
  { label: "Daily", seconds: 86_400 },
  { label: "Weekly", seconds: 604_800 },
  { label: "Bi-weekly", seconds: 1_209_600 },
  { label: "Monthly", seconds: 2_592_000 },
];

export default function NewCirclePage() {
  const { address, connect } = useWallet();
  const router = useRouter();

  const [name, setName] = useState("");
  const [contribution, setContribution] = useState("10");
  const [collateral, setCollateral] = useState("5");
  const [cycleSeconds, setCycleSeconds] = useState(604_800);
  const [maxMembers, setMaxMembers] = useState(5);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!address) {
      await connect();
      return;
    }
    setBusy(true);
    setError(null);
    setStatus("Creating circle… (wallet signature 1/2)");
    try {
      await createCircle(address, {
        name: name.trim(),
        contribution: parseAmount(contribution),
        cycleSeconds,
        maxMembers: Math.max(2, Math.min(100, maxMembers)),
        collateral: parseAmount(collateral || "0"),
      });
      setStatus("Joining as first member… (wallet signature 2/2)");
      // Creator joins immediately so the circle can start once friends join.
      // The new circle id is the current counter — fetch it.
      const { getCounter } = await import("@/lib/circles");
      const id = await getCounter();
      await joinCircle(address, id);
      setStatus("Done! Opening your circle…");
      router.push(`/circles/${id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStatus(null);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto max-w-xl">
      <h1 className="text-2xl font-bold">Create a circle</h1>
      <p className="mt-2 text-sm text-white/50">
        You will sign twice: once to create the circle, once to join it (pays
        the first contribution + your collateral).
      </p>

      <form onSubmit={submit} className="mt-8 space-y-5">
        <label className="block">
          <span className="text-sm text-white/70">Circle name</span>
          <input
            required
            maxLength={64}
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Market traders — Tuesday"
            className="mt-1 w-full rounded-lg border border-white/15 bg-white/5 px-3 py-2 outline-none focus:border-amber-500/60"
          />
        </label>

        <label className="block">
          <span className="text-sm text-white/70">Contribution per cycle (USDC)</span>
          <input
            required
            inputMode="decimal"
            value={contribution}
            onChange={(e) => setContribution(e.target.value)}
            className="mt-1 w-full rounded-lg border border-white/15 bg-white/5 px-3 py-2 outline-none focus:border-amber-500/60"
          />
        </label>

        <label className="block">
          <span className="text-sm text-white/70">
            Collateral (USDC) — forfeited if you miss a cycle
          </span>
          <input
            inputMode="decimal"
            value={collateral}
            onChange={(e) => setCollateral(e.target.value)}
            className="mt-1 w-full rounded-lg border border-white/15 bg-white/5 px-3 py-2 outline-none focus:border-amber-500/60"
          />
        </label>

        <div>
          <span className="text-sm text-white/70">Cycle length</span>
          <div className="mt-2 grid grid-cols-4 gap-2">
            {CYCLE_PRESETS.map((p) => (
              <button
                key={p.seconds}
                type="button"
                onClick={() => setCycleSeconds(p.seconds)}
                className={`rounded-lg border px-2 py-2 text-sm ${
                  cycleSeconds === p.seconds
                    ? "border-amber-500/60 bg-amber-500/15 text-amber-200"
                    : "border-white/15 text-white/70 hover:bg-white/5"
                }`}
              >
                {p.label}
              </button>
            ))}
          </div>
        </div>

        <label className="block">
          <span className="text-sm text-white/70">Max members (2–100)</span>
          <input
            type="number"
            min={2}
            max={100}
            value={maxMembers}
            onChange={(e) => setMaxMembers(Number(e.target.value))}
            className="mt-1 w-full rounded-lg border border-white/15 bg-white/5 px-3 py-2 outline-none focus:border-amber-500/60"
          />
        </label>

        <button
          type="submit"
          disabled={busy}
          className="w-full rounded-lg bg-amber-500 py-3 font-semibold text-black hover:bg-amber-400 disabled:opacity-50"
        >
          {busy ? "Working…" : address ? "Create & join" : "Connect wallet to continue"}
        </button>

        {status && <p className="text-sm text-sky-300">{status}</p>}
        {error && (
          <p className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-200">
            {error}
          </p>
        )}
      </form>
    </div>
  );
}
