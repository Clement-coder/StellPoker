"use client";

import { useEffect, useState } from "react";
import { THEMES, loadTheme, saveTheme, applyTheme } from "@/lib/themes";

export function ThemeSelector() {
  const [currentThemeId, setCurrentThemeId] = useState<string>("classic-green");
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const theme = loadTheme();
    setCurrentThemeId(theme.id);
    applyTheme(theme);
  }, []);

  const handleSelect = (themeId: string) => {
    const theme = THEMES.find((t) => t.id === themeId);
    if (!theme) return;
    setCurrentThemeId(themeId);
    saveTheme(theme);
    applyTheme(theme);
  };

  const currentTheme = THEMES.find((t) => t.id === currentThemeId) ?? THEMES[0];

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((prev) => !prev)}
        className="pixel-btn text-[8px]"
        style={{
          padding: "4px 8px",
          background: "#2c3e50",
          color: "#c8e6ff",
          borderColor: "#8b6914",
        }}
        title="Table Theme"
      >
        THEME
      </button>

      {open && (
        <div
          className="absolute top-full right-0 mt-1 z-50 pixel-border-thin p-2"
          style={{
            background: "rgba(20, 12, 8, 0.96)",
            borderColor: "var(--ui-border)",
            minWidth: "160px",
          }}
        >
          <div className="flex flex-col gap-1">
            {THEMES.map((theme) => (
              <button
                key={theme.id}
                onClick={() => {
                  handleSelect(theme.id);
                  setOpen(false);
                }}
                className="text-[8px] text-left px-2 py-1.5 flex items-center gap-2"
                style={{
                  background: currentThemeId === theme.id ? "rgba(241,196,15,0.15)" : "transparent",
                  border: currentThemeId === theme.id ? "1px solid #f1c40f" : "1px solid transparent",
                  color: currentThemeId === theme.id ? "#f1c40f" : "#f5e6c8",
                  cursor: "pointer",
                }}
              >
                <span
                  className="inline-block w-3 h-3"
                  style={{
                    background: `radial-gradient(circle at 40% 40%, ${theme.feltLight}, ${theme.feltDark})`,
                    border: "1px solid rgba(255,255,255,0.2)",
                  }}
                />
                {theme.label}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}