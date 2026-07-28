/**
 * Replay data model and Horizon event indexer.
 *
 * A completed hand is reconstructed by fetching the Soroban contract events
 * from Horizon for the poker-table contract, filtering by the hand's sequence
 * of events (deal_committed → board_revealed × n → hand_settled / fold_win),
 * and assembling them into a ReplayHand that the viewer can step through.
 *
 * Hand IDs are encoded as "<table_id>-<hand_number>" so they are stable,
 * shareable as URL params, and can be looked up without a backend.
 */

import { getChainConfig } from "./api";
import { stellarExpertUrl } from "./explorer";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ReplayStep =
  | { kind: "deal";    holeCards: Record<string, [number, number]>; deckRoot: string; txHash: string | null }
  | { kind: "flop";    cards: [number, number, number]; txHash: string | null }
  | { kind: "turn";    card: number; txHash: string | null }
  | { kind: "river";   card: number; txHash: string | null }
  | { kind: "action";  player: string; action: string; amount: number | null; street: string }
  | { kind: "showdown"; winner: string; holecards: Record<string, [number, number]>; txHash: string | null }
  | { kind: "fold_win"; winner: string; pot: number; txHash: string | null };

export interface ReplayHand {
  /** "<table_id>-<hand_number>" */
  id: string;
  tableId: number;
  handNumber: number;
  /** Unix ms when hand_settled / fold_win event was emitted. */
  settledAt: number | null;
  steps: ReplayStep[];
  finalPot: number;
  boardCards: number[];
  winner: string | null;
  /** tx hash for the showdown / fold-win proof verification */
  proofTxHash: string | null;
  /** Links to stellar.expert for each proof tx (keyed by step kind). */
  proofLinks: Partial<Record<"deal" | "flop" | "turn" | "river" | "showdown", string>>;
}

// ── Horizon event fetcher ─────────────────────────────────────────────────────

interface HorizonEffectRecord {
  type: string;
  /** operation id */
  id: string;
  created_at: string;
}

interface HorizonOperationRecord {
  id: string;
  transaction_hash: string;
  created_at: string;
  type_i: number;
  type: string;
}

interface RpcEventValue {
  type?: string;
  value?: string;
}

/** Raw event as returned by Horizon's /effects or the Soroban RPC getEvents. */
interface HorizonContractEvent {
  id: string;
  paging_token: string;
  transaction_hash: string;
  ledger_closed_at: string;
  /** The first topic symbol (event name). */
  topic: string[];
  value: RpcEventValue;
}

interface HorizonEventsPage {
  _embedded?: {
    records?: HorizonContractEvent[];
  };
  /** Cursor for the next page. */
  next?: { href: string };
}

function horizonBase(): string {
  const rpcUrl = process.env.NEXT_PUBLIC_HORIZON_URL;
  if (rpcUrl) return rpcUrl.replace(/\/+$/, "");
  // Derive from coordinator env as fallback.
  return "https://horizon-testnet.stellar.org";
}

/**
 * Fetch all contract events for a given contract from Horizon (paginated).
 * Stops once we have `maxRecords` or reach the end.
 */
async function fetchContractEvents(
  contractId: string,
  limit = 200
): Promise<HorizonContractEvent[]> {
  const base = horizonBase();
  const url = `${base}/contract/${contractId}/events?limit=${Math.min(limit, 200)}&order=asc`;
  try {
    const res = await fetch(url);
    if (!res.ok) return [];
    const page = (await res.json()) as HorizonEventsPage;
    return page._embedded?.records ?? [];
  } catch {
    return [];
  }
}

// ── RPC getEvents fallback ────────────────────────────────────────────────────

interface RpcEventsResponse {
  result?: {
    events?: Array<{
      id: string;
      pagingToken: string;
      ledger: number;
      ledgerClosedAt: string;
      contractId: string;
      type: string;
      txHash: string;
      topic: string[];
      value: { xdr: string };
    }>;
    cursor?: string;
  };
  events?: Array<{
    id: string;
    pagingToken: string;
    ledger: number;
    ledgerClosedAt: string;
    contractId: string;
    type: string;
    txHash: string;
    topic: string[];
    value: { xdr: string };
  }>;
}

