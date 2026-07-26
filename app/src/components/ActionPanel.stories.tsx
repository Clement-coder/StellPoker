import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { useState } from "react";
import { ActionPanel } from "./ActionPanel";

/**
 * ## ActionPanel
 *
 * The betting controls panel — fold, check/call, bet/raise, all-in, plus a
 * bet slider and quick-bet presets (50%, 75%, MAX).
 *
 * ### Accessibility
 * - Each button has a keyboard shortcut tooltip (e.g. `[F]` for Fold).
 * - Buttons are disabled (with `disabled` attribute) when it's not the user's
 *   turn, providing correct keyboard navigation.
 * - The bet slider uses native `<input type="range">` for keyboard and
 *   assistive technology support.
 * - Status hints and waiting messages are visible text, not just visual cues.
 *
 * ### Visual States
 * - **Waiting Phase**: "DEAL CARDS" button shown.
 * - **Settlement Phase**: "NEW HAND" button shown.
 * - **Active (My Turn)**: Full betting controls with slider.
 * - **Active (Opponent Turn)**: Buttons disabled, "WAITING FOR OPPONENT..." hint.
 * - **Loading**: Buttons disabled during transaction processing.
 * - **Solo Mode**: "SOLO VS AI" hint displayed.
 */
const meta: Meta<typeof ActionPanel> = {
  title: "Components/ActionPanel",
  component: ActionPanel,
  tags: ["autodocs"],
  argTypes: {
    phase: {
      control: "select",
      options: ["waiting", "dealing", "preflop", "flop", "turn", "river", "showdown", "settlement"],
      description: "Current game phase.",
    },
    isMyTurn: { control: "boolean", description: "Whether it is the current user's turn." },
    currentBet: { control: "number", description: "Current table bet to match." },
    myBet: { control: "number", description: "User's bet in the current round." },
    myStack: { control: "number", description: "User's remaining chip stack." },
    onAction: { description: "Callback: `(action, amount?) => void`." },
    canStartHand: { control: "boolean", description: "Whether the DEAL CARDS button is enabled." },
    canResolveShowdown: { control: "boolean", description: "Whether showdown can be resolved." },
    statusHint: { control: "text", description: "Optional status message shown below buttons." },
    loading: { control: "boolean", description: "Disable all buttons during transaction processing." },
    isSolo: { control: "boolean", description: "Show solo-mode indicator." },
    betAmount: { control: "number", description: "Current bet slider value." },
    setBetAmount: { description: "Callback to update bet slider." },
  },
};

export default meta;
type Story = StoryObj<typeof ActionPanel>;

function ActionPanelWithState(props: Partial<React.ComponentProps<typeof ActionPanel>>) {
  const [betAmount, setBetAmount] = useState(props.betAmount ?? 100);
  return (
    <ActionPanel
      phase="preflop"
      isMyTurn={true}
      currentBet={100}
      myBet={50}
      myStack={5000}
      onAction={(action, amount) => console.log("Action:", action, amount)}
      betAmount={betAmount}
      setBetAmount={setBetAmount}
      {...props}
    />
  );
}

/** Active betting — user's turn with all controls visible. */
export const ActiveMyTurn: Story = {
  render: () => <ActionPanelWithState />,
};

/** Waiting for opponent — buttons disabled. */
export const WaitingForOpponent: Story = {
  render: () => <ActionPanelWithState isMyTurn={false} />,
};

/** Waiting phase — show DEAL CARDS button. */
export const WaitingPhase: Story = {
  render: () => <ActionPanelWithState phase="waiting" />,
};

/** Settlement phase — show NEW HAND button. */
export const SettlementPhase: Story = {
  render: () => <ActionPanelWithState phase="settlement" />,
};

/** Loading state — all buttons disabled during proof generation. */
export const Loading: Story = {
  render: () => <ActionPanelWithState loading={true} />,
};

/** Solo mode — playing against AI bot. */
export const SoloMode: Story = {
  render: () => <ActionPanelWithState isSolo={true} />,
};

/** Check available — no bet to call. */
export const CheckAvailable: Story = {
  render: () => <ActionPanelWithState currentBet={0} myBet={0} />,
};

/** Status hint displayed. */
export const WithStatusHint: Story = {
  render: () => (
    <ActionPanelWithState
      statusHint="Connected wallet is not seated. Click JOIN TABLE first."
    />
  ),
};

/** Low stack — can't raise, only call or all-in. */
export const LowStack: Story = {
  render: () => <ActionPanelWithState myStack={80} currentBet={100} myBet={0} />,
};

/** Showdown phase — returns null (no panel). */
export const ShowdownPhase: Story = {
  render: () => (
    <div style={{ color: "#95a5a6", fontSize: 10 }}>
      <p style={{ marginBottom: 8 }}>ActionPanel returns null during showdown:</p>
      <ActionPanelWithState phase="showdown" />
      <p>(nothing rendered above)</p>
    </div>
  ),
};
