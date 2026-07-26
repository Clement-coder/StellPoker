export const THEME_STORAGE_KEY = "stellpoker-theme";

export interface TableTheme {
  id: string;
  label: string;
  feltDark: string;
  feltMid: string;
  feltLight: string;
  feltShadow: string;
  uiBorder: string;
  cardBackStart: string;
  cardBackEnd: string;
  cardBackBorder: string;
  cardBackSymbol: string;
}

export const THEMES: TableTheme[] = [
  {
    id: "classic-green",
    label: "Classic Green",
    feltDark: "#1a5c2a",
    feltMid: "#237a3a",
    feltLight: "#2d9648",
    feltShadow: "#0f3b1a",
    uiBorder: "#8b6914",
    cardBackStart: "#1a3a5c",
    cardBackEnd: "#0d2137",
    cardBackBorder: "#2a5a8c",
    cardBackSymbol: "#3498db",
  },
  {
    id: "blue",
    label: "Blue",
    feltDark: "#1a2a5c",
    feltMid: "#234a7a",
    feltLight: "#2d6a96",
    feltShadow: "#0f1a3b",
    uiBorder: "#8b6914",
    cardBackStart: "#1a3a3c",
    cardBackEnd: "#0d2127",
    cardBackBorder: "#2a5a6c",
    cardBackSymbol: "#3498db",
  },
  {
    id: "red",
    label: "Red",
    feltDark: "#5c1a1a",
    feltMid: "#7a2323",
    feltLight: "#962d2d",
    feltShadow: "#3b0f0f",
    uiBorder: "#b8860b",
    cardBackStart: "#3c1a2a",
    cardBackEnd: "#270d17",
    cardBackBorder: "#6c2a4a",
    cardBackSymbol: "#db3498",
  },
  {
    id: "dark",
    label: "Dark Mode",
    feltDark: "#0d0d0d",
    feltMid: "#1a1a1a",
    feltLight: "#2a2a2a",
    feltShadow: "#000000",
    uiBorder: "#555555",
    cardBackStart: "#1a1a2e",
    cardBackEnd: "#0a0a15",
    cardBackBorder: "#2a2a5e",
    cardBackSymbol: "#5b5bdb",
  },
  {
    id: "amber",
    label: "Amber",
    feltDark: "#3a2a0a",
    feltMid: "#5a4020",
    feltLight: "#7a5a30",
    feltShadow: "#2a1a05",
    uiBorder: "#c8a030",
    cardBackStart: "#2e1a1a",
    cardBackEnd: "#150d0d",
    cardBackBorder: "#5e2a2a",
    cardBackSymbol: "#db9b34",
  },
];

export function loadTheme(): TableTheme {
  if (typeof window === "undefined") return THEMES[0];
  try {
    const saved = localStorage.getItem(THEME_STORAGE_KEY);
    if (saved) {
      const parsed = JSON.parse(saved);
      const match = THEMES.find((t) => t.id === parsed.id || t.id === parsed);
      if (match) return match;
    }
  } catch { /* ignore */ }
  return THEMES[0];
}

export function saveTheme(theme: TableTheme): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, JSON.stringify(theme.id));
  } catch { /* ignore */ }
}

export function applyTheme(theme: TableTheme): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.style.setProperty("--felt-dark", theme.feltDark);
  root.style.setProperty("--felt-mid", theme.feltMid);
  root.style.setProperty("--felt-light", theme.feltLight);
  root.style.setProperty("--felt-shadow", theme.feltShadow);
  root.style.setProperty("--ui-border", theme.uiBorder);
  root.style.setProperty("--card-back-start", theme.cardBackStart);
  root.style.setProperty("--card-back-end", theme.cardBackEnd);
  root.style.setProperty("--card-back-border", theme.cardBackBorder);
  root.style.setProperty("--card-back-symbol", theme.cardBackSymbol);
}