import { vi } from "vitest";

export const mockStellarAddress = "GABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFG";
export const mockSignature = "0x1234567890abcdef";

/**
 * Creates a mock wallet session for testing
 */
export function createMockWalletSession(overrides: Partial<{
  address: string;
  walletType: "freighter" | "lobstr";
  signMessage: (message: string) => Promise<string>;
}> = {}) {
  return {
    address: mockStellarAddress,
    walletType: "freighter" as const,
    signMessage: vi.fn().mockResolvedValue(mockSignature),
    ...overrides,
  };
}

/**
 * Creates mock chain config for testing
 */
export function createMockChainConfig(overrides: Partial<{
  rpc_url: string;
  network_passphrase: string;
  poker_table_contract: string;
}> = {}) {
  return {
    rpc_url: "https://soroban-testnet.stellar.org",
    network_passphrase: "Test SDF Network ; September 2015", 
    poker_table_contract: "CCONTRACT123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    ...overrides,
  };
}

/**
 * Creates mock simulation result for testing
 */
export function createMockSimulationResult(overrides: Partial<{
  success: boolean;
  fee: string;
  gasUsed: number;
  error?: string;
}> = {}) {
  return {
    success: true,
    fee: "0.1",
    gasUsed: 50000,
    stateChanges: [
      {
        type: "balance" as const,
        description: "Transaction fee: 0.1 XLM",
      },
      {
        type: "contract_data" as const,
        description: "Contract state will be updated",
      },
      {
        type: "account" as const,
        description: "Account sequence number will increase by 1",
      },
    ],
    ...overrides,
  };
}

/**
 * Mocks localStorage for testing
 */
export function mockLocalStorage() {
  const store: Record<string, string> = {};
  
  return {
    getItem: vi.fn().mockImplementation((key: string) => store[key] || null),
    setItem: vi.fn().mockImplementation((key: string, value: string) => {
      store[key] = value;
    }),
    removeItem: vi.fn().mockImplementation((key: string) => {
      delete store[key];
    }),
    clear: vi.fn().mockImplementation(() => {
      Object.keys(store).forEach(key => delete store[key]);
    }),
    length: 0,
    key: vi.fn(),
  };
}

/**
 * Waits for async operations to complete in tests
 */
export function waitForAsync(ms = 0) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Creates mock Soroban server responses
 */
export function createMockSorobanServer(overrides: Partial<{
  simulateSuccess: boolean;
  simulationError?: string;
  minResourceFee: string;
  gasUsed: string;
}> = {}) {
  const config = {
    simulateSuccess: true,
    minResourceFee: "1000000", // 0.1 XLM
    gasUsed: "50000",
    ...overrides,
  };

  return {
    getAccount: vi.fn().mockResolvedValue({
      accountId: mockStellarAddress,
      sequenceNumber: "123456789",
    }),
    simulateTransaction: vi.fn().mockResolvedValue(
      config.simulateSuccess
        ? {
            minResourceFee: config.minResourceFee,
            cost: { cpuInsns: config.gasUsed },
            results: [{ xdr: "base64_result" }],
          }
        : {
            error: config.simulationError || "Simulation failed",
          }
    ),
    prepareTransaction: vi.fn().mockImplementation((tx) => tx),
    sendTransaction: vi.fn().mockResolvedValue({
      status: "SUCCESS",
      hash: "abcd1234",
    }),
    pollTransaction: vi.fn().mockResolvedValue({
      status: "SUCCESS",
    }),
  };
}
