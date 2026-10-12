import Link from "next/link";
import { CONFIG } from "@/lib/config";

export default function Home() {
  return (
    <div className="py-16">
      <section className="text-center">
        <h1 className="mx-auto max-w-3xl text-4xl font-bold tracking-tight sm:text-5xl">
          Save together.{" "}
          <span className="text-amber-400">Get paid on time.</span>
        </h1>
        <p className="mx-auto mt-6 max-w-xl text-lg text-white/60">
          Akiba Circles is an on-chain rotating savings circle (esusu, tontine,
          stokvel). Contributions are held by a Soroban contract, payouts rotate
          deterministically, and defectors lose their collateral — not your
          money.
        </p>
        <div className="mt-10 flex items-center justify-center gap-4">
          <Link
            href="/circles"
            className="rounded-lg bg-amber-500 px-6 py-3 font-semibold text-black hover:bg-amber-400"
          >
            Browse circles
          </Link>
          <Link
            href="/circles/new"
            className="rounded-lg border border-white/20 px-6 py-3 font-semibold text-white hover:bg-white/5"
          >
            Start a circle
          </Link>
        </div>
      </section>

      <section className="mt-20 grid gap-6 sm:grid-cols-3">
        {[
          {
            title: "No collector to trust",
            body: "The contract holds every contribution. Nobody — not even the circle creator — can move funds outside the rules.",
          },
          {
            title: "Rotation enforced on-chain",
            body: "Each cycle one member receives the full pot, in join order. Miss a contribution and you are dropped; your collateral stays with the members who paid.",
          },
          {
            title: "Built on Stellar",
            body: "Soroban smart contract, USDC-denominated, pennies per transaction. Connect Freighter and join from anywhere in Africa.",
          },
        ].map((f) => (
          <div
            key={f.title}
            className="rounded-xl border border-white/10 bg-white/[0.03] p-6"
          >
            <h3 className="font-semibold text-amber-300">{f.title}</h3>
            <p className="mt-2 text-sm text-white/60">{f.body}</p>
          </div>
        ))}
      </section>

      <section className="mt-16 rounded-xl border border-white/10 bg-white/[0.02] p-6 text-sm text-white/50">
        <p>
          Contract:{" "}
          <code className="text-white/80">
            {CONFIG.circleContractId || "(not configured)"}
          </code>{" "}
          · Network: <code className="text-white/80">{CONFIG.network}</code>
        </p>
      </section>
    </div>
  );
}
