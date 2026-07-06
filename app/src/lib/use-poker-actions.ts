import { useCallback, useState } from "react";
import type { GameState, GamePhase } from "@/lib/game-state";
import * as api from "@/lib/api";
import { joinTableOnChain, playerActionOnChain } from "@/lib/onchain";
import type { WalletSession } from "@/lib/wallet";
import { computeSoloBet } from "./use-solo-betting";
import { useJoinTableSimulation, usePlayerActionSimulation } from "./use-transaction-simulation";
import type { SimulationResult } from "./transaction-simulation";

type PlayMode = "single" | "headsup" | "multi";

function isStellarAddress(address: string): boolean {
  return /^G[A-Z2-7]{55}$/.test(address.trim());
}

function normalizeTxHash(hash: string | null | undefined): string | undefined {
  if (!hash) return undefined;
  if (hash === "null" || hash === "submitted") return undefined;
  return hash;
}

function toBigInt(value: unknown, fallback: bigint): bigint {
  if (typeof value === "bigint") return value;
  if (typeof value === "number" && Number.isFinite(value)) {
    return BigInt(Math.trunc(value));
  }
  if (typeof value === "string" && value.trim().length > 0) {
    try {
      return BigInt(value.trim());
    } catch {
      return fallback;
    }
  }
  return fallback;
}

interface PokerActionsConfig {
  tableId: number;
  wallet: WalletSession | null;
  playMode: PlayMode;
  game: GameState;
  lobby: api.TableLobbyResponse | null;
  setGame: React.Dispatch<React.SetStateAction<GameState>>;
  setError: (error: string | null) => void;
  setLoading: (loading: boolean) => void;
  setActiveRequest: (request: "deal" | "flop" | "turn" | "river" | "showdown" | null) => void;
  setWinnerAddress: (address: string | null) => void;
  setBotLine: (line: string | null) => void;
  setJoiningTable: (joining: boolean) => void;
  syncOnChainState: () => Promise<void>;
  hydrateMyCards: (auth: WalletSession) => Promise<void>;
}

