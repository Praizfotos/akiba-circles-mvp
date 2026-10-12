"use client";

/**
 * Stellar Wallets Kit singleton + thin wrappers.
 *
 * The kit is initialized once on the client (it touches window/localStorage),
 * then reused by the wallet provider and the transaction helpers.
 */

import {
  StellarWalletsKit,
  KitEventType,
  Networks,
} from "@creit.tech/stellar-wallets-kit";
import { FreighterModule } from "@creit.tech/stellar-wallets-kit/modules/freighter";
import { AlbedoModule } from "@creit.tech/stellar-wallets-kit/modules/albedo";
import { xBullModule } from "@creit.tech/stellar-wallets-kit/modules/xbull";
import { LobstrModule } from "@creit.tech/stellar-wallets-kit/modules/lobstr";
import { CONFIG } from "./config";

let initialized = false;

export function initWalletKit(): void {
  if (initialized || typeof window === "undefined") return;
  StellarWalletsKit.init({
    modules: [new FreighterModule(), new AlbedoModule(), new xBullModule(), new LobstrModule()],
    network: CONFIG.network === "PUBLIC" ? Networks.PUBLIC : Networks.TESTNET,
  });
  initialized = true;
}

/** Opens the wallet-picker modal; resolves with the connected address. */
export async function connectWallet(): Promise<{ address: string }> {
  initWalletKit();
  return StellarWalletsKit.authModal();
}

export async function getAddress(): Promise<{ address: string }> {
  initWalletKit();
  return StellarWalletsKit.getAddress();
}

export async function disconnectWallet(): Promise<void> {
  initWalletKit();
  return StellarWalletsKit.disconnect();
}

/** Subscribe to kit state changes; returns an unsubscribe function. */
export function onWalletState(
  cb: (event: { address?: string }) => void,
): () => void {
  initWalletKit();
  const offUpdated = StellarWalletsKit.on(KitEventType.STATE_UPDATED, (e) =>
    cb({ address: e.payload.address }),
  );
  const offDisconnect = StellarWalletsKit.on(KitEventType.DISCONNECT, () =>
    cb({ address: undefined }),
  );
  return () => {
    offUpdated();
    offDisconnect();
  };
}

/** Sign a prepared (simulation-complete) transaction XDR and return the signed XDR. */
export async function signXdr(xdr: string, address: string): Promise<string> {
  initWalletKit();
  const { signedTxXdr } = await StellarWalletsKit.signTransaction(xdr, {
    networkPassphrase: CONFIG.networkPassphrase,
    address,
  });
  return signedTxXdr;
}
