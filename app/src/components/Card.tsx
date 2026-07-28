"use client";

import type { CSSProperties } from "react";
import { decodeCard } from "@/lib/cards";
import type { HandStrength } from "@/lib/hand-strength";
import { getHandStrengthColor, getHandStrengthLabel } from "@/lib/hand-strength";

interface CardProps {
  value?: number;
  faceDown?: boolean;
  size?: "sm" | "md" | "lg";
  /**
   * Play a 3D back-to-front flip when the (face-up) card mounts — used for
   * dealing hole cards and revealing community cards. Ignored for face-down
   * cards. Respects `prefers-reduced-motion` (see globals.css).
   */
  flip?: boolean;
  /** Stagger delay in seconds applied to the flip animation. */
  flipDelay?: number;
  /** Hand strength indicator color ring (shown for own hole cards after flop). */
  strength?: HandStrength | null;
}

type CardDims = {
  w: number;
  h: number;
  suitSize: string;
  rankSize: string;
};

const DIMS: Record<NonNullable<CardProps["size"]>, CardDims> = {
  sm: { w: 44, h: 62, suitSize: "16px", rankSize: "7px" },
  md: { w: 56, h: 80, suitSize: "22px", rankSize: "9px" },
  lg: { w: 72, h: 100, suitSize: "28px", rankSize: "11px" },
};

const SUIT_SYMBOLS: Record<string, string> = {
  hearts: '♥',
  diamonds: '♦',
  clubs: '♣',
  spades: '♠',
};

/* Pixel card back: dark blue with a small star/S pattern */
function CardBack({ w, h }: { w: number; h: number }) {
  return (
    <div
      className="pixel-border-thin flex items-center justify-center"
      style={{
        width: `${w}px`,
        height: `${h}px`,
        background: 'linear-gradient(180deg, #1a3a5c 0%, #0d2137 100%)',
        position: 'relative',
        overflow: 'hidden',
      }}
    >
      {/* Crosshatch pixel pattern */}
      <div style={{
        position: 'absolute',
        inset: '6px',
        border: '2px solid #2a5a8c',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}>
        <div style={{
          color: '#3498db',
          fontSize: '10px',
          textShadow: '1px 1px 0 #0d2137',
        }}>
          S
        </div>
      </div>
    </div>
  );
}

/* The face (rank + suit) of a revealed card. */
function CardFace({ value, d, className, strength }: { value: number; d: CardDims; className?: string; strength?: HandStrength | null }) {
  const card = decodeCard(value);
  const color = card.color === 'red' ? '#e74c3c' : '#2c3e50';
  const suitSymbol = SUIT_SYMBOLS[card.suit] || '♠';
  const strengthColor = strength ? getHandStrengthColor(strength) : null;
  const strengthLabel = strength ? getHandStrengthLabel(strength) : null;

  return (
    <div
      className={`pixel-border-white flex flex-col items-center justify-between${className ? ` ${className}` : ''}`}
      style={{
        width: `${d.w}px`,
        height: `${d.h}px`,
        background: '#fefefe',
        padding: '4px',
        imageRendering: 'auto',
        position: 'relative',
        ...(strengthColor ? {
          boxShadow: `inset 0 0 0 3px ${strengthColor}, 0 0 8px ${strengthColor}40`,
        } : {}),
      }}
    >
      {strengthLabel && (
        <div
          className="absolute -top-2.5 left-1/2 -translate-x-1/2 px-1.5 py-0.5"
          style={{
            background: strengthColor ?? '#27ae60',
            color: '#fff',
            fontSize: '6px',
            lineHeight: 1,
            whiteSpace: 'nowrap',
            zIndex: 10,
            border: '1px solid rgba(255,255,255,0.3)',
          }}
        >
          {strengthLabel}
        </div>
      )}
      {/* Top-left rank + suit */}
      <div className="w-full flex flex-col items-start" style={{
        color,
        lineHeight: 1,
        paddingLeft: '2px',
      }}>
        <span style={{ fontSize: d.rankSize }}>{card.rank}</span>
        <span style={{ fontSize: d.rankSize, fontFamily: 'serif' }}>{suitSymbol}</span>
      </div>

      {/* Center suit */}
      <div className="flex items-center justify-center flex-1" style={{
        color,
        fontSize: d.suitSize,
        fontFamily: 'serif',
        lineHeight: 1,
      }}>
        {suitSymbol}
      </div>

      {/* Bottom-right rank + suit (inverted) */}
      <div className="w-full flex flex-col items-end" style={{
        color,
        lineHeight: 1,
        paddingRight: '2px',
        transform: 'rotate(180deg)',
      }}>
        <span style={{ fontSize: d.rankSize }}>{card.rank}</span>
        <span style={{ fontSize: d.rankSize, fontFamily: 'serif' }}>{suitSymbol}</span>
      </div>
    </div>
  );
}

export function Card({ value, faceDown = false, size = "md", flip = false, flipDelay = 0, strength = null }: CardProps) {
  const d = DIMS[size];

  if (faceDown || value === undefined) {
    return (
      <div className="card-responsive inline-block">
        <CardBack w={d.w} h={d.h} />
      </div>
    );
  }

  // 3D flip: render both faces and rotate from back (180deg) to front (0deg).
  if (flip) {
    return (
      <div className="card-responsive inline-block">
        <div
          className="card-flip"
          style={{ width: `${d.w}px`, height: `${d.h}px`, "--flip-delay": `${flipDelay}s` } as CSSProperties}
        >
          <div className="card-flip-inner">
            <div className="card-flip-face card-flip-back">
              <CardBack w={d.w} h={d.h} />
            </div>
            <div className="card-flip-face card-flip-front">
              <CardFace value={value} d={d} strength={strength} />
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="card-responsive inline-block">
      <CardFace value={value} d={d} className="animate-card-deal" strength={strength} />
    </div>
  );
}
