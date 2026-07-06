# Wallet Integration Testing

This document describes the wallet integration testing framework implemented for issue #297.

## Overview

Comprehensive test suite covering wallet connection flows, signature requests, transaction approval/rejection, account switching, and network detection using a sophisticated Freighter mock.

## Test Structure

### Core Test Files

1. **`wallet-integration.test.ts`** - Main wallet integration tests
2. **`transaction-simulation-integration.test.ts`** - Simulation + wallet flow tests
3. **`mocks/freighter-mock.ts`** - Sophisticated Freighter API mock
4. **`test-utils.ts`** - Shared testing utilities

## FreighterMock Features

### Complete API Coverage
```typescript
const freighterMock = new FreighterMock({
  isInstalled: true,
  address: "GABCDEFG...",
  connected: true,
  signResponse: "signature_string",
  networkPassphrase: "Test SDF Network ; September 2015"
});
```

### Dynamic State Management
```typescript
// Simulate account switching
freighterMock.simulateAccountSwitch("GNEWADDRESS...");

// Simulate user rejection
freighterMock.simulateUserRejection();

// Simulate disconnection
freighterMock.simulateDisconnection();

// Reset to initial state
freighterMock.reset();
```

### Installation/Uninstallation
```typescript
// Install on all possible window properties
freighterMock.install();

// Clean removal
freighterMock.uninstall();
```

## Test Coverage

### ✅ Wallet Connection Flow
- Successful connection to Freighter
- Connection failure when not installed
- Access request rejection handling
- Disconnected wallet state handling

### ✅ Signature Requests  
- String response format
- Object response format (`{ signature: "..." }`)
- User rejection handling
- Invalid response handling
- Malformed response handling

### ✅ Transaction Approval/Rejection
- Successful transaction signing
- User rejection of transactions
- Transaction XDR handling
- Network parameter validation

### ✅ Account Switching
- Detection of account changes
- Handling disconnection during switch
- Address update propagation
- Session invalidation

### ✅ Network Detection
- Testnet network detection
- Mainnet network detection
- Network details retrieval
- Network passphrase validation

### ✅ Silent Reconnection
- Successful reconnect with stored credentials
- Graceful handling when no stored data
- Reconnection failure handling
- localStorage interaction

### ✅ Error Handling
- Wallet API errors
- Malformed responses
- Network errors
- Timeout scenarios

### ✅ Integration Testing
- End-to-end wallet + simulation flow
- Prevention of execution on failed simulation
- Full transaction lifecycle testing

## Usage Examples

### Basic Test Setup
```typescript
import { FreighterMock } from "./mocks/freighter-mock";

describe("Wallet Tests", () => {
  let freighterMock: FreighterMock;

  beforeEach(() => {
    freighterMock = new FreighterMock();
    freighterMock.install();
  });

  afterEach(() => {
    freighterMock.uninstall();
  });

  it("connects to wallet", async () => {
    const { connectFreighterWallet } = await import("../lib/freighter");
    const session = await connectFreighterWallet();
    
    expect(session.walletType).toBe("freighter");
    expect(session.address).toBeDefined();
  });
});
```

### Testing Error Scenarios
```typescript
it("handles user rejection", async () => {
  const { connectFreighterWallet } = await import("../lib/freighter");
  const session = await connectFreighterWallet();
  
  freighterMock.simulateUserRejection();
  
  await expect(session.signMessage("test"))
    .rejects.toThrow("User rejected request");
});
```

### Testing Account Switching
```typescript
it("detects account changes", async () => {
  const { connectFreighterWallet } = await import("../lib/freighter");
  let session = await connectFreighterWallet();
  
  expect(session.address).toBe("GABCDEFG...");
  
  freighterMock.simulateAccountSwitch("GNEWADDR...");
  
  session = await connectFreighterWallet();
  expect(session.address).toBe("GNEWADDR...");
});
```

## Mock Configurations

### Successful Flow
```typescript
new FreighterMock({
  isInstalled: true,
  connected: true,
  address: "GABCDEFG...",
  signResponse: "signature_string"
})
```

### Error Scenarios
```typescript
// Not installed
new FreighterMock({ isInstalled: false })

// Access denied
new FreighterMock({ accessError: "User denied access" })

// Signature rejection
new FreighterMock({ signError: "User rejected request" })

// Not connected
new FreighterMock({ connected: false })
```

### Different Networks
```typescript
// Testnet
new FreighterMock({
  networkPassphrase: "Test SDF Network ; September 2015"
})

// Mainnet  
new FreighterMock({
  networkPassphrase: "Public Global Stellar Network ; September 2015"
})
```

## Running Tests

```bash
# Run all wallet tests
npm test wallet

# Run with coverage
npm run test:coverage

# Run integration tests specifically
npm test wallet-integration

# Run simulation integration tests
npm test transaction-simulation-integration
```

## Mock Verification

The mock provides access to spy functions for verification:

```typescript
const api = freighterMock.getMockApi();

expect(api.getAddress).toHaveBeenCalledTimes(1);
expect(api.signMessage).toHaveBeenCalledWith("test message", undefined);
expect(api.requestAccess).toHaveBeenCalled();
```

## Testing Best Practices

1. **Isolation**: Each test gets fresh mock instances
2. **Cleanup**: Always uninstall mocks in `afterEach`  
3. **State Management**: Use mock methods to change state during tests
4. **Error Testing**: Test both success and failure paths
5. **Integration**: Test wallet integration with other modules

## Future Enhancements

- React component testing with wallet hooks
- Multi-wallet testing scenarios
- Performance testing under load
- Mobile wallet testing
- Hardware wallet simulation
