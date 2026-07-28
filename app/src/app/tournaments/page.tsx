"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { PixelWorld } from "@/components/PixelWorld";
import {
  listTournaments,
  createTournament,
  registerPlayer,
  startTournament,
  cancelTournament,
  stroopsToXlm,
  statusLabel,
  statusColor,
  shortAddr,
  type TournamentSummary,
  type TournamentDetail,
  type CreateTournamentParams,
} from "@/lib/tournament";
import {
  trySilentReconnect,
  connectWallet,
  detectInstalledWallets,
  type WalletSession,
  type WalletType,
} from "@/lib/wallet";

const STROOPS_PER_XLM = 10_000_000;

// ── Small reusable components ─────────────────────────────────────────────────

function StatusBadge({ status }: { status: TournamentSummary["status"] }) {
  return (
    <span
      className="text-[8px] px-2 py-0.5"
      style={{
        border: `1px solid ${statusColor(status)}`,
        color: statusColor(status),
        background: `${statusColor(status)}18`,
      }}
    >
      {statusLabel(status)}
    </span>
  );
}

function TournamentCard({
  t,
  onSelect,
}: {
  t: TournamentSummary;
  onSelect: (id: string) => void;
}) {
  const spotsLeft = t.max_players - t.registered;
  return (
    <button
      className="w-full text-left pixel-border px-4 py-3 flex flex-col gap-1"
      style={{ background: "rgba(12,10,24,0.92)", borderColor: "#2a2a4a" }}
      onClick={() => onSelect(t.id)}
      aria-label={`Open tournament ${t.name}`}
    >
      <div className="flex items-center justify-between">
        <span className="text-[10px]" style={{ color: "#f5e6c8" }}>
          {t.name}
        </span>
        <StatusBadge status={t.status} />
      </div>
      <div className="flex gap-4 text-[8px]" style={{ color: "#95a5a6" }}>
        <span>BUY-IN: {stroopsToXlm(t.buy_in)} XLM</span>
        <span>PRIZE: {stroopsToXlm(t.prize_pool)} XLM</span>
        <span>
          {t.registered}/{t.max_players} PLAYERS
        </span>
      </div>
      <div className="flex gap-4 text-[8px]" style={{ color: "#7f8c8d" }}>
        <span>
          BLINDS: {stroopsToXlm(t.current_small_blind)}/
          {stroopsToXlm(t.current_big_blind)} XLM
        </span>
        <span>LEVEL {t.blind_level + 1}</span>
        {t.status === "registration" && (
          <span style={{ color: spotsLeft > 0 ? "#27ae60" : "#e74c3c" }}>
            {spotsLeft > 0 ? `${spotsLeft} SPOT${spotsLeft > 1 ? "S" : ""} LEFT` : "FULL"}
          </span>
        )}
      </div>
    </button>
  );
}

// ── Create tournament form ────────────────────────────────────────────────────

