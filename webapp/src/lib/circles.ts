/**
 * Typed client for the savings-circle contract.
 *
 * All amounts are raw i128 token units (USDC stroops, 7 decimals). Use
 * `formatAmount`/`parseAmount` at the UI boundary.
 */

import { CONFIG, assertConfigured } from "./config";
import { invokeContract, readContract, sc } from "./soroban";

export type CircleStatus = "Open" | "Active" | "Completed" | "Cancelled";

export interface Circle {
  id: bigint;
  creator: string;
  name: string;
  contribution: bigint;
  collateral: bigint;
  cycle_seconds: bigint;
  max_members: number;
  status: CircleStatus;
  current_cycle: number;
  cycle_deadline: bigint;
  active_count: number;
  created_at: bigint;
  pot: bigint;
}

export interface MemberMeta {
  joined_at: bigint;
  slot: number;
  active: boolean;
  total_contributed: bigint;
  total_received: bigint;
}

/** A ledger timestamp (u64 seconds) as JS Date. */
export function tsToDate(ts: bigint): Date {
  return new Date(Number(ts) * 1000);
}

// ── Amount helpers ────────────────────────────────────────────────────────────

export function formatAmount(raw: bigint, decimals = CONFIG.tokenDecimals): string {
  const neg = raw < 0n;
  const abs = neg ? -raw : raw;
  const base = 10n ** BigInt(decimals);
  const whole = abs / base;
  const frac = abs % base;
  const fracStr = frac.toString().padStart(decimals, "0").replace(/0+$/, "");
  return `${neg ? "-" : ""}${whole}${fracStr ? `.${fracStr}` : ""}`;
}

export function parseAmount(input: string, decimals = CONFIG.tokenDecimals): bigint {
  const trimmed = input.trim();
  if (!/^\d+(\.\d+)?$/.test(trimmed)) {
    throw new Error(`Invalid amount: "${input}"`);
  }
  const [whole, frac = ""] = trimmed.split(".");
  if (frac.length > decimals) {
    throw new Error(`At most ${decimals} decimal places`);
  }
  const fracPadded = frac.padEnd(decimals, "0");
  return BigInt(whole) * 10n ** BigInt(decimals) + BigInt(fracPadded || "0");
}

// ── Reads ─────────────────────────────────────────────────────────────────────

/** Normalize the raw scValToNative shape (arrays/objects vary by SDK version). */
function toCircle(raw: unknown): Circle {
  const r = raw as Record<string, unknown>;
  return {
    id: BigInt(r.id as string | number | bigint),
    creator: String(r.creator),
    name: String(r.name),
    contribution: BigInt(r.contribution as string | number | bigint),
    collateral: BigInt(r.collateral as string | number | bigint),
    cycle_seconds: BigInt(r.cycle_seconds as string | number | bigint),
    max_members: Number(r.max_members),
    status: String(r.status) as CircleStatus,
    current_cycle: Number(r.current_cycle),
    cycle_deadline: BigInt(r.cycle_deadline as string | number | bigint),
    active_count: Number(r.active_count),
    created_at: BigInt(r.created_at as string | number | bigint),
    pot: BigInt(r.pot as string | number | bigint),
  };
}

function toMember(raw: unknown): MemberMeta {
  const r = raw as Record<string, unknown>;
  return {
    joined_at: BigInt(r.joined_at as string | number | bigint),
    slot: Number(r.slot),
    active: Boolean(r.active),
    total_contributed: BigInt(r.total_contributed as string | number | bigint),
    total_received: BigInt(r.total_received as string | number | bigint),
  };
}

export async function getCounter(): Promise<number> {
  assertConfigured();
  const v = await readContract(CONFIG.circleContractId, "get_counter");
  return Number(v as bigint);
}

export async function getCircle(id: number | bigint): Promise<Circle> {
  assertConfigured();
  const raw = await readContract(CONFIG.circleContractId, "get_circle", [sc.u64(id)]);
  return toCircle(raw);
}

