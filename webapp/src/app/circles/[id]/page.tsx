"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import StatusBadge from "@/components/StatusBadge";
import { formatAmount, getCounter, getFees, type Circle } from "@/lib/circles";
import { getCircle, getMembers, type MemberMeta, rollover, contribute, cancelCircle, claimCollateral } from "@/lib/circles";
import { parseAmount } from "@/lib/circles";
import { useWallet } from "@/app/providers";

export default function CircleDetailPage() {
  const router = useRouter();
  const { address, connect } = useWallet();
  const [circleId, setCircleId] = useState<number | null>(null);
  const [circle, setCircle] = useState<Circle | null>(null);
  const [members, setMembers] = useState<MemberMeta[]>([]);
  const [actions, setActions] = useState<{
    status: string | null;
    error: string | null;
  }>({ status: null, error: null });
  const [pendingTx, setPendingTx] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [isAdmin, setIsAdmin] = useState(false);

  useEffect(() => {
    // parse circle id from pathname, e.g. /circles/123
// @ts-ignore
    const segments = router.pathname.split("/").filter(Boolean);
    const maybe = segments.find((p: string) => /^\d+$/.test(p));
    if (maybe) setCircleId(Number(maybe));
  }, [router.pathname]);

  useEffect(() => {
    if (!circleId || !CONFIG.circleContractId) return;
    let cancelled = false;
    ;(async () => {
      const c = await getCircle(circleId);
      if (!cancelled) setCircle(c);
      const m = await getMembers(circleId);
      if (!cancelled) setMembers(m ?? []);
      const a = await getFees();
      // Simple admin check: if creator is the connected address.
      if (!cancelled && circle && address) setIsAdmin(circle.creator.toLowerCase() === address.toLowerCase());
    })();
    return () => { cancelled = true; };
  }, [circleId]);

  // refresh when circle changes
  useEffect(() => {
    if (!circleId) return;
    ;(async () => {
      const c = await getCircle(circleId);
      if (!cancelled) setCircle(c);
      const m = await getMembers(circleId);
      if (!cancelled) setMembers(m ?? []);
    })();
  }, [circleId]);

  if (!circleId) {
    return (
      <div className="rounded-xl border border-amber-500/30 bg-amber-500/10 p-6 text-amber-200">
        Circle ID not found in URL. Browse circles then select one.
      </div>
    );
  }

  if (!circle) {
    return <p className="text-white/50">Loading circle…</p>;
  }

  // Helper to submit transaction
  async function submitTx<T>(fn: () => Promise<T>): Promise<void> {
    setBusy(true);
    setActions({ status: "Sending transaction…", error: null });
    try {
      await fn();
      setActions({ status: "Done", error: null });
    } catch (err) {
      setActions({ status: null, error: err instanceof Error ? err.message : String(err) });
    } finally {
      setBusy(false);
    }
  }

  // All action wrappers follow: submitTx → invokeContract → setActions
  async function doContribute() {
    if (!address || !circle?.status === "Active") return;
    await submitTx(() => contribute(address, circle.id));
    // refresh
    ;(async () => {
      const c = await getCircle(circle.id);
      if (!cancelled) setCircle(c);
      const m = await getMembers(circle.id);
      if (!cancelled) setMembers(m ?? []);
    })();
  }

  async function doRollover() {
    if (!address || circle.status !== "Active" || circle.active_count < 2) return;
    await submitTx(() => rollover(address, circle.id));
    ;(async () => {
      const c = await getCircle(circle.id);
      if (!cancelled) setCircle(c);
      const m = await getMembers(circle.id);
      if (!cancelled) setMembers(m ?? []);
    })();
  }

  async function doCancel() {
    if (!address || !isAdmin || circle.status !== "Open") return;
    await submitTx(() => cancelCircle(address, circle.id));
    ;(async () => {
      const c = await getCircle(circle.id);
      if (!cancelled) setCircle(c);
      const m = await getMembers(circle.id);
      if (!cancelled) setMembers(m ?? []);
    })();
  }

  async function doClaim() {
    if (!address || circle.status !== "Completed") return;
    await submitTx(() => claimCollateral(address, circle.id));
    ;(async () => {
      const c = await getCircle(circle.id);
      if (!cancelled) setCircle(c);
      const m = await getMembers(circle.id);
      if (!cancelled) setMembers(m ?? []);
    })();
  }

  return (
    <div className="mx-auto max-w-4xl">
      <Link href="/circles" className="mb-4 block text-sm text-amber-300 hover:text-white">
        ← Back to circles
      </Link>

      <div className="grid gap-6">
        {/* Left: circle metadata */}
        <div className="rounded-xl border border-white/10 bg-white/[0.03] p-5">
          <h2 className="font-semibold text-amber-300">{circle.name}</h2>

          <dl className="mt-4 grid grid-cols-2 gap-4 text-sm text-white/50">
            <div>
              <dt className="text-white/40">Status</dt>
              <dd><StatusBadge status={circle.status} /></dd>
            </div>
            <div>
              <dt className="text-white/40">Members</dt>
              <dd>{circle.active_count}/{circle.max_members}</dd>
            </div>
            <div>
              <dt className="text-white/40">Contribution</dt>
              <dd>{formatAmount(circle.contribution)}</dd>
            </div>
            <div>
              <dt className="text-white/40">Cycle</dt>
              <dd>{formatAmount(circle.pot)} (this cycle)</dd>
            </div>
          </dl>
        </div>

        {/* Right: members */}
        <div className="rounded-xl border border-white/10 bg-white/[0.03] p-5">
          <h3 className="font-semibold text-amber-300 mb-3">Members ({circle.active_count} active)</h3>
          {members.map((m) => (
            <div key={`${m.joined_at}-${m.slot}`} className="mt-2 p-2 rounded bg-white/5">
              <span className="font-medium text-white">{`Member #${m.slot + 1}`}</span>
              <br />
              <span className={m.active ? "text-amber-300" : "text-red-400"}>
                {m.active ? "active" : "dropped"}
              </span>
              <br />
              <span className="text-xs text-white/40">
                contributed: {formatAmount(m.total_contributed)}; received:
                {formatAmount(m.total_received)}
              </span>
            </div>
          ))}
        </div>

        {/* Bottom: actions */}
        <div className="mt-6 rounded-xl border border-white/10 bg-white/[0.03] p-5">
          <h4 className="font-medium text-amber-300 mb-3">Actions</h4>
          <div className="space-y-2 text-sm">
            {circle.status === "Open" && circle.creator.toLowerCase() === address?.toLowerCase() && (
              <>
                <button
                  onClick={() => doCancel()}
                  className="rounded-lg border border-red-500/40 bg-red-500/5 px-3 py-1.5 text-xs text-red-300 hover:bg-red-500/10 disabled:opacity-50"
                >
                  Cancel circle & refund all
                </button>
              </>
            )}

            {circle.status === "Open" && circle.creator.toLowerCase() !== address?.toLowerCase() && (
              <button
                onClick={() => router.push("/circles/new")}
                className="rounded-lg border border-amber-500/40 bg-amber-500/5 px-3 py-1.5 text-xs text-amber-300 hover:bg-amber-500/10"
              >
                {address ? "Create your own circle" : "Connect wallet"}
              </button>
            )}

            {circle.status === "Active" && circle.active_count >= 2 && address && (
              <button
                onClick={() => doContribute()}
                className="rounded-lg border border-amber-500/40 bg-amber-500/5 px-3 py-1.5 text-xs text-amber-300 hover:bg-amber-500/10 disabled:opacity-50"
              >
                Contribute {formatAmount(circle.contribution)}
              </button>
            )}

            {circle.status === "Active" && circle.active_count >= 2 && address && (
              <button
                onClick={() => doRollover()}
                className="rounded-lg border border-emerald-500/40 bg-emerald-500/5 px-3 py-1.5 text-xs text-emerald-300 hover:bg-emerald-500/10 disabled:opacity-50"
              >
                Rollover (anyone)
              </button>
            )}

            {circle.status === "Completed" && circle.creator.toLowerCase() === address?.toLowerCase() && (
              <button
                onClick={() => doClaim()}
                className="rounded-lg border border-green-500/40 bg-green-500/5 px-3 py-1.5 text-xs text-green-300 hover:bg-green-500/10 disabled:opacity-50"
              >
                Claim remaining collateral
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}