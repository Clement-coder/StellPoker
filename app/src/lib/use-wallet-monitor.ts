"use client";

import { useEffect, useRef } from "react";
import { checkWalletStillConnected, clearWallet, type WalletSession } from "./wallet";

const POLL_MS = 3_000;

/**
 * Polls the active wallet every 3 s while a session exists.
 * Supports both Freighter and Lobstr via the WalletSession.walletType field.
 *
 * The first time the wallet reports as disconnected, clears its localStorage
 * entry and calls onDisconnect so the caller can sign the user out.
 *
 * Pass wallet={null} to disable — the hook is a no-op until a session exists,
 * avoiding false positives during the initial silent-reconnect window.
 */
export function useWalletMonitor({
  wallet,
  onDisconnect,
}: {
  wallet: WalletSession | null;
  onDisconnect: () => void;
}) {
  // Keep the latest callback in a ref so changing it doesn't restart the effect.
  const onDisconnectRef = useRef(onDisconnect);
  onDisconnectRef.current = onDisconnect;

  // Capture the wallet type in a ref so it's readable inside the effect without
  // being listed as a dependency (avoids restarting the interval on re-renders).
  const walletTypeRef = useRef(wallet?.walletType ?? null);
  if (wallet?.walletType) {
    walletTypeRef.current = wallet.walletType;
  }

  const isActive = !!wallet;

  useEffect(() => {
    if (!isActive || !walletTypeRef.current) return;
    const walletType = walletTypeRef.current;

    let cancelled = false;
    let fired = false;

    const check = async () => {
      if (cancelled || fired) return;
      try {
        const connected = await checkWalletStillConnected(walletType);
        if (cancelled || fired) return;
        if (!connected) {
          fired = true;
          clearWallet(walletType);
          onDisconnectRef.current();
        }
      } catch {
        // Extension unreachable or threw — don't treat as disconnect.
      }
    };

    const id = setInterval(() => void check(), POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [isActive]);
}
