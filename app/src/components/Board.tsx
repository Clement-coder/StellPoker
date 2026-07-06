"use client";

import { useEffect, useRef } from "react";
import { Card } from "./Card";
import { PotChipPile } from "./PixelChip";

interface BoardProps {
  cards: number[];
  pot: number;
}

/** Stagger between consecutive cards flipping in, in seconds. */
const FLIP_STAGGER = 0.1;

export function Board({ cards, pot }: BoardProps) {
  // Track how many cards were already on the board so that only the
  // newly-revealed cards (flop, then turn, then river) animate, and they
  // stagger starting from zero rather than from their absolute index.
  const revealedCountRef = useRef(0);
  const firstNewIndex = revealedCountRef.current;
  useEffect(() => {
    revealedCountRef.current = cards.length;
  }, [cards.length]);

  return (
    <div className="flex flex-col items-center gap-3">
      {/* Pot display */}
      <div className="flex items-center gap-2">
        <PotChipPile amount={pot} size={2} />
        <span className="text-[12px]" style={{
          color: '#f1c40f',
          textShadow: '1px 1px 0 rgba(0,0,0,0.6)',
          marginLeft: '4px',
        }}>
          POT: {pot.toLocaleString()} CHIPS
        </span>
      </div>

      {/* Community cards */}
      <div className="flex gap-2 items-center">
        {cards.map((card, i) => (
          <Card
            key={i}
            value={card}
            size="md"
            flip
            flipDelay={Math.max(0, i - firstNewIndex) * FLIP_STAGGER}
          />
        ))}
        {/* Empty slots */}
        {Array.from({ length: 5 - cards.length }).map((_, i) => (
          <div
            key={`empty-${i}`}
            style={{
              width: '56px',
              height: '80px',
              border: '3px dashed rgba(139, 105, 20, 0.3)',
              background: 'rgba(0, 0, 0, 0.15)',
            }}
          />
        ))}
      </div>
    </div>
  );
}
