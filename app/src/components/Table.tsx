"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import Link from "next/link";
import { Board } from "./Board";
import { Card } from "./Card";
import { PlayerSeat } from "./PlayerSeat";
import { ActionPanel } from "./ActionPanel";
import { PixelWorld } from "./PixelWorld";
import { PixelCat } from "./PixelCat";
import { PixelChip } from "./PixelChip";
import type { GameState, GamePhase } from "@/lib/game-state";
import { createInitialState } from "@/lib/game-state";
import * as api from "@/lib/api";
import {
  trySilentReconnect,
  getActiveAddress,
  type WalletSession,
} from "@/lib/wallet";
import { GameBoyButton, GameBoyModal } from "./GameBoyModal";
import { HandHistoryPanel } from "./HandHistoryPanel";
import { TransactionSimulation } from "./TransactionSimulation";
import { usePokerActions } from "@/lib/use-poker-actions";
import { getDealerLine } from "@/lib/dealer-lines";
import { subscribePokerTableEvents } from "@/lib/events";
import { getAlias, setAlias } from "@/lib/alias-store";
import {
  loadHandHistory,
  saveHandHistoryEntry,
  buildHandRankName,
  type HandHistoryEntry,
  type Street,
} from "@/lib/hand-history";
import {
  useTurnNotification,
  requestPermissionOnJoin,
} from "@/lib/use-notifications";

type ActiveRequest = "deal" | "flop" | "turn" | "river" | "showdown" | null;
type PlayMode = "single" | "headsup" | "multi";

interface TableProps {
  tableId: number;
  initialPlayMode?: PlayMode;
}

function isStellarAddress(address: string): boolean {
  return /^G[A-Z2-7]{55}$/.test(address.trim());
}

function shortAddress(address: string): string {
  return `${address.slice(0, 6)}...${address.slice(-6)}`;
}

function toNumber(value: unknown, fallback: number): number {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}

function mapOnChainPhase(phase: string): GamePhase | null {
  switch (phase) {
    case "Waiting":
      return "waiting";
    case "Dealing":
      return "dealing";
    case "Preflop":
      return "preflop";
    case "Flop":
      return "flop";
    case "Turn":
      return "turn";
    case "River":
      return "river";
    case "Showdown":
      return "showdown";
    case "Settlement":
      return "settlement";
    case "DealingFlop":
      return "preflop";
    case "DealingTurn":
      return "flop";
    case "DealingRiver":
      return "turn";
    default:
      return null;
  }
}

