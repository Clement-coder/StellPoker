# Frontend Transaction Simulation

This document describes the transaction simulation feature implemented for issue #311.

## Overview

Before requesting wallet signatures, the frontend now simulates Soroban transactions client-side and shows users:
- Expected gas costs and fees
- Predicted state changes
- Success/failure status
- Technical details (optional)

This provides informed consent before signing transactions.

## Architecture

```
User Action → Simulation → Confirmation Dialog → Wallet Signature → Execution
```

### Core Components

1. **Transaction Simulation Library** (`lib/transaction-simulation.ts`)
   - `simulateJoinTable()` - Simulates joining a poker table
   - `simulatePlayerAction()` - Simulates betting actions
   - `SimulationResult` interface for standardized results

2. **Simulation Hook** (`lib/use-transaction-simulation.ts`)
   - `useTransactionSimulation()` - Core simulation workflow
   - `useJoinTableSimulation()` - Specialized for table joining
   - `usePlayerActionSimulation()` - Specialized for betting actions

3. **UI Component** (`components/TransactionSimulation.tsx`)
   - Modal dialog showing simulation results
   - Cost breakdown (fees, gas usage)
   - State change descriptions
   - Confirmation/cancellation buttons

### Integration Points

1. **Main Page** (`app/page.tsx`)
   - Table creation with buy-in uses simulation
   - Shows simulation dialog before wallet signature

2. **Poker Actions** (`lib/use-poker-actions.ts`)
   - All betting actions trigger simulation first
   - Table joining includes simulation step

3. **Table Component** (`components/Table.tsx`)
   - Renders simulation dialogs for all transactions
   - Handles confirmation/cancellation workflows

## Usage Examples

### Join Table Simulation
```typescript
const joinSim = useJoinTableSimulation(wallet, onSuccess);

// Trigger simulation
joinSim.joinTable(tableId, buyIn);

// In UI - show dialog when simulation completes
{joinSim.showSimulation && (
  <TransactionSimulation
    simulation={joinSim.simulation}
    onConfirm={() => joinSim.confirmJoin(tableId, buyIn)}
    onCancel={() => joinSim.cancelSimulation()}
  />
)}
```

### Player Action Simulation
```typescript
const actionSim = usePlayerActionSimulation(wallet, onSuccess);

// Trigger simulation  
actionSim.performAction(tableId, "bet", 1000);

// Confirm after user reviews
actionSim.confirmAction(tableId, "bet", 1000);
```

## Simulation Results

The `SimulationResult` interface provides:

```typescript
interface SimulationResult {
  success: boolean;           // Whether simulation succeeded
  fee: string;               // Fee in XLM 
  gasUsed?: number;          // Gas consumption
  stateChanges: StateChange[]; // Predicted changes
  error?: string;            // Error message if failed
  rawResult?: any;           // Full Soroban response
}
```

### State Change Types

- `balance` - XLM balance changes (fees)
- `contract_data` - Contract state updates  
- `account` - Account sequence increments
- `trustline` - Asset trustline changes

## User Experience

1. **Immediate Feedback**: Users see costs before signing
2. **Informed Decisions**: Clear breakdown of what will happen
3. **Error Prevention**: Failed simulations prevent bad transactions
4. **Technical Details**: Optional detailed view for advanced users

## Error Handling

- **Simulation Failures**: Show error message, prevent signing
- **Network Issues**: Graceful fallback with error display
- **Cancellation**: Clean state reset, no side effects

## Security Considerations

- Simulations use read-only RPC calls
- No sensitive data in simulation requests
- Actual signing still requires wallet approval
- Simulation results are informational only

## Future Enhancements

- Custom fee estimation
- Multi-step transaction simulation
- Balance change predictions
- Advanced gas optimization hints
- Simulation caching for repeated actions

## Testing

Test the simulation feature:

1. **Happy Path**: Create table with buy-in, verify simulation shows
2. **Error Handling**: Try invalid actions, confirm error display
3. **Cancellation**: Cancel simulation, verify clean state
4. **Confirmation**: Complete simulation flow, verify transaction executes

## Dependencies

- `@stellar/stellar-sdk` ^13.3.0 for RPC simulation
- React hooks for state management
- Tailwind CSS for styling
