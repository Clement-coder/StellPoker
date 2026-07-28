"use client";

import { useEffect, useState } from "react";

const THEME_KEY = "stellpoker-ui-theme";

export function ThemeToggle() {
  const [theme, setTheme] = useState<string>(() => {
    if (typeof window === "undefined") return "system";
    return localStorage.getItem(THEME_KEY) || "system";
  });

  useEffect(() => {
    const apply = (t: string) => {
      const root = document.documentElement;
      if (t === "dark" || (t === "system" && window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches)) {
        root.setAttribute("data-theme", "dark");
      } else {
        root.setAttribute("data-theme", "light");
      }
    };
    apply(theme);
    try { localStorage.setItem(THEME_KEY, theme); } catch {}
  }, [theme]);

  const toggle = () => {
    setTheme((prev) => (prev === "dark" ? "light" : prev === "light" ? "system" : "dark"));
  };

  return (
    <button
      onClick={toggle}
      title="Toggle theme"
      className="pixel-btn pixel-btn-dark text-[8px]"
      style={{ padding: "6px 10px" }}
    >
      {theme === "system" ? "SYS" : theme === "dark" ? "DARK" : "LIGHT"}
    </button>
  );
}
