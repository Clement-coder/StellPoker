import {
  getAddress as freighterGetAddress,
  isConnected as freighterIsConnected,
  requestAccess as freighterRequestAccess,
  signMessage as freighterSignMessage,
} from "@stellar/freighter-api";

export interface WalletSession {
  address: string;
  walletType: "freighter" | "lobstr";
  signMessage: (message: string) => Promise<string>;
}

type FreighterAddressResponse =
  | string
  | {
      address?: string;
      publicKey?: string;
      error?: string;
    };

type FreighterSignResponse =
  | string
  | {
      signature?: string;
      signedMessage?: string;
      signed_message?: string;
      error?: string;
    };

type FreighterApi = {
  requestAccess?: () => Promise<unknown>;
  setAllowed?: () => Promise<unknown>;
  getAddress?: () => Promise<FreighterAddressResponse>;
  getPublicKey?: () => Promise<FreighterAddressResponse>;
  signMessage?: (
    message: string,
    opts?: { address?: string }
  ) => Promise<FreighterSignResponse>;
};

declare global {
  interface Window {
    freighter?: unknown;
    freighterApi?: FreighterApi;
    stellar?: {
      freighterApi?: FreighterApi;
    };
  }
}

function errorMessage(raw: unknown, fallback: string): string {
  if (typeof raw === "string" && raw.trim()) {
    return raw;
  }
  if (
    typeof raw === "object" &&
    raw !== null &&
    "message" in raw &&
    typeof (raw as { message?: unknown }).message === "string"
  ) {
    return (raw as { message: string }).message;
  }
  return fallback;
}

function parseAddress(result: FreighterAddressResponse): string {
  if (typeof result === "string" && result.length > 0) {
    return result;
  }
  if (typeof result === "object" && result !== null) {
    if (result.error) {
      throw new Error(errorMessage(result.error, "Freighter rejected address request"));
    }
    if (typeof result.address === "string" && result.address.length > 0) {
      return result.address;
    }
    if (typeof result.publicKey === "string" && result.publicKey.length > 0) {
      return result.publicKey;
    }
  }
  throw new Error("Freighter returned an invalid address response");
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    const chunk = bytes.subarray(i, i + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return btoa(binary);
}

function parseSignedPayload(payload: unknown): string {
  if (typeof payload === "string" && payload.length > 0) {
    return payload;
  }
  if (ArrayBuffer.isView(payload)) {
    const bytes = new Uint8Array(payload.buffer, payload.byteOffset, payload.byteLength);
    return bytesToBase64(bytes);
  }
  if (payload instanceof ArrayBuffer) {
    return bytesToBase64(new Uint8Array(payload));
  }
  if (typeof payload === "object" && payload !== null) {
    if ("signature" in payload && typeof (payload as { signature: unknown }).signature === "string") {
      return (payload as { signature: string }).signature;
    }
    if ("signedMessage" in payload && typeof (payload as { signedMessage: unknown }).signedMessage === "string") {
      return (payload as { signedMessage: string }).signedMessage;
    }
    if ("signed_message" in payload && typeof (payload as { signed_message: unknown }).signed_message === "string") {
      return (payload as { signed_message: string }).signed_message;
    }
    if ("data" in payload && Array.isArray((payload as { data?: unknown }).data)) {
      return bytesToBase64(Uint8Array.from((payload as { data: number[] }).data));
    }
  }
  throw new Error("Freighter returned an invalid signature response");
}

function parseSignature(result: FreighterSignResponse): string {
  if (typeof result === "string" && result.length > 0) {
    return result;
  }
  if (typeof result === "object" && result !== null) {
    if (result.error) {
      throw new Error(errorMessage(result.error, "Freighter rejected sign request"));
    }
    if (typeof result.signature === "string" && result.signature.length > 0) {
      return result.signature;
    }
    if (typeof result.signedMessage === "string" && result.signedMessage.length > 0) {
      return result.signedMessage;
    }
    if (typeof result.signed_message === "string" && result.signed_message.length > 0) {
      return result.signed_message;
    }
  }
  throw new Error("Freighter returned an invalid signature response");
}

function parseModernSignature(
  result:
    | {
        signedMessage: unknown;
        signerAddress: string;
        error?: unknown;
      }
    | {
        signedMessage: string | null;
        signerAddress: string;
        error?: unknown;
      }
): string {
  if (result.error) {
    throw new Error(errorMessage(result.error, "Freighter rejected sign request"));
  }
  return parseSignedPayload(result.signedMessage);
}

function getLegacyApiCandidate(): FreighterApi | null {
  if (typeof window === "undefined") {
    return null;
  }

  const candidates: unknown[] = [
    window.freighterApi,
    window.stellar?.freighterApi,
    typeof window.freighter === "object" ? window.freighter : null,
  ];

  for (const candidate of candidates) {
    if (!candidate || typeof candidate !== "object") {
      continue;
    }
    const api = candidate as FreighterApi;
    if (
      typeof api.requestAccess === "function" ||
      typeof api.setAllowed === "function" ||
      typeof api.getAddress === "function" ||
      typeof api.getPublicKey === "function" ||
      typeof api.signMessage === "function"
    ) {
      return api;
    }
  }
  return null;
}

async function waitForLegacyApi(timeoutMs = 3000): Promise<FreighterApi | null> {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    const api = getLegacyApiCandidate();
    if (api) {
      return api;
    }
    await new Promise((resolve) => setTimeout(resolve, 120));
  }
  return getLegacyApiCandidate();
}

