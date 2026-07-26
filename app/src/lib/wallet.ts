export type WalletType = "freighter" | "lobstr";

export interface WalletSession {
  address: string;
  walletType: WalletType;
  signMessage: (message: string) => Promise<string>;
}

export interface WalletInfo {
  type: WalletType;
  name: string;
  isInstalled: boolean;
}

import {
  connectFreighterWallet as connectFreighter,
  trySilentReconnect as tryReconnectFreighter,
  isFreighterInstalled,
  getActiveAddress as freighterGetActiveAddress,
  clearSavedWallet as clearFreighterWallet,
  getActiveAddress as getActiveFreighterAddress,
} from "./freighter";

import {
  connectLobstrWallet as connectLobstr,
  trySilentReconnectLobstr as tryReconnectLobstr,
  isLobstrInstalled,
  clearSavedWallet as clearLobstrWallet,
} from "./lobstr";

const WALLET_META: Record<WalletType, { name: string }> = {
  freighter: { name: "Freighter" },
  lobstr: { name: "Lobstr" },
};

export function detectInstalledWallets(): WalletInfo[] {
  const results: WalletInfo[] = [];

  if (typeof window === "undefined") return results;

  if (isFreighterInstalled()) {
    results.push({
      type: "freighter",
      name: WALLET_META.freighter.name,
      isInstalled: true,
    });
  }

  if (isLobstrInstalled()) {
    results.push({
      type: "lobstr",
      name: WALLET_META.lobstr.name,
      isInstalled: true,
    });
  }

  if (results.length === 0) {
    results.push(
      { type: "freighter", name: WALLET_META.freighter.name, isInstalled: false },
      { type: "lobstr", name: WALLET_META.lobstr.name, isInstalled: false }
    );
  }

  return results;
}

export async function connectWallet(type: WalletType): Promise<WalletSession> {
  switch (type) {
    case "freighter":
      return connectFreighter();
    case "lobstr":
      return connectLobstr();
  }
}

export async function trySilentReconnect(): Promise<WalletSession | null> {
  const freighterSession = await tryReconnectFreighter();
  if (freighterSession) return freighterSession;

  const lobstrSession = await tryReconnectLobstr();
  if (lobstrSession) return lobstrSession;

  return null;
}

export function getWalletDisplayName(session: WalletSession): string {
  const meta = WALLET_META[session.walletType];
  return meta ? meta.name : session.walletType;
}

export async function getActiveAddress(): Promise<string | null> {
  return freighterGetActiveAddress();
}

/**
 * Checks whether the given wallet type is still connected.
 * Used by useWalletMonitor to detect disconnection without showing a popup.
 */
export async function checkWalletStillConnected(walletType: WalletType): Promise<boolean> {
  switch (walletType) {
    case "freighter": {
      const addr = await freighterGetActiveAddress();
      return addr !== null;
    }
    case "lobstr": {
      if (typeof window === "undefined") return false;
      const api = (window as Window & { lobstr?: { getAddress?: () => Promise<unknown> } }).lobstr;
      if (!api || typeof api.getAddress !== "function") return false;
      try {
        const result = await api.getAddress();
        return typeof result === "string"
          ? result.length > 0
          : typeof result === "object" && result !== null && !("error" in result);
      } catch {
        return false;
      }
    }
  }
}

/** Clears the saved wallet address from localStorage for the given wallet type. */
export function clearWallet(walletType: WalletType): void {
  switch (walletType) {
    case "freighter":
      clearFreighterWallet();
      break;
    case "lobstr":
      clearLobstrWallet();
      break;
  }
}
