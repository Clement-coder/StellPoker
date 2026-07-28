"use client";

import { useEffect, useRef } from "react";
import { checkWalletStillConnected, clearWallet, type WalletSession } from "./wallet";
import { subscribeToAccountChanges } from "./freighter";

const POLL_MS = 3_000;

/**
 * Monitors the active wallet session for two conditions:
 *
 * 1. **Disconnection** — wallet reports as no longer connected.
 *    Clears localStorage and calls `onDisconnect`.
 *
 * 2. **Account switch** — user selects a different account in Freighter
 *    without disconnecting first.  Calls `onAccountSwitch(newAddress)` so
 *    the caller can re-initialise the session with the new address.
 *    Only active for Freighter (Lobstr does not support in-browser switching).
 *
 * Pass `wallet={null}` to disable — the hook is a no-op until a session
 * exists, avoiding false positives during the initial silent-reconnect window.
 */
export function useWalletMonitor({
  wallet,
  onDisconnect,
  onAccountSwitch,
}: {
  wallet: WalletSession | null;
  onDisconnect: () => void;
  onAccountSwitch?: (newAddress: string) => void;
}) {
  // Keep latest callbacks in refs so changes don't restart the intervals.
  const onDisconnectRef = useRef(onDisconnect);
  onDisconnectRef.current = onDisconnect;
  const onAccountSwitchRef = useRef(onAccountSwitch);
  onAccountSwitchRef.current = onAccountSwitch;

  const walletTypeRef = useRef(wallet?.walletType ?? null);
  const walletAddressRef = useRef(wallet?.address ?? "");
  if (wallet?.walletType) walletTypeRef.current = wallet.walletType;
  if (wallet?.address) walletAddressRef.current = wallet.address;

  const isActive = !!wallet;

  // ── Disconnection polling ──────────────────────────────────────────────────
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
        // Extension unreachable — don't treat as disconnect.
      }
    };

    const id = setInterval(() => void check(), POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [isActive]);

  // ── Account-switch polling (Freighter only) ────────────────────────────────
  useEffect(() => {
    if (!isActive || walletTypeRef.current !== "freighter") return;
    if (!walletAddressRef.current) return;

    const stop = subscribeToAccountChanges(
      walletAddressRef.current,
      (newAddress) => {
        if (newAddress === null) {
          // Became null → treat as disconnect (handled by the other poller too,
          // but handle here defensively so we don't leave a stale session).
          clearWallet("freighter");
          onDisconnectRef.current();
        } else {
          onAccountSwitchRef.current?.(newAddress);
        }
      }
    );

    return stop;
  // Re-run only when the active wallet address actually changes so we poll
  // against the correct baseline.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isActive, wallet?.address]);
}
