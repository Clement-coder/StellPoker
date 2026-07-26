import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { Board } from "./Board";

/**
 * ## Board
 *
 * Renders the community cards and pot display at the center of the poker table.
 * Unfilled slots show dashed placeholders. New cards flip in with staggered
 * 3D animations.
 *
 * ### Accessibility
 * - Card values are rendered as text (rank + suit symbol), readable by
 *   screen readers.
 * - Empty slots use a dashed border placeholder — visually distinct from dealt
 *   cards for low-vision users.
 * - Flip animations respect `prefers-reduced-motion`.
 *
 * ### Visual States
 * - **Empty**: No community cards dealt — 5 dashed placeholders.
 * - **Flop**: 3 cards revealed, 2 remaining slots.
 * - **Turn**: 4 cards revealed, 1 remaining slot.
 * - **River**: All 5 cards revealed.
 */
const meta: Meta<typeof Board> = {
  title: "Components/Board",
  component: Board,
  tags: ["autodocs"],
  argTypes: {
    cards: {
      control: "object",
      description: "Array of card values (0–51) representing dealt community cards.",
    },
    pot: {
      control: { type: "number", min: 0 },
      description: "Current pot size in chips.",
    },
  },
};

export default meta;
type Story = StoryObj<typeof Board>;

/** Empty board — waiting for flop. */
export const Empty: Story = {
  args: { cards: [], pot: 0 },
};

/** Flop — 3 community cards revealed. */
export const Flop: Story = {
  args: { cards: [38, 12, 25], pot: 1500 },
};

/** Turn — 4 community cards. */
export const Turn: Story = {
  args: { cards: [38, 12, 25, 51], pot: 3200 },
};

/** River — all 5 community cards revealed. */
export const River: Story = {
  args: { cards: [38, 12, 25, 51, 0], pot: 7800 },
};

/** Large pot display. */
export const LargePot: Story = {
  args: { cards: [1, 14, 27, 40, 50], pot: 25000 },
};
