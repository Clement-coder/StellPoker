import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { ProofExplorer } from "./ProofExplorer";

/**
 * ## ProofExplorer
 *
 * Displays the zero-knowledge proof pipeline for a poker hand — deal, flop,
 * turn, river, and showdown. Each phase shows verification status, an info
 * tooltip explaining what/why, proof size, and a transaction hash link.
 *
 * ### Accessibility
 * - Info tooltips use `aria-label="More information"` on the trigger button.
 * - Tooltips close when clicking outside (backdrop click pattern).
 * - "How it works" section is a collapsible with a visible toggle indicator
 *   (▶ / ▼).
 * - Status labels use both icons and text: "✔ VERIFIED", "✗ MISSING",
 *   "— PENDING".
 * - Transaction hashes are displayed as plain text (not links to external
 *   explorers in this component).
 *
 * ### Visual States
 * - **No Data**: All phases show as "PENDING".
 * - **Partial Verification**: Some phases verified, others pending.
 * - **Fully Verified**: All 5 phases show "VERIFIED" with tx hashes.
 */
const meta: Meta<typeof ProofExplorer> = {
  title: "Components/ProofExplorer",
  component: ProofExplorer,
  tags: ["autodocs"],
  argTypes: {
    data: {
      control: "object",
      description: "Proof data: deal tx hash, per-street reveal tx hashes, showdown tx hash.",
    },
  },
  decorators: [
    (Story) => (
      <div
        style={{
          background: "#b8c4a0",
          padding: 16,
          maxWidth: 340,
          fontFamily: "'Press Start 2P', monospace",
        }}
      >
        <Story />
      </div>
    ),
  ],
};

export default meta;
type Story = StoryObj<typeof ProofExplorer>;

/** All proofs pending — no data provided. */
export const AllPending: Story = {
  args: {},
};

/** Partially verified — deal and flop proofs confirmed. */
export const PartiallyVerified: Story = {
  args: {
    data: {
      dealTxHash: "abc123def456abc123def456abc123def456abc123def456abc123def456abcd",
      revealTxHashes: {
        flop: "flopabcdef123456flopabcdef123456flopabcdef123456flopabcdef12345678",
      },
    },
  },
};

/** Fully verified — all proofs have tx hashes. */
export const FullyVerified: Story = {
  args: {
    data: {
      dealTxHash: "abc123def456abc123def456abc123def456abc123def456abc123def456abcd",
      revealTxHashes: {
        flop: "flopabcdef123456flopabcdef123456flopabcdef123456flopabcdef12345678",
        turn: "turnabcdef123456turnabcdef123456turnabcdef123456turnabcdef12345678",
        river: "riverabcdef12345riverabcdef12345riverabcdef12345riverabcdef1234567",
      },
      showdownTxHash: "showdownabcdef12showdownabcdef12showdownabcdef12showdownabcdef12",
    },
  },
};
