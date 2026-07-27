"use client";

import { useEffect, useRef, useState } from "react";

/**
 * PixelChip — CSS-only pixel art poker chip sprites.
 * Four denominations: white (25), red (100), blue (500), gold (1000).
 */

type ChipColor = "white" | "red" | "blue" | "gold";

const CHIP_COLORS: Record<ChipColor, { outer: string; inner: string; edge: string; highlight: string }> = {
  white: { outer: "#e0e0e0", inner: "#ffffff", edge: "#bdbdbd", highlight: "#f5f5f5" },
  red:   { outer: "#c0392b", inner: "#e74c3c", edge: "#962d22", highlight: "#ff6b6b" },
  blue:  { outer: "#2471a3", inner: "#3498db", edge: "#1a5276", highlight: "#5dade2" },
  gold:  { outer: "#d4ac0d", inner: "#f1c40f", edge: "#b7950b", highlight: "#f4d03f" },
};

interface PixelChipProps {
  color?: ChipColor;
  size?: number;
}

export function PixelChip({ color = "red", size = 3 }: PixelChipProps) {
  const c = CHIP_COLORS[color];
  const px = size;

  return (
    <div style={{ display: "inline-block", width: `${8 * px}px`, height: `${7 * px}px` }}>
      <div
        style={{
          width: `${px}px`,
          height: `${px}px`,
          background: "transparent",
          boxShadow: `
            /* Top edge */
            ${2 * px}px ${0 * px}px 0 ${c.edge},
            ${3 * px}px ${0 * px}px 0 ${c.edge},
            ${4 * px}px ${0 * px}px 0 ${c.edge},
            ${5 * px}px ${0 * px}px 0 ${c.edge},

            /* Row 1 */
            ${1 * px}px ${1 * px}px 0 ${c.edge},
            ${2 * px}px ${1 * px}px 0 ${c.outer},
            ${3 * px}px ${1 * px}px 0 ${c.highlight},
            ${4 * px}px ${1 * px}px 0 ${c.highlight},
            ${5 * px}px ${1 * px}px 0 ${c.outer},
            ${6 * px}px ${1 * px}px 0 ${c.edge},

            /* Row 2 */
            ${0 * px}px ${2 * px}px 0 ${c.edge},
            ${1 * px}px ${2 * px}px 0 ${c.outer},
            ${2 * px}px ${2 * px}px 0 ${c.highlight},
            ${3 * px}px ${2 * px}px 0 ${c.inner},
            ${4 * px}px ${2 * px}px 0 ${c.inner},
            ${5 * px}px ${2 * px}px 0 ${c.outer},
            ${6 * px}px ${2 * px}px 0 ${c.outer},
            ${7 * px}px ${2 * px}px 0 ${c.edge},

            /* Row 3 - center */
            ${0 * px}px ${3 * px}px 0 ${c.edge},
            ${1 * px}px ${3 * px}px 0 ${c.outer},
            ${2 * px}px ${3 * px}px 0 ${c.inner},
            ${3 * px}px ${3 * px}px 0 ${c.highlight},
            ${4 * px}px ${3 * px}px 0 ${c.highlight},
            ${5 * px}px ${3 * px}px 0 ${c.inner},
            ${6 * px}px ${3 * px}px 0 ${c.outer},
            ${7 * px}px ${3 * px}px 0 ${c.edge},

            /* Row 4 */
            ${0 * px}px ${4 * px}px 0 ${c.edge},
            ${1 * px}px ${4 * px}px 0 ${c.outer},
            ${2 * px}px ${4 * px}px 0 ${c.outer},
            ${3 * px}px ${4 * px}px 0 ${c.inner},
            ${4 * px}px ${4 * px}px 0 ${c.inner},
            ${5 * px}px ${4 * px}px 0 ${c.highlight},
            ${6 * px}px ${4 * px}px 0 ${c.outer},
            ${7 * px}px ${4 * px}px 0 ${c.edge},

            /* Row 5 */
            ${1 * px}px ${5 * px}px 0 ${c.edge},
            ${2 * px}px ${5 * px}px 0 ${c.outer},
            ${3 * px}px ${5 * px}px 0 ${c.outer},
            ${4 * px}px ${5 * px}px 0 ${c.outer},
            ${5 * px}px ${5 * px}px 0 ${c.outer},
            ${6 * px}px ${5 * px}px 0 ${c.edge},

            /* Bottom edge */
            ${2 * px}px ${6 * px}px 0 ${c.edge},
            ${3 * px}px ${6 * px}px 0 ${c.edge},
            ${4 * px}px ${6 * px}px 0 ${c.edge},
            ${5 * px}px ${6 * px}px 0 ${c.edge}
          `,
        }}
      />
    </div>
  );
}

/** Pick 1-3 representative chip colors based on the magnitude of the amount. */
function representativeChips(amount: number): ChipColor[] {
  if (amount <= 0) return [];
  if (amount < 100) return ["white"];
  if (amount < 500) return ["red"];
  if (amount < 1000) return ["red", "red"];
  if (amount < 5000) return ["gold"];
  if (amount < 10000) return ["gold", "blue"];
  return ["gold", "gold", "blue"];
}

interface PixelChipStackProps {
  amount: number;
  size?: number;
}

export function PixelChipStack({ amount, size = 2 }: PixelChipStackProps) {
  const chips = representativeChips(amount);
  if (chips.length === 0) return null;

  // Chip is 8*size wide, 7*size tall. Overlap so stack is compact.
  const chipHeight = size * 7;
  const overlap = Math.round(chipHeight * 0.6);

  return (
    <div
      style={{
        display: "inline-flex",
        flexDirection: "column-reverse",
        alignItems: "center",
      }}
    >
      {chips.map((color, i) => (
        <div
          key={i}
          style={{
            marginTop: i > 0 ? `-${overlap}px` : "0px",
            zIndex: i,
          }}
        >
          <PixelChip color={color} size={size} />
        </div>
      ))}
    </div>
  );
}

interface PotChipPileProps {
  amount: number;
  size?: number;
}

export function PotChipPile({ amount, size = 3 }: PotChipPileProps) {
  const chips = representativeChips(amount);
  if (chips.length === 0) return null;

  return (
    <div
      style={{
        display: "inline-flex",
        flexDirection: "row",
        alignItems: "flex-end",
        gap: `${size}px`,
      }}
    >
      {chips.map((color, i) => (
        <div
          key={i}
          style={{
            marginBottom: `${(i % 2) * size}px`,
          }}
        >
          <PixelChip color={color} size={size} />
        </div>
      ))}
    </div>
  );
}

/** Custom hook for animating numeric chip counts using requestAnimationFrame on bet/raise (#54). */
export function useAnimatedCounter(targetValue: number, duration = 400): number {
  const [displayValue, setDisplayValue] = useState(targetValue);
  const prevValueRef = useRef(targetValue);

  useEffect(() => {
    const startValue = prevValueRef.current;
    if (startValue === targetValue) return;

    let startTime: number | null = null;
    let animId: number;

    const step = (timestamp: number) => {
      if (!startTime) startTime = timestamp;
      const progress = Math.min((timestamp - startTime) / duration, 1);
      // Ease-out cubic formula
      const easeProgress = 1 - Math.pow(1 - progress, 3);
      const current = Math.round(startValue + (targetValue - startValue) * easeProgress);
      setDisplayValue(current);

      if (progress < 1) {
        animId = requestAnimationFrame(step);
      } else {
        prevValueRef.current = targetValue;
      }
    };

    animId = requestAnimationFrame(step);
    return () => cancelAnimationFrame(animId);
  }, [targetValue, duration]);

  return displayValue;
}

/** Component that renders an animated chip counter text value. */
export function AnimatedChipCounter({ value, prefix = "", suffix = "", className, style }: {
  value: number;
  prefix?: string;
  suffix?: string;
  className?: string;
  style?: React.CSSProperties;
}) {
  const animatedValue = useAnimatedCounter(value);
  return (
    <span className={className} style={style}>
      {prefix}{animatedValue.toLocaleString()}{suffix}
    </span>
  );
}

/** Animated version of PixelChipStack that smoothly animates chip stack counts. */
export function AnimatedChipStack({ amount, size = 2 }: PixelChipStackProps) {
  const animatedAmount = useAnimatedCounter(amount);
  return <PixelChipStack amount={animatedAmount} size={size} />;
}