interface NormalisedEvent {
  id: string;
  txHash: string;
  closedAt: string;
  topics: string[];
  valueXdr: string;
}

/**
 * Fetch events from the Soroban RPC (getEvents JSON-RPC call). Used when
 * Horizon doesn't expose contract event records for a given testnet deployment.
 */
async function fetchRpcEvents(
  contractId: string,
  rpcUrl: string,
  startLedger = 1
): Promise<NormalisedEvent[]> {
  try {
    const body = JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "getEvents",
      params: {
        startLedger,
        filters: [{ type: "contract", contractIds: [contractId] }],
        limit: 200,
      },
    });
    const res = await fetch(rpcUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
    });
    if (!res.ok) return [];
    const data = (await res.json()) as RpcEventsResponse;
    const raw = data.result?.events ?? data.events ?? [];
    return raw.map((e) => ({
      id: e.id,
      txHash: e.txHash,
      closedAt: e.ledgerClosedAt,
      topics: e.topic,
      valueXdr: e.value?.xdr ?? "",
    }));
  } catch {
    return [];
  }
}

// ── XDR / ScVal decoding (minimal, no SDK dependency in lib) ─────────────────

/**
 * Best-effort decode of a base64 XDR ScVal into a plain JS value.
 * We dynamically import stellar-sdk only when this function is called so
 * the module stays tree-shakeable in server components.
 */
async function decodeScVal(base64Xdr: string): Promise<unknown> {
  try {
    const { xdr, scValToNative } = await import("@stellar/stellar-sdk");
    const val = xdr.ScVal.fromXDR(base64Xdr, "base64");
    return scValToNative(val);
  } catch {
    return null;
  }
}

async function decodeTopicSymbol(base64Xdr: string): Promise<string | null> {
  try {
    const { xdr } = await import("@stellar/stellar-sdk");
    const val = xdr.ScVal.fromXDR(base64Xdr, "base64");
    if (val.switch().name === "scvSymbol") {
      return val.sym().toString();
    }
    return null;
  } catch {
    return null;
  }
}

// ── Hand reconstruction ───────────────────────────────────────────────────────

/**
 * Parse a raw event value into a plain object map.
 * Returns an empty object on failure.
 */
async function parseEventData(valueXdr: string): Promise<Record<string, unknown>> {
  const native = await decodeScVal(valueXdr);
  if (native && typeof native === "object" && !Array.isArray(native)) {
    return native as Record<string, unknown>;
  }
  // Some events emit a vec — wrap it so callers can iterate.
  if (Array.isArray(native)) return { _items: native };
  return {};
}

function asNumber(v: unknown): number {
  if (typeof v === "number") return v;
  if (typeof v === "bigint") return Number(v);
  if (typeof v === "string") return Number(v);
  return 0;
}

function asString(v: unknown): string {
  if (typeof v === "string") return v;
  if (v instanceof Uint8Array) return Buffer.from(v).toString("hex");
  return String(v ?? "");
}

/**
 * Reconstruct a ReplayHand from a list of normalised events that belong to one
 * specific hand (table_id + hand_number).
 */