async function connectViaOfficialApi(): Promise<WalletSession | null> {
  if (typeof window === "undefined") {
    return null;
  }

  const connected = await freighterIsConnected();
  if (connected && connected.error) {
    throw new Error(errorMessage(connected.error, "Failed to query Freighter connection state"));
  }
  if (!connected || !connected.isConnected) {
    return null;
  }

  const access = await freighterRequestAccess();
  if (access && access.error) {
    throw new Error(errorMessage(access.error, "Freighter access was denied"));
  }

  let address = access?.address;
  if (!address) {
    const current = await freighterGetAddress();
    if (current && current.error) {
      throw new Error(errorMessage(current.error, "Failed to read Freighter address"));
    }
    address = current?.address;
  }

  if (!address) {
    throw new Error("Freighter did not return an address");
  }

  return {
    address,
    walletType: "freighter" as const,
    signMessage: async (message: string) => {
      const result = await freighterSignMessage(message, { address });
      return parseModernSignature(result);
    },
  };
}

async function connectViaLegacyApi(): Promise<WalletSession | null> {
  const api = await waitForLegacyApi();
  if (!api) {
    return null;
  }

  if (api.requestAccess) {
    await api.requestAccess();
  } else if (api.setAllowed) {
    await api.setAllowed();
  }

  const getAddress = api.getAddress ?? api.getPublicKey;
  if (!getAddress) {
    throw new Error("Freighter getAddress API is unavailable");
  }
  const address = parseAddress(await getAddress.call(api));

  if (!api.signMessage) {
    throw new Error("Freighter signMessage API is unavailable");
  }

  return {
    address,
    walletType: "freighter" as const,
    signMessage: async (message: string): Promise<string> => {
      const sig = await api.signMessage!(message, { address });
      return parseSignature(sig);
    },
  };
}

// ── localStorage wallet persistence ──

const WALLET_KEY = "stellar_poker_wallet";

function saveWalletAddress(address: string): void {
  try {
    localStorage.setItem(WALLET_KEY, address);
  } catch {
    // Storage full or unavailable — ignore.
  }
}

export function getSavedWalletAddress(): string | null {
  try {
    return localStorage.getItem(WALLET_KEY);
  } catch {
    return null;
  }
}

export function clearSavedWallet(): void {
  try {
    localStorage.removeItem(WALLET_KEY);
  } catch {
    // ignore
  }
}

export function isFreighterInstalled(): boolean {
  if (typeof window === "undefined") return false;

  const legacyApi = getLegacyApiCandidate();
  return legacyApi !== null;
}

const ALLOWED_EXTENSION_IDS = [
  "bcacfldlkkdogcmkkibnjlakofdplcbk", // Chrome Production
  "freighter@stellar.org" // Firefox Production
];

const MIN_VERSION = "5.0.0";

function isVersionAllowed(version: string): boolean {
  try {
    const parts = version.split(".").map(Number);
    const minParts = MIN_VERSION.split(".").map(Number);
    for (let i = 0; i < 3; i++) {
      const v = parts[i] || 0;
      const m = minParts[i] || 0;
      if (v > m) return true;
      if (v < m) return false;
    }
    return true;
  } catch {
    return false;
  }
}

