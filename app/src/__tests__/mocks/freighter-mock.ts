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
  extensionId?: string;
  version?: string;
}

// Module-level mock state variables to bypass Vitest caching issues
let activeConfig: FreighterMockConfig | null = null;
let activeMockApi: any = null;
let isMockInstalled = false;

// Mock the @stellar/freighter-api module globally and statically
vi.mock("@stellar/freighter-api", () => {
  return {
    isConnected: vi.fn().mockImplementation(async () => {
      if (!isMockInstalled || !activeConfig) {
        return { isConnected: false };
      }
      return { isConnected: activeConfig.connected !== false };
    }),
    getAddress: vi.fn().mockImplementation(async () => {
      if (!isMockInstalled || !activeConfig) {
        return { error: "Freighter not installed" };
      }
      if (activeConfig.accessError) {
        return { error: activeConfig.accessError };
      }
      if (activeConfig.connected === false) {
        return { error: "Not connected" };
      }
      return { address: activeConfig.address };
    }),
    requestAccess: vi.fn().mockImplementation(async () => {
      if (!isMockInstalled || !activeConfig) {
        return { error: "Freighter not installed" };
      }
      if (activeConfig.accessError) {
        return { error: activeConfig.accessError };
      }
      if (activeConfig.connected === false) {
        return { error: "Not connected" };
      }
      return { address: activeConfig.address };
    }),
    signMessage: vi.fn().mockImplementation(async (message: string, opts?: any) => {
      if (!isMockInstalled || !activeConfig || !activeMockApi) {
        return { error: "Freighter not installed" };
      }
      try {
        const res = await activeMockApi.signMessage(message, opts);
        if (res && typeof res === "object" && "error" in res) {
          return { error: res.error };
        }
        return {
          signedMessage: res,
          signerAddress: activeConfig.address,
        };
      } catch (err) {
        throw err;
      }
    }),
    getNetwork: vi.fn().mockImplementation(async () => {
      if (!isMockInstalled || !activeConfig) {
        return { error: "Freighter not installed" };
      }
      return { networkPassphrase: activeConfig.networkPassphrase };
    }),
    getNetworkDetails: vi.fn().mockImplementation(async () => {
      if (!isMockInstalled || !activeConfig) {
        return { error: "Freighter not installed" };
      }
      return {
        networkPassphrase: activeConfig.networkPassphrase,
        networkUrl: "https://horizon-testnet.stellar.org",
        network: "TESTNET",
      };
    }),
    signTransaction: vi.fn().mockImplementation(async (txXdr: string, opts?: any) => {
      if (!isMockInstalled || !activeConfig) {
        return { error: "Freighter not installed" };
      }
      if (activeConfig.signError) {
        return { error: activeConfig.signError };
      }
      return {
        signedTxXdr: "signed_" + txXdr,
        signerAddress: activeConfig.address,
      };
    }),
  };
});

export class FreighterMock {
  private config: FreighterMockConfig;
  private mockAddress = "GABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFG";
  private mockSignature = "0x1234567890abcdef";
  private originalAddEventListener: any = null;
  private originalDispatchEvent: any = null;
  private mockApi: any;

  constructor(config: Partial<FreighterMockConfig> = {}) {
    this.config = {
      isInstalled: true,
      address: this.mockAddress,
      connected: true,
      signResponse: this.mockSignature,
      networkPassphrase: "Test SDF Network ; September 2015",
      extensionId: "bcacfldlkkdogcmkkibnjlakofdplcbk",
      version: "5.42.1",
      ...config,
    };

    // Construct legacy mockApi mapping to the configuration
    this.mockApi = {
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
        return Promise.resolve({ address: this.config.address });
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
      isConnected: vi.fn().mockResolvedValue({ isConnected: this.config.connected }),
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
  }

  install() {
    activeConfig = this.config;
    activeMockApi = this.mockApi;
    isMockInstalled = true;

    if (!this.config.isInstalled) {
      this.uninstall();
      return;
    }

    const win = (globalThis as any).window;
    if (win) {
      win.location = win.location || { origin: "http://localhost" };
      win.freighterApi = this.mockApi;
      win.stellar = win.stellar || {};
      win.stellar.freighterApi = this.mockApi;
      win.freighter = this.mockApi;

      this.originalAddEventListener = win.addEventListener;
      this.originalDispatchEvent = win.dispatchEvent;

      const listeners = new Set<any>();
      win.addEventListener = vi.fn().mockImplementation((type: string, cb: any) => {
        if (type === "message") {
          listeners.add(cb);
        }
      });
      win.dispatchEvent = vi.fn().mockImplementation((event: any) => {
        if (event.type === "message") {
          listeners.forEach(cb => cb(event));
        }
        return true;
      });

      win.chrome = win.chrome || {};
      win.chrome.runtime = win.chrome.runtime || {};
      win.chrome.runtime.sendMessage = vi.fn().mockImplementation(
        (extId: string, message: any, responseCallback?: (response: any) => void) => {
          if (extId === this.config.extensionId) {
            if (message?.type === "get-version" || message?.type === "GET_VERSION") {
              const response = { id: extId, version: this.config.version };
              if (responseCallback) {
                responseCallback(response);
              }
              return Promise.resolve(response);
            }
          }
          if (responseCallback) {
            responseCallback(undefined);
          }
          return Promise.resolve(undefined);
        }
      );
    }
  }

  uninstall() {
    activeConfig = null;
    activeMockApi = null;
    isMockInstalled = false;

    const win = (globalThis as any).window;
    if (win) {
      delete win.freighterApi;
      delete win.freighter;
      if (win.stellar) {
        delete win.stellar.freighterApi;
      }
      delete win.chrome;

      if (this.originalAddEventListener) {
        win.addEventListener = this.originalAddEventListener;
      } else {
        delete win.addEventListener;
      }
      if (this.originalDispatchEvent) {
        win.dispatchEvent = this.originalDispatchEvent;
      } else {
        delete win.dispatchEvent;
      }
    }
  }

  getMockApi() {
    return this.mockApi;
  }

  setConnected(connected: boolean) {
    this.config.connected = connected;
    if (this.mockApi?.isConnected) {
      this.mockApi.isConnected.mockResolvedValue({ isConnected: connected });
    }
  }

  setAddress(address: string) {
    this.config.address = address;
    if (this.mockApi?.getAddress) {
      this.mockApi.getAddress.mockResolvedValue({ address });
    }
    if (this.mockApi?.getPublicKey) {
      this.mockApi.getPublicKey.mockResolvedValue({ address });
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
