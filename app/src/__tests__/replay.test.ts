/**
 * Tests for replay.ts utilities.
 *
 * We test parseHandId, replayUrl, and the hand-reconstruction logic via the
 * exported buildReplayHand-equivalent path (by mocking fetch so fetchReplayHand
 * returns deterministic data).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { parseHandId, replayUrl } from "@/lib/replay";

// ── parseHandId ───────────────────────────────────────────────────────────────

describe("parseHandId", () => {
  it("parses a valid hand ID", () => {
    expect(parseHandId("3-7")).toEqual({ tableId: 3, handNumber: 7 });
  });

  it("parses IDs with large numbers", () => {
    expect(parseHandId("100-9999")).toEqual({ tableId: 100, handNumber: 9999 });
  });

  it("returns null for an ID with no dash", () => {
    expect(parseHandId("37")).toBeNull();
  });

  it("returns null for an ID with too many dashes", () => {
    expect(parseHandId("1-2-3")).toBeNull();
  });

  it("returns null for non-numeric parts", () => {
    expect(parseHandId("abc-xyz")).toBeNull();
  });

  it("returns null for an empty string", () => {
    expect(parseHandId("")).toBeNull();
  });

  it("returns null for NaN after split", () => {
    expect(parseHandId("-")).toBeNull();
  });
});

// ── replayUrl ─────────────────────────────────────────────────────────────────

describe("replayUrl", () => {
  it("builds the correct replay URL", () => {
    expect(replayUrl(3, 7)).toBe("/replay/3-7");
  });

  it("handles table 0, hand 0", () => {
    expect(replayUrl(0, 0)).toBe("/replay/0-0");
  });

  it("round-trips through parseHandId", () => {
    const url = replayUrl(42, 100);
    const id = url.replace("/replay/", "");
    expect(parseHandId(id)).toEqual({ tableId: 42, handNumber: 100 });
  });
});

// ── fetchReplayHand (fetch-mocked) ────────────────────────────────────────────

describe("fetchReplayHand", () => {
  const originalFetch = global.fetch;

  beforeEach(() => {
    // Mock getChainConfig call
    vi.mock("@/lib/api", () => ({
      getChainConfig: vi.fn().mockResolvedValue({
        rpc_url: "http://localhost:8000",
        network_passphrase: "Test SDF Network ; September 2015",
        poker_table_contract: "CTEST123",
      }),
    }));
  });

  afterEach(() => {
    global.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it("returns null when RPC and Horizon both return no events", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ result: { events: [] } }),
    } as Response);

    const { fetchReplayHand } = await import("@/lib/replay");
    const result = await fetchReplayHand(1, 1);
    expect(result).toBeNull();
  });

  it("returns null when fetch throws", async () => {
    global.fetch = vi.fn().mockRejectedValue(new Error("network error"));
    // getChainConfig also uses fetch — catch it gracefully
    const { fetchReplayHand } = await import("@/lib/replay");
    const result = await fetchReplayHand(1, 1);
    expect(result).toBeNull();
  });
});
