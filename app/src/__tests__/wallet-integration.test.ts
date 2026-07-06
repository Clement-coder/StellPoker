import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { FreighterMock } from "./mocks/freighter-mock";

describe("Wallet Integration Tests", () => {
  let freighterMock: FreighterMock;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
    freighterMock = new FreighterMock();
    freighterMock.install();
  });

  afterEach(() => {
    freighterMock.uninstall();
  });

  describe("Wallet Connection Flow", () => {
    it("successfully connects to Freighter wallet", async () => {
      const { connectFreighterWallet } = await import("../lib/freighter");
      
      const session = await connectFreighterWallet();
      
      expect(session.walletType).toBe("freighter");
      expect(session.address).toBe("GABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFG");
      expect(typeof session.signMessage).toBe("function");
    });

    it("handles connection failure when Freighter not installed", async () => {
      freighterMock.uninstall();
      
      const { connectFreighterWallet } = await import("../lib/freighter");
      
      await expect(connectFreighterWallet()).rejects.toThrow(/Freighter wallet not found/);
    });

    it("handles access request rejection", async () => {
      freighterMock = new FreighterMock({ accessError: "User denied access" });
      freighterMock.install();
      
      const { connectFreighterWallet } = await import("../lib/freighter");
      
      await expect(connectFreighterWallet()).rejects.toThrow("User denied access");
    });

    it("handles disconnected wallet state", async () => {
      freighterMock.setConnected(false);
      
      const { connectFreighterWallet } = await import("../lib/freighter");
      
      await expect(connectFreighterWallet()).rejects.toThrow(/Not connected/);
    });
  });

  describe("Signature Requests", () => {
    it("successfully signs message with string response", async () => {
      freighterMock = new FreighterMock({ signResponse: "signature_string" });
      freighterMock.install();
      
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      const signature = await session.signMessage("test message");
      expect(signature).toBe("signature_string");
    });

    it("successfully signs message with object response", async () => {
      freighterMock = new FreighterMock({
        signResponse: { signature: "object_signature" }
      });
      freighterMock.install();
      
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      const signature = await session.signMessage("test message");
      expect(signature).toBe("object_signature");
    });

    it("handles signature rejection by user", async () => {
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      freighterMock.simulateUserRejection();
      
      await expect(session.signMessage("test")).rejects.toThrow("User rejected request");
    });

    it("handles invalid signature response", async () => {
      freighterMock = new FreighterMock({ signResponse: {} });
      freighterMock.install();
      
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      await expect(session.signMessage("test")).rejects.toThrow(/invalid signature response/);
    });
  });

  describe("Transaction Approval/Rejection", () => {
    it("successfully signs transaction", async () => {
      const mockTxXdr = "AAAAAgAAAAA...";
      
      const api = freighterMock.getMockApi();
      const result = await api.signTransaction(mockTxXdr, {
        networkPassphrase: "Test SDF Network ; September 2015",
        address: "GABCDEFG...",
      });
      
      expect(result.signedTxXdr).toBe("signed_" + mockTxXdr);
      expect(result.signerAddress).toBe("GABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFG");
    });

    it("handles transaction rejection", async () => {
      freighterMock.setSignError("Transaction rejected by user");
      
      const api = freighterMock.getMockApi();
      const result = await api.signTransaction("AAAAAgAAAAA...", {
        networkPassphrase: "Test SDF Network ; September 2015",
        address: "GABCDEFG...",
      });
      
      expect(result.error).toBe("Transaction rejected by user");
    });
  });

  describe("Account Switching", () => {
    it("detects account changes", async () => {
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      expect(session.address).toBe("GABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFG");
      
      // Simulate account switch
      freighterMock.simulateAccountSwitch("GCDVWXYZ0123456789ABCDEFGHIJKLMNOPQRSTUVW");
      
      // Reconnect to get new address
      const newSession = await connectFreighterWallet();
      expect(newSession.address).toBe("GCDVWXYZ0123456789ABCDEFGHIJKLMNOPQRSTUVW");
    });

    it("handles disconnection during account switch", async () => {
      const { connectFreighterWallet } = await import("../lib/freighter");
      await connectFreighterWallet();
      
      freighterMock.simulateDisconnection();
      
      await expect(connectFreighterWallet()).rejects.toThrow(/Not connected/);
    });
  });

  describe("Network Detection", () => {
    it("detects testnet network", async () => {
      const api = freighterMock.getMockApi();
      const network = await api.getNetwork();
      
      expect(network.networkPassphrase).toBe("Test SDF Network ; September 2015");
    });

    it("detects mainnet network", async () => {
      freighterMock.setNetwork("Public Global Stellar Network ; September 2015");
      
      const api = freighterMock.getMockApi();
      const network = await api.getNetwork();
      
      expect(network.networkPassphrase).toBe("Public Global Stellar Network ; September 2015");
    });

    it("provides network details", async () => {
      const api = freighterMock.getMockApi();
      const details = await api.getNetworkDetails();
      
      expect(details.networkPassphrase).toBe("Test SDF Network ; September 2015");
      expect(details.networkUrl).toBe("https://horizon-testnet.stellar.org");
      expect(details.network).toBe("TESTNET");
    });
  });

  describe("Silent Reconnection", () => {
    it("successfully reconnects with stored credentials", async () => {
      // Mock localStorage with saved wallet type
      vi.spyOn(Storage.prototype, "getItem").mockReturnValue("freighter");
      
      // Ensure Freighter is installed and connected
      freighterMock.setConnected(true);
      
      const { trySilentReconnect } = await import("../lib/wallet");
      const session = await trySilentReconnect();
      
      expect(session).not.toBeNull();
      expect(session?.walletType).toBe("freighter");
    });

    it("returns null when no stored credentials", async () => {
      vi.spyOn(Storage.prototype, "getItem").mockReturnValue(null);
      
      const { trySilentReconnect } = await import("../lib/wallet");
      const session = await trySilentReconnect();
      
      expect(session).toBeNull();
    });

    it("handles reconnection failure gracefully", async () => {
      vi.spyOn(Storage.prototype, "getItem").mockReturnValue("freighter");
      freighterMock.setConnected(false);
      
      const { trySilentReconnect } = await import("../lib/wallet");
      const session = await trySilentReconnect();
      
      expect(session).toBeNull();
    });
  });

  describe("Error Handling", () => {
    it("handles wallet API errors gracefully", async () => {
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      // Simulate API error
      const api = freighterMock.getMockApi();
      api.signMessage.mockRejectedValue(new Error("Network error"));
      
      await expect(session.signMessage("test")).rejects.toThrow("Network error");
    });

    it("handles malformed responses", async () => {
      freighterMock = new FreighterMock({ 
        signResponse: { malformed: "response" } 
      });
      freighterMock.install();
      
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      await expect(session.signMessage("test")).rejects.toThrow(/invalid signature response/);
    });
  });

  describe("Integration with onchain module", () => {
    it("uses wallet session for transaction signing", async () => {
      // Mock the transaction building and signing flow
      const { connectFreighterWallet } = await import("../lib/freighter");
      const session = await connectFreighterWallet();
      
      expect(session.walletType).toBe("freighter");
      expect(typeof session.signMessage).toBe("function");
      
      // Verify the session can be used for signing
      const signature = await session.signMessage("transaction_data");
      expect(signature).toBeDefined();
    });
  });
});
