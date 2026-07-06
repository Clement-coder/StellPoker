import { vi } from "vitest";

export interface FreighterMockConfig {
  isInstalled: boolean;
  address?: string;
  connected?: boolean;
  signResponse?: string | object;
  signError?: string;
  accessError?: string;
  networkPassphrase?: string;
  accountSwitchCallback?: (address: string) => void;
}

export class FreighterMock {
  private config: FreighterMockConfig;
  private mockAddress = "GABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFG";
  private mockSignature = "0x1234567890abcdef";

  constructor(config: Partial<FreighterMockConfig> = {}) {
    this.config = {
      isInstalled: true,
      address: this.mockAddress,
      connected: true,
      signResponse: this.mockSignature,
      networkPassphrase: "Test SDF Network ; September 2015",
      ...config,
    };
  }

  install() {
    if (!this.config.isInstalled) {
      this.uninstall();
      return;
    }

    const mockApi = {
      requestAccess: vi.fn().mockImplementation(() => {
        if (this.config.accessError) {
          return Promise.reject(new Error(this.config.accessError));
        }
        return Promise.resolve();
      }),

      getAddress: vi.fn().mockImplementation(() => {
        if (!this.config.connected) {
          return Promise.resolve({ error: "Not connected" });
        }
        return Promise.resolve(this.config.address);
      }),

      getPublicKey: vi.fn().mockImplementation(() => {
        return this.mockApi.getAddress();
      }),

      signMessage: vi.fn().mockImplementation((message: string, opts?: { address?: string }) => {
        if (this.config.signError) {
          return Promise.resolve({ error: this.config.signError });
        }
        if (!this.config.connected) {
          return Promise.resolve({ error: "Not connected" });
        }
        return Promise.resolve(this.config.signResponse);
      }),

      getNetwork: vi.fn().mockResolvedValue({
        networkPassphrase: this.config.networkPassphrase,
      }),

      getNetworkDetails: vi.fn().mockResolvedValue({
        networkPassphrase: this.config.networkPassphrase,
        networkUrl: "https://horizon-testnet.stellar.org",
        network: "TESTNET",
      }),

      isConnected: vi.fn().mockResolvedValue(this.config.connected),

      signTransaction: vi.fn().mockImplementation((txXdr: string, opts?: any) => {
        if (this.config.signError) {
          return Promise.resolve({ error: this.config.signError });
        }
        return Promise.resolve({ 
          signedTxXdr: "signed_" + txXdr,
          signerAddress: this.config.address 
        });
      }),
    };

    // Store reference for access to mock methods
    this.mockApi = mockApi;

    // Install on window in all the ways Freighter might be detected
    (globalThis as any).window = (globalThis as any).window || {};
    const win = (globalThis as any).window;
    
    win.freighterApi = mockApi;
    win.stellar = win.stellar || {};
    win.stellar.freighterApi = mockApi;
    win.freighter = mockApi;

    // Mock the @stellar/freighter-api module
    vi.doMock("@stellar/freighter-api", () => ({
      getAddress: mockApi.getAddress,
      requestAccess: mockApi.requestAccess,
      isConnected: mockApi.isConnected,
      signMessage: mockApi.signMessage,
      getNetwork: mockApi.getNetwork,
      getNetworkDetails: mockApi.getNetworkDetails,
      signTransaction: vi.fn().mockImplementation((txXdr: string, opts?: any) => {
        if (this.config.signError) {
          return Promise.resolve({ error: this.config.signError });
        }
        return Promise.resolve({ 
          signedTxXdr: "signed_" + txXdr,
          signerAddress: this.config.address 
        });
      }),
    }));
  }

  uninstall() {
    const win = (globalThis as any).window;
    if (win) {
      delete win.freighterApi;
      delete win.freighter;
      if (win.stellar) {
        delete win.stellar.freighterApi;
      }
    }
    vi.doUnmock("@stellar/freighter-api");
  }

  // Test utilities
  private mockApi: any;

  getMockApi() {
    return this.mockApi;
  }

  setConnected(connected: boolean) {
    this.config.connected = connected;
    if (this.mockApi?.isConnected) {
      this.mockApi.isConnected.mockResolvedValue(connected);
    }
  }

  setAddress(address: string) {
    this.config.address = address;
    if (this.mockApi?.getAddress) {
      this.mockApi.getAddress.mockResolvedValue(address);
    }
    if (this.mockApi?.getPublicKey) {
      this.mockApi.getPublicKey.mockResolvedValue(address);
    }
  }

  setSignError(error?: string) {
    this.config.signError = error;
    if (this.mockApi?.signMessage) {
      if (error) {
        this.mockApi.signMessage.mockResolvedValue({ error });
      } else {
        this.mockApi.signMessage.mockResolvedValue(this.config.signResponse);
      }
    }
  }

  setNetwork(networkPassphrase: string) {
    this.config.networkPassphrase = networkPassphrase;
    if (this.mockApi?.getNetwork) {
      this.mockApi.getNetwork.mockResolvedValue({ networkPassphrase });
    }
    if (this.mockApi?.getNetworkDetails) {
      this.mockApi.getNetworkDetails.mockResolvedValue({
        networkPassphrase,
        networkUrl: "https://horizon-testnet.stellar.org",
        network: "TESTNET",
      });
    }
  }

  simulateAccountSwitch(newAddress: string) {
    this.setAddress(newAddress);
    // Trigger any registered callback
    if (this.config.accountSwitchCallback) {
      this.config.accountSwitchCallback(newAddress);
    }
  }

  simulateUserRejection() {
    this.setSignError("User rejected request");
  }

  simulateDisconnection() {
    this.setConnected(false);
  }

  reset() {
    this.config = {
      isInstalled: true,
      address: this.mockAddress,
      connected: true,
      signResponse: this.mockSignature,
      networkPassphrase: "Test SDF Network ; September 2015",
    };
    this.install();
  }
}
