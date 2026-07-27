import { describe, it, expect, beforeEach, afterAll } from "vitest";
import {
  loadOpenTables,
  trackOpenTable,
  untrackOpenTable,
  tableHref,
} from "@/lib/open-tables";

const WALLET = "GABC123";
const OTHER_WALLET = "GXYZ789";

/**
 * The shared test setup stubs `localStorage` with no-op spies, which is right
 * for suites that only assert a write happened. These tests exercise the
 * round-trip, so they need storage that actually remembers things.
 */
function inMemoryStorage(): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    key: (index: number) => [...data.keys()][index] ?? null,
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, String(value)),
    removeItem: (key: string) => void data.delete(key),
    clear: () => data.clear(),
  } as Storage;
}

const originalLocalStorage = Object.getOwnPropertyDescriptor(
  window,
  "localStorage"
);

describe("open-tables store", () => {
  beforeEach(() => {
    Object.defineProperty(window, "localStorage", {
      value: inMemoryStorage(),
      writable: true,
      configurable: true,
    });
  });

  afterAll(() => {
    if (originalLocalStorage) {
      Object.defineProperty(window, "localStorage", originalLocalStorage);
    }
  });

  it("returns nothing for a wallet that has not opened a table", () => {
    expect(loadOpenTables(WALLET)).toEqual([]);
  });

  it("returns nothing without a wallet address", () => {
    expect(loadOpenTables("")).toEqual([]);
  });

  it("tracks a visited table and persists it", () => {
    trackOpenTable(WALLET, 7, "multi");

    const tables = loadOpenTables(WALLET);
    expect(tables).toHaveLength(1);
    expect(tables[0].tableId).toBe(7);
    expect(tables[0].mode).toBe("multi");
  });

  it("keeps several tables open at once, ordered by id", () => {
    trackOpenTable(WALLET, 9);
    trackOpenTable(WALLET, 2);
    trackOpenTable(WALLET, 5);

    expect(loadOpenTables(WALLET).map((t) => t.tableId)).toEqual([2, 5, 9]);
  });

  it("does not duplicate a table that is revisited", () => {
    trackOpenTable(WALLET, 3, "headsup");
    trackOpenTable(WALLET, 3, "headsup");

    expect(loadOpenTables(WALLET)).toHaveLength(1);
  });

  it("remembers the mode a table was opened with when a later visit omits it", () => {
    trackOpenTable(WALLET, 4, "single");
    trackOpenTable(WALLET, 4);

    expect(loadOpenTables(WALLET)[0].mode).toBe("single");
  });

  it("lets a later visit change the mode", () => {
    trackOpenTable(WALLET, 4, "headsup");
    trackOpenTable(WALLET, 4, "multi");

    expect(loadOpenTables(WALLET)[0].mode).toBe("multi");
  });

  it("scopes tables per wallet", () => {
    trackOpenTable(WALLET, 1);
    trackOpenTable(OTHER_WALLET, 2);

    expect(loadOpenTables(WALLET).map((t) => t.tableId)).toEqual([1]);
    expect(loadOpenTables(OTHER_WALLET).map((t) => t.tableId)).toEqual([2]);
  });

  it("drops the least recently visited entry past the strip limit", () => {
    for (let id = 0; id < 15; id += 1) {
      trackOpenTable(WALLET, id);
    }

    const tables = loadOpenTables(WALLET);
    expect(tables).toHaveLength(12);
    // The three oldest visits fell off; the rest survive.
    expect(tables.map((t) => t.tableId)).toEqual([
      3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
    ]);
  });

  it("untracks a single table and leaves the others", () => {
    trackOpenTable(WALLET, 1);
    trackOpenTable(WALLET, 2);
    trackOpenTable(WALLET, 3);

    const remaining = untrackOpenTable(WALLET, 2);
    expect(remaining.map((t) => t.tableId)).toEqual([1, 3]);
    expect(loadOpenTables(WALLET).map((t) => t.tableId)).toEqual([1, 3]);
  });

  it("survives corrupt stored data", () => {
    window.localStorage.setItem("stellpoker:open-tables:" + WALLET, "not json");
    expect(loadOpenTables(WALLET)).toEqual([]);

    window.localStorage.setItem(
      "stellpoker:open-tables:" + WALLET,
      JSON.stringify({ nope: true })
    );
    expect(loadOpenTables(WALLET)).toEqual([]);

    window.localStorage.setItem(
      "stellpoker:open-tables:" + WALLET,
      JSON.stringify([{ tableId: "five" }, { tableId: 6, lastVisited: 1 }])
    );
    expect(loadOpenTables(WALLET).map((t) => t.tableId)).toEqual([6]);
  });

  it("builds hrefs that preserve the play mode", () => {
    expect(tableHref({ tableId: 8, mode: "multi", lastVisited: 0 })).toBe(
      "/table/8?mode=multi"
    );
    expect(tableHref({ tableId: 8, lastVisited: 0 })).toBe("/table/8");
  });
});
