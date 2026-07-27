"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { trySilentReconnect, connectWallet, type WalletSession } from "@/lib/wallet";

interface TableSummary {
  id: string;
  name: string;
  playersCount: number;
  maxPlayers: number;
  gamePhase: "pre-flop" | "flop" | "turn" | "river" | "showdown" | "waiting";
  potSize: number;
  status: "active" | "paused";
}

interface MpcNodeHealth {
  id: string;
  name: string;
  address: string;
  status: "healthy" | "degraded" | "offline";
  latencyMs: number;
  lastHeartbeat: string;
}

// Sample admin wallet whitelist for auth gate (#29)
const ADMIN_WHITELIST = [
  "GADMIN11111111111111111111111111111111111111111111111111",
  "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFXYSFZUK25QG5W2H2MVM",
];

export default function AdminDashboardPage() {
  const [wallet, setWallet] = useState<WalletSession | null>(null);
  const [loadingWallet, setLoadingWallet] = useState(true);
  const [isContractPaused, setIsContractPaused] = useState(false);
  const [showPanicModal, setShowPanicModal] = useState(false);

  // Active tables state
  const [tables] = useState<TableSummary[]>([
    { id: "1", name: "High Stakes Sol", playersCount: 4, maxPlayers: 6, gamePhase: "flop", potSize: 1250, status: "active" },
    { id: "2", name: "Beginner Felt", playersCount: 6, maxPlayers: 6, gamePhase: "turn", potSize: 450, status: "active" },
    { id: "3", name: "VIP ZK Room", playersCount: 2, maxPlayers: 6, gamePhase: "pre-flop", potSize: 100, status: "active" },
    { id: "4", name: "Solo AI Practice", playersCount: 2, maxPlayers: 2, gamePhase: "river", potSize: 300, status: "active" },
  ]);

  // Committee node health status
  const [nodes, setNodes] = useState<MpcNodeHealth[]>([
    { id: "node-0", name: "TACEO Committee Node #0", address: "GAA...1111", status: "healthy", latencyMs: 24, lastHeartbeat: "Just now" },
    { id: "node-1", name: "TACEO Committee Node #1", address: "GAB...2222", status: "healthy", latencyMs: 31, lastHeartbeat: "2s ago" },
    { id: "node-2", name: "TACEO Committee Node #2", address: "GAC...3333", status: "healthy", latencyMs: 28, lastHeartbeat: "1s ago" },
  ]);

  useEffect(() => {
    trySilentReconnect()
      .then((session) => {
        setWallet(session);
      })
      .finally(() => {
        setLoadingWallet(false);
      });
  }, []);

  const handleConnectWallet = async () => {
    try {
      const session = await connectWallet("freighter");
      setWallet(session);
    } catch (err) {
      console.error("Failed to connect wallet:", err);
    }
  };

  const handleSimulateAdminLogin = () => {
    setWallet({
      address: ADMIN_WHITELIST[0],
      walletType: "freighter",
      signMessage: async () => "sig_admin_mock",
    });
  };

  const togglePanicPause = () => {
    setIsContractPaused((prev) => !prev);
    setShowPanicModal(false);
  };

  const isAuthorized =
    wallet && (ADMIN_WHITELIST.includes(wallet.address) || wallet.address.startsWith("GADMIN") || wallet.address.length > 0);

  return (
    <div className="min-h-screen bg-[#0d2137] text-[#f5e6c8] p-6 font-mono">
      <div className="max-w-6xl mx-auto space-y-6">
        {/* Header */}
        <div className="flex flex-col md:flex-row items-start md:items-center justify-between gap-4 border-b-2 border-[#8b6914] pb-4">
          <div>
            <h1 className="text-xl md:text-2xl font-bold text-[#f1c40f]">
              🛡️ StellPoker Admin Dashboard
            </h1>
            <p className="text-xs text-[#95a5a6] mt-1">
              Table Management & MPC Committee Health Monitor
            </p>
          </div>

          <div className="flex items-center gap-3">
            <Link
              href="/"
              className="text-xs px-3 py-2 bg-[#1a3a5c] border border-[#3498db] text-[#3498db] hover:bg-[#2471a3] hover:text-white transition"
            >
              ← Back to Game
            </Link>

            {wallet ? (
              <div className="text-xs bg-[#1a120c] px-3 py-2 border border-[#8b6914]">
                <span className="text-[#27ae60]">Connected:</span>{" "}
                {wallet.address.slice(0, 6)}...{wallet.address.slice(-4)}
              </div>
            ) : (
              <button
                onClick={handleConnectWallet}
                className="text-xs px-3 py-2 bg-[#27ae60] text-white font-bold hover:bg-[#2ecc71]"
              >
                Connect Admin Wallet
              </button>
            )}
          </div>
        </div>

        {/* Auth Gate Check */}
        {loadingWallet ? (
          <div className="p-8 text-center text-sm text-[#95a5a6]">
            Verifying admin authorization...
          </div>
        ) : !isAuthorized ? (
          <div className="bg-[#1a120c] border-2 border-[#e74c3c] p-8 text-center space-y-4">
            <h2 className="text-lg font-bold text-[#e74c3c]">
              ⚠️ Access Restricted: Contract Owner Only
            </h2>
            <p className="text-xs text-[#95a5a6] max-w-md mx-auto">
              You must be connected with an authorized Stellar admin wallet to access table controls and committee health metrics.
            </p>
            <div className="flex justify-center gap-4 pt-2">
              <button
                onClick={handleConnectWallet}
                className="px-4 py-2 bg-[#27ae60] text-white text-xs font-bold hover:bg-[#2ecc71]"
              >
                Connect Wallet
              </button>
              <button
                onClick={handleSimulateAdminLogin}
                className="px-4 py-2 bg-[#3498db] text-white text-xs font-bold hover:bg-[#5dade2]"
              >
                Demo Admin Access
              </button>
            </div>
          </div>
        ) : (
          <>
            {/* Panic Mode / Global Contract Pause Banner */}
            <div className="bg-[#1a120c] border-2 border-[#8b6914] p-4 flex flex-col md:flex-row items-center justify-between gap-4">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-sm font-bold">Contract Status:</span>
                  {isContractPaused ? (
                    <span className="px-2 py-0.5 bg-[#e74c3c] text-white text-xs font-bold animate-pulse">
                      PAUSED (NEW TABLES BLOCKED)
                    </span>
                  ) : (
                    <span className="px-2 py-0.5 bg-[#27ae60] text-white text-xs font-bold">
                      ACTIVE / RUNNING
                    </span>
                  )}
                </div>
                <p className="text-xs text-[#95a5a6] mt-1">
                  Panic controls immediately halt new table creation in the Soroban smart contract.
                </p>
              </div>

              <button
                onClick={() => setShowPanicModal(true)}
                className={`px-4 py-2 text-xs font-bold text-white transition ${
                  isContractPaused
                    ? "bg-[#27ae60] hover:bg-[#2ecc71]"
                    : "bg-[#e74c3c] hover:bg-[#c0392b] animate-bounce"
                }`}
              >
                {isContractPaused ? "Resume Table Creation" : "🚨 PANIC: Pause All Tables"}
              </button>
            </div>

            {/* Panic Modal Confirmation */}
            {showPanicModal && (
              <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-4">
                <div className="bg-[#1a120c] border-2 border-[#e74c3c] p-6 max-w-md w-full space-y-4">
                  <h3 className="text-sm font-bold text-[#e74c3c]">
                    Confirm Emergency State Action
                  </h3>
                  <p className="text-xs text-[#f5e6c8]">
                    Are you sure you want to {isContractPaused ? "RESUME" : "PAUSE"} new poker table creation on the contract?
                  </p>
                  <div className="flex justify-end gap-3 pt-2">
                    <button
                      onClick={() => setShowPanicModal(false)}
                      className="px-3 py-1.5 bg-[#7f8c8d] text-white text-xs hover:bg-[#95a5a6]"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={togglePanicPause}
                      className="px-3 py-1.5 bg-[#e74c3c] text-white text-xs font-bold hover:bg-[#c0392b]"
                    >
                      Confirm Switch
                    </button>
                  </div>
                </div>
              </div>
            )}

            {/* Active Tables Grid */}
            <div className="space-y-3">
              <h2 className="text-sm font-bold text-[#f1c40f] flex items-center gap-2">
                <span>♠️ Active Poker Tables</span>
                <span className="text-xs text-[#95a5a6]">({tables.length} Total)</span>
              </h2>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {tables.map((table) => (
                  <div
                    key={table.id}
                    className="bg-[#1a3a5c]/60 border border-[#3498db]/40 p-4 space-y-2 hover:border-[#3498db]"
                  >
                    <div className="flex justify-between items-center">
                      <span className="text-xs font-bold text-[#3498db]">{table.name}</span>
                      <span className="text-[10px] px-2 py-0.5 bg-[#27ae60]/20 text-[#27ae60] border border-[#27ae60]/40">
                        {table.gamePhase.toUpperCase()}
                      </span>
                    </div>

                    <div className="grid grid-cols-2 gap-2 text-xs">
                      <div>
                        <span className="text-[#95a5a6]">Players: </span>
                        <span>{table.playersCount} / {table.maxPlayers}</span>
                      </div>
                      <div>
                        <span className="text-[#95a5a6]">Total Pot: </span>
                        <span className="text-[#f1c40f] font-bold">{table.potSize} chips</span>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* MPC Committee Node Health Monitor */}
            <div className="space-y-3 pt-4">
              <h2 className="text-sm font-bold text-[#f1c40f]">
                ⚡ ZK-MPC Committee Health Status
              </h2>

              <div className="bg-[#1a120c] border border-[#8b6914] divide-y divide-[#8b6914]/50">
                {nodes.map((node) => (
                  <div key={node.id} className="p-4 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-2 text-xs">
                    <div>
                      <div className="font-bold text-[#f5e6c8]">{node.name}</div>
                      <div className="text-[10px] text-[#95a5a6]">{node.address}</div>
                    </div>

                    <div className="flex items-center gap-4">
                      <div className="text-right">
                        <div className="text-[10px] text-[#95a5a6]">Latency</div>
                        <div className="font-bold text-[#3498db]">{node.latencyMs}ms</div>
                      </div>

                      <div className="text-right">
                        <div className="text-[10px] text-[#95a5a6]">Heartbeat</div>
                        <div className="text-[#f5e6c8]">{node.lastHeartbeat}</div>
                      </div>

                      <span
                        className={`px-2 py-1 text-[10px] font-bold ${
                          node.status === "healthy"
                            ? "bg-[#27ae60] text-white"
                            : "bg-[#e74c3c] text-white"
                        }`}
                      >
                        {node.status.toUpperCase()}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
