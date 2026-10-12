import Link from "next/link";
import type { Circle } from "@/lib/circles";
import { formatAmount } from "@/lib/circles";
import StatusBadge from "./StatusBadge";

export default function CircleCard({ circle }: { circle: Circle }) {
  const hours = Number(circle.cycle_seconds) / 3600;
  return (
    <Link
      href={`/circles/${circle.id}`}
      className="block rounded-xl border border-white/10 bg-white/[0.03] p-5 transition hover:border-amber-500/40 hover:bg-white/[0.06]"
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="font-semibold text-white">{circle.name}</h3>
          <p className="mt-1 text-sm text-white/50">
            #{circle.id.toString()} · {circle.active_count}/{circle.max_members} members
          </p>
        </div>
        <StatusBadge status={circle.status} />
      </div>
      <dl className="mt-4 grid grid-cols-3 gap-2 text-sm">
        <div>
          <dt className="text-white/40">Contribution</dt>
          <dd className="font-medium text-white">{formatAmount(circle.contribution)}</dd>
        </div>
        <div>
          <dt className="text-white/40">Cycle</dt>
          <dd className="font-medium text-white">{hours}h</dd>
        </div>
        <div>
          <dt className="text-white/40">Pot</dt>
          <dd className="font-medium text-amber-300">{formatAmount(circle.pot)}</dd>
        </div>
      </dl>
    </Link>
  );
}
