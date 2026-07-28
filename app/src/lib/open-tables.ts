/**
 * Tracks which tables a player currently has open in this browser, so a
 * multi-tabling player can switch between them without going back to the lobby
 * and re-entering table IDs (#72).
 *
 * The contract keeps the authoritative list of seats (`get_player_tables`);
 * this store is the *client's* view — it also remembers tables the player is
 * only watching, the play mode each was opened in, and the order the player
 * arranged them in. It is a local display preference, so like
 * `alias-store.ts` it lives in localStorage and is scoped per wallet address.
 */

export type PlayMode = "single" | "headsup" | "multi";

export interface OpenTable {
  tableId: number;
  /** Mode the table was opened in, so switching back restores the same view. */
  mode?: PlayMode;
  /** Last time the player looked at this table, for "most recent" ordering. */
  lastVisited: number;
}

const STORAGE_PREFIX = "stellpoker:open-tables:";

/** Beyond this the tab strip stops being usable; the least recently visited
 * entry is dropped. This caps the *strip*, never how many tables a wallet may
 * actually be seated at — the contract has no such limit. */
const MAX_TRACKED_TABLES = 12;

function storageKey(address: string): string {
  return `${STORAGE_PREFIX}${address}`;
}

export function loadOpenTables(address: string): OpenTable[] {
  if (typeof window === "undefined" || !address) return [];
  try {
    const raw = window.localStorage.getItem(storageKey(address));
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (entry): entry is OpenTable =>
        typeof entry === "object" &&
        entry !== null &&
        Number.isInteger((entry as OpenTable).tableId)
    );
  } catch {
    return [];
  }
}

function persist(address: string, tables: OpenTable[]): void {
  try {
    window.localStorage.setItem(storageKey(address), JSON.stringify(tables));
  } catch {
    // Storage unavailable (private browsing, quota) — the strip just won't
    // survive a reload.
  }
}

/**
 * Record that the player is looking at `tableId` now. Returns the updated list
 * so a caller can render it without a second read.
 */
export function trackOpenTable(
  address: string,
  tableId: number,
  mode?: PlayMode
): OpenTable[] {
  if (typeof window === "undefined" || !address) return [];

  const existing = loadOpenTables(address);
  const previous = existing.find((t) => t.tableId === tableId);

  // `Date.now()` only has millisecond resolution, so switching quickly between
  // tables can stamp several with the same value and leave "least recently
  // visited" ambiguous. Forcing each visit strictly past the highest stamp on
  // record keeps the eviction order well defined.
  const highest = existing.reduce((max, t) => Math.max(max, t.lastVisited), 0);
  const updated: OpenTable = {
    tableId,
    // Keep the mode a table was opened with if this visit didn't specify one.
    mode: mode ?? previous?.mode,
    lastVisited: Math.max(Date.now(), highest + 1),
  };

  const next = [...existing.filter((t) => t.tableId !== tableId), updated]
    .sort((a, b) => b.lastVisited - a.lastVisited)
    .slice(0, MAX_TRACKED_TABLES)
    .sort((a, b) => a.tableId - b.tableId);

  persist(address, next);
  return next;
}

/** Stop tracking a table — called when the player leaves their seat. */
export function untrackOpenTable(address: string, tableId: number): OpenTable[] {
  if (typeof window === "undefined" || !address) return [];
  const next = loadOpenTables(address).filter((t) => t.tableId !== tableId);
  persist(address, next);
  return next;
}

/** Build the href for a tracked table, preserving the mode it was opened in. */
export function tableHref(table: OpenTable): string {
  return table.mode
    ? `/table/${table.tableId}?mode=${table.mode}`
    : `/table/${table.tableId}`;
}
