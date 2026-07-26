import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { Card } from "./Card";

/**
 * ## Card
 *
 * Renders a single poker card as pixel art. Supports face-up (with rank/suit),
 * face-down (card back), and a 3D flip animation for deals and reveals.
 *
 * ### Accessibility
 * - Card rank and suit are rendered as visible text (not images), so screen
 *   readers can announce them.
 * - The flip animation respects `prefers-reduced-motion` — when enabled, cards
 *   appear instantly without animation (see `globals.css`).
 * - Face-down cards display a consistent visual pattern with no information leakage.
 *
 * ### Card Values
 * Cards are encoded as integers 0–51:
 * - `0–12` = Clubs (2 through A)
 * - `13–25` = Diamonds (2 through A)
 * - `26–38` = Hearts (2 through A)
 * - `39–51` = Spades (2 through A)
 */
const meta: Meta<typeof Card> = {
  title: "Components/Card",
  component: Card,
  tags: ["autodocs"],
  argTypes: {
    value: {
      control: { type: "number", min: 0, max: 51 },
      description:
        "Card value (0–51). Encodes suit and rank. Clubs 0–12, Diamonds 13–25, Hearts 26–38, Spades 39–51.",
    },
    faceDown: {
      control: "boolean",
      description: "Render the card back instead of the face.",
    },
    size: {
      control: "radio",
      options: ["sm", "md", "lg"],
      description: "Card dimensions — sm (44×62), md (56×80), lg (72×100).",
    },
    flip: {
      control: "boolean",
      description:
        "Play a 3D flip animation on mount. Ignored for face-down cards. Respects prefers-reduced-motion.",
    },
    flipDelay: {
      control: { type: "number", min: 0, max: 1, step: 0.05 },
      description: "Stagger delay (seconds) for the flip animation.",
    },
  },
};

export default meta;
type Story = StoryObj<typeof Card>;

/** A face-up Ace of Spades at default (md) size. */
export const Default: Story = {
  args: { value: 51, size: "md" },
};

/** Face-down card showing the card back pattern. */
export const FaceDown: Story = {
  args: { faceDown: true, size: "md" },
};

/** Small card used for opponent hands. */
export const Small: Story = {
  args: { value: 38, size: "sm" },
};

/** Large card for emphasis or close-up views. */
export const Large: Story = {
  args: { value: 26, size: "lg" },
};

/** Card with 3D flip animation (Ace of Hearts). */
export const FlipAnimation: Story = {
  args: { value: 38, size: "md", flip: true },
};

/** Staggered flip — delays the animation start by 0.2s. */
export const StaggeredFlip: Story = {
  args: { value: 25, size: "md", flip: true, flipDelay: 0.2 },
};

/** Red card — King of Diamonds. */
export const RedCard: Story = {
  args: { value: 24, size: "md" },
};

/** Black card — King of Clubs. */
export const BlackCard: Story = {
  args: { value: 11, size: "md" },
};

/** All three sizes side by side. */
export const AllSizes: Story = {
  render: () => (
    <div style={{ display: "flex", alignItems: "flex-end", gap: 16 }}>
      <Card value={51} size="sm" />
      <Card value={51} size="md" />
      <Card value={51} size="lg" />
    </div>
  ),
};

/** No value provided — renders as face-down. */
export const EmptyState: Story = {
  args: { size: "md" },
};
