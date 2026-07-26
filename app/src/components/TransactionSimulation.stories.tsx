import type { Meta, StoryObj } from "@storybook/react-webpack5";
import { TransactionSimulation } from "./TransactionSimulation";
import type { SimulationResult } from "@/lib/transaction-simulation";

const successSimulation: SimulationResult = {
  success: true,
  fee: "0.0001",
  gasUsed: 245000,
  stateChanges: [
    { type: "contract_data", description: "Update table state to Preflop" },
    { type: "balance", description: "Transfer 1000 XLM buy-in to contract" },
    { type: "account", description: "Add player to seat 0" },
  ],
};

const failedSimulation: SimulationResult = {
  success: false,
  fee: "0",
  stateChanges: [],
  error: "InsufficientBalance: account balance too low for buy-in amount",
};

/**
 * ## TransactionSimulation
 *
 * Modal overlay that previews a Stellar/Soroban transaction before the user
 * signs it. Shows simulation status, fee, gas usage, expected state changes,
 * and optional raw technical details.
 *
 * ### Accessibility
 * - Confirm button is disabled when simulation failed — prevents signing a
 *   known-bad transaction.
 * - Error messages are displayed in a high-contrast red box.
 * - Success indicator uses both icon (✅) and text for redundancy.
 * - "Show/Hide technical details" toggle is focusable and keyboard-accessible.
 *
 * ### Visual States
 * - **Success**: Green status, fee/gas info, state changes, confirm enabled.
 * - **Failed**: Red status with error message, confirm disabled.
 * - **Loading**: "Signing..." text on confirm button, both buttons disabled.
 */
const meta: Meta<typeof TransactionSimulation> = {
  title: "Components/TransactionSimulation",
  component: TransactionSimulation,
  tags: ["autodocs"],
  argTypes: {
    simulation: { control: "object", description: "SimulationResult from the transaction preview." },
    onConfirm: { description: "Callback when user clicks Sign & Send." },
    onCancel: { description: "Callback when user clicks Cancel." },
    loading: { control: "boolean", description: "Show loading state on confirm button." },
  },
};

export default meta;
type Story = StoryObj<typeof TransactionSimulation>;

/** Successful simulation — ready to sign. */
export const Success: Story = {
  args: {
    simulation: successSimulation,
    onConfirm: () => console.log("Confirmed"),
    onCancel: () => console.log("Cancelled"),
  },
};

/** Failed simulation — confirm disabled, error shown. */
export const Failed: Story = {
  args: {
    simulation: failedSimulation,
    onConfirm: () => console.log("Confirmed"),
    onCancel: () => console.log("Cancelled"),
  },
};

/** Loading state — signing in progress. */
export const Signing: Story = {
  args: {
    simulation: successSimulation,
    onConfirm: () => console.log("Confirmed"),
    onCancel: () => console.log("Cancelled"),
    loading: true,
  },
};
