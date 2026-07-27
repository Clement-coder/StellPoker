/**
 * Tests for tournament.ts utility functions.
 */
import { describe, it, expect } from "vitest";
import {
  stroopsToXlm,
  statusLabel,
  statusColor,
  shortAddr,
  placeLabel,
  type TournamentStatus,
} from "@/lib/tournament";

describe("stroopsToXlm", () => {
  it("converts whole XLM amounts", () => {
    expect(stroopsToXlm(10_000_000)).toBe("1");
    expect(stroopsToXlm(100_000_000)).toBe("10");
  });

  it("converts fractional amounts", () => {
    expect(stroopsToXlm(5_000_000)).toBe("0.50");
    expect(stroopsToXlm(1_500_000)).toBe("0.15");
  });

  it("handles zero", () => {
    expect(stroopsToXlm(0)).toBe("0");
  });
});

describe("statusLabel", () => {
  const cases: [TournamentStatus, string][] = [
    ["registration", "REGISTRATION"],
    ["running", "IN PROGRESS"],
    ["finalizing", "FINALIZING"],
    ["completed", "COMPLETED"],
    ["cancelled", "CANCELLED"],
  ];
  for (const [status, expected] of cases) {
    it(`maps ${status} to ${expected}`, () => {
      expect(statusLabel(status)).toBe(expected);
    });
  }
});

describe("statusColor", () => {
  it("returns a non-empty colour string for every status", () => {
    const statuses: TournamentStatus[] = [
      "registration", "running", "finalizing", "completed", "cancelled",
    ];
    for (const s of statuses) {
      expect(statusColor(s)).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});

describe("shortAddr", () => {
  it("truncates a long Stellar address", () => {
    const addr = "GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const short = shortAddr(addr);
    expect(short).toContain("…");
    expect(short.startsWith("GABC12")).toBe(true);
    expect(short.length).toBeLessThan(addr.length);
  });

  it("returns short strings unchanged", () => {
    expect(shortAddr("GABC")).toBe("GABC");
  });

  it("handles empty string", () => {
    expect(shortAddr("")).toBe("");
  });
});

describe("placeLabel", () => {
  it("returns ST/ND/RD/TH suffixes", () => {
    expect(placeLabel(1)).toBe("1ST");
    expect(placeLabel(2)).toBe("2ND");
    expect(placeLabel(3)).toBe("3RD");
    expect(placeLabel(4)).toBe("4TH");
    expect(placeLabel(10)).toBe("10TH");
  });
});
