import { describe, it, expect, beforeEach } from "vitest";
import {
  persistHandStateClientSide,
  restoreHandStateClientSide,
  clearHandStateClientSide,
} from "../lib/reconnect-state";

describe("Encrypted Reconnect State (Issue #14)", () => {
  const tableId = 42;
  const playerAddress = "GBRPYHIL2CI3FNQ4BXLFMNDLFPPPU2P6KGUTTXQW4TLI425RXRVD2Y57";
  const holeCards: [number, number] = [14, 27]; // Ace of spades, King of hearts

  beforeEach(() => {
    localStorage.clear();
  });

  it("encrypts and persists hole cards client-side", async () => {
    await persistHandStateClientSide(tableId, 1, playerAddress, holeCards, "turn");
    const raw = localStorage.getItem(`stellpoker-encrypted-session-${tableId}-${playerAddress}`);
    expect(raw).toBeTruthy();

    const record = JSON.parse(raw!);
    expect(record.ciphertext).toBeTruthy();
    expect(record.iv).toBeTruthy();
    // Raw unencrypted cards must not be stored in plain text key
    expect(record.ciphertext).not.toContain("Ace");
  });

  it("decrypts and restores in-progress hand state on reconnect", async () => {
    await persistHandStateClientSide(tableId, 2, playerAddress, holeCards, "flop");
    const restored = await restoreHandStateClientSide(tableId, playerAddress);

    expect(restored).toBeTruthy();
    expect(restored?.tableId).toBe(tableId);
    expect(restored?.handNumber).toBe(2);
    expect(restored?.playerAddress).toBe(playerAddress);
    expect(restored?.cards).toEqual(holeCards);
    expect(restored?.phase).toBe("flop");
  });

  it("clears hand state when cleared", async () => {
    await persistHandStateClientSide(tableId, 3, playerAddress, holeCards, "river");
    clearHandStateClientSide(tableId, playerAddress);

    const restored = await restoreHandStateClientSide(tableId, playerAddress);
    expect(restored).toBeNull();
  });
});
