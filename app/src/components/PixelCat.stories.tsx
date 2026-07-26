import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { PixelCat, PixelHeart } from "./PixelCat";

/**
 * ## PixelCat
 *
 * Renders a pixel-art cat sprite from `/cat_sprites/`. Available sprites: 17–21.
 * Sprite 18 is the player's cat (shown with a golden glow).
 *
 * ### Accessibility
 * - Each sprite has a descriptive `alt` attribute (`Cat sprite {n}`).
 * - The idle bounce animation is purely decorative and does not convey information.
 * - Uses `imageRendering: pixelated` for crisp scaling.
 *
 * ### Usage
 * - Use `sprite={18}` with `isUser={true}` for the current player's avatar.
 * - Use `opponentSprite(seatIndex)` helper to assign opponent sprites deterministically.
 */
const meta: Meta<typeof PixelCat> = {
  title: "Components/PixelCat",
  component: PixelCat,
  tags: ["autodocs"],
  argTypes: {
    sprite: {
      control: { type: "number", min: 17, max: 21 },
      description: "Sprite number (17–21). Default 18 (user cat).",
    },
    size: {
      control: { type: "number", min: 16, max: 128 },
      description: "Width/height in pixels. Default 48.",
    },
    idle: {
      control: "boolean",
      description: "Enable idle bounce animation. Default true.",
    },
    flipped: {
      control: "boolean",
      description: "Mirror the sprite horizontally.",
    },
    isUser: {
      control: "boolean",
      description: "Show the golden glow ring (current player indicator).",
    },
  },
};

export default meta;
type Story = StoryObj<typeof PixelCat>;

/** Default user cat with golden glow. */
export const UserCat: Story = {
  args: { sprite: 18, size: 72, isUser: true },
};

/** Opponent cat — no glow, smaller size. */
export const OpponentCat: Story = {
  args: { sprite: 17, size: 48, isUser: false },
};

/** Mirrored opponent cat. */
export const FlippedCat: Story = {
  args: { sprite: 20, size: 48, flipped: true },
};

/** Static cat — idle animation disabled. */
export const StaticCat: Story = {
  args: { sprite: 19, size: 48, idle: false },
};

/** All available sprites. */
export const AllSprites: Story = {
  render: () => (
    <div style={{ display: "flex", gap: 24, alignItems: "center" }}>
      {[17, 18, 19, 20, 21].map((s) => (
        <div key={s} style={{ textAlign: "center" }}>
          <PixelCat sprite={s} size={48} isUser={s === 18} />
          <div style={{ fontSize: 9, marginTop: 4, color: "#95a5a6" }}>#{s}</div>
        </div>
      ))}
    </div>
  ),
};

/**
 * ## PixelHeart
 *
 * A CSS-only pixel art heart, optionally with a beating animation.
 */
export const Heart: StoryObj<typeof PixelHeart> = {
  render: () => (
    <div style={{ display: "flex", gap: 24, alignItems: "center" }}>
      <PixelHeart size={4} />
      <PixelHeart size={4} beating />
    </div>
  ),
};
