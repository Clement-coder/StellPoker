import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { GameBoyModal, GameBoyButton } from "./GameBoyModal";

/**
 * ## GameBoyModal
 *
 * A Game Boy-themed modal that houses settings (volume, logout), a ZK proof
 * explorer, and a built-in Flappy Bird mini-game. Styled to look like a
 * physical Game Boy with an LCD screen, D-pad, and A/B buttons.
 *
 * ### Accessibility
 * - Clicking the backdrop closes the modal (click-outside-to-close pattern).
 * - Keyboard: `Escape` closes the modal; `Space`/`ArrowUp`/`A` controls
 *   Flappy Bird; B button also closes.
 * - Tab switcher buttons are keyboard-focusable.
 * - Volume slider uses a native `<input type="range">` (hidden visually,
 *   overlaying a pixel-art track) for screen reader and keyboard support.
 *
 * ### Visual States
 * - **Settings Tab**: Volume slider + logout button.
 * - **ZK Proofs Tab**: Proof explorer showing deal/flop/turn/river/showdown.
 * - **Flappy Bird Tab**: Playable canvas mini-game.
 * - **Closed**: Returns `null` when `open` is false.
 *
 * ## GameBoyButton
 *
 * Pixel-art Game Boy icon button that opens the modal. Scales up on hover.
 */
const meta: Meta<typeof GameBoyModal> = {
  title: "Components/GameBoyModal",
  component: GameBoyModal,
  tags: ["autodocs"],
  argTypes: {
    open: { control: "boolean", description: "Whether the modal is visible." },
    onClose: { description: "Callback to close the modal." },
    onLogout: { description: "Callback triggered by the LOGOUT button." },
  },
};

export default meta;
type Story = StoryObj<typeof GameBoyModal>;

/** Modal open — Settings tab by default. */
export const Open: Story = {
  args: {
    open: true,
    onClose: () => console.log("Close"),
    onLogout: () => console.log("Logout"),
  },
};

/** Modal closed — returns null. */
export const Closed: Story = {
  args: {
    open: false,
    onClose: () => console.log("Close"),
    onLogout: () => console.log("Logout"),
  },
};

/** The Game Boy icon button that opens the modal. */
export const IconButton: StoryObj<typeof GameBoyButton> = {
  render: () => <GameBoyButton onClick={() => alert("Open modal")} />,
};
