"use client";

import { useState, useRef, useCallback } from "react";
import { PixelChip } from "./PixelChip";

interface ChipTrayProps {
  myStack: number;
  betAmount: number;
  setBetAmount: (amount: number) => void;
  disabled: boolean;
}

type ChipDenom = { label: string; value: number; color: "white" | "red" | "blue" | "gold" };

const DENOMS: ChipDenom[] = [
  { label: "25", value: 25, color: "white" },
  { label: "100", value: 100, color: "red" },
  { label: "500", value: 500, color: "blue" },
  { label: "1K", value: 1000, color: "gold" },
];

export function ChipTray({ myStack, betAmount, setBetAmount, disabled }: ChipTrayProps) {
  const [selectedChips, setSelectedChips] = useState<number[]>([]);
  const [dragAmount, setDragAmount] = useState(0);
  const dragCountRef = useRef(0);

  const handleDragStart = useCallback((e: React.DragEvent, denom: ChipDenom) => {
    if (disabled) return;
    e.dataTransfer.setData("text/plain", String(denom.value));
    e.dataTransfer.effectAllowed = "copy";
    dragCountRef.current = 1;
    setDragAmount(denom.value);
  }, [disabled]);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    const value = parseInt(e.dataTransfer.getData("text/plain"), 10);
    if (isNaN(value) || disabled) return;
    const newTotal = Math.min(betAmount + value, myStack);
    setBetAmount(newTotal);
    setSelectedChips((prev) => [...prev, value]);
    setDragAmount(0);
  }, [betAmount, myStack, setBetAmount, disabled]);

  const handleChipClick = useCallback((denom: ChipDenom) => {
    if (disabled) return;
    const newTotal = Math.min(betAmount + denom.value, myStack);
    setBetAmount(newTotal);
    setSelectedChips((prev) => [...prev, denom.value]);
  }, [betAmount, myStack, setBetAmount, disabled]);

  const clearChips = useCallback(() => {
    setSelectedChips([]);
    setBetAmount(0);
  }, [setBetAmount]);

  return (
    <div
      className="flex flex-col items-center gap-2 px-2 py-2"
      onDragOver={handleDragOver}
      onDrop={handleDrop}
      style={{
        background: "rgba(0,0,0,0.15)",
        border: "2px dashed rgba(139, 105, 20, 0.4)",
        borderRadius: "4px",
        minHeight: "60px",
        opacity: disabled ? 0.5 : 1,
      }}
    >
      <span className="text-[8px]" style={{ color: "#95a5a6" }}>DRAG CHIPS TO BET</span>
      <div className="flex items-center gap-2">
        {DENOMS.filter((d) => d.value <= myStack).map((denom) => (
          <div
            key={denom.label}
            draggable={!disabled}
            onDragStart={(e) => handleDragStart(e, denom)}
            onClick={() => handleChipClick(denom)}
            className="cursor-grab active:cursor-grabbing transition-transform hover:scale-110"
            title={`Drag or click to add ${denom.label}`}
          >
            <PixelChip color={denom.color} size={3} />
            <div className="text-[7px] text-center mt-0.5" style={{ color: "#c8e6ff" }}>
              {denom.label}
            </div>
          </div>
        ))}
      </div>
      {selectedChips.length > 0 && (
        <div className="flex items-center gap-2 mt-1">
          <span className="text-[8px]" style={{ color: "#f1c40f" }}>
            SELECTED: {selectedChips.length} CHIP{selectedChips.length > 1 ? "S" : ""} ({betAmount})
          </span>
          <button
            onClick={clearChips}
            className="text-[7px]"
            style={{
              background: "none",
              border: "1px solid #7f8c8d",
              color: "#e74c3c",
              padding: "2px 6px",
              cursor: "pointer",
            }}
          >
            CLEAR
          </button>
        </div>
      )}
      {dragAmount > 0 && (
        <div className="text-[8px]" style={{ color: "#f39c12" }}>
          DRAGGING: +{dragAmount}
        </div>
      )}
    </div>
  );
}