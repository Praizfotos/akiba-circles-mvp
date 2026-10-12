"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  connectWallet,
  disconnectWallet,
  getAddress,
  onWalletState,
} from "@/lib/wallet";

interface WalletState {
  address?: string;
  connecting: boolean;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
}

const WalletContext = createContext<WalletState>({
  address: undefined,
  connecting: false,
  connect: async () => {},
  disconnect: async () => {},
});

export function useWallet(): WalletState {
  return useContext(WalletContext);
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | undefined>(undefined);
  const [connecting, setConnecting] = useState(false);

  useEffect(() => {
    // Restore a previous session if the kit still knows about it.
    getAddress()
      .then(({ address }) => setAddress(address))
      .catch(() => setAddress(undefined));
    return onWalletState((e) => setAddress(e.address));
  }, []);

  const connect = useCallback(async () => {
    setConnecting(true);
    try {
      const { address } = await connectWallet();
      setAddress(address);
    } finally {
      setConnecting(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    await disconnectWallet();
    setAddress(undefined);
  }, []);

  const value = useMemo(
    () => ({ address, connecting, connect, disconnect }),
    [address, connecting, connect, disconnect],
  );

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}