export async function getMembers(
  id: number | bigint,
  start = 0,
  limit = 50,
): Promise<string[]> {
  assertConfigured();
  const raw = await readContract(CONFIG.circleContractId, "get_members", [
    sc.u64(id),
    sc.u32(start),
    sc.u32(limit),
  ]);
  return Array.isArray(raw) ? (raw as unknown[]).map(String) : [];
}

export async function getMember(id: number | bigint, address: string): Promise<MemberMeta | null> {
  assertConfigured();
  try {
    const raw = await readContract(CONFIG.circleContractId, "get_member", [
      sc.u64(id),
      sc.addr(address),
    ]);
    return toMember(raw);
  } catch {
    return null; // NotMember
  }
}

export async function getFees(): Promise<bigint> {
  assertConfigured();
  return BigInt((await readContract(CONFIG.circleContractId, "get_fees")) as bigint);
}

/** Fetch up to `cap` most-recent circles (enumerated 1..=counter). */
export async function listCircles(cap = 25): Promise<Circle[]> {
  const counter = await getCounter();
  if (counter === 0) return [];
  const ids: number[] = [];
  for (let i = counter; i >= 1 && ids.length < cap; i--) ids.push(i);
  const settled = await Promise.allSettled(ids.map((id) => getCircle(id)));
  return settled
    .filter((s): s is PromiseFulfilledResult<Circle> => s.status === "fulfilled")
    .map((s) => s.value);
}

// ── Writes (wallet signs) ─────────────────────────────────────────────────────

export async function createCircle(
  address: string,
  params: {
    name: string;
    contribution: bigint;
    cycleSeconds: number;
    maxMembers: number;
    collateral: bigint;
  },
): Promise<string> {
  return invokeContract(address, CONFIG.circleContractId, "create_circle", [
    sc.addr(address),
    sc.str(params.name),
    sc.i128(params.contribution),
    sc.u64(params.cycleSeconds),
    sc.u32(params.maxMembers),
    sc.i128(params.collateral),
  ]);
}

/** Token approve (SAC) + join_circle — two chained wallet signatures. */
export async function joinCircle(address: string, id: number | bigint): Promise<string> {
  const circle = await getCircle(id);
  const amount = circle.contribution + circle.collateral;
  // SAC approve(owner, spender, amount, expiration_ledger)
  await invokeContract(address, CONFIG.usdcContractId, "approve", [
    sc.addr(address),
    sc.addr(CONFIG.circleContractId),
    sc.i128(amount),
    sc.u32(6_000_000),
  ]);
  return invokeContract(address, CONFIG.circleContractId, "join_circle", [
    sc.addr(address),
    sc.u64(id),
  ]);
}

export async function startCircle(address: string, id: number | bigint): Promise<string> {
  return invokeContract(address, CONFIG.circleContractId, "start_circle", [
    sc.addr(address),
    sc.u64(id),
  ]);
}

/** Token approve for the contribution amount + contribute. */
export async function contribute(address: string, id: number | bigint): Promise<string> {
  const circle = await getCircle(id);
  await invokeContract(address, CONFIG.usdcContractId, "approve", [
    sc.addr(address),
    sc.addr(CONFIG.circleContractId),
    sc.i128(circle.contribution),
    sc.u32(6_000_000),
  ]);
  return invokeContract(address, CONFIG.circleContractId, "contribute", [
    sc.addr(address),
    sc.u64(id),
  ]);
}

export async function rollover(address: string, id: number | bigint): Promise<string> {
  return invokeContract(address, CONFIG.circleContractId, "rollover", [
    sc.addr(address),
    sc.u64(id),
  ]);
}

export async function cancelCircle(address: string, id: number | bigint): Promise<string> {
  return invokeContract(address, CONFIG.circleContractId, "cancel_circle", [
    sc.addr(address),
    sc.u64(id),
  ]);
}

export async function claimCollateral(address: string, id: number | bigint): Promise<string> {
  return invokeContract(address, CONFIG.circleContractId, "claim_collateral", [
    sc.addr(address),
    sc.u64(id),
  ]);
}
