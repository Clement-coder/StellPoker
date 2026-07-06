"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { PixelWorld } from "@/components/PixelWorld";
import { PixelCat } from "@/components/PixelCat";
import { PixelChip } from "@/components/PixelChip";
import { TransactionSimulation } from "@/components/TransactionSimulation";
import * as api from "@/lib/api";
import { useJoinTableSimulation } from "@/lib/use-transaction-simulation";
import {
  detectInstalledWallets,
  connectWallet,
  trySilentReconnect,
  getWalletDisplayName,
  type WalletSession,
  type WalletType,
} from "@/lib/wallet";

type Screen = "splash" | "connect" | "menu" | "create" | "join";
const STROOPS_PER_XLM = BigInt("10000000");

function parseXlmToStroops(value: string): bigint | null {
  const trimmed = value.trim();
  if (!/^\d+(\.\d{1,7})?$/.test(trimmed)) {
    return null;
  }
  const [whole, fraction = ""] = trimmed.split(".");
  const fracPadded = (fraction + "0000000").slice(0, 7);
  try {
    return BigInt(whole) * STROOPS_PER_XLM + BigInt(fracPadded);
  } catch {
    return null;
  }
}

function formatStroopsToXlm(value: bigint): string {
  const whole = value / STROOPS_PER_XLM;
  const fractional = (value % STROOPS_PER_XLM).toString().padStart(7, "0");
  const trimmedFraction = fractional.replace(/0+$/, "");
  return trimmedFraction ? `${whole}.${trimmedFraction}` : whole.toString();
}

