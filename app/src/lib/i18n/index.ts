/**
 * Lightweight i18n (Issue #60).
 * English + Spanish dictionaries, browser language auto-detect, manual override.
 * Avoids next-intl dependency to keep the frontend dep-light.
 */

import en from "./messages/en.json";
import es from "./messages/es.json";

export type Locale = "en" | "es";

export const LOCALES: Locale[] = ["en", "es"];
export const DEFAULT_LOCALE: Locale = "en";
const STORAGE_KEY = "stellpoker.locale";

type Dict = typeof en;
const dictionaries: Record<Locale, Dict> = { en, es };

type NestedKeyOf<T, Prefix extends string = ""> = T extends object
  ? {
      [K in keyof T & string]: T[K] extends object
        ? NestedKeyOf<T[K], `${Prefix}${K}.`>
        : `${Prefix}${K}`;
    }[keyof T & string]
  : never;

export type MessageKey = NestedKeyOf<Dict>;

function getByPath(obj: unknown, path: string): string | undefined {
  const parts = path.split(".");
  let cur: unknown = obj;
  for (const p of parts) {
    if (cur == null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[p];
  }
  return typeof cur === "string" ? cur : undefined;
}

/** Resolve a dotted key with optional `{var}` interpolation. */
export function translate(
  locale: Locale,
  key: MessageKey | string,
  vars?: Record<string, string | number>
): string {
  const dict = dictionaries[locale] ?? dictionaries.en;
  let text =
    getByPath(dict, key) ??
    getByPath(dictionaries.en, key) ??
    key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      text = text.replaceAll(`{${k}}`, String(v));
    }
  }
  return text;
}

/** Detect browser language; falls back to English. */
export function detectBrowserLocale(): Locale {
  if (typeof navigator === "undefined") return DEFAULT_LOCALE;
  const candidates = [
    navigator.language,
    ...(navigator.languages ?? []),
  ]
    .filter(Boolean)
    .map((l) => l.toLowerCase().slice(0, 2));
  for (const c of candidates) {
    if (c === "es") return "es";
    if (c === "en") return "en";
  }
  return DEFAULT_LOCALE;
}

export function loadStoredLocale(): Locale | null {
  if (typeof window === "undefined") return null;
  try {
    const v = window.localStorage.getItem(STORAGE_KEY);
    if (v === "en" || v === "es") return v;
  } catch {
    /* ignore */
  }
  return null;
}

export function storeLocale(locale: Locale): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    /* ignore */
  }
}

/** Resolved locale: manual override > stored > browser > default. */
export function resolveLocale(override?: Locale | null): Locale {
  if (override === "en" || override === "es") return override;
  return loadStoredLocale() ?? detectBrowserLocale();
}

export { en, es };