export function usePokerActions(config: PokerActionsConfig) {
  const {
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
  } = config;

  const [pendingAction, setPendingAction] = useState<{
    action: string;
    amount?: number;
  } | null>(null);

  const joinTableSim = useJoinTableSimulation(wallet, async () => {
    await syncOnChainState();
    await hydrateMyCards(wallet!);
  });

  const playerActionSim = usePlayerActionSimulation(wallet, async () => {
    await syncOnChainState();
    setPendingAction(null);
  });

  const claimedWallets = (lobby?.seats ?? [])
    .map((seat) => seat.wallet_address)
    .filter((address): address is string => !!address);

  const handleJoinTable = useCallback(async () => {
    if (!wallet) {
      setError("Connect Freighter wallet before joining a table");
      return;
    }
    setJoiningTable(true);
    setError(null);
    try {
      const tableState = await api.getParsedTableState(tableId);
      const minBuyInRaw =
        tableState.parsed &&
        typeof tableState.parsed === "object" &&
        "config" in tableState.parsed
          ? (tableState.parsed.config as { min_buy_in?: unknown })?.min_buy_in
          : undefined;
      const buyIn = toBigInt(minBuyInRaw, BigInt("1000000000"));

      // Start simulation instead of direct execution
      joinTableSim.joinTable(tableId, buyIn);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Join table failed");
    } finally {
      setJoiningTable(false);
    }
  }, [joinTableSim, tableId, wallet, setError, setJoiningTable]);

  const resolvePlayersForDeal = useCallback((): string[] | null => {
    if (!wallet) {
      setError("Connect Freighter wallet before starting a hand");
      return null;
    }

    if (!isStellarAddress(wallet.address)) {
      setError("Connected wallet address is invalid");
      return null;
    }

    if (!claimedWallets.includes(wallet.address)) {
      setError("Join table first so your wallet is seated");
      return null;
    }

    const joinedWallets = lobby?.joined_wallets ?? claimedWallets.length;
    if (playMode === "headsup" && joinedWallets < 2) {
      setError("Two-player mode needs 2 joined wallets");
      return null;
    }
    if (playMode === "multi" && joinedWallets < 3) {
      setError("3-6 player mode needs at least 3 joined wallets");
      return null;
    }

    // Empty list tells coordinator to resolve all on-chain seats from lobby.
    return [];
  }, [wallet, claimedWallets, lobby, playMode, setError]);

  const handleDeal = useCallback(
    async (players: string[]) => {
      if (!wallet) {
        setError("Connect Freighter wallet before dealing");
        return;
      }

      setLoading(true);
      setActiveRequest("deal");
      setError(null);
      setWinnerAddress(null);

      try {
        const result = await api.requestDeal(tableId, players, wallet);
        const txHash = normalizeTxHash(result.tx_hash);

        setGame((prev) => ({
          ...prev,
          phase: "preflop" as GamePhase,
          boardCards: [],
          handNumber: prev.handNumber + 1,
          pot: playMode === "single" ? 0 : prev.pot,
          lastTxHash: txHash,
          proofSize: result.proof_size,
          onChainConfirmed: !!txHash,
          players: playMode === "single"
            ? prev.players.map((p) => ({
                ...p,
                stack: 100,
                betThisRound: 0,
                folded: false,
                allIn: false,
              }))
            : players.length > 0
              ? players.map((address, seat) => ({
                  address,
                  seat,
                  stack: 10000,
                  betThisRound: 0,
                  folded: false,
                  allIn: false,
                }))
              : prev.players,
        }));

        await hydrateMyCards(wallet);
        await syncOnChainState();
      } catch (e) {
        setError(e instanceof Error ? e.message : "Deal failed");
      } finally {
        setLoading(false);
        setActiveRequest(null);
      }
    },
    [hydrateMyCards, playMode, syncOnChainState, tableId, wallet, setGame, setError, setLoading, setActiveRequest, setWinnerAddress]
  );

  const handleReveal = useCallback(
    async (phase: "flop" | "turn" | "river") => {
      if (!wallet) {
        setError("Connect Freighter wallet before requesting reveal");
        return;
      }

      setLoading(true);
      setActiveRequest(phase);
      setError(null);
      try {
        const result = await api.requestReveal(tableId, phase, wallet);
        const txHash = normalizeTxHash(result.tx_hash);
        setGame((prev) => ({
          ...prev,
          phase: phase as GamePhase,
          boardCards: [...prev.boardCards, ...result.cards],
          lastTxHash: txHash ?? prev.lastTxHash,
          proofSize: result.proof_size,
          onChainConfirmed: !!txHash || prev.onChainConfirmed,
        }));
        await syncOnChainState();
      } catch (e) {
        setError(e instanceof Error ? e.message : "Reveal failed");
      } finally {
        setLoading(false);
        setActiveRequest(null);
      }
    },
    [syncOnChainState, tableId, wallet, setGame, setError, setLoading, setActiveRequest]
  );

  const handleShowdown = useCallback(async () => {
    if (!wallet) {
      setError("Connect Freighter wallet before requesting showdown");
      return;
    }

    setLoading(true);
    setActiveRequest("showdown");
    setError(null);
    try {
      const result = await api.requestShowdown(tableId, wallet);
      const txHash = normalizeTxHash(result.tx_hash);

      // Map the winner's chain address to a wallet address via the lobby
      let resolvedWinner = result.winner;
      if (lobby?.seats) {
        for (const seat of lobby.seats) {
          if (seat.chain_address === result.winner && seat.wallet_address) {
            resolvedWinner = seat.wallet_address;
            break;
          }
        }
      }
      // Also check if the winner_index maps to a known player in our game state
      if (resolvedWinner === result.winner) {
        // Chain address didn't match any lobby wallet — try matching by seat index
        const playerByIndex = game.players[result.winner_index];
        if (playerByIndex && isStellarAddress(playerByIndex.address)) {
          resolvedWinner = playerByIndex.address;
        }
      }
      setWinnerAddress(resolvedWinner);

      setGame((prev) => ({
        ...prev,
        phase: "settlement" as GamePhase,
        lastTxHash: txHash ?? prev.lastTxHash,
        proofSize: result.proof_size,
        onChainConfirmed: !!txHash || prev.onChainConfirmed,
      }));
      await syncOnChainState();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Showdown failed");
    } finally {
      setLoading(false);
      setActiveRequest(null);
    }
  }, [game.players, lobby, syncOnChainState, tableId, wallet, setGame, setError, setLoading, setActiveRequest, setWinnerAddress]);

  const handleAction = useCallback(
    async (action: string, amount?: number) => {
      if (action === "showdown") {
        if (!wallet) {
          setError("Connect Freighter wallet before requesting showdown");
          return;
        }
        await handleShowdown();
        return;
      }

      const bettingActions = ["fold", "check", "call", "bet", "raise", "allin"];
      if (bettingActions.includes(action)) {
        if (playMode === "single") {
          if (!wallet) {
            setError("Connect Freighter wallet before betting");
            return;
          }

          const me = game.players.find((p) => p.address === wallet.address);
          const bot = game.players.find((p) => p.address !== wallet.address);
          if (!me || !bot) {
            setError("Waiting for solo seats to initialize");
            return;
          }

          if (action === "fold") {
            setWinnerAddress(bot.address);
            setGame((prev) => ({
              ...prev,
              phase: "settlement" as GamePhase,
              players: prev.players.map((p) =>
                p.address === me.address ? { ...p, folded: true } : p
              ),
            }));
            return;
          }

          const result = computeSoloBet(game, tableId, me.address, bot.address, action, amount);

          if (result.aiFolded) {
            setWinnerAddress(me.address);
            setBotLine(result.aiLine);
            setGame((prev) => ({
              ...prev,
              phase: "settlement" as GamePhase,
              pot: result.pot,
              players: prev.players.map((p) => {
                if (p.address === me.address) {
                  return { ...p, stack: result.userStack, betThisRound: 0, allIn: result.userStack <= 0 };
                }
                if (p.address === bot.address) {
                  return { ...p, stack: result.botStack, betThisRound: 0, folded: true, allIn: result.botStack <= 0 };
                }
                return p;
              }),
            }));
            return;
          }

          setGame((prev) => ({
            ...prev,
            pot: result.pot,
            players: prev.players.map((p) => {
              if (p.address === me.address) {
                return { ...p, stack: result.userStack, betThisRound: 0, allIn: result.userStack <= 0 };
              }
              if (p.address === bot.address) {
                return { ...p, stack: result.botStack, betThisRound: 0, allIn: result.botStack <= 0 };
              }
              return p;
            }),
          }));

          const step = game.phase;
          if (step === "preflop") {
            setBotLine(`${result.aiLine} Dealer reveals the flop...`);
            void handleReveal("flop");
          } else if (step === "flop") {
            setBotLine(`${result.aiLine} Dealer reveals the turn...`);
            void handleReveal("turn");
          } else if (step === "turn") {
            setBotLine(`${result.aiLine} Dealer reveals the river...`);
            void handleReveal("river");
          } else if (step === "river") {
            setBotLine(`${result.aiLine} Dealer runs showdown...`);
            void handleShowdown();
          }
          return;
        }
        if (!wallet) {
          setError("Connect Freighter wallet before betting");
          return;
        }
        setLoading(true);
        setError(null);
        try {
          const normalizedAmount =
            typeof amount === "number" && Number.isFinite(amount)
              ? Math.max(1, Math.floor(amount))
              : undefined;
          
          // Store pending action and start simulation
          setPendingAction({ action: action as string, amount: normalizedAmount });
          playerActionSim.performAction(
            tableId,
            action as string,
            normalizedAmount
          );
        } catch (e) {
          setError(e instanceof Error ? e.message : "Bet action failed");
        } finally {
          setLoading(false);
        }
        return;
      }

      if (action !== "start") {
        setError("Use the betting controls on the table.");
        return;
      }

      const players = resolvePlayersForDeal();
      if (!players) {
        return;
      }
      await handleDeal(players);
    },
    [
      game.phase,
      game.players,
      game.pot,
      wallet,
      playMode,
      syncOnChainState,
      tableId,
      resolvePlayersForDeal,
      handleDeal,
      handleReveal,
      handleShowdown,
      setGame,
      setError,
      setLoading,
      setWinnerAddress,
      setBotLine,
    ]
  );

  return {
    handleJoinTable,
    handleDeal,
    handleReveal,
    handleShowdown,
    handleAction,
    // Transaction simulation states
    joinSimulation: joinTableSim,
    actionSimulation: playerActionSim,
    pendingAction,
  };
}
