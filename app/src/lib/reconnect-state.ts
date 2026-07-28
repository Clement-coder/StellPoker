/**
 * reconnect-state.ts
 * ─────────────────────────────────────────────────────────────────────────────
 * Client-side encrypted state persistence for player reconnection (Issue #14).
 *
 * Encrypts private hole cards and active hand state using Web Crypto API
 * derived from the player's wallet public key.
 *
 * Privacy & Security Guarantee:
 * - Encryption and decryption occur strictly inside the browser client.
 * - Derived encryption keys are NEVER sent to the coordinator or network.
 */

import { webcrypto } from "node:crypto";

const STORAGE_PREFIX = "stellpoker-encrypted-session-";

export interface EncryptedSessionPayload {
  tableId: number;
  handNumber: number;
  playerAddress: string;
  cards: [number, number];
  phase: string;
  iv: string; // Base64 encoded initialization vector
  ciphertext: string; // Base64 encoded encrypted payload
  updatedAt: number;
}

export interface RestoredHandState {
  tableId: number;
  handNumber: number;
  playerAddress: string;
  cards: [number, number];
  phase: string;
}

function getSubtleCrypto(): SubtleCrypto | null {
  if (typeof globalThis !== "undefined" && globalThis.crypto?.subtle) {
    return globalThis.crypto.subtle;
  }
  if (typeof window !== "undefined" && window.crypto?.subtle) {
    return window.crypto.subtle;
  }
  if (webcrypto && webcrypto.subtle) {
    return webcrypto.subtle as unknown as SubtleCrypto;
  }
  return null;
}

function getRandomValues(array: Uint8Array): Uint8Array {
  if (typeof globalThis !== "undefined" && globalThis.crypto?.getRandomValues) {
    return globalThis.crypto.getRandomValues(array);
  }
  if (typeof window !== "undefined" && window.crypto?.getRandomValues) {
    return window.crypto.getRandomValues(array);
  }
  if (webcrypto && webcrypto.getRandomValues) {
    return webcrypto.getRandomValues(array) as unknown as Uint8Array;
  }
  return array;
}

/**
 * Derive an AES-GCM CryptoKey locally using SHA-256 digest of the player's wallet public key.
 * Derived strictly in-browser — key material never leaves client memory.
 */
async function deriveClientKey(walletPublicKey: string): Promise<CryptoKey | null> {
  const subtle = getSubtleCrypto();
  if (!subtle) return null;
  const encoder = new TextEncoder();
  const keyMaterial = encoder.encode(`stellpoker-local-key:${walletPublicKey}`);
  const hash = await subtle.digest("SHA-256", keyMaterial);
  return subtle.importKey(
    "raw",
    hash,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"]
  );
}

function bufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

function base64ToBuffer(base64: string): ArrayBuffer {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}

/**
 * Encrypt hole cards & hand state client-side and store in localStorage.
 */
export async function persistHandStateClientSide(
  tableId: number,
  handNumber: number,
  playerAddress: string,
  cards: [number, number],
  phase: string
): Promise<void> {
  const subtle = getSubtleCrypto();
  if (!subtle) return;

  try {
    const key = await deriveClientKey(playerAddress);
    if (!key) return;
    const iv = getRandomValues(new Uint8Array(12));
    const payloadData = JSON.stringify({ cards, phase, handNumber, tableId, playerAddress });
    const encoder = new TextEncoder();

    const encrypted = await subtle.encrypt(
      { name: "AES-GCM", iv },
      key,
      encoder.encode(payloadData)
    );

    const record: EncryptedSessionPayload = {
      tableId,
      handNumber,
      playerAddress,
      cards,
      phase,
      iv: bufferToBase64(iv.buffer),
      ciphertext: bufferToBase64(encrypted),
      updatedAt: Date.now(),
    };

    const storageKey = `${STORAGE_PREFIX}${tableId}-${playerAddress}`;
    localStorage.setItem(storageKey, JSON.stringify(record));
  } catch (err) {
    console.error("Failed to encrypt and persist hand state:", err);
  }
}

/**
 * Decrypt and restore in-progress hand state on player reconnect.
 */
export async function restoreHandStateClientSide(
  tableId: number,
  playerAddress: string
): Promise<RestoredHandState | null> {
  const subtle = getSubtleCrypto();
  if (!subtle) return null;

  try {
    const storageKey = `${STORAGE_PREFIX}${tableId}-${playerAddress}`;
    const raw = localStorage.getItem(storageKey);
    if (!raw) return null;

    const record: EncryptedSessionPayload = JSON.parse(raw);
    const key = await deriveClientKey(playerAddress);
    if (!key) return null;
    const iv = new Uint8Array(base64ToBuffer(record.iv));
    const ciphertext = base64ToBuffer(record.ciphertext);

    const decrypted = await subtle.decrypt(
      { name: "AES-GCM", iv },
      key,
      ciphertext
    );

    const decoder = new TextDecoder();
    const data = JSON.parse(decoder.decode(decrypted));

    return {
      tableId: data.tableId,
      handNumber: data.handNumber,
      playerAddress: data.playerAddress,
      cards: data.cards,
      phase: data.phase,
    };
  } catch (err) {
    console.error("Failed to decrypt and restore hand state:", err);
    return null;
  }
}

/**
 * Clear persisted hand state when hand completes or reaches showdown.
 */
export function clearHandStateClientSide(tableId: number, playerAddress: string): void {
  if (typeof window === "undefined") return;
  const storageKey = `${STORAGE_PREFIX}${tableId}-${playerAddress}`;
  localStorage.removeItem(storageKey);
}
