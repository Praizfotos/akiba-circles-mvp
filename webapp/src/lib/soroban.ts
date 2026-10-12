/**
 * Soroban RPC helpers: read-only simulation + prepare/sign/send pipeline.
 *
 * Read calls are simulated against a throwaway source account (no signature,
 * no funds needed — Soroban RPC accepts simulations for non-existent sources).
 * Write calls are built from the connected wallet's account, prepared
 * (fees + auth filled in by the RPC), signed by the wallet, then submitted.
 */

import {
  Account,
  BASE_FEE,
  Contract,
  Keypair,
  TransactionBuilder,
  nativeToScVal,
  scValToNative,
  Address,
  type ScVal,
  rpc,
} from "@stellar/stellar-sdk";
import { CONFIG } from "./config";
import { signXdr } from "./wallet";

export function getServer(): rpc.Server {
  return new rpc.Server(CONFIG.rpcUrl);
}

// ── ScVal builders ────────────────────────────────────────────────────────────

export const sc = {
  u32: (v: number): ScVal => nativeToScVal(v, { type: "u32" }),
  u64: (v: bigint | number): ScVal => nativeToScVal(v, { type: "u64" }),
  i128: (v: bigint | number): ScVal => nativeToScVal(v, { type: "i128" }),
  str: (v: string): ScVal => nativeToScVal(v, { type: "string" }),
  addr: (v: string): ScVal => Address.fromString(v).toScVal(),
};

// ── Reads ─────────────────────────────────────────────────────────────────────

/** Simulate a contract invocation and return the native return value. */
export async function readContract(
  contractId: string,
  fn: string,
  args: ScVal[] = [],
): Promise<unknown> {
  const server = getServer();
  const contract = new Contract(contractId);
  const source = Keypair.random().publicKey();
  const tx = new TransactionBuilder(new Account(source, "0"), {
    fee: BASE_FEE,
    networkPassphrase: CONFIG.networkPassphrase,
  })
    .addOperation(contract.call(fn, ...args))
    .setTimeout(60)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`simulate ${fn}: ${sim.error}`);
  }
  if (!sim.result?.retval) {
    throw new Error(`simulate ${fn}: no return value`);
  }
  return scValToNative(sim.result.retval);
}

// ── Writes ────────────────────────────────────────────────────────────────────

/** Build + prepare a write transaction; returns XDR ready for wallet signing. */
export async function prepareCall(
  address: string,
  contractId: string,
  fn: string,
  args: ScVal[] = [],
): Promise<string> {
  const server = getServer();
  const account = await server.getAccount(address);
  const contract = new Contract(contractId);
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: CONFIG.networkPassphrase,
  })
    .addOperation(contract.call(fn, ...args))
    .setTimeout(180)
    .build();

  const prepared = await server.prepareTransaction(tx);
  return prepared.toXDR();
}

/** Sign with the connected wallet, submit, and wait for the ledger result. */
export async function signAndSubmit(xdr: string, address: string): Promise<string> {
  const signedXdr = await signXdr(xdr, address);
  const server = getServer();
  const tx = TransactionBuilder.fromXDR(signedXdr, CONFIG.networkPassphrase);

  const sent = await server.sendTransaction(tx);
  if (sent.status === "ERROR") {
    throw new Error(`send failed: ${JSON.stringify(sent.errorResult ?? sent.status)}`);
  }

  // Poll until the transaction lands or disappears.
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const status = await server.getTransaction(sent.hash);
    if (status.status === rpc.Api.GetTransactionStatus.SUCCESS) {
      return sent.hash;
    }
    if (status.status === rpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`transaction ${sent.hash} failed on-chain`);
    }
    if (status.status === rpc.Api.GetTransactionStatus.NOT_FOUND) {
      await new Promise((r) => setTimeout(r, 2_000));
      continue;
    }
    await new Promise((r) => setTimeout(r, 1_000));
  }
  throw new Error(`transaction ${sent.hash} timed out waiting for confirmation`);
}

/** Convenience: prepare → sign → submit in one call. */
export async function invokeContract(
  address: string,
  contractId: string,
  fn: string,
  args: ScVal[] = [],
): Promise<string> {
  const xdr = await prepareCall(address, contractId, fn, args);
  return signAndSubmit(xdr, address);
}
