/**
 * Client-side hand history capture/persistence for the current browser
 * session. Each table keeps its own localStorage entry so the viewer panel
 * can show completed hands (street-by-street pot/board progression, the
 * viewing player's own hole cards when known, final pot, winner, and the
 * settlement proof tx) even after a hand has ended.
 */

import { bestHandRank } from "./hand-rank";

export type Street = "preflop" | "flop" | "turn" | "river";

export interface StreetSnapshot {
  street: Street;
  pot: number;
  boardCards: number[];
}

export interface HandHistoryEntry {
  tableId: number;
  handNumber: number;
  timestamp: number;
  streets: StreetSnapshot[];
  finalPot: number;
  boardCards: number[];
  holeCards?: [number, number];
  handRankName?: string;
  winnerAddress?: string | null;
  txHash?: string;
}

// ── Replayer frames ───────────────────────────────────────────────────────────

/** A single step shown during replay. */
export interface ReplayFrame {
  /** Human-readable label for this moment in the hand. */
  label: string;
  /** Street this frame belongs to. */
  street: Street | "settlement";
  /** Board cards visible at this point (grows as streets are revealed). */
  boardCards: number[];
  /** Pot size at this point. */
  pot: number;
  /** Hole cards (only shown when known). */
  holeCards?: [number, number];
  /** Best hand name at this frame (requires ≥5 cards). */
  handRankName?: string;
  /** Whether this is the final settlement frame. */
  isSettlement?: boolean;
  /** Winner address if known (settlement frame). */
  winnerAddress?: string | null;
}

/**
 * Build a sequence of replay frames from a completed hand history entry.
 * Each street snapshot becomes one frame, with an extra settlement frame at
 * the end showing the final board and winner.
 */
export function buildReplayFrames(entry: HandHistoryEntry): ReplayFrame[] {
  const frames: ReplayFrame[] = [];

  const STREET_LABELS: Record<Street, string> = {
    preflop: "PRE-FLOP",
    flop: "FLOP",
    turn: "TURN",
    river: "RIVER",
  };

  // One frame per street snapshot captured during the hand
  for (const snap of entry.streets) {
    const allCards = entry.holeCards
      ? [...entry.holeCards, ...snap.boardCards]
      : snap.boardCards;
    const rankName =
      allCards.length >= 5
        ? bestHandRank(allCards)?.name
        : undefined;

    frames.push({
      label: STREET_LABELS[snap.street],
      street: snap.street,
      boardCards: snap.boardCards,
      pot: snap.pot,
      holeCards: entry.holeCards,
      handRankName: rankName,
    });
  }

  // Final settlement frame
  frames.push({
    label: "SHOWDOWN",
    street: "settlement",
    boardCards: entry.boardCards,
    pot: entry.finalPot,
    holeCards: entry.holeCards,
    handRankName: entry.handRankName,
    isSettlement: true,
    winnerAddress: entry.winnerAddress,
  });

  return frames;
}

const STORAGE_PREFIX = "stellpoker:hand-history:";
const MAX_ENTRIES_PER_TABLE = 50;

function storageKey(tableId: number): string {
  return `${STORAGE_PREFIX}${tableId}`;
}

export function loadHandHistory(tableId: number): HandHistoryEntry[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(storageKey(tableId));
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as HandHistoryEntry[]) : [];
  } catch {
    return [];
  }
}

export function saveHandHistoryEntry(entry: HandHistoryEntry): void {
  if (typeof window === "undefined") return;
  try {
    const existing = loadHandHistory(entry.tableId);
    const next = [entry, ...existing].slice(0, MAX_ENTRIES_PER_TABLE);
    window.localStorage.setItem(storageKey(entry.tableId), JSON.stringify(next));
  } catch {
    // Storage unavailable (private browsing, quota) — history just won't persist.
  }
}

export function buildHandRankName(
  holeCards: [number, number] | undefined,
  boardCards: number[]
): string | undefined {
  if (!holeCards) return undefined;
  return bestHandRank([...holeCards, ...boardCards])?.name;
}
