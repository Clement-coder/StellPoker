import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { Identicon } from "./Identicon";

/**
 * ## Identicon
 *
 * Deterministic pixel-grid avatar derived from a seed string (typically a
 * Stellar address). Produces a unique, stable visual fingerprint for each
 * address, independent of the cat sprite (which is assigned by seat index).
 *
 * ### Accessibility
 * - Purely decorative identity badge — does not replace the text address.
 * - Rendered as a CSS grid with `imageRendering: pixelated` for crispness.
 * - Color contrast between `fg` and `bg` is deterministic from the seed hash.
 *
 * ### Usage
 * Shown as a small overlay badge on the player's cat avatar to help distinguish
 * players who might share the same cat sprite index.
 */
const meta: Meta<typeof Identicon> = {
  title: "Components/Identicon",
  component: Identicon,
  tags: ["autodocs"],
  argTypes: {
    seed: {
      control: "text",
      description:
        "Input seed (e.g. a Stellar address). Different seeds produce different patterns.",
    },
    size: {
      control: { type: "number", min: 3, max: 9 },
      description: "Grid size (NxN cells). Default 5.",
    },
    cellSize: {
      control: { type: "number", min: 1, max: 8 },
      description: "Size of each cell in pixels. Default 3.",
    },
  },
};

export default meta;
type Story = StoryObj<typeof Identicon>;

/** Default 5×5 identicon from a Stellar address. */
export const Default: Story = {
  args: {
    seed: "GBZXN7PIRZGNMHGA7MUUUF4GWDAXSX4KOYU2CJLOVE2AHQB2XNKF5BH",
    size: 5,
    cellSize: 3,
  },
};

/** Larger cell size for visibility. */
export const LargeCells: Story = {
  args: {
    seed: "GBZXN7PIRZGNMHGA7MUUUF4GWDAXSX4KOYU2CJLOVE2AHQB2XNKF5BH",
    size: 5,
    cellSize: 6,
  },
};

/** Multiple unique identicons from different addresses. */
export const MultipleSeeds: Story = {
  render: () => (
    <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
      {[
        "GBZXN7PIRZGNMHGA7MUUUF4GWDAXSX4KOYU2CJLOVE2AHQB2XNKF5BH",
        "GCKFBEIYV2U22IO2BJ4KVJOIP7XPWQGQFKKFKR4V3MIG44MVCZAKJCP4",
        "GCEZWKCA5VLDNRLN3RPRJMRZOX3Z6G5CHCGSNFHEBD9AFZQ7TM4JRS9A",
        "GDJ4X5NVRMIB3ZYB2VO5I7P3HSEQSEN3MVXZWCFSEBXLODIBPIQM2DQC",
      ].map((addr) => (
        <div key={addr} style={{ textAlign: "center" }}>
          <Identicon seed={addr} size={5} cellSize={4} />
          <div style={{ fontSize: 8, marginTop: 4, color: "#95a5a6" }}>
            {addr.slice(0, 8)}...
          </div>
        </div>
      ))}
    </div>
  ),
};

/** Larger grid (7×7) for more detail. */
export const LargeGrid: Story = {
  args: {
    seed: "GBZXN7PIRZGNMHGA7MUUUF4GWDAXSX4KOYU2CJLOVE2AHQB2XNKF5BH",
    size: 7,
    cellSize: 4,
  },
};
