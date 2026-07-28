"use client";

import { useState } from "react";
import type { GamePhase } from "@/lib/game-state";
import { PixelChip } from "./PixelChip";
import { ChipTray } from "./ChipTray";

interface ActionPanelProps {
  phase: GamePhase;
  isMyTurn: boolean;
  currentBet: number;
  myBet: number;
  myStack: number;
  pot?: number;
  onAction: (action: string, amount?: number) => void;
  onChainConfirmed?: boolean;
  canStartHand?: boolean;
  canResolveShowdown?: boolean;
  statusHint?: string | null;
  loading?: boolean;
  isSolo?: boolean;
  betAmount: number;
  setBetAmount: (amount: number) => void;
}

export function ActionPanel({
  phase,
  isMyTurn,
  currentBet,
  myBet,
  myStack,
  pot = 0,
  onAction,
  onChainConfirmed,
  canStartHand = true,
  canResolveShowdown = true,
  statusHint = null,
  loading = false,
  isSolo = false,
  betAmount,
  setBetAmount,
}: ActionPanelProps) {

  const callAmount = Math.max(currentBet - myBet, 0);
  const minBet = Math.max(currentBet * 2, 1);
  const maxBet = myStack;
  const potOddsRatio = callAmount > 0 ? `${callAmount}:${pot}` : null;
  const potOddsPercent = callAmount > 0 && pot > 0
    ? `${((callAmount / (pot + callAmount)) * 100).toFixed(1)}%`
    : null;

  if (phase === "waiting") {
    return (
      <div className="flex flex-col items-center gap-2">
        <button
          onClick={() => onAction("start")}
          disabled={!canStartHand || loading}
          className="pixel-btn pixel-btn-green text-[12px]"
          style={{ padding: "8px 20px", opacity: canStartHand && !loading ? 1 : 0.6 }}
        >
          DEAL CARDS
        </button>
        {statusHint && (
          <span className="text-[9px]" style={{ color: "#f39c12" }}>
            {statusHint}
          </span>
        )}
      </div>
    );
  }

  if (phase === "showdown") {
    return null;
  }

  if (phase === "settlement") {
    return (
      <div className="flex flex-col items-center gap-3">
        <button
          onClick={() => onAction("start")}
          disabled={!canStartHand || loading}
          className="pixel-btn pixel-btn-green text-[11px]"
          style={{ padding: "8px 18px", opacity: canStartHand && !loading ? 1 : 0.6 }}
        >
          NEW HAND
        </button>
      </div>
    );
  }

  // Active betting phase (preflop, flop, turn, river)
  const isActive = ["preflop", "flop", "turn", "river"].includes(phase);
  const disabled = !isMyTurn || loading;
  if (!isActive) {
    return null;
  }

  return (
    <div
      className="pixel-border-thin flex flex-col items-center gap-3 px-4 py-3"
      style={{
        background: "rgba(10, 20, 30, 0.7)",
        borderColor: "rgba(140, 170, 200, 0.4)",
      }}
    >
      {/* Bet info row */}
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-1">
          <PixelChip color="red" size={2} />
          <span className="text-[9px]" style={{ color: "#95a5a6" }}>
            TABLE BET: {currentBet}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <PixelChip color="blue" size={2} />
          <span className="text-[9px]" style={{ color: "#95a5a6" }}>
            YOUR BET: {myBet}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <PixelChip color="gold" size={2} />
          <span className="text-[9px]" style={{ color: "#27ae60" }}>
            STACK: {myStack.toLocaleString()}
          </span>
        </div>
      </div>
      {isSolo && (
        <span className="text-[9px]" style={{ color: "#f39c12", fontStyle: "italic" }}>
          SOLO VS AI: using fake chips for local play.
        </span>
      )}
      {/* Action buttons */}
      <div className="flex items-center gap-2">
        {/* FOLD */}
        <button
          onClick={() => onAction("fold")}
          disabled={disabled}
          className="pixel-btn text-[10px] has-tooltip"
          style={{
            padding: "6px 14px",
            background: !disabled ? "#7b241c" : "#4a4a4a",
            opacity: !disabled ? 1 : 0.5,
            color: "white",
            textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
          }}
        >
          FOLD
          <span className="tooltip">Shortcut: F</span>
        </button>

        {/* CHECK / CALL */}
        <div className="flex flex-col items-center">
          <button
            onClick={() => onAction(callAmount === 0 ? "check" : "call", callAmount)}
            disabled={disabled}
            className="pixel-btn text-[10px] has-tooltip"
            style={{
              padding: "6px 14px",
              background: !disabled ? "#1a5276" : "#4a4a4a",
              opacity: !disabled ? 1 : 0.5,
              color: "white",
              textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
            }}
          >
            {callAmount === 0 ? "CHECK" : `CALL ${callAmount}`}
            <span className="tooltip">Shortcut: {callAmount === 0 ? "C" : "B"}</span>
          </button>
          {potOddsRatio && potOddsPercent && (
            <span className="text-[7px] mt-1" style={{ color: "#f1c40f" }}>
              POT ODDS: {potOddsRatio} ({potOddsPercent})
            </span>
          )}
        </div>

        {/* BET / RAISE */}
        <button
          onClick={() => onAction(currentBet === 0 ? "bet" : "raise", betAmount || minBet)}
          disabled={disabled || myStack <= callAmount}
          className="pixel-btn text-[10px] has-tooltip"
          style={{
            padding: "6px 14px",
            background: !disabled && myStack > callAmount ? "#7d6608" : "#4a4a4a",
            opacity: !disabled && myStack > callAmount ? 1 : 0.5,
            color: "white",
            textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
          }}
        >
          {currentBet === 0 ? "BET" : "RAISE"} {betAmount || minBet}
          <span className="tooltip">Shortcut: {currentBet === 0 ? "B" : "R"}</span>
        </button>

        {/* ALL IN */}
        <button
          onClick={() => onAction("allin", myStack)}
          disabled={disabled || myStack <= 0}
          className="pixel-btn text-[10px] has-tooltip"
          style={{
            padding: "6px 14px",
            background: !disabled && myStack > 0 ? "#d4ac0d" : "#4a4a4a",
            opacity: !disabled && myStack > 0 ? 1 : 0.5,
            color: "#1a1a1a",
            textShadow: "1px 1px 0 rgba(255,255,255,0.3)",
            fontWeight: "bold",
          }}
        >
          ALL IN
          <span className="tooltip">Shortcut: A</span>
        </button>
      </div>

      {/* Bet slider + quick buttons */}
      {!disabled && myStack > callAmount && (
        <div className="flex items-center gap-3 w-full max-w-sm">
          <input
            type="range"
            min={minBet}
            max={maxBet}
            value={betAmount || minBet}
            onChange={(e) => setBetAmount(Number(e.target.value))}
            className="flex-1"
            style={{
              accentColor: "#f1c40f",
              height: "4px",
            }}
          />
          <div className="flex gap-1">
            {[
              { label: "50%", value: Math.floor(myStack * 0.5) },
              { label: "75%", value: Math.floor(myStack * 0.75) },
              { label: "MAX", value: myStack },
            ].map((preset) => (
              <button
                key={preset.label}
                onClick={() => setBetAmount(Math.max(preset.value, minBet))}
                className="pixel-btn text-[8px]"
                style={{
                  padding: "4px 8px",
                  background: "#2c3e50",
                  color: "#c8e6ff",
                }}
              >
                {preset.label}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Chip tray for drag-and-drop betting */}
      {!disabled && myStack > callAmount && (
        <div className="w-full max-w-sm">
          <ChipTray
            myStack={myStack}
            betAmount={betAmount}
            setBetAmount={setBetAmount}
            disabled={disabled}
          />
        </div>
      )}

      {disabled && !loading && (
        <span className="text-[9px]" style={{ color: "#95a5a6", fontStyle: "italic" }}>
          WAITING FOR OPPONENT...
        </span>
      )}

      {statusHint && (
        <span className="text-[9px]" style={{ color: "#f39c12" }}>
          {statusHint}
        </span>
      )}
    </div>
  );
}
