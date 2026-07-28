/**
 * Tests for the ReplayViewer component.
 *
 * We render a minimal ReplayHand and verify:
 *   - PLAY / PAUSE / ← / → controls are present and accessible
 *   - The timeline renders every step
 *   - Clicking a timeline item updates the current step
 *   - The community cards section is present
 *   - Proof links are shown when available
 */

import React from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ReplayViewer } from "@/components/ReplayViewer";
import type { ReplayHand } from "@/lib/replay";

// Stub Next.js Link. JSX cannot be used inside a vi.mock() factory because
// vi.mock() is hoisted before the JSX transform, so we use React.createElement.
vi.mock("next/link", () => ({
  default: ({ href, children }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href }, children),
}));

// Minimal hand fixture (no real XDR decoding needed).
const MOCK_HAND: ReplayHand = {
  id: "1-1",
  tableId: 1,
  handNumber: 1,
  settledAt: 1_700_000_000_000,
  finalPot: 5000,
  boardCards: [0, 13, 26, 1, 14],
  winner: "GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ",
  proofTxHash: "abcdef1234567890",
  proofLinks: {
    deal: "https://stellar.expert/explorer/testnet/tx/abc",
    showdown: "https://stellar.expert/explorer/testnet/tx/def",
  },
  steps: [
    {
      kind: "deal",
      holeCards: { GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ: [2, 15] },
      deckRoot: "0xdeadbeef",
      txHash: "abc",
    },
    {
      kind: "action",
      player: "GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ",
      action: "bet",
      amount: 100,
      street: "preflop",
    },
    {
      kind: "flop",
      cards: [0, 13, 26],
      txHash: "bcd",
    },
    {
      kind: "turn",
      card: 1,
      txHash: "cde",
    },
    {
      kind: "river",
      card: 14,
      txHash: "def",
    },
    {
      kind: "showdown",
      winner: "GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ",
      holecards: { GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ: [2, 15] },
      txHash: "efg",
    },
  ],
};

describe("ReplayViewer", () => {
  it("renders without crashing", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
  });

  it("shows table and hand number in header", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByText(/TABLE #1/)).toBeTruthy();
    expect(screen.getByText(/HAND #1/)).toBeTruthy();
  });

  it("shows winner address (truncated)", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    // Truncated winner appears in header
    expect(screen.getAllByText(/GABC12/).length).toBeGreaterThan(0);
  });

  it("renders PLAY button", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByLabelText("Play replay")).toBeTruthy();
  });

  it("renders Previous and Next step buttons", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByLabelText("Previous step")).toBeTruthy();
    expect(screen.getByLabelText("Next step")).toBeTruthy();
  });

  it("Previous button is disabled on first step", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    const prev = screen.getByLabelText("Previous step") as HTMLButtonElement;
    expect(prev.disabled).toBe(true);
  });

  it("Next button advances to step 2", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    const next = screen.getByLabelText("Next step");
    fireEvent.click(next);
    // Step 2 label should now be shown
    expect(screen.getByText(/STEP 2\/6/)).toBeTruthy();
  });

  it("renders timeline with all step labels", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByLabelText("Go to step 1: DEAL")).toBeTruthy();
    expect(screen.getByLabelText("Go to step 2: ACTION")).toBeTruthy();
    expect(screen.getByLabelText("Go to step 3: FLOP")).toBeTruthy();
    expect(screen.getByLabelText("Go to step 4: TURN")).toBeTruthy();
    expect(screen.getByLabelText("Go to step 5: RIVER")).toBeTruthy();
    expect(screen.getByLabelText("Go to step 6: SHOWDOWN")).toBeTruthy();
  });

  it("clicking a timeline step jumps to that step", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    const flopBtn = screen.getByLabelText("Go to step 3: FLOP");
    fireEvent.click(flopBtn);
    expect(screen.getByText(/STEP 3\/6/)).toBeTruthy();
  });

  it("renders community cards section", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByLabelText("Community cards")).toBeTruthy();
  });

  it("renders on-chain proof links when provided", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByLabelText("On-chain proof links")).toBeTruthy();
    expect(screen.getByText("DEAL PROOF ↗")).toBeTruthy();
    expect(screen.getByText("SHOWDOWN PROOF ↗")).toBeTruthy();
  });

  it("shows final pot in summary", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByText(/FINAL POT: 5,000/)).toBeTruthy();
  });

  it("renders a scrubber input", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    expect(screen.getByLabelText("Replay scrubber")).toBeTruthy();
  });

  it("PLAY changes to PAUSE after click", () => {
    render(<ReplayViewer hand={MOCK_HAND} />);
    const play = screen.getByLabelText("Play replay");
    fireEvent.click(play);
    expect(screen.getByLabelText("Pause replay")).toBeTruthy();
  });

  it("renders for a fold-win hand", () => {
    const foldHand: ReplayHand = {
      ...MOCK_HAND,
      id: "1-2",
      handNumber: 2,
      steps: [
        {
          kind: "deal",
          holeCards: {},
          deckRoot: "0xfeed",
          txHash: null,
        },
        {
          kind: "fold_win",
          winner: "GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ",
          pot: 200,
          txHash: null,
        },
      ],
      proofLinks: {},
    };
    render(<ReplayViewer hand={foldHand} />);
    expect(screen.getByLabelText("Go to step 2: FOLD WIN")).toBeTruthy();
  });
});
