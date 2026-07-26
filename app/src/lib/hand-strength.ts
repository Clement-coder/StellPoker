import { bestHandRank } from "./hand-rank";

export type HandStrength = "strong" | "marginal" | "weak" | "drawing-dead";

const CATEGORY_STRENGTH: Record<number, HandStrength> = {
  9: "strong",
  8: "strong",
  7: "strong",
  6: "strong",
  5: "strong",
  4: "strong",
  3: "strong",
  2: "marginal",
  1: "weak",
};

export function classifyHandStrength(holeCards: [number, number], boardCards: number[]): HandStrength {
  if (boardCards.length === 0) {
    const r1 = (holeCards[0] % 13) + 2;
    const r2 = (holeCards[1] % 13) + 2;
    const pair = r1 === r2;
    const bothHigh = r1 >= 10 && r2 >= 10;
    if (pair && r1 >= 10) return "strong";
    if (pair) return "marginal";
    if (bothHigh && Math.abs(r1 - r2) <= 2) return "marginal";
    return "weak";
  }

  const allCards = [...holeCards, ...boardCards];
  if (allCards.length < 5) {
    const r1 = (holeCards[0] % 13) + 2;
    const r2 = (holeCards[1] % 13) + 2;
    const s1 = Math.floor(holeCards[0] / 13);
    const s2 = Math.floor(holeCards[1] / 13);
    const boardRanks = boardCards.map((c) => (c % 13) + 2);
    const boardSuits = boardCards.map((c) => Math.floor(c / 13));

    const hasPairWithBoard = boardRanks.includes(r1) || boardRanks.includes(r2);
    if (hasPairWithBoard) return "strong";

    const flushDraw = boardSuits.every((s) => s === s1) || boardSuits.every((s) => s === s2);
    const suited = s1 === s2;
    if (flushDraw && suited) return "marginal";

    const maxBoardRank = Math.max(...boardRanks);
    if (r1 >= maxBoardRank && r2 >= maxBoardRank) return "marginal";

    return "weak";
  }

  const hand = bestHandRank(allCards);
  if (!hand) return "weak";

  const strength = CATEGORY_STRENGTH[hand.category];
  if (strength === "strong") return "strong";
  if (hand.category === 1) {
    const ranks = allCards.map((c) => (c % 13) + 2).sort((a, b) => b - a);
    if (ranks[0] >= 14) return "marginal";
    if (ranks[0] >= 12 && ranks[1] >= 12) return "marginal";
    return "weak";
  }
  return strength;
}

export function getHandStrengthColor(strength: HandStrength): string {
  switch (strength) {
    case "strong": return "#27ae60";
    case "marginal": return "#f39c12";
    case "weak": return "#e74c3c";
    case "drawing-dead": return "#7f8c8d";
  }
}

export function getHandStrengthLabel(strength: HandStrength): string {
  switch (strength) {
    case "strong": return "STRONG";
    case "marginal": return "MARGINAL";
    case "weak": return "WEAK";
    case "drawing-dead": return "DEAD";
  }
}