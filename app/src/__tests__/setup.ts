import { vi } from "vitest";

// Mock localStorage
vi.stubGlobal("localStorage", {
  getItem: vi.fn(),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
  length: 0,
  key: vi.fn(),
});

// Mock window for server-side rendering checks
Object.defineProperty(globalThis, "window", {
  value: Object.create(globalThis.window || {}),
  writable: true,
  configurable: true,
});
