"use client";

import { useI18n } from "@/lib/i18n/context";
import type { Locale } from "@/lib/i18n";

const OPTIONS: { value: Locale; labelKey: string }[] = [
  { value: "en", labelKey: "settings.english" },
  { value: "es", labelKey: "settings.spanish" },
];

interface LanguageSelectorProps {
  /** Compact pixel style for header; default for settings panel. */
  variant?: "header" | "settings";
}

export function LanguageSelector({ variant = "header" }: LanguageSelectorProps) {
  const { locale, setLocale, t } = useI18n();

  if (variant === "header") {
    return (
      <select
        value={locale}
        onChange={(e) => setLocale(e.target.value as Locale)}
        aria-label={t("settings.language")}
        className="text-[8px]"
        style={{
          background: "rgba(12,10,24,0.8)",
          color: "#c8e6ff",
          border: "1px solid #8b6914",
          padding: "2px 4px",
          fontFamily: "'Press Start 2P', monospace",
          cursor: "pointer",
        }}
      >
        {OPTIONS.map((o) => (
          <option key={o.value} value={o.value}>
            {o.value.toUpperCase()}
          </option>
        ))}
      </select>
    );
  }

  return (
    <div>
      <div
        style={{
          fontSize: "7px",
          fontFamily: "'Press Start 2P', monospace",
          color: "#3a4438",
          marginBottom: "5px",
        }}
      >
        {t("settings.language")}
      </div>
      <div style={{ display: "flex", gap: "4px" }}>
        {OPTIONS.map((o) => (
          <button
            key={o.value}
            type="button"
            onClick={() => setLocale(o.value)}
            style={{
              flex: 1,
              background: locale === o.value ? "#3a4438" : "#6b7a60",
              color: locale === o.value ? "#b8c4a0" : "#a0b090",
              border: "2px solid #3a4438",
              padding: "4px 2px",
              fontSize: "6px",
              fontFamily: "'Press Start 2P', monospace",
              cursor: "pointer",
            }}
          >
            {t(o.labelKey)}
          </button>
        ))}
      </div>
    </div>
  );
}
