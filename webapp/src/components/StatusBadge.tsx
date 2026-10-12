import type { CircleStatus } from "@/lib/circles";

const STYLES: Record<CircleStatus, string> = {
  Open: "bg-sky-500/15 text-sky-300 border-sky-500/30",
  Active: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  Completed: "bg-white/10 text-white/60 border-white/15",
  Cancelled: "bg-red-500/15 text-red-300 border-red-500/30",
};

export default function StatusBadge({ status }: { status: CircleStatus }) {
  return (
    <span
      className={`inline-block rounded-full border px-2.5 py-0.5 text-xs font-medium ${STYLES[status] ?? STYLES.Completed}`}
    >
      {status}
    </span>
  );
}