async function buildReplayHand(
  tableId: number,
  handNumber: number,
  events: NormalisedEvent[]
): Promise<ReplayHand> {
  const steps: ReplayStep[] = [];
  const proofLinks: ReplayHand["proofLinks"] = {};
  let finalPot = 0;
  const boardCards: number[] = [];
  let winner: string | null = null;
  let proofTxHash: string | null = null;
  let settledAt: number | null = null;
  let currentStreet = "preflop";

  for (const ev of events) {
    // Decode first topic to get the event name.
    const topicName =
      ev.topics.length > 0
        ? (await decodeTopicSymbol(ev.topics[0])) ?? ev.topics[0]
        : "unknown";

    const data = await parseEventData(ev.valueXdr);

    switch (topicName) {
      case "deal_committed": {
        const deckRoot = asString(data.deck_root ?? data[0] ?? "");
        const rawHoleCards = data.hole_cards ?? data.hand_commitments ?? {};
        const holeCards: Record<string, [number, number]> = {};
        if (rawHoleCards && typeof rawHoleCards === "object") {
          for (const [addr, cards] of Object.entries(rawHoleCards as Record<string, unknown>)) {
            if (Array.isArray(cards) && cards.length >= 2) {
              holeCards[addr] = [asNumber(cards[0]), asNumber(cards[1])];
            }
          }
        }
        steps.push({ kind: "deal", holeCards, deckRoot, txHash: ev.txHash });
        if (ev.txHash) proofLinks.deal = stellarExpertUrl("tx", ev.txHash);
        currentStreet = "preflop";
        break;
      }

      case "board_revealed": {
        const cards = (data.cards ?? data._items ?? []) as unknown[];
        const indices = (data.indices ?? []) as unknown[];
        const phase = asString(data.phase ?? "");
        if (ev.txHash) {
          const phaseKey = phase || (boardCards.length === 0 ? "flop" : boardCards.length === 3 ? "turn" : "river");
          if (phaseKey === "flop") proofLinks.flop = stellarExpertUrl("tx", ev.txHash);
          if (phaseKey === "turn") proofLinks.turn = stellarExpertUrl("tx", ev.txHash);
          if (phaseKey === "river") proofLinks.river = stellarExpertUrl("tx", ev.txHash);
        }
        const decoded = cards.map(asNumber);
        if (boardCards.length === 0 && decoded.length >= 3) {
          steps.push({ kind: "flop", cards: [decoded[0], decoded[1], decoded[2]], txHash: ev.txHash });
          boardCards.push(...decoded.slice(0, 3));
          currentStreet = "flop";
        } else if (boardCards.length === 3 && decoded.length >= 1) {
          steps.push({ kind: "turn", card: decoded[0], txHash: ev.txHash });
          boardCards.push(decoded[0]);
          currentStreet = "turn";
        } else if (boardCards.length === 4 && decoded.length >= 1) {
          steps.push({ kind: "river", card: decoded[0], txHash: ev.txHash });
          boardCards.push(decoded[0]);
          currentStreet = "river";
        } else if (decoded.length > 0) {
          // Fallback: use indices length to determine street
          const idx = indices.length;
          if (idx <= 3) {
            steps.push({ kind: "flop", cards: [decoded[0], decoded[1] ?? decoded[0], decoded[2] ?? decoded[0]], txHash: ev.txHash });
            boardCards.push(...decoded.slice(0, 3));
            currentStreet = "flop";
          } else if (idx === 4) {
            steps.push({ kind: "turn", card: decoded[idx - 1], txHash: ev.txHash });
            boardCards.push(decoded[idx - 1]);
            currentStreet = "turn";
          } else {
            steps.push({ kind: "river", card: decoded[idx - 1], txHash: ev.txHash });
            boardCards.push(decoded[idx - 1]);
            currentStreet = "river";
          }
        }
        break;
      }

      case "player_action": {
        const player = asString(data.player ?? data[0] ?? "");
        const action = asString(data.action ?? data[1] ?? "");
        const amount = data.amount != null ? asNumber(data.amount) : null;
        steps.push({ kind: "action", player, action, amount, street: currentStreet });
        break;
      }

      case "hand_settled": {
        winner = asString(data.winner ?? data[0] ?? "");
        finalPot = asNumber(data.pot ?? data[1] ?? 0);
        const rawHoleCards = data.hole_cards ?? data.revealed_cards ?? {};
        const holecards: Record<string, [number, number]> = {};
        if (rawHoleCards && typeof rawHoleCards === "object") {
          for (const [addr, cards] of Object.entries(rawHoleCards as Record<string, unknown>)) {
            if (Array.isArray(cards) && cards.length >= 2) {
              holecards[addr] = [asNumber(cards[0]), asNumber(cards[1])];
            }
          }
        }
        steps.push({ kind: "showdown", winner, holecards, txHash: ev.txHash });
        if (ev.txHash) {
          proofTxHash = ev.txHash;
          proofLinks.showdown = stellarExpertUrl("tx", ev.txHash);
        }
        settledAt = ev.closedAt ? new Date(ev.closedAt).getTime() : null;
        break;
      }

      case "fold_win": {
        winner = asString(data.winner ?? data[0] ?? "");
        finalPot = asNumber(data.pot ?? data[1] ?? 0);
        steps.push({ kind: "fold_win", winner, pot: finalPot, txHash: ev.txHash });
        if (ev.txHash) proofTxHash = ev.txHash;
        settledAt = ev.closedAt ? new Date(ev.closedAt).getTime() : null;
        break;
      }

      default:
        break;
    }
  }

  return {
    id: `${tableId}-${handNumber}`,
    tableId,
    handNumber,
    settledAt,
    steps,
    finalPot,
    boardCards,
    winner,
    proofTxHash,
    proofLinks,
  };
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Fetch and reconstruct a completed hand for the replay viewer.
 *
 * Strategy:
 *  1. Try Horizon contract events endpoint.
 *  2. Fall back to Soroban RPC getEvents if Horizon returns nothing.
 *
 * Events are filtered to those belonging to the requested table_id + hand_number
 * by inspecting topic[1] (table_id) and topic[2] (hand_number) where present,
 * or by ordering and grouping when they aren't.
 *
 * Returns null if no events are found.
 */
export async function fetchReplayHand(
  tableId: number,
  handNumber: number
): Promise<ReplayHand | null> {
  let cfg: { rpc_url: string; poker_table_contract: string };
  try {
    cfg = await getChainConfig();
  } catch {
    return null;
  }

  // Try RPC getEvents (more reliable for testnet).
  const rpcEvents = await fetchRpcEvents(cfg.poker_table_contract, cfg.rpc_url);

  let events: NormalisedEvent[] = [];

  if (rpcEvents.length > 0) {
    events = rpcEvents;
  } else {
    // Fallback: Horizon
    const horizonEvents = await fetchContractEvents(cfg.poker_table_contract);
    events = horizonEvents.map((h) => ({
      id: h.id,
      txHash: h.transaction_hash,
      closedAt: h.ledger_closed_at,
      topics: h.topic,
      valueXdr: typeof h.value === "object" ? (h.value.value ?? "") : "",
    }));
  }

  if (events.length === 0) return null;

  // Filter events to the requested hand.
  // Contract events include topic[1] = table_id, topic[2] = hand_number (if
  // the contract emits them). We attempt to decode; if they aren't present we
  // fall back to a positional heuristic (events between consecutive
  // "deal_committed" for this table).
  const handEvents: NormalisedEvent[] = [];

  for (const ev of events) {
    const topicName =
      ev.topics.length > 0
        ? (await decodeTopicSymbol(ev.topics[0])) ?? ev.topics[0]
        : "unknown";

    // Try to read table_id / hand_number from topics[1] and topics[2].
    let evTableId: number | null = null;
    let evHandNumber: number | null = null;
    if (ev.topics.length > 1) {
      const t1 = await decodeScVal(ev.topics[1]);
      evTableId = t1 != null ? asNumber(t1) : null;
    }
    if (ev.topics.length > 2) {
      const t2 = await decodeScVal(ev.topics[2]);
      evHandNumber = t2 != null ? asNumber(t2) : null;
    }

    const matchesTable = evTableId === null || evTableId === tableId;
    const matchesHand = evHandNumber === null || evHandNumber === handNumber;

    const HAND_EVENTS = new Set([
      "deal_committed", "board_revealed", "player_action",
      "hand_settled", "fold_win",
    ]);

    if (HAND_EVENTS.has(topicName) && matchesTable && matchesHand) {
      handEvents.push(ev);
    }
  }

  if (handEvents.length === 0) return null;

  return buildReplayHand(tableId, handNumber, handEvents);
}

/**
 * Parse a hand ID string ("<tableId>-<handNumber>") back to its components.
 * Returns null if the format is invalid.
 */
export function parseHandId(
  handId: string
): { tableId: number; handNumber: number } | null {
  const parts = handId.split("-");
  if (parts.length !== 2) return null;
  const tableId = parseInt(parts[0], 10);
  const handNumber = parseInt(parts[1], 10);
  if (isNaN(tableId) || isNaN(handNumber)) return null;
  return { tableId, handNumber };
}

/**
 * Build a replay URL for a given table + hand number.
 */
export function replayUrl(tableId: number, handNumber: number): string {
  return `/replay/${tableId}-${handNumber}`;
}