function CreateForm({
  onCreated,
  onCancel,
}: {
  onCreated: (t: TournamentDetail) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("Sit-and-Go #1");
  const [buyInXlm, setBuyInXlm] = useState("10");
  const [maxPlayers, setMaxPlayers] = useState(9);
  const [minPlayers, setMinPlayers] = useState(2);
  const [playersPerTable, setPlayersPerTable] = useState(6);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleCreate = async () => {
    const buyIn = Math.round(parseFloat(buyInXlm) * STROOPS_PER_XLM);
    if (!buyIn || buyIn <= 0) {
      setError("Invalid buy-in amount.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const t = await createTournament({
        name,
        buy_in: buyIn,
        max_players: maxPlayers,
        min_players: minPlayers,
        players_per_table: playersPerTable,
      });
      onCreated(t);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create tournament.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="pixel-border p-4 flex flex-col gap-3"
      style={{ background: "rgba(12,10,24,0.97)", borderColor: "#c47d2e" }}
      role="dialog"
      aria-label="Create tournament"
    >
      <div className="text-[10px]" style={{ color: "#f5e6c8" }}>
        CREATE SIT-AND-GO
      </div>

      <label className="flex flex-col gap-1">
        <span className="text-[8px]" style={{ color: "#95a5a6" }}>NAME</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="pixel-border px-2 py-1 text-[9px] w-full"
          style={{ background: "rgba(255,255,255,0.05)", color: "#f5e6c8", borderColor: "#4a4a6a" }}
          aria-label="Tournament name"
        />
      </label>

      <div className="flex gap-3">
        <label className="flex flex-col gap-1 flex-1">
          <span className="text-[8px]" style={{ color: "#95a5a6" }}>BUY-IN (XLM)</span>
          <input
            type="number"
            min="0"
            step="0.1"
            value={buyInXlm}
            onChange={(e) => setBuyInXlm(e.target.value)}
            className="pixel-border px-2 py-1 text-[9px] w-full"
            style={{ background: "rgba(255,255,255,0.05)", color: "#f1c40f", borderColor: "#4a4a6a" }}
            aria-label="Buy-in in XLM"
          />
        </label>

        <label className="flex flex-col gap-1 flex-1">
          <span className="text-[8px]" style={{ color: "#95a5a6" }}>MAX PLAYERS</span>
          <input
            type="number"
            min="2"
            max="100"
            value={maxPlayers}
            onChange={(e) => setMaxPlayers(Number(e.target.value))}
            className="pixel-border px-2 py-1 text-[9px] w-full"
            style={{ background: "rgba(255,255,255,0.05)", color: "#f5e6c8", borderColor: "#4a4a6a" }}
            aria-label="Maximum players"
          />
        </label>
      </div>

      <div className="flex gap-3">
        <label className="flex flex-col gap-1 flex-1">
          <span className="text-[8px]" style={{ color: "#95a5a6" }}>MIN TO START</span>
          <input
            type="number"
            min="2"
            max={maxPlayers}
            value={minPlayers}
            onChange={(e) => setMinPlayers(Number(e.target.value))}
            className="pixel-border px-2 py-1 text-[9px] w-full"
            style={{ background: "rgba(255,255,255,0.05)", color: "#f5e6c8", borderColor: "#4a4a6a" }}
            aria-label="Minimum players to start"
          />
        </label>

        <label className="flex flex-col gap-1 flex-1">
          <span className="text-[8px]" style={{ color: "#95a5a6" }}>SEATS/TABLE</span>
          <select
            value={playersPerTable}
            onChange={(e) => setPlayersPerTable(Number(e.target.value))}
            className="pixel-border px-2 py-1 text-[9px] w-full"
            style={{ background: "rgba(12,10,24,0.97)", color: "#f5e6c8", borderColor: "#4a4a6a" }}
            aria-label="Players per table"
          >
            {[2, 3, 4, 5, 6].map((n) => (
              <option key={n} value={n}>{n}</option>
            ))}
          </select>
        </label>
      </div>

      {error && (
        <div className="text-[8px]" style={{ color: "#e74c3c" }} role="alert">
          {error}
        </div>
      )}

      <div className="flex gap-2 mt-1">
        <button
          onClick={handleCreate}
          disabled={busy}
          className="pixel-btn text-[9px] flex-1"
          style={{ padding: "6px 0", background: busy ? "#555" : "#27ae60", color: "white" }}
          aria-label="Confirm create tournament"
        >
          {busy ? "CREATING…" : "CREATE"}
        </button>
        <button
          onClick={onCancel}
          className="pixel-btn text-[9px]"
          style={{ padding: "6px 14px", background: "#2c3e50", color: "white" }}
          aria-label="Cancel create tournament"
        >
          CANCEL
        </button>
      </div>
    </div>
  );
}

// ── Tournament detail panel ───────────────────────────────────────────────────

function DetailPanel({
  detail,
  wallet,
  onRefresh,
  onClose,
}: {
  detail: TournamentDetail;
  wallet: WalletSession | null;
  onRefresh: (t: TournamentDetail) => void;
  onClose: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isRegistered =
    wallet != null &&
    detail.players.some((p) => p.address === wallet.address);

  const canRegister =
    detail.status === "registration" && wallet != null && !isRegistered;

  const canStart =
    detail.status === "registration" &&
    detail.registered >= detail.min_players;

  const handleRegister = async () => {
    if (!wallet) return;
    setBusy(true);
    setError(null);
    try {
      // In a real flow the player would first call join_table on a PokerTable
      // contract and pass that contract address here. For the lobby we use a
      // placeholder until the escrow step is wired.
      const updated = await registerPlayer(
        detail.id,
        wallet.address,
        detail.table_contracts[0] ?? "pending"
      );
      onRefresh(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Registration failed.");
    } finally {
      setBusy(false);
    }
  };

  const handleStart = async () => {
    setBusy(true);
    setError(null);
    try {
      const updated = await startTournament(detail.id);
      onRefresh(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start.");
    } finally {
      setBusy(false);
    }
  };

  const handleCancel = async () => {
    setBusy(true);
    setError(null);
    try {
      const updated = await cancelTournament(detail.id);
      onRefresh(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to cancel.");
    } finally {
      setBusy(false);
    }
  };

  const activePlayers = detail.players.filter((p) => p.finish_position == null);
  const eliminated = detail.players
    .filter((p) => p.finish_position != null)
    .sort((a, b) => (a.finish_position ?? 0) - (b.finish_position ?? 0));

  return (
    <div
      className="pixel-border p-4 flex flex-col gap-3"
      style={{ background: "rgba(12,10,24,0.97)", borderColor: statusColor(detail.status) }}
      role="region"
      aria-label={`Tournament detail: ${detail.name}`}
    >
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <div className="text-[11px]" style={{ color: "#f5e6c8" }}>{detail.name}</div>
          <div className="flex gap-3 mt-1 text-[8px]" style={{ color: "#95a5a6" }}>
            <span>BUY-IN: {stroopsToXlm(detail.buy_in)} XLM</span>
            <span>PRIZE POOL: {stroopsToXlm(detail.prize_pool)} XLM</span>
          </div>
        </div>
        <div className="flex flex-col items-end gap-1">
          <StatusBadge status={detail.status} />
          <button
            onClick={onClose}
            className="text-[9px]"
            style={{ background: "none", border: "none", color: "#7f8c8d", cursor: "pointer" }}
            aria-label="Close detail panel"
          >
            ✕ CLOSE
          </button>
        </div>
      </div>

      {/* Payout schedule */}
      <div className="pixel-border-thin p-2" style={{ borderColor: "#2a2a4a" }}>
        <div className="text-[8px] mb-1" style={{ color: "#95a5a6" }}>PAYOUT SCHEDULE</div>
        <div className="flex gap-3">
          {detail.payout_schedule.shares.map((pct, i) => (
            <div key={i} className="text-center">
              <div className="text-[9px]" style={{ color: "#f1c40f" }}>
                {i === 0 ? "🥇" : i === 1 ? "🥈" : "🥉"}
              </div>
              <div className="text-[8px]" style={{ color: "#c47d2e" }}>{pct}%</div>
              <div className="text-[7px]" style={{ color: "#7f8c8d" }}>
                {stroopsToXlm(Math.floor(detail.prize_pool * pct / 100))} XLM
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Blind info */}
      <div className="text-[8px] flex gap-4" style={{ color: "#95a5a6" }}>
        <span>LEVEL {detail.blind_level + 1}</span>
        <span>
          BLINDS {stroopsToXlm(detail.current_small_blind)} /{" "}
          {stroopsToXlm(detail.current_big_blind)} XLM
        </span>
        <span>{detail.registered}/{detail.max_players} PLAYERS</span>
      </div>

      {/* Active players */}
      {activePlayers.length > 0 && (
        <div>
          <div className="text-[8px] mb-1" style={{ color: "#95a5a6" }}>ACTIVE PLAYERS</div>
          <div className="flex flex-col gap-1">
            {activePlayers
              .sort((a, b) => b.stack - a.stack)
              .map((p) => (
                <div
                  key={p.address}
                  className="flex items-center justify-between text-[8px] px-2 py-0.5"
                  style={{
                    background:
                      wallet?.address === p.address
                        ? "rgba(196,125,46,0.15)"
                        : "transparent",
                    borderLeft: wallet?.address === p.address
                      ? "2px solid #c47d2e"
                      : "2px solid transparent",
                  }}
                >
                  <span style={{ color: "#f5e6c8" }}>{shortAddr(p.address)}</span>
                  <span style={{ color: "#27ae60" }}>
                    {stroopsToXlm(p.stack)} XLM
                  </span>
                </div>
              ))}
          </div>
        </div>
      )}

      {/* Eliminated / final results */}
      {eliminated.length > 0 && (
        <div>
          <div className="text-[8px] mb-1" style={{ color: "#95a5a6" }}>
            {detail.status === "completed" ? "FINAL RESULTS" : "ELIMINATIONS"}
          </div>
          <div className="flex flex-col gap-1">
            {eliminated.map((p) => (
              <div
                key={p.address}
                className="flex items-center justify-between text-[8px] px-2 py-0.5"
                style={{ color: "#7f8c8d" }}
              >
                <span>
                  #{p.finish_position}{" "}
                  <span style={{ color: p.finish_position === 1 ? "#f1c40f" : "#95a5a6" }}>
                    {shortAddr(p.address)}
                  </span>
                </span>
                {p.payout != null && (
                  <span style={{ color: p.finish_position === 1 ? "#f1c40f" : "#95a5a6" }}>
                    {stroopsToXlm(p.payout)} XLM
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="text-[8px]" style={{ color: "#e74c3c" }} role="alert">
          {error}
        </div>
      )}

      {/* Actions */}
      <div className="flex gap-2 flex-wrap">
        {canRegister && (
          <button
            onClick={handleRegister}
            disabled={busy}
            className="pixel-btn text-[9px]"
            style={{ padding: "6px 14px", background: busy ? "#555" : "#3498db", color: "white" }}
            aria-label="Register for tournament"
          >
            {busy ? "…" : "REGISTER"}
          </button>
        )}
        {isRegistered && detail.status === "registration" && (
          <div className="text-[8px] self-center" style={{ color: "#27ae60" }}>
            ✓ REGISTERED
          </div>
        )}
        {canStart && (
          <button
            onClick={handleStart}
            disabled={busy}
            className="pixel-btn text-[9px]"
            style={{ padding: "6px 14px", background: busy ? "#555" : "#27ae60", color: "white" }}
            aria-label="Start tournament"
          >
            {busy ? "…" : "START"}
          </button>
        )}
        {detail.status === "registration" && (
          <button
            onClick={handleCancel}
            disabled={busy}
            className="pixel-btn text-[9px]"
            style={{ padding: "6px 14px", background: "#2c3e50", color: "#e74c3c" }}
            aria-label="Cancel tournament"
          >
            CANCEL
          </button>
        )}
      </div>
    </div>
  );
}

// ── Main lobby page ───────────────────────────────────────────────────────────

export default function TournamentsPage() {
  const [tournaments, setTournaments] = useState<TournamentSummary[]>([]);
  const [selected, setSelected] = useState<TournamentDetail | null>(null);
  const [creating, setCreating] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [wallet, setWallet] = useState<WalletSession | null>(null);

  // Silent wallet reconnect
  useEffect(() => {
    trySilentReconnect().then((s) => {
      if (s) setWallet(s);
    });
  }, []);

  const fetchList = useCallback(async () => {
    try {
      const list = await listTournaments();
      setTournaments(list);
      setError(null);
    } catch {
      setError("Could not load tournaments. Is the coordinator running?");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchList();
    const interval = setInterval(fetchList, 10_000);
    return () => clearInterval(interval);
  }, [fetchList]);

  const handleSelect = useCallback(
    async (id: string) => {
      try {
        const { getTournament } = await import("@/lib/tournament");
        const detail = await getTournament(id);
        setSelected(detail);
      } catch {
        setError("Failed to load tournament details.");
      }
    },
    []
  );

  const handleRefresh = useCallback((updated: TournamentDetail) => {
    setSelected(updated);
    setTournaments((prev) =>
      prev.map((t) =>
        t.id === updated.id
          ? {
              id: updated.id,
              name: updated.name,
              buy_in: updated.buy_in,
              max_players: updated.max_players,
              registered: updated.registered,
              status: updated.status,
              prize_pool: updated.prize_pool,
              current_small_blind: updated.current_small_blind,
              current_big_blind: updated.current_big_blind,
              blind_level: updated.blind_level,
            }
          : t
      )
    );
  }, []);

  const openTournaments = tournaments.filter(
    (t) => t.status === "registration" || t.status === "running"
  );
  const closedTournaments = tournaments.filter(
    (t) => t.status === "finalizing" || t.status === "completed" || t.status === "cancelled"
  );

  return (
    <PixelWorld>
      <div className="min-h-screen flex flex-col items-center p-6 gap-6">
        {/* Nav */}
        <div className="w-full" style={{ maxWidth: 640 }}>
          <div className="flex items-center justify-between">
            <Link
              href="/"
              className="text-[9px]"
              style={{ color: "#c47d2e", textDecoration: "none", fontFamily: "'Press Start 2P', monospace" }}
            >
              ← HOME
            </Link>
            <div className="text-[10px]" style={{ color: "#f5e6c8" }}>
              TOURNAMENT LOBBY
            </div>
            <button
              onClick={() => { setCreating(true); setSelected(null); }}
              className="pixel-btn text-[9px]"
              style={{ padding: "4px 10px", background: "#c47d2e", color: "white" }}
              aria-label="Create new tournament"
            >
              + NEW
            </button>
          </div>
        </div>

        <div className="w-full flex flex-col gap-4" style={{ maxWidth: 640 }}>
          {/* Create form */}
          {creating && (
            <CreateForm
              onCreated={(t) => {
                setCreating(false);
                setTournaments((prev) => [
                  {
                    id: t.id, name: t.name, buy_in: t.buy_in,
                    max_players: t.max_players, registered: t.registered,
                    status: t.status, prize_pool: t.prize_pool,
                    current_small_blind: t.current_small_blind,
                    current_big_blind: t.current_big_blind,
                    blind_level: t.blind_level,
                  },
                  ...prev,
                ]);
                setSelected(t);
              }}
              onCancel={() => setCreating(false)}
            />
          )}

          {/* Detail panel */}
          {selected && !creating && (
            <DetailPanel
              detail={selected}
              wallet={wallet}
              onRefresh={handleRefresh}
              onClose={() => setSelected(null)}
            />
          )}

          {/* Loading */}
          {loading && (
            <div
              className="pixel-border px-6 py-4 text-[10px]"
              style={{ borderColor: "#c47d2e", background: "rgba(12,10,24,0.92)", color: "#f5e6c8" }}
              aria-live="polite"
              aria-busy="true"
            >
              LOADING TOURNAMENTS…
            </div>
          )}

          {/* Error */}
          {!loading && error && (
            <div
              className="pixel-border px-4 py-3 text-[9px]"
              style={{ borderColor: "#e74c3c", background: "rgba(12,10,24,0.92)", color: "#e74c3c" }}
              role="alert"
            >
              {error}
            </div>
          )}

          {/* Open tournaments */}
          {!loading && openTournaments.length > 0 && (
            <div>
              <div className="text-[8px] mb-2 px-1" style={{ color: "#95a5a6" }}>
                OPEN TOURNAMENTS
              </div>
              <div className="flex flex-col gap-2">
                {openTournaments.map((t) => (
                  <TournamentCard key={t.id} t={t} onSelect={handleSelect} />
                ))}
              </div>
            </div>
          )}

          {/* Completed tournaments */}
          {!loading && closedTournaments.length > 0 && (
            <div>
              <div className="text-[8px] mb-2 px-1" style={{ color: "#7f8c8d" }}>
                PAST TOURNAMENTS
              </div>
              <div className="flex flex-col gap-2">
                {closedTournaments.map((t) => (
                  <TournamentCard key={t.id} t={t} onSelect={handleSelect} />
                ))}
              </div>
            </div>
          )}

          {/* Empty state */}
          {!loading && !error && tournaments.length === 0 && !creating && (
            <div
              className="pixel-border px-4 py-6 text-center"
              style={{ borderColor: "#2a2a4a", background: "rgba(12,10,24,0.7)", color: "#7f8c8d" }}
              aria-label="No tournaments available"
            >
              <div className="text-[9px] mb-2">NO TOURNAMENTS YET</div>
              <div className="text-[8px]">
                Create a sit-and-go to get started.
              </div>
            </div>
          )}

          {/* Escrow note */}
          <div
            className="pixel-border-thin px-4 py-3 text-[8px]"
            style={{
              borderColor: "#2a2a4a",
              background: "rgba(12,10,24,0.7)",
              color: "#7f8c8d",
              lineHeight: 1.8,
            }}
          >
            <span style={{ color: "#c47d2e" }}>ESCROW &amp; SETTLEMENT — </span>
            Buy-ins are held in each table's Soroban contract until the
            tournament concludes. Eliminated players' tokens are released in
            finish order. No single party — including the coordinator — can
            move funds without on-chain proof verification.
          </div>
        </div>
      </div>
    </PixelWorld>
  );
}
