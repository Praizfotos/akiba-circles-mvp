"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import CircleCard from "@/components/CircleCard";
import { listCircles, type Circle } from "@/lib/circles";
import { CONFIG } from "@/lib/config";

export default function CirclesPage() {
  const [circles, setCircles] = useState<Circle[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    listCircles(25)
      .then((c) => !cancelled && setCircles(c))
      .catch((e: Error) => !cancelled && setError(e.message))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, []);

  if (!CONFIG.circleContractId) {
    return (
      <div className="rounded-xl border border-amber-500/30 bg-amber-500/10 p-6 text-amber-200">
        Contract not configured. Set{" "}
        <code>NEXT_PUBLIC_SAVINGS_CIRCLE_CONTRACT_ID</code> in{" "}
        <code>webapp/.env.local</code> after deploying to Testnet.
      </div>
    );
  }

  return (
    <div>
      <div className="mb-8 flex items-center justify-between">
        <h1 className="text-2xl font-bold">Circles</h1>
        <Link
          href="/circles/new"
          className="rounded-lg bg-amber-500 px-4 py-2 text-sm font-semibold text-black hover:bg-amber-400"
        >
          + New circle
        </Link>
      </div>

      {loading && <p className="text-white/50">Loading circles…</p>}
      {error && (
        <p className="rounded-lg border border-red-500/30 bg-red-500/10 p-4 text-red-200">
          {error}
        </p>
      )}
      {!loading && !error && circles.length === 0 && (
        <p className="text-white/50">No circles yet. Be the first to create one.</p>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        {circles.map((c) => (
          <CircleCard key={c.id.toString()} circle={c} />
        ))}
      </div>
    </div>
  );
}
