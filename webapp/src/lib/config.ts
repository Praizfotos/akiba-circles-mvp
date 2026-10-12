/**
 * Runtime configuration for the Akiba Circles dApp.
 *
 * Everything is env-driven (NEXT_PUBLIC_* so the browser bundle can read it).
 * Defaults target Stellar Testnet.
 */

export type NetworkName = "TESTNET" | "PUBLIC";

const PASSPHRASES: Record<NetworkName, string> = {
  TESTNET: "Test SDF Network ; September 2015",
  PUBLIC: "Public Global Stellar Network ; September 2015",
};

const RPC_URLS: Record<NetworkName, string> = {
  TESTNET: "https://soroban-testnet.stellar.org",
  PUBLIC: "https://soroban.stellar.org",
};

const network = (process.env.NEXT_PUBLIC_STELLAR_NETWORK ?? "TESTNET") as NetworkName;

export const CONFIG = {
  network,
  networkPassphrase: PASSPHRASES[network],
  rpcUrl: process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ?? RPC_URLS[network],
  /** Deployed savings-circle contract (required for the app to do anything). */
  circleContractId: process.env.NEXT_PUBLIC_SAVINGS_CIRCLE_CONTRACT_ID ?? "",
  /** USDC (or any SAC token) the circles are denominated in. */
  usdcContractId: process.env.NEXT_PUBLIC_USDC_CONTRACT_ID ?? "",
  /** USDC has 7 decimal places. */
  tokenDecimals: Number(process.env.NEXT_PUBLIC_TOKEN_DECIMALS ?? 7),
  explorerUrl:
    network === "PUBLIC"
      ? "https://stellar.expert/explorer/public/contract/"
      : "https://stellar.expert/explorer/testnet/contract/",
} as const;

export function assertConfigured(): void {
  if (!CONFIG.circleContractId) {
    throw new Error(
      "NEXT_PUBLIC_SAVINGS_CIRCLE_CONTRACT_ID is not set. Deploy the contract (CONTRACT=savings-circle ./contracts/deploy.sh) and copy the ID into webapp/.env.local.",
    );
  }
  if (!CONFIG.usdcContractId) {
    throw new Error(
      "NEXT_PUBLIC_USDC_CONTRACT_ID is not set. Point it at the Testnet USDC SAC (or the token you deployed).",
    );
  }
}
