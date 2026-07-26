import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { PlayerSeat } from "./PlayerSeat";
import type { Player } from "@/lib/game-state";

const mockPlayer: Player = {
  address: "GBZXN7PIRZGNMHGA7MUUUF4GWDAXSX4KOYU2CJLOVE2AHQB2XNKF5BH",
  seat: 0,
  stack: 5000,
  betThisRound: 0,
  folded: false,
  allIn: false,
  cards: [38, 51],
};

/**
 * ## PlayerSeat
 *
 * Renders a player's seat at the table — avatar, cards, chip stack, bet amount,
 * and status tags (folded, all-in, winner, turn indicator).
 *
 * ### Accessibility
 * - Labels are rendered as visible text (not just color), so status is conveyed
 *   to screen readers (e.g. "FOLDED", "ALL IN!", "YOUR TURN").
 * - The edit alias button has a descriptive `title` attribute.
 * - Turn indicator includes directional text ("▼ YOUR TURN ▼") in addition to
 *   the pulsing glow animation.
 * - The identicon badge provides a visual fingerprint independent of cat sprite.
 *
 * ### Visual States
 * - **Active**: Normal play, cards dealt, chip stack visible.
 * - **Current Turn**: Pulsing glow border + "YOUR TURN" / "THEIR TURN" label.
 * - **Folded**: 50% opacity + "FOLDED" tag.
 * - **All-In**: Pulsing "ALL IN!" label.
 * - **Winner**: "★ WINNER ★" badge.
 * - **Bot**: AI bot sprite instead of cat.
 * - **Emote**: Speech bubble with emoji above the seat.
 */
const meta: Meta<typeof PlayerSeat> = {
  title: "Components/PlayerSeat",
  component: PlayerSeat,
  tags: ["autodocs"],
  argTypes: {
    player: { control: "object", description: "Player data (address, stack, cards, status)." },
    isCurrentTurn: { control: "boolean", description: "Whether it is this player's turn." },
    isDealer: { control: "boolean", description: "Show the [D] dealer badge." },
    isUser: { control: "boolean", description: "Is this the current user's seat." },
    isWinner: { control: "boolean", description: "Show the ★ WINNER ★ badge." },
    isBot: { control: "boolean", description: "Render bot avatar instead of cat sprite." },
    labelOverride: { control: "text", description: "Override the default label text." },
    alias: { control: "text", description: "Client-side display alias for this player." },
    onEditAlias: { description: "Callback to edit alias. Renders an [EDIT] button when set." },
    hideChipStats: { control: "boolean", description: "Hide the stack and bet display." },
    activeEmote: { control: "text", description: "Emoji emote displayed in a speech bubble." },
  },
};

export default meta;
type Story = StoryObj<typeof PlayerSeat>;

/** Current user's seat — face-up cards, golden label. */
export const UserSeat: Story = {
  args: {
    player: mockPlayer,
    isCurrentTurn: false,
    isDealer: true,
    isUser: true,
    isWinner: false,
  },
};

/** Opponent seat — face-down cards, truncated address. */
export const OpponentSeat: Story = {
  args: {
    player: { ...mockPlayer, seat: 1, cards: [10, 20] },
    isCurrentTurn: false,
    isDealer: false,
    isUser: false,
  },
};

/** User's turn — pulsing glow border and turn indicator. */
export const UserTurn: Story = {
  args: {
    player: mockPlayer,
    isCurrentTurn: true,
    isDealer: false,
    isUser: true,
  },
};

/** Opponent's turn — "THEIR TURN" indicator. */
export const OpponentTurn: Story = {
  args: {
    player: { ...mockPlayer, seat: 1 },
    isCurrentTurn: true,
    isDealer: false,
    isUser: false,
  },
};

/** Folded player — dimmed opacity + "FOLDED" tag. */
export const Folded: Story = {
  args: {
    player: { ...mockPlayer, folded: true },
    isCurrentTurn: false,
    isDealer: false,
    isUser: true,
  },
};

/** All-in player — pulsing "ALL IN!" tag. */
export const AllIn: Story = {
  args: {
    player: { ...mockPlayer, allIn: true, betThisRound: 5000, stack: 0 },
    isCurrentTurn: false,
    isDealer: false,
    isUser: true,
  },
};

/** Winner badge displayed. */
export const Winner: Story = {
  args: {
    player: mockPlayer,
    isCurrentTurn: false,
    isDealer: false,
    isUser: true,
    isWinner: true,
  },
};

/** AI Bot opponent — different avatar sprite. */
export const BotPlayer: Story = {
  args: {
    player: { ...mockPlayer, seat: 1 },
    isCurrentTurn: false,
    isDealer: false,
    isUser: false,
    isBot: true,
  },
};

/** Player with a custom alias. */
export const WithAlias: Story = {
  args: {
    player: mockPlayer,
    isCurrentTurn: false,
    isDealer: true,
    isUser: true,
    alias: "StellarShark",
    onEditAlias: () => alert("Edit alias clicked"),
  },
};

/** Player with an active bet. */
export const ActiveBet: Story = {
  args: {
    player: { ...mockPlayer, betThisRound: 500 },
    isCurrentTurn: false,
    isDealer: false,
    isUser: true,
  },
};

/** Player sending an emote. */
export const WithEmote: Story = {
  args: {
    player: mockPlayer,
    isCurrentTurn: false,
    isDealer: false,
    isUser: true,
    activeEmote: "😎",
  },
};

/** Low stack player (red chip indicator). */
export const LowStack: Story = {
  args: {
    player: { ...mockPlayer, stack: 50 },
    isCurrentTurn: false,
    isDealer: false,
    isUser: true,
  },
};
