import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { HandHistoryPanel } from "./HandHistoryPanel";
import type { HandHistoryEntry } from "@/lib/hand-history";

const mockEntries: HandHistoryEntry[] = [
  {
    tableId: 1,
    handNumber: 3,
    timestamp: Date.now() - 120000,
    streets: [
      { street: "preflop", pot: 300, boardCards: [] },
      { street: "flop", pot: 900, boardCards: [38, 12, 25] },
      { street: "turn", pot: 2100, boardCards: [38, 12, 25, 51] },
      { street: "river", pot: 4500, boardCards: [38, 12, 25, 51, 0] },
    ],
    finalPot: 4500,
    boardCards: [38, 12, 25, 51, 0],
    holeCards: [38, 51],
    handRankName: "Pair of Aces",
    winnerAddress: "GBZXN7PIRZGNMHGA7MUUUF4GWDAXSX4KOYU2CJLOVE2AHQB2XNKF5BH",
    txHash: "abc123def456abc123def456abc123def456abc123def456abc123def456abcd",
  },
  {
    tableId: 1,
    handNumber: 2,
    timestamp: Date.now() - 300000,
    streets: [
      { street: "preflop", pot: 200, boardCards: [] },
      { street: "flop", pot: 600, boardCards: [1, 14, 27] },
    ],
    finalPot: 600,
    boardCards: [1, 14, 27],
    holeCards: [10, 23],
    handRankName: "High Card",
    winnerAddress: "GCKFBEIYV2U22IO2BJ4KVJOIP7XPWQGQFKKFKR4V3MIG44MVCZAKJCP4",
  },
];

/**
 * ## HandHistoryPanel
 *
 * Modal overlay showing completed hands from the current session. Each entry
 * displays street-by-street pot progression, community cards, hole cards,
 * hand rank, winner, and an optional Stellar explorer link.
 *
 * ### Accessibility
 * - The modal has a backdrop that can be clicked to dismiss (click-outside-to-close).
 * - Close button uses a visible "✕" character.
 * - Card values are rendered as text, not images.
 * - The explorer link opens in a new tab with `rel="noopener noreferrer"`.
 *
 * ### Visual States
 * - **Empty**: "No completed hands yet this session." message.
 * - **With Entries**: Street breakdown, cards, and winner for each hand.
 * - **Closed**: Component returns `null` when `open` is false.
 */
const meta: Meta<typeof HandHistoryPanel> = {
  title: "Components/HandHistoryPanel",
  component: HandHistoryPanel,
  tags: ["autodocs"],
  argTypes: {
    open: { control: "boolean", description: "Whether the panel is visible." },
    onClose: { description: "Callback to close the panel." },
    entries: { control: "object", description: "Array of HandHistoryEntry records." },
  },
};

export default meta;
type Story = StoryObj<typeof HandHistoryPanel>;

/** Panel with multiple hand history entries. */
export const WithEntries: Story = {
  args: {
    open: true,
    onClose: () => console.log("Close"),
    entries: mockEntries,
  },
};

/** Empty state — no hands played yet. */
export const Empty: Story = {
  args: {
    open: true,
    onClose: () => console.log("Close"),
    entries: [],
  },
};

/** Closed state — returns null. */
export const Closed: Story = {
  args: {
    open: false,
    onClose: () => console.log("Close"),
    entries: mockEntries,
  },
};