export async function verifyFreighterExtensionIntegrity(): Promise<void> {
  if (typeof window === "undefined") return;

  // 1. Verify Extension ID and Version via chrome.runtime.sendMessage
  const chrome = (window as any).chrome;
  if (chrome && chrome.runtime && typeof chrome.runtime.sendMessage === "function") {
    let verified = false;
    let versionValid = false;

    for (const extId of ALLOWED_EXTENSION_IDS) {
      try {
        const response = await new Promise<any>((resolve) => {
          const timeout = setTimeout(() => resolve(undefined), 800);
          try {
            chrome.runtime.sendMessage(extId, { type: "get-version" }, (res: any) => {
              clearTimeout(timeout);
              resolve(res);
            });
          } catch {
            clearTimeout(timeout);
            resolve(undefined);
          }
        });

        if (response && (response.version || response.id)) {
          verified = true;
          const version = response.version || "";
          if (isVersionAllowed(version)) {
            versionValid = true;
          }
          break;
        }
      } catch {
        // Skip to next ID
      }
    }

    if (!verified) {
      throw new Error(
        "Security Alert: Unrecognized or altered Freighter wallet extension ID detected. Please install the official extension from freighter.app."
      );
    }
    if (!versionValid) {
      throw new Error(
        `Security Alert: Outdated Freighter wallet extension detected. Minimum required version is ${MIN_VERSION}.`
      );
    }
  }

  // 2. Message Origin verification
  if (typeof window !== "undefined" && typeof window.addEventListener === "function") {
    window.addEventListener("message", (event) => {
      if (
        event.data &&
        typeof event.data === "object" &&
        (event.data.source === "freighter" ||
          event.data.type?.includes("freighter") ||
          event.data.freighter)
      ) {
      const origin = event.origin;
      const allowedOrigins = [
        window.location.origin,
        "chrome-extension://bcacfldlkkdogcmkkibnjlakofdplcbk"
      ];
      const isAllowed = allowedOrigins.some(
        (ao) => origin === ao || origin.startsWith(ao)
      );
      if (!isAllowed) {
        console.warn("Security Alert: Blocked message from untrusted origin", origin);
        throw new Error("Security Alert: Blocked message from untrusted origin: " + origin);
      }
      }
    });
  }
}

export async function connectFreighterWallet(): Promise<WalletSession> {
  await verifyFreighterExtensionIntegrity();
  try {
    const modern = await connectViaOfficialApi();
    if (modern) {
      saveWalletAddress(modern.address);
      return modern;
    }
  } catch (err) {
    const legacy = await connectViaLegacyApi();
    if (legacy) {
      saveWalletAddress(legacy.address);
      return legacy;
    }
    throw err;
  }

  const legacy = await connectViaLegacyApi();
  if (legacy) {
    saveWalletAddress(legacy.address);
    return legacy;
  }

  throw new Error(
    "Freighter wallet not found. Open Freighter, unlock it, and allow this site."
  );
}

/**
 * Silently reconnect if localStorage has a saved wallet address and
 * Freighter is already approved (no popup). Uses getAddress instead of
 * requestAccess to avoid triggering a Freighter approval popup.
 */
export async function trySilentReconnect(): Promise<WalletSession | null> {
  const saved = getSavedWalletAddress();
  if (!saved) return null;

  await verifyFreighterExtensionIntegrity();
  try {
    const connected = await freighterIsConnected();
    if (!connected || !connected.isConnected) {
      clearSavedWallet();
      return null;
    }

    const current = await freighterGetAddress();
    const address = parseAddress(current);

    return {
      address,
      walletType: "freighter" as const,
      signMessage: async (message: string) => {
        const result = await freighterSignMessage(message, { address });
        return parseModernSignature(result);
      },
    };
  } catch {
    clearSavedWallet();
    return null;
  }
}

export async function getActiveAddress(): Promise<string | null> {
  try {
    const connected = await freighterIsConnected();
    if (!connected || !connected.isConnected) return null;
    const current = await freighterGetAddress();
    if (!current || current.error || !current.address) return null;
    return current.address;
  } catch {
    return null;
  }
}
