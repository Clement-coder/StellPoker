import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { PixelChip, PixelChipStack, PotChipPile } from "./PixelChip";

/**
 * ## PixelChip
 *
 * CSS-only pixel art poker chip sprite. Four denominations: white (25),
 * red (100), blue (500), gold (1000). Box-shadow pixel art — no images.
 *
 * ### Accessibility
 * - Purely decorative — used alongside text labels that convey the chip value.
 * - No interactive elements; no ARIA roles needed.
 *
 * ### Related Components
 * - `PixelChipStack` — vertical stack of chips by amount.
 * - `PotChipPile` — horizontal pot display layout.
 */
const meta: Meta<typeof PixelChip> = {
  title: "Components/PixelChip",
  component: PixelChip,
  tags: ["autodocs"],
  argTypes: {
    color: {
      control: "radio",
      options: ["white", "red", "blue", "gold"],
      description:
        "Chip color/denomination: white (25), red (100), blue (500), gold (1000).",
    },
    size: {
      control: { type: "number", min: 1, max: 8 },
      description: "Pixel scale factor. Default 3.",
    },
  },
};

export default meta;
type Story = StoryObj<typeof PixelChip>;

/** Default red chip. */
export const Default: Story = {
  args: { color: "red", size: 3 },
};

/** All four chip colors. */
export const AllColors: Story = {
  render: () => (
    <div style={{ display: "flex", gap: 16, alignItems: "center" }}>
      {(["white", "red", "blue", "gold"] as const).map((c) => (
        <div key={c} style={{ textAlign: "center" }}>
          <PixelChip color={c} size={3} />
          <div style={{ fontSize: 9, marginTop: 4, color: "#95a5a6" }}>{c}</div>
        </div>
      ))}
    </div>
  ),
};

/** Chip stack — automatic color selection by amount. */
export const ChipStack: StoryObj<typeof PixelChipStack> = {
  render: () => (
    <div style={{ display: "flex", gap: 32, alignItems: "flex-end" }}>
      {[50, 500, 2000, 8000].map((amount) => (
        <div key={amount} style={{ textAlign: "center" }}>
          <PixelChipStack amount={amount} size={2} />
          <div style={{ fontSize: 9, marginTop: 4, color: "#95a5a6" }}>
            {amount.toLocaleString()}
          </div>
        </div>
      ))}
    </div>
  ),
};

/** Pot chip pile — horizontal layout for the community pot. */
export const PotPile: StoryObj<typeof PotChipPile> = {
  render: () => (
    <div style={{ display: "flex", gap: 32, alignItems: "flex-end" }}>
      {[100, 1000, 5000, 15000].map((amount) => (
        <div key={amount} style={{ textAlign: "center" }}>
          <PotChipPile amount={amount} size={3} />
          <div style={{ fontSize: 9, marginTop: 4, color: "#95a5a6" }}>
            {amount.toLocaleString()}
          </div>
        </div>
      ))}
    </div>
  ),
};

/** Empty state — zero amount renders nothing. */
export const EmptyStack: StoryObj<typeof PixelChipStack> = {
  render: () => <PixelChipStack amount={0} />,
};