export default function Home() {
  const router = useRouter();
  const [screen, setScreen] = useState<Screen>("splash");
  const [showContent, setShowContent] = useState(false);
  const [wallet, setWallet] = useState<WalletSession | null>(null);
  const [connecting, setConnecting] = useState<WalletType | null>(null);
  const [availableWallets, setAvailableWallets] = useState<{ type: WalletType; name: string; isInstalled: boolean }[]>([]);
  const [busy, setBusy] = useState(false);
  const [maxPlayers, setMaxPlayers] = useState(2);
  const [buyInXlm, setBuyInXlm] = useState(
    formatStroopsToXlm(BigInt("1000000000"))
  );
  const [joinTableId, setJoinTableId] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pendingTableId, setPendingTableId] = useState<number | null>(null);

  const joinTableSim = useJoinTableSimulation(wallet, () => {
    if (pendingTableId) {
      const query = maxPlayers >= 3 ? "?mode=multi" : "?mode=headsup";
      router.push(`/table/${pendingTableId}${query}`);
      setPendingTableId(null);
    }
  });

  // Fade-in timer for splash
  useEffect(() => {
    const timer = setTimeout(() => setShowContent(true), 300);
    return () => clearTimeout(timer);
  }, []);

  // Silent reconnect on mount
  useEffect(() => {
    void trySilentReconnect().then((session) => {
      if (session) setWallet(session);
    });
  }, []);

  // Detect available wallets when connect screen is shown
  useEffect(() => {
    if (screen === "connect") {
      setAvailableWallets(detectInstalledWallets());
    }
  }, [screen]);

  // Auto-advance from connect → menu when wallet connects
  useEffect(() => {
    if (screen === "connect" && wallet) {
      setScreen("menu");
    }
  }, [screen, wallet]);

  const handleConnect = async (type: WalletType) => {
    setConnecting(type);
    setError(null);
    try {
      const session = await connectWallet(type);
      setWallet(session);
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to connect ${type} wallet`);
    } finally {
      setConnecting(null);
    }
  };

  const handleCreateTable = async (solo = false) => {
    if (!wallet) return;
    let buyIn: bigint | null = null;
    if (!solo) {
      buyIn = parseXlmToStroops(buyInXlm);
      if (buyIn === null || buyIn <= BigInt(0)) {
        setError("Enter a valid buy-in amount in XLM");
        return;
      }
    }
    setBusy(true);
    setError(null);
    try {
      const players = solo ? 2 : maxPlayers;
      const created = await api.createTable(
        wallet,
        players,
        solo,
        buyIn ? buyIn.toString() : undefined
      );

      if (!solo && buyIn) {
        setPendingTableId(created.table_id);
        joinTableSim.joinTable(created.table_id, buyIn);
        return; // Simulation will handle navigation
      }

      const query = solo
        ? "?mode=single"
        : players >= 3
          ? "?mode=multi"
          : "?mode=headsup";
      router.push(`/table/${created.table_id}${query}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Create table failed");
    } finally {
      setBusy(false);
    }
  };

  const handleJoinById = async () => {
    if (!wallet) return;
    const id = Number(joinTableId);
    if (!Number.isFinite(id) || id < 0) {
      setError("Enter a valid table ID");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      router.push(`/table/${id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Join table failed");
    } finally {
      setBusy(false);
    }
  };

  const handleJoinOpen = async () => {
    if (!wallet) return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.listOpenTables();
      const first = result.tables[0];
      if (!first) {
        setError("No open tables found");
        return;
      }
      const query = first.max_players >= 3 ? "?mode=multi" : "?mode=headsup";
      router.push(`/table/${first.table_id}${query}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Join open table failed");
    } finally {
      setBusy(false);
    }
  };

  const shortAddr = wallet
    ? `${wallet.address.slice(0, 6)}...${wallet.address.slice(-4)}`
    : "";

  const walletLabel = wallet
    ? `${getWalletDisplayName(wallet)}: ${shortAddr}`
    : "";

  const playerOptions = [
    { count: 2, label: "2" },
    { count: 3, label: "3" },
    { count: 4, label: "4" },
    { count: 5, label: "5" },
    { count: 6, label: "6" },
  ];

  // ────────── SPLASH ──────────
  if (screen === "splash") {
    return (
      <PixelWorld>
        <div
          className="min-h-screen flex flex-col items-center justify-center gap-6 p-8 cursor-pointer select-none"
          onClick={() => setScreen(wallet ? "menu" : "connect")}
        >
          <div
            className="flex gap-3 mb-2"
            style={{
              opacity: showContent ? 1 : 0,
              transition: "opacity 0.5s ease-in",
              transitionDelay: "0.2s",
            }}
          >
            <PixelChip color="red" size={5} />
            <PixelChip color="gold" size={5} />
            <PixelChip color="blue" size={5} />
          </div>

          <div
            className="text-center"
            style={{
              opacity: showContent ? 1 : 0,
              transform: showContent ? "translateY(0)" : "translateY(-20px)",
              transition: "all 0.6s ease-out",
              transitionDelay: "0.4s",
            }}
          >
            <h1
              className="text-4xl md:text-5xl leading-relaxed"
              style={{
                color: "white",
                textShadow:
                  "4px 4px 0 #2c3e50, -1px -1px 0 #2c3e50, 1px -1px 0 #2c3e50, -1px 1px 0 #2c3e50",
                letterSpacing: "3px",
              }}
            >
              POKER
            </h1>
            <h2
              className="text-2xl md:text-3xl mt-1"
              style={{
                color: "white",
                textShadow:
                  "3px 3px 0 #2c3e50, -1px -1px 0 #2c3e50, 1px -1px 0 #2c3e50, -1px 1px 0 #2c3e50",
                letterSpacing: "2px",
              }}
            >
              ON STELLAR
            </h2>
          </div>

          <div
            className="mt-6"
            style={{
              opacity: showContent ? 1 : 0,
              transition: "opacity 0.5s ease-in",
              transitionDelay: "0.8s",
              animation: showContent
                ? "textPulse 1.5s ease-in-out infinite"
                : undefined,
              color: "#f5e6c8",
              textShadow: "2px 2px 0 #2c3e50",
              fontSize: "14px",
              fontFamily: "'Press Start 2P', monospace",
            }}
          >
            CLICK ANYWHERE TO START
          </div>

          <div
            className="fixed bottom-[12%] left-[6%] z-[5]"
            style={{
              opacity: showContent ? 1 : 0,
              transition: "opacity 0.5s",
              transitionDelay: "1s",
            }}
          >
            <PixelCat sprite={17} size={80} />
          </div>
          <div
            className="fixed bottom-[4%] left-[38%] z-[5]"
            style={{
              opacity: showContent ? 1 : 0,
              transition: "opacity 0.5s",
              transitionDelay: "1.2s",
            }}
          >
            <PixelCat sprite={18} size={96} />
          </div>
          <div
            className="fixed bottom-[12%] right-[6%] z-[5]"
            style={{
              opacity: showContent ? 1 : 0,
              transition: "opacity 0.5s",
              transitionDelay: "1.4s",
            }}
          >
            <PixelCat sprite={21} size={96} flipped />
          </div>
        </div>
      </PixelWorld>
    );
  }

  // ────────── SHARED WRAPPER FOR NON-SPLASH SCREENS ──────────
  const backTarget: Screen =
    screen === "connect"
      ? "splash"
      : screen === "create" || screen === "join"
        ? "menu"
        : "splash";

  return (
    <PixelWorld>
      <div className="min-h-screen flex flex-col items-center justify-center gap-8 p-8 relative">
        {/* Back button */}
        <button
          onClick={() => {
            setError(null);
            setScreen(backTarget);
          }}
          className="absolute top-6 left-6 z-20 text-[24px]"
          style={{
            color: "#f5e6c8",
            textShadow: "2px 2px 0 #2c3e50",
            background: "none",
            border: "none",
            cursor: "pointer",
            fontFamily: "'Press Start 2P', monospace",
          }}
        >
          ←
        </button>

{/* Wallet indicator moved below main panel — see after screen content */}

        {/* Logo area */}
        <div className="text-center">
          <div className="flex gap-2 justify-center mb-3">
            <PixelChip color="red" size={4} />
            <PixelChip color="gold" size={4} />
            <PixelChip color="blue" size={4} />
          </div>
          <h1
            className="text-3xl md:text-4xl leading-relaxed"
            style={{
              color: "white",
              textShadow: "3px 3px 0 #2c3e50",
              letterSpacing: "2px",
            }}
          >
            POKER ON STELLAR
          </h1>
          <p
            className="text-[11px] mt-3"
            style={{
              color: "#c8e6ff",
              textShadow: "1px 1px 0 rgba(0,0,0,0.5)",
            }}
          >
            PRIVATE POKER ON THE BLOCKCHAIN WITH ZK-MPC
          </p>
        </div>

        {/* ────── CONNECT SCREEN ────── */}
        {screen === "connect" && (
          <div
            className="p-6 flex flex-col items-center gap-5"
            style={{
              background: "rgba(12, 10, 24, 0.88)",
              border: "4px solid #c47d2e",
              boxShadow:
                "inset -4px -4px 0px 0px rgba(0,0,0,0.3), inset 4px 4px 0px 0px rgba(255,255,255,0.08), 0 4px 0 0 rgba(0,0,0,0.4), 0 0 20px rgba(196, 125, 46, 0.08)",
              minWidth: "360px",
            }}
          >
            <h2
              className="text-sm"
              style={{
                color: "#ffc078",
                textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
              }}
            >
              CONNECT WALLET
            </h2>

            {availableWallets.length === 0 ? (
              <div className="text-[9px]" style={{ color: "#95a5a6" }}>
                Scanning for wallets...
              </div>
            ) : (
              availableWallets.map((w) => (
                <button
                  key={w.type}
                  onClick={() => void handleConnect(w.type)}
                  disabled={connecting !== null || !w.isInstalled}
                  className="pixel-btn text-[12px] w-full"
                  style={{
                    padding: "12px 24px",
                    opacity: !w.isInstalled || connecting !== null ? 0.5 : 1,
                    background: w.type === "freighter" ? "#1a5276" : "#6c3483",
                    color: "white",
                  }}
                >
                  {connecting === w.type
                    ? `CONNECTING ${w.name}...`
                    : w.isInstalled
                      ? `CONNECT ${w.name}`
                      : `${w.name} NOT DETECTED`}
                </button>
              ))
            )}

            <div
              className="pixel-border-thin w-full p-2 text-[9px]"
              style={{
                background: "rgba(20,20,40,0.5)",
                borderColor: "#4a6a8a",
                color: "#c8e6ff",
              }}
            >
              OPEN YOUR WALLET EXTENSION, UNLOCK IT, AND CLICK THE BUTTON ABOVE.
            </div>
          </div>
        )}

        {/* ────── MENU SCREEN ────── */}
        {screen === "menu" && (
          <div
            className="p-6 flex flex-col items-center gap-5"
            style={{
              background: "rgba(12, 10, 24, 0.88)",
              border: "4px solid #c47d2e",
              boxShadow:
                "inset -4px -4px 0px 0px rgba(0,0,0,0.3), inset 4px 4px 0px 0px rgba(255,255,255,0.08), 0 4px 0 0 rgba(0,0,0,0.4), 0 0 20px rgba(196, 125, 46, 0.08)",
              minWidth: "360px",
            }}
          >
            <h2
              className="text-sm"
              style={{
                color: "#ffc078",
                textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
              }}
            >
              MAIN MENU
            </h2>

            <button
              onClick={() => setScreen("create")}
              className="pixel-btn pixel-btn-green text-[12px] w-full"
              style={{ padding: "14px 24px" }}
            >
              CREATE TABLE
            </button>

            <button
              onClick={() => setScreen("join")}
              className="pixel-btn pixel-btn-gold text-[12px] w-full"
              style={{ padding: "14px 24px" }}
            >
              JOIN TABLE
            </button>
          </div>
        )}

        {/* ────── CREATE SCREEN ────── */}
        {screen === "create" && (
          <div
            className="p-6 flex flex-col items-center gap-5"
            style={{
              background: "rgba(12, 10, 24, 0.88)",
              border: "4px solid #c47d2e",
              boxShadow:
                "inset -4px -4px 0px 0px rgba(0,0,0,0.3), inset 4px 4px 0px 0px rgba(255,255,255,0.08), 0 4px 0 0 rgba(0,0,0,0.4), 0 0 20px rgba(196, 125, 46, 0.08)",
              minWidth: "360px",
            }}
          >
            <h2
              className="text-sm"
              style={{
                color: "#ffc078",
                textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
              }}
            >
              CREATE TABLE
            </h2>

            <button
              onClick={() => void handleCreateTable(true)}
              disabled={busy || !wallet}
              className="pixel-btn pixel-btn-blue text-[11px] w-full"
              style={{
                padding: "12px 24px",
                opacity: busy || !wallet ? 0.6 : 1,
              }}
            >
              {busy ? "CREATING..." : "SOLO VS AI"}
            </button>
            <div
              className="w-full flex items-center gap-3"
              style={{ color: "#4a6a8a" }}
            >
              <div className="flex-1 h-[1px]" style={{ background: "#4a6a8a" }} />
              <span className="text-[9px]">MULTIPLAYER</span>
              <div className="flex-1 h-[1px]" style={{ background: "#4a6a8a" }} />
            </div>

            <div className="flex gap-2">
              {playerOptions.map((opt) => (
                <button
                  key={opt.count}
                  onClick={() => setMaxPlayers(opt.count)}
                  className="pixel-btn text-[10px]"
                  style={{
                    padding: "6px 14px",
                    background:
                      maxPlayers === opt.count ? "#145a32" : "#2c3e50",
                    opacity: maxPlayers === opt.count ? 1 : 0.7,
                    color: "white",
                  }}
                >
                  {opt.label}
                </button>
              ))}
            </div>

            <div className="w-full flex flex-col gap-2">
              <div className="text-[10px]" style={{ color: "#c8e6ff" }}>
                BUY-IN (XLM)
              </div>
              <input
                type="text"
                value={buyInXlm}
                onChange={(e) => setBuyInXlm(e.target.value)}
                placeholder="100"
                disabled={busy}
                className="w-full text-center text-[12px]"
                style={{ padding: "8px 10px" }}
              />
              <div className="text-[9px]" style={{ color: "#95a5a6" }}>
                Multiplayer only.
              </div>
            </div>

            <button
              onClick={() => void handleCreateTable(false)}
              disabled={busy || !wallet}
              className="pixel-btn pixel-btn-green text-[11px] w-full"
              style={{
                padding: "12px 24px",
                opacity: busy || !wallet ? 0.6 : 1,
              }}
            >
              {busy ? "CREATING..." : "START MULTIPLAYER"}
            </button>
          </div>
        )}

        {/* ────── JOIN SCREEN ────── */}
        {screen === "join" && (
          <div
            className="p-6 flex flex-col items-center gap-5"
            style={{
              background: "rgba(12, 10, 24, 0.88)",
              border: "4px solid #c47d2e",
              boxShadow:
                "inset -4px -4px 0px 0px rgba(0,0,0,0.3), inset 4px 4px 0px 0px rgba(255,255,255,0.08), 0 4px 0 0 rgba(0,0,0,0.4), 0 0 20px rgba(196, 125, 46, 0.08)",
              minWidth: "360px",
            }}
          >
            <h2
              className="text-sm"
              style={{
                color: "#ffc078",
                textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
              }}
            >
              JOIN TABLE
            </h2>

            {/* Join by ID */}
            <div className="flex items-center gap-2 w-full">
              <input
                type="number"
                value={joinTableId}
                onChange={(e) => setJoinTableId(e.target.value)}
                placeholder="TABLE ID"
                min={0}
                className="flex-1 text-center text-[12px]"
                style={{ padding: "8px 10px" }}
              />
              <button
                onClick={() => void handleJoinById()}
                disabled={busy || !wallet || !joinTableId}
                className="pixel-btn pixel-btn-gold text-[10px]"
                style={{
                  padding: "8px 18px",
                  opacity: busy || !wallet || !joinTableId ? 0.6 : 1,
                }}
              >
                {busy ? "JOINING..." : "JOIN"}
              </button>
            </div>

            {/* Divider */}
            <div
              className="w-full flex items-center gap-3"
              style={{ color: "#4a6a8a" }}
            >
              <div className="flex-1 h-[1px]" style={{ background: "#4a6a8a" }} />
              <span className="text-[9px]">OR</span>
              <div className="flex-1 h-[1px]" style={{ background: "#4a6a8a" }} />
            </div>

            {/* Join open table */}
            <button
              onClick={() => void handleJoinOpen()}
              disabled={busy || !wallet}
              className="pixel-btn pixel-btn-blue text-[11px] w-full"
              style={{
                padding: "12px 24px",
                opacity: busy || !wallet ? 0.6 : 1,
              }}
            >
              {busy ? "SEARCHING..." : "JOIN OPEN TABLE"}
            </button>
          </div>
        )}

        {/* Error display */}
        {error && (
          <div
            className="text-[9px]"
            style={{ color: "#ff7675", textAlign: "center" }}
          >
            {error}
          </div>
        )}

          {/* Wallet status — centered below panel with dim pulse */}
        {wallet ? (
          <div
            className="pixel-border-thin px-3 py-1"
            style={{
              background: "rgba(39, 174, 96, 0.15)",
              fontSize: "9px",
              color: "#27ae60",
              animation: "walletPulse 3s ease-in-out infinite",
            }}
          >
            {walletLabel}
          </div>
        ) : screen !== "connect" ? (
          <button
            onClick={() => setScreen("connect")}
            className="text-[9px]"
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              color: "#c47d2e",
              animation: "walletPulse 3s ease-in-out infinite",
              fontFamily: "'Press Start 2P', monospace",
            }}
          >
            CONNECT WALLET
          </button>
        ) : null}

        {/* Cats at bottom */}
        <div className="fixed bottom-[12%] left-[6%] z-[5]">
          <PixelCat sprite={19} size={80} />
        </div>
        <div className="fixed bottom-[4%] left-[38%] z-[5]">
          <PixelCat sprite={18} size={96} />
        </div>
        <div className="fixed bottom-[12%] right-[6%] z-[5]">
          <PixelCat sprite={20} size={96} flipped />
        </div>

        {/* Stats link */}
        <Link
          href="/stats"
          className="fixed top-3 right-3 z-10 text-[8px] opacity-60 hover:opacity-100 transition-opacity"
          style={{
            color: "#c47d2e",
            fontFamily: "'Press Start 2P', monospace",
            textDecoration: "none",
          }}
        >
          📊 STATS
        </Link>

        {/* Transaction Simulation */}
        {joinTableSim.showSimulation && joinTableSim.simulation && (
          <TransactionSimulation
            simulation={joinTableSim.simulation}
            loading={joinTableSim.loading}
            onConfirm={() => {
              if (pendingTableId) {
                const buyIn = parseXlmToStroops(buyInXlm);
                if (buyIn) {
                  joinTableSim.confirmJoin(pendingTableId, buyIn);
                }
              }
            }}
            onCancel={() => {
              joinTableSim.cancelSimulation();
              setPendingTableId(null);
              setBusy(false);
            }}
          />
        )}
      </div>
    </PixelWorld>
  );
}