export function Table({ tableId, initialPlayMode }: TableProps) {
  const [game, setGame] = useState<GameState>(() => createInitialState(tableId));
  const [wallet, setWallet] = useState<WalletSession | null>(null);
  const [playMode, setPlayMode] = useState<PlayMode>(initialPlayMode ?? "headsup");
  const [error, setError] = useState<string | null>(null);
  const [walletVerificationError, setWalletVerificationError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [joiningTable, setJoiningTable] = useState(false);
  const [activeRequest, setActiveRequest] = useState<ActiveRequest>(null);
  const [onChainPhase, setOnChainPhase] = useState<string>("unknown");
  const [winnerAddress, setWinnerAddress] = useState<string | null>(null);
  const [lobby, setLobby] = useState<api.TableLobbyResponse | null>(null);
  const [botLine, setBotLine] = useState<string | null>(null);
  const [gameboyOpen, setGameboyOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyEntries, setHistoryEntries] = useState<HandHistoryEntry[]>(() =>
    loadHandHistory(tableId)
  );
  const [elapsed, setElapsed] = useState(0);
  const [, bumpAliasTick] = useState(0);
  const [betAmount, setBetAmount] = useState(0);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [chatInput, setChatInput] = useState("");
  const [chatMessages, setChatMessages] = useState<Array<{ alias: string; text: string; senderColor: string }>>([]);
  const [newMessagesCount, setNewMessagesCount] = useState(0);
  const [seatEmotes, setSeatEmotes] = useState<Record<number, string>>({});
  const wsRef = useRef<WebSocket | null>(null);
  const chatScrollRef = useRef<HTMLDivElement>(null);
  const autoStreetRef = useRef<string>("");
  const inferredModeRef = useRef(false);
  const streetLogRef = useRef<{ handNumber: number; streets: { street: Street; pot: number; boardCards: number[] }[] }>({
    handNumber: 0,
    streets: [],
  });

  const userAddress = wallet?.address;
  const userPlayer = userAddress
    ? game.players.find((p) => p.address === userAddress)
    : undefined;
  const isSoloBettingPhase =
    playMode === "single" &&
    ["preflop", "flop", "turn", "river"].includes(game.phase);
  const onChainTurnAddress = game.players[game.currentTurn]?.address;
  const displayedTurnAddress = isSoloBettingPhase
    ? userAddress
    : onChainTurnAddress;
  const isMyTurn = !!userAddress && displayedTurnAddress === userAddress;

  // Issue #47: browser notification + sound when it becomes the user's turn.
  useTurnNotification({ isMyTurn, tableName: `Table #${tableId}` });

  const isWalletSeated = !!wallet && !!userPlayer;
  const seatedAddresses = game.players
    .filter((p) => isStellarAddress(p.address))
    .map((p) => p.address);
  const tableSeatLabel =
    seatedAddresses.length > 0
      ? seatedAddresses.map(shortAddress).join(" vs ")
      : "NO SEATS YET";

  const syncOnChainState = useCallback(async () => {
    try {
      const [tableState, lobbyState] = await Promise.all([
        api.getParsedTableState(tableId),
        api.getTableLobby(tableId).catch(() => null),
      ]);
      const { parsed } = tableState;
      if (!parsed) return;
      if (lobbyState) {
        setLobby(lobbyState);
      }

      const phaseRaw = typeof parsed.phase === "string" ? parsed.phase : null;
      if (phaseRaw) {
        setOnChainPhase(phaseRaw);
      }
      const mappedPhase = phaseRaw ? mapOnChainPhase(phaseRaw) : null;

      const boardCards = Array.isArray(parsed.board_cards)
        ? parsed.board_cards
            .map((v) => toNumber(v, -1))
            .filter((v) => v >= 0)
        : null;

      const rawPlayers = Array.isArray(parsed.players)
        ? (parsed.players as Array<Record<string, unknown>>)
        : null;
      const walletByChain = new Map<string, string>();
      if (lobbyState?.seats) {
        for (const seat of lobbyState.seats) {
          if (seat.wallet_address) {
            walletByChain.set(seat.chain_address, seat.wallet_address);
          }
        }
      }

      setGame((prev) => {
        const rawHasWallet =
          !!userAddress &&
          !!rawPlayers?.some((raw) => typeof raw.address === "string" && raw.address === userAddress);
        const prevHasWallet = !!userAddress && prev.players.some((p) => p.address === userAddress);
        const aliasWalletSeatForLocalDev =
          !!userAddress && !!rawPlayers && rawPlayers.length > 0 && !rawHasWallet && phaseRaw !== "Waiting";
        const preserveLocalSeatAddresses =
          !!userAddress &&
          prevHasWallet &&
          !!rawPlayers &&
          rawPlayers.length === prev.players.length &&
          !rawHasWallet &&
          prev.phase !== "waiting";

        const mergedPlayers =
          rawPlayers && rawPlayers.length > 0
            ? rawPlayers.map((raw, index) => {
                const chainAddress =
                  typeof raw.address === "string"
                    ? raw.address
                    : prev.players[index]?.address ?? `seat-${index}`;
                const lobbyAddress = walletByChain.get(chainAddress);
                const address = preserveLocalSeatAddresses
                  ? prev.players[index]?.address ?? chainAddress
                  : lobbyAddress ?? chainAddress;
                const normalizedAddress =
                  aliasWalletSeatForLocalDev && index === 0 ? userAddress ?? address : address;
                const existing =
                  prev.players.find((p) => p.address === normalizedAddress) ?? prev.players[index];
                return {
                  address: normalizedAddress,
                  seat: toNumber(raw.seat_index, existing?.seat ?? index),
                  stack:
                    playMode === "single"
                      ? existing?.stack ?? 100
                      : toNumber(raw.stack, existing?.stack ?? 0),
                  betThisRound:
                    playMode === "single"
                      ? existing?.betThisRound ?? 0
                      : toNumber(raw.bet_this_round, existing?.betThisRound ?? 0),
                  folded: Boolean(raw.folded),
                  allIn: Boolean(raw.all_in),
                  cards: existing?.cards,
                };
              })
            : prev.players;

        return {
          ...prev,
          phase: mappedPhase ?? prev.phase,
          boardCards: boardCards ?? prev.boardCards,
          pot: playMode === "single" ? prev.pot : toNumber(parsed.pot, prev.pot),
          currentTurn: toNumber(parsed.current_turn, prev.currentTurn),
          dealerSeat: toNumber(parsed.dealer_seat, prev.dealerSeat),
          handNumber: toNumber(parsed.hand_number, prev.handNumber),
          players: mergedPlayers,
        };
      });
    } catch {
      // Non-fatal; UI still works off latest known state.
    }
  }, [playMode, tableId, userAddress]);

  const hydrateMyCards = useCallback(
    async (auth: WalletSession) => {
      try {
        const cards = await api.getPlayerCards(tableId, auth.address, auth);
        setGame((prev) => ({
          ...prev,
          players: prev.players.map((p) =>
            p.address === auth.address
              ? { ...p, cards: [cards.card1, cards.card2] }
              : p
          ),
        }));
      } catch {
        // Cards may not be available yet; keep UI usable.
      }
    },
    [tableId]
  );

  const {
    handleJoinTable,
    handleReveal,
    handleShowdown,
    handleAction,
    joinSimulation,
    actionSimulation,
    pendingAction,
  } = usePokerActions({
    tableId,
    wallet,
    playMode,
    game,
    lobby,
    setGame,
    setError,
    setLoading,
    setActiveRequest,
    setWinnerAddress,
    setBotLine,
    setJoiningTable,
    syncOnChainState,
    hydrateMyCards,
  });

  useEffect(() => {
    void syncOnChainState();

    // Event-driven refresh: subscribe to the poker table contract's events and
    // re-sync immediately whenever a state transition is emitted. A slower
    // interval remains as a safety net in case an event is missed.
    let unsubscribe: (() => void) | undefined;
    void subscribePokerTableEvents(() => {
      void syncOnChainState();
    })
      .then((sub) => {
        unsubscribe = sub.stop;
      })
      .catch(() => {
        // Event subscription unavailable — fall back to interval polling only.
      });

    const interval = setInterval(() => {
      void syncOnChainState();
    }, 8000);
    return () => {
      clearInterval(interval);
      unsubscribe?.();
    };
  }, [syncOnChainState]);

  // Infer sensible default play mode from table capacity when no explicit mode was provided.
  useEffect(() => {
    if (inferredModeRef.current) return;
    if (initialPlayMode) {
      inferredModeRef.current = true;
      return;
    }
    if (!lobby) return;
    setPlayMode(lobby.max_players >= 3 ? "multi" : "headsup");
    inferredModeRef.current = true;
  }, [initialPlayMode, lobby]);

  // Silent reconnect on mount + request notification permission (#47).
  useEffect(() => {
    void requestPermissionOnJoin();
    if (!wallet) {
      void trySilentReconnect().then((session) => {
        if (session) {
          setWallet(session);
        }
      });
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Periodic wallet spoofing and ownership verification check
  useEffect(() => {
    if (!wallet) {
      setWalletVerificationError(null);
      return;
    }

    let isSubscribed = true;
    let timerId: NodeJS.Timeout;

    const performVerification = async () => {
      try {
        const activeAddr = await getActiveAddress();
        if (!isSubscribed) return;

        if (!activeAddr) {
          setWalletVerificationError("Freighter wallet is locked or disconnected. Please unlock Freighter.");
          return;
        }

        if (activeAddr.trim().toLowerCase() !== wallet.address.trim().toLowerCase()) {
          setWalletVerificationError(
            `Wallet mismatch detected! Active Freighter account (${activeAddr.slice(0, 6)}...${activeAddr.slice(-4)}) does not match playing account (${wallet.address.slice(0, 6)}...${wallet.address.slice(-4)}).`
          );
          return;
        }

        // Silent check succeeded, now perform cryptographic challenge-response
        const { challenge } = await api.getWalletChallenge(wallet.address);
        if (!isSubscribed) return;

        const signature = await wallet.signMessage(challenge);
        if (!isSubscribed) return;

        const { verified } = await api.verifyWalletChallenge(wallet.address, challenge, signature);
        if (!isSubscribed) return;

        if (!verified) {
          setWalletVerificationError("Cryptographic verification failed. Wallet spoofing detected!");
        } else {
          setWalletVerificationError(null);
        }
      } catch (err) {
        if (!isSubscribed) return;
        setWalletVerificationError(
          "Wallet ownership verification failed: " +
            (err instanceof Error ? err.message : String(err))
        );
      }
    };

    // Run verification immediately, then every 30 seconds
    void performVerification();
    timerId = setInterval(() => {
      void performVerification();
    }, 30000);

    return () => {
      isSubscribed = false;
      clearInterval(timerId);
    };
  }, [wallet]);

  // Elapsed timer while loading
  useEffect(() => {
    if (loading) {
      setElapsed(0);
      const interval = setInterval(() => {
        setElapsed((prev) => prev + 1);
      }, 1000);
      return () => clearInterval(interval);
    } else {
      setElapsed(0);
    }
  }, [loading]);

  // Capture a street-by-street snapshot (pot + board) as each hand progresses,
  // then persist a hand-history entry locally once the hand reaches
  // settlement so it stays viewable for the rest of the session even after
  // the live table state moves on to the next hand.
  useEffect(() => {
    if (streetLogRef.current.handNumber !== game.handNumber) {
      streetLogRef.current = { handNumber: game.handNumber, streets: [] };
    }

    const street = game.phase;
    if (street === "preflop" || street === "flop" || street === "turn" || street === "river") {
      const alreadyLogged = streetLogRef.current.streets.some((s) => s.street === street);
      if (!alreadyLogged) {
        streetLogRef.current.streets.push({
          street,
          pot: game.pot,
          boardCards: [...game.boardCards],
        });
      }
      return;
    }

    if (street === "settlement" && streetLogRef.current.streets.length > 0) {
      const entry: HandHistoryEntry = {
        tableId,
        handNumber: game.handNumber,
        timestamp: Date.now(),
        streets: streetLogRef.current.streets,
        finalPot: game.pot,
        boardCards: game.boardCards,
        holeCards: userPlayer?.cards,
        handRankName: buildHandRankName(userPlayer?.cards, game.boardCards),
        winnerAddress,
        txHash: game.lastTxHash,
      };
      saveHandHistoryEntry(entry);
      setHistoryEntries(loadHandHistory(tableId));
      streetLogRef.current = { handNumber: game.handNumber, streets: [] };
    }
  }, [game.phase, game.handNumber, game.pot, game.boardCards, game.lastTxHash, tableId, userPlayer, winnerAddress]);

  const currentBet = Math.max(...game.players.map((p) => p.betThisRound), 0);
  const displayCurrentBet = currentBet;
  const displayPot = game.pot;
  const displayMyBet = userPlayer?.betThisRound || 0;
  const displayMyStack = userPlayer?.stack || 0;
  const canStartHand = !!wallet && isWalletSeated;
  const seatStatusHint =
    wallet && !isWalletSeated && seatedAddresses.length > 0
      ? "Connected wallet is not seated in this hand. Click JOIN TABLE first, then DEAL."
      : null;

  useEffect(() => {
    if (!wallet || loading) {
      return;
    }

    const key = `${game.handNumber}:${onChainPhase}`;
    let next: (() => Promise<void>) | null = null;

    switch (onChainPhase) {
      case "DealingFlop":
        next = async () => handleReveal("flop");
        break;
      case "DealingTurn":
        next = async () => handleReveal("turn");
        break;
      case "DealingRiver":
        next = async () => handleReveal("river");
        break;
      case "Showdown":
        next = handleShowdown;
        break;
      case "Waiting":
      case "Settlement":
      case "Preflop":
      case "Flop":
      case "Turn":
      case "River":
      case "Dealing":
        autoStreetRef.current = "";
        return;
      default:
        return;
    }

    if (!next || autoStreetRef.current === key) {
      return;
    }
    autoStreetRef.current = key;
    void next();
  }, [game.handNumber, handleReveal, handleShowdown, loading, onChainPhase, wallet]);

  const dealerLine = getDealerLine({
    loading,
    elapsed,
    activeRequest,
    playMode,
    botLine,
    onChainPhase,
    gamePhase: game.phase,
    wallet: !!wallet,
    isWalletSeated,
    seatedAddresses,
    tableSeatLabel,
    winnerAddress,
    userAddress,
    lobby,
  });

  // Keyboard shortcuts listener
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }

      if (e.key === "?" || e.key === "/") {
        e.preventDefault();
        setShortcutsOpen((prev) => !prev);
        return;
      }

      const disabled = !isMyTurn || loading;
      if (disabled) return;

      const activeBetting = ["preflop", "flop", "turn", "river"].includes(game.phase);
      if (!activeBetting) return;

      const callAmount = Math.max(displayCurrentBet - displayMyBet, 0);
      const minBet = Math.max(displayCurrentBet * 2, 1);

      switch (e.key.toLowerCase()) {
        case "f":
          e.preventDefault();
          handleAction("fold");
          break;
        case "c":
          if (callAmount === 0) {
            e.preventDefault();
            handleAction("check");
          }
          break;
        case "b":
          e.preventDefault();
          if (callAmount > 0) {
            handleAction("call", callAmount);
          } else {
            handleAction("bet", betAmount || minBet);
          }
          break;
        case "r":
          if (displayCurrentBet > 0 && displayMyStack > callAmount) {
            e.preventDefault();
            handleAction("raise", betAmount || minBet);
          }
          break;
        case "a":
          if (displayMyStack > 0) {
            e.preventDefault();
            handleAction("allin", displayMyStack);
          }
          break;
        case "1":
        case "2":
        case "3":
        case "4":
        case "5":
          e.preventDefault();
          if (displayMyStack > callAmount) {
            const pot = displayPot;
            let sizeMultiplier = 0.5;
            if (e.key === "2") sizeMultiplier = 2/3;
            else if (e.key === "3") sizeMultiplier = 0.75;
            else if (e.key === "4") sizeMultiplier = 1.0;
            else if (e.key === "5") sizeMultiplier = 2.0;

            const calculated = Math.floor(pot * sizeMultiplier);
            const clamped = Math.max(minBet, Math.min(calculated, displayMyStack));
            setBetAmount(clamped);
          }
          break;
        default:
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    isMyTurn,
    loading,
    game.phase,
    displayCurrentBet,
    displayMyBet,
    displayMyStack,
    displayPot,
    betAmount,
    handleAction,
  ]);

  const EMOTES = ["😃", "😢", "😠", "😎", "🤔", "🎉"];

  useEffect(() => {
    if (chatScrollRef.current) {
      chatScrollRef.current.scrollTop = chatScrollRef.current.scrollHeight;
    }
  }, [chatMessages, chatOpen]);

  useEffect(() => {
    let active = true;
    let ws: WebSocket | null = null;
    let reconnectTimeout: NodeJS.Timeout;

    const connect = () => {
      if (!active) return;
      const base = api.COORDINATOR_API_BASE;
      const wsBase = base.replace(/^http:/, "ws:").replace(/^https:/, "wss:");
      const wsUrl = `${wsBase}/api/table/${tableId}/chat/ws`;

      ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as {
            seat_index: number;
            alias: string;
            text?: string;
            emote?: string;
          };

          if (data.text) {
            const colors = ["#ff6b6b", "#4dabf7", "#51cf66", "#fcc419", "#cc5de8", "#20c997"];
            const senderColor = colors[data.seat_index % colors.length];
            setChatMessages((prev) => [
              ...prev,
              { alias: data.alias, text: data.text || "", senderColor },
            ]);
            if (!chatOpen) {
              setNewMessagesCount((c) => c + 1);
            }
          }

          if (data.emote) {
            setSeatEmotes((prev) => ({ ...prev, [data.seat_index]: data.emote || "" }));
            setTimeout(() => {
              setSeatEmotes((prev) => {
                const copy = { ...prev };
                delete copy[data.seat_index];
                return copy;
              });
            }, 3000);
          }
        } catch {
          // Ignore parse errors
        }
      };

      ws.onclose = () => {
        if (active) {
          reconnectTimeout = setTimeout(connect, 3000);
        }
      };

      ws.onerror = () => {
        ws?.close();
      };
    };

    connect();

    return () => {
      active = false;
      clearTimeout(reconnectTimeout);
      ws?.close();
    };
  }, [tableId, chatOpen]);

  const sendChatMessage = (e: React.FormEvent) => {
    e.preventDefault();
    if (!chatInput.trim() || !wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      return;
    }

    const mySeat = userPlayer ? userPlayer.seat : 0;
    const myAlias = userAddress ? (getAlias(userAddress) || `Seat ${mySeat}`) : `Seat ${mySeat}`;

    const payload = {
      seat_index: mySeat,
      alias: myAlias,
      text: chatInput.trim(),
    };

    wsRef.current.send(JSON.stringify(payload));
    setChatInput("");
  };

  const sendEmote = (emote: string) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      return;
    }

    const mySeat = userPlayer ? userPlayer.seat : 0;
    const myAlias = userAddress ? (getAlias(userAddress) || `Seat ${mySeat}`) : `Seat ${mySeat}`;

    const payload = {
      seat_index: mySeat,
      alias: myAlias,
      emote,
    };

    wsRef.current.send(JSON.stringify(payload));
  };

  return (
    <PixelWorld>
      <div className="min-h-screen flex flex-col items-center gap-4 p-4 pt-6 relative z-[10]">
        {/* Header bar */}
        <div className="w-full max-w-3xl flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Link
              href="/"
              className="text-[24px]"
              style={{
                color: "#f5e6c8",
                textShadow: "2px 2px 0 #2c3e50",
                textDecoration: "none",
                fontFamily: "'Press Start 2P', monospace",
              }}
            >
              ←
            </Link>
            <h1
              className="text-[13px]"
              style={{
                color: "white",
                textShadow: "2px 2px 0 #2c3e50",
              }}
            >
              TABLE #{tableId}
            </h1>
            <GameBoyButton onClick={() => setGameboyOpen(true)} />
            <button
              onClick={() => setHistoryOpen(true)}
              className="text-[9px] mr-2"
              style={{
                background: "none",
                border: "none",
                color: "#c8e6ff",
                textDecoration: "underline",
                cursor: "pointer",
                padding: 0,
              }}
              title="Hand History"
            >
              HISTORY
            </button>
            <button
              onClick={() => setShortcutsOpen(true)}
              className="text-[9px]"
              style={{
                background: "none",
                border: "none",
                color: "#ffc078",
                textDecoration: "underline",
                cursor: "pointer",
                padding: 0,
              }}
              title="Keyboard Shortcuts"
            >
              KEYS [?]
            </button>
          </div>

          <div className="flex items-center gap-3">
            <div className="text-[9px]" style={{ color: "#c8e6ff" }}>
              HAND #{game.handNumber} | {game.phase.toUpperCase()}
            </div>

            {(() => {
              const explorerUrl = game.lastTxHash
                ? `https://stellar.expert/explorer/testnet/tx/${game.lastTxHash}`
                : wallet
                  ? `https://stellar.expert/explorer/testnet/account/${wallet.address}`
                  : null;
              if (!explorerUrl) return null;
              return (
                <a
                  href={explorerUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[9px]"
                  style={{
                    color: "#ffc078",
                    textDecoration: "none",
                    textShadow: "1px 1px 0 rgba(0,0,0,0.5)",
                  }}
                >
                  {game.lastTxHash ? "VIEW TX ↗" : "EXPLORER ↗"}
                </a>
              );
            })()}

            {wallet && (
              <div
                className="pixel-border-thin px-2 py-1"
                style={{
                  background: "rgba(39, 174, 96, 0.2)",
                  fontSize: "9px",
                  color: "#27ae60",
                }}
              >
                {shortAddress(wallet.address)}
              </div>
            )}
          </div>
        </div>


        {/* Dealer line */}
        <div
          className="w-full max-w-3xl pixel-border-thin px-4 py-2"
          style={{
            background: loading
              ? "rgba(40, 20, 8, 0.9)"
              : "rgba(12, 10, 24, 0.88)",
            borderColor: loading ? "#f1c40f" : "#c47d2e",
            animation: loading
              ? "dealerPulse 1.5s ease-in-out infinite"
              : undefined,
          }}
        >
          {loading && (
            <div className="flex items-center gap-2 mb-1">
              <div
                style={{
                  width: "8px",
                  height: "8px",
                  border: "2px solid #f1c40f",
                  borderTopColor: "transparent",
                  borderRadius: "50%",
                  animation: "spin 0.6s linear infinite",
                }}
              />
              <span
                className="text-[10px]"
                style={{ color: "#f1c40f", fontWeight: "bold" }}
              >
                GENERATING PROOF...
              </span>
            </div>
          )}
          <span
            className={loading ? "text-[10px]" : "text-[9px]"}
            style={{ color: loading ? "#ffeaa7" : "#f5e6c8" }}
          >
            {dealerLine}
          </span>
        </div>

        <style jsx>{`
          @keyframes dealerPulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.85; }
          }
        `}</style>

        {/* Error display */}
        {error && (
          <div
            className="pixel-border-thin px-4 py-2"
            style={{
              background: "rgba(231, 76, 60, 0.2)",
              borderColor: "#e74c3c",
            }}
          >
            <span className="text-[9px]" style={{ color: "#e74c3c" }}>
              {error}
            </span>
          </div>
        )}

        {/* ═══ THE POKER TABLE ═══ */}
        <div className="w-full max-w-3xl relative" style={{ minHeight: "400px" }}>
          <div
            className="pixel-border relative w-full flex flex-col items-center justify-center gap-4"
            style={{
              background:
                "radial-gradient(ellipse at center, var(--felt-light) 0%, var(--felt-mid) 40%, var(--felt-dark) 100%)",
              borderColor: "#6b4f12",
              padding: "40px 20px 40px 20px",
              minHeight: "360px",
              boxShadow:
                "inset 0 0 60px rgba(0,0,0,0.3), 0 8px 0 0 rgba(0,0,0,0.4), inset -4px -4px 0px 0px rgba(0,0,0,0.3), inset 4px 4px 0px 0px rgba(255,255,255,0.1)",
            }}
          >
            <div
              className="absolute inset-2 pointer-events-none"
              style={{
                border: "2px solid rgba(139, 105, 20, 0.3)",
              }}
            />

            {/* ── OPPONENTS (top) ── */}
            <div className="flex flex-wrap gap-6 items-end justify-center">
              {game.players
                .filter((p) => !userAddress || p.address !== userAddress)
                .map((player) => (
                  <PlayerSeat
                    key={player.address}
                    player={player}
                    isCurrentTurn={displayedTurnAddress === player.address}
                    isDealer={player.seat === game.dealerSeat}
                    isUser={false}
                    isWinner={!!winnerAddress && player.address === winnerAddress}
                    isBot={playMode === "single"}
                    alias={getAlias(player.address) ?? undefined}
                    hideChipStats={false}
                    activeEmote={seatEmotes[player.seat]}
                  />
                ))}

              {game.players.filter((p) => !userAddress || p.address !== userAddress).length === 0 && (
                <>
                  {[
                    { sprite: 17, flipped: false },
                    { sprite: 20, flipped: true },
                  ].map((seat, i) => (
                    <div key={i} className="flex flex-col items-center gap-2" style={{ opacity: 0.25 }}>
                      <PixelCat sprite={seat.sprite} size={48} flipped={seat.flipped} />
                      <div className="flex gap-1">
                        <Card faceDown size="sm" />
                        <Card faceDown size="sm" />
                      </div>
                      <div className="text-[8px]" style={{ color: 'rgba(255,255,255,0.3)' }}>
                        EMPTY
                      </div>
                    </div>
                  ))}
                </>
              )}
            </div>

            {/* ── BOARD (center) ── */}
            <div className="w-full flex flex-col items-center gap-2 my-2" style={{
              borderTop: '2px solid rgba(139, 105, 20, 0.2)',
              borderBottom: '2px solid rgba(139, 105, 20, 0.2)',
              padding: '12px 0',
            }}>
              <Board cards={game.boardCards} pot={displayPot} />

              {game.phase === "waiting" && wallet && !isWalletSeated && playMode !== "single" && (
                <button
                  onClick={() => void handleJoinTable()}
                  disabled={loading || joiningTable}
                  className="pixel-btn pixel-btn-blue text-[9px]"
                  style={{ padding: "6px 14px", opacity: loading || joiningTable ? 0.7 : 1 }}
                >
                  {joiningTable ? "JOINING..." : "JOIN TABLE"}
                </button>
              )}

              <div className="w-full max-w-xl mt-1">
                <ActionPanel
                  phase={game.phase}
                  isMyTurn={isMyTurn}
                  currentBet={displayCurrentBet}
                  myBet={displayMyBet}
                  myStack={displayMyStack}
                  onAction={handleAction}
                  onChainConfirmed={game.onChainConfirmed}
                  canStartHand={canStartHand}
                  canResolveShowdown={!!wallet}
                  statusHint={seatStatusHint}
                  loading={loading}
                  isSolo={playMode === "single"}
                  betAmount={betAmount}
                  setBetAmount={setBetAmount}
                />
              </div>
            </div>

            {/* ── YOU (bottom) ── */}
            <div className="flex gap-4 items-start">
              {userPlayer ? (
                <PlayerSeat
                  player={userPlayer}
                  isCurrentTurn={isMyTurn}
                  isDealer={userPlayer.seat === game.dealerSeat}
                  isUser={true}
                  isWinner={!!winnerAddress && userPlayer.address === winnerAddress}
                  alias={getAlias(userPlayer.address) ?? undefined}
                  onEditAlias={() => {
                    const next = window.prompt(
                      "Set your table alias (max 16 chars):",
                      getAlias(userPlayer.address) ?? ""
                    );
                    if (next !== null) {
                      setAlias(userPlayer.address, next);
                      bumpAliasTick((t) => t + 1);
                    }
                  }}
                  hideChipStats={false}
                  activeEmote={seatEmotes[userPlayer.seat]}
                />
              ) : (
                <div className="flex flex-col items-center gap-2" style={{ opacity: 0.25 }}>
                  <PixelCat sprite={18} size={72} />
                  <div className="flex gap-1">
                    <Card faceDown size="md" />
                    <Card faceDown size="md" />
                  </div>
                  <div className="text-[9px]" style={{ color: 'rgba(255,255,255,0.3)' }}>
                    {wallet ? "WAITING TO JOIN..." : "CONNECT WALLET"}
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* MPC Status footer */}
        <div className="flex flex-col items-center gap-1 mt-2">
          <div className="flex items-center gap-2">
            <div
              style={{
                width: "6px",
                height: "6px",
                background: "#27ae60",
                boxShadow: "0 0 4px #27ae60",
              }}
            />
            <span className="text-[8px]" style={{ color: "#7f8c8d" }}>
              MPC: 3/3 NODES | TACEO CO-NOIR REP3
            </span>
          </div>
          {game.lastTxHash && (
            <div className="flex items-center gap-1">
              {game.onChainConfirmed ? (
                <PixelChip color="gold" size={2} />
              ) : (
                <div style={{ width: "4px", height: "4px", background: "#f1c40f" }} />
              )}
              <span className="text-[8px]" style={{ color: "#7f8c8d" }}>
                TX:{" "}
                <a
                  href={`https://stellar.expert/explorer/testnet/tx/${game.lastTxHash}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  style={{ color: "#ffc078", textShadow: "1px 1px 0 rgba(0,0,0,0.5)" }}
                >
                  {game.lastTxHash.slice(0, 8)}...{game.lastTxHash.slice(-8)}
                </a>
              </span>
            </div>
          )}
        </div>

        <div className="fixed bottom-0 left-[5%] z-[5]" style={{ transform: 'translateY(15%)' }}>
          <PixelCat sprite={17} size={36} />
        </div>
        <div className="fixed bottom-0 right-[5%] z-[5]" style={{ transform: 'translateY(10%)' }}>
          <PixelCat sprite={21} size={48} flipped />
        </div>
      </div>

      <GameBoyModal
        open={gameboyOpen}
        onClose={() => setGameboyOpen(false)}
        onLogout={() => setWallet(null)}
      />

      <HandHistoryPanel
        open={historyOpen}
        onClose={() => setHistoryOpen(false)}
        entries={historyEntries}
      />

      {shortcutsOpen && (
        <div
          className="fixed inset-0 bg-black/60 flex items-center justify-center z-[1000] p-4"
          onClick={() => setShortcutsOpen(false)}
        >
          <div
            className="pixel-border max-w-sm w-full p-6 text-left relative"
            style={{ background: "#1a120c" }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={() => setShortcutsOpen(false)}
              className="absolute top-3 right-3 text-[12px]"
              style={{ background: "none", border: "none", color: "#f5e6c8", cursor: "pointer" }}
            >
              ✕
            </button>
            <h2 className="text-[11px] mb-4 text-[#f1c40f] text-center" style={{ fontFamily: "'Press Start 2P', monospace" }}>KEYBOARD SHORTCUTS</h2>
            <div className="flex flex-col gap-3 text-[8px] leading-relaxed" style={{ fontFamily: "'Press Start 2P', monospace" }}>
              <div className="flex justify-between border-b border-gray-800 pb-1">
                <span>[F]</span> <span style={{ color: "#bdc3c7" }}>Fold</span>
              </div>
              <div className="flex justify-between border-b border-gray-800 pb-1">
                <span>[C]</span> <span style={{ color: "#bdc3c7" }}>Check</span>
              </div>
              <div className="flex justify-between border-b border-gray-800 pb-1">
                <span>[B]</span> <span style={{ color: "#bdc3c7" }}>Bet / Call</span>
              </div>
              <div className="flex justify-between border-b border-gray-800 pb-1">
                <span>[R]</span> <span style={{ color: "#bdc3c7" }}>Raise</span>
              </div>
              <div className="flex justify-between border-b border-gray-800 pb-1">
                <span>[A]</span> <span style={{ color: "#bdc3c7" }}>All-in</span>
              </div>
              <div className="flex justify-between border-b border-gray-800 pb-1">
                <span>[1] - [5]</span> <span style={{ color: "#bdc3c7" }}>Quick Bet: 1/2, 2/3, 3/4, pot, 2x pot</span>
              </div>
              <div className="flex justify-between border-b border-gray-800 pb-1">
                <span>[?]</span> <span style={{ color: "#bdc3c7" }}>Toggle Help Overlay</span>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Chat Overlay Toggle Button */}
      <button
        onClick={() => {
          setChatOpen((prev) => !prev);
          setNewMessagesCount(0);
        }}
        className="pixel-btn pixel-btn-blue text-[9px] fixed bottom-4 right-4 z-40"
        style={{ padding: "8px 12px" }}
      >
        CHAT {newMessagesCount > 0 ? `(${newMessagesCount})` : ""}
      </button>

      {/* Floating Chat Drawer */}
      {chatOpen && (
        <div
          className="pixel-border-thin fixed bottom-16 right-4 z-40 flex flex-col w-72 h-64 p-3 gap-2"
          style={{
            background: "rgba(20, 12, 8, 0.95)",
            borderColor: "var(--ui-border)",
          }}
        >
          <div className="flex justify-between items-center border-b border-gray-800 pb-1" style={{ fontFamily: "'Press Start 2P', monospace" }}>
            <span className="text-[9px] text-[#f1c40f]">TABLE CHAT</span>
            <button
              onClick={() => setChatOpen(false)}
              className="text-[9px] text-[#95a5a6]"
              style={{ background: "none", border: "none", cursor: "pointer" }}
            >
              [X]
            </button>
          </div>
          
          {/* Messages list */}
          <div
            ref={chatScrollRef}
            className="flex-1 overflow-y-auto flex flex-col gap-2 text-[8px] text-left p-1"
            style={{ scrollbarWidth: "thin", fontFamily: "'Press Start 2P', monospace", maxHeight: "140px" }}
          >
            {chatMessages.length === 0 ? (
              <div className="text-gray-600 text-center mt-8 italic" style={{ fontSize: "8px" }}>No messages yet.</div>
            ) : (
              chatMessages.map((msg, idx) => (
                <div key={idx} className="leading-relaxed">
                  <span style={{ color: msg.senderColor }}>{msg.alias}: </span>
                  <span className="text-white">{msg.text}</span>
                </div>
              ))
            )}
          </div>
          
          {/* 6 Quick Emotes */}
          <div className="flex justify-between items-center gap-1 mt-1 border-t border-gray-900 pt-2">
            {EMOTES.map((emote, idx) => (
              <button
                key={idx}
                onClick={() => sendEmote(emote)}
                className="hover:scale-110 transition-transform p-1 text-[14px]"
                style={{ background: "none", border: "none", cursor: "pointer" }}
              >
                {emote}
              </button>
            ))}
          </div>
          
          {/* Input row */}
          <form onSubmit={sendChatMessage} className="flex gap-1 mt-1">
            <input
              type="text"
              value={chatInput}
              onChange={(e) => setChatInput(e.target.value)}
              placeholder="Say something..."
              className="flex-1 px-2 py-1 text-[8px]"
            />
            <button
              type="submit"
              className="pixel-btn pixel-btn-green text-[8px]"
              style={{ padding: "4px 8px" }}
            >
              SEND
            </button>
          </form>
        </div>
      )}

      {/* Wallet Verification Mismatch/Spoof Warning Overlay */}
      {walletVerificationError && (
        <div
          className="fixed inset-0 bg-black/85 flex items-center justify-center z-[9999] p-4 backdrop-blur-sm"
          onClick={(e) => e.stopPropagation()}
        >
          <div
            className="max-w-md w-full pixel-border-thin p-6 text-center flex flex-col gap-4"
            style={{
              background: "rgba(20, 10, 10, 0.95)",
              borderColor: "#ff7675",
              color: "#ff7675",
              boxShadow: "0 0 25px rgba(255, 118, 117, 0.3)",
            }}
          >
            <h2
              className="text-[14px]"
              style={{
                fontFamily: "'Press Start 2P', monospace",
                textShadow: "2px 2px 0 #000",
              }}
            >
              ⚠️ SECURITY WARNING ⚠️
            </h2>
            <p
              className="text-[10px] leading-relaxed"
              style={{
                color: "#ff8c8a",
                fontFamily: "'Press Start 2P', monospace",
              }}
            >
              {walletVerificationError}
            </p>
            <div
              className="text-[9px] mt-2 p-3 bg-red-950/40 border border-red-500/20 text-left"
              style={{ color: "#fab1a0", fontFamily: "'Press Start 2P', monospace", lineHeight: "1.6" }}
            >
              TO RESOLVE THIS:
              <br />
              1. OPEN YOUR FREIGHTER EXTENSION.
              <br />
              2. SWAP BACK TO THE ACCOUNT FOR ADDRESS:
              <br />
              <span className="font-mono text-white select-all break-all block mt-2 text-[8px]">
                {wallet?.address}
              </span>
            </div>
          </div>
        </div>
      )}

      {/* Transaction Simulations */}
      {joinSimulation.showSimulation && joinSimulation.simulation && (
        <TransactionSimulation
          simulation={joinSimulation.simulation}
          loading={joinSimulation.loading}
          onConfirm={() => {
            // The join table simulation will be handled by the hook
            // but we need to provide the table ID and buy-in parameters
            const tableState = game.tableState?.parsed;
            if (tableState && typeof tableState === "object" && "config" in tableState) {
              const config = tableState.config as { min_buy_in?: unknown };
              const buyIn = typeof config.min_buy_in === "bigint" 
                ? config.min_buy_in 
                : BigInt("1000000000");
              joinSimulation.confirmJoin(tableId, buyIn);
            }
          }}
          onCancel={() => {
            joinSimulation.cancelSimulation();
          }}
        />
      )}

      {actionSimulation.showSimulation && actionSimulation.simulation && pendingAction && (
        <TransactionSimulation
          simulation={actionSimulation.simulation}
          loading={actionSimulation.loading}
          onConfirm={() => {
            actionSimulation.confirmAction(
              tableId, 
              pendingAction.action, 
              pendingAction.amount
            );
          }}
          onCancel={() => {
            actionSimulation.cancelSimulation();
          }}
        />
      )}
    </PixelWorld>
  );
}
