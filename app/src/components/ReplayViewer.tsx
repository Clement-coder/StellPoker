"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { Card } from "./Card";
import { stellarExpertUrl } from "@/lib/explorer";
import type { ReplayHand, ReplayStep } from "@/lib/replay";

// ── Helpers ───────────────────────────────────────────────────────────────────

function shortAddr(address: string): string {
  if (!address || address.length < 12) return address;
  return `${address.slice(0, 6)}…${address.slice(-4)}`;
}

const STEP_LABELS: Record<ReplayStep["kind"], string> = {
  deal: "DEAL",
  flop: "FLOP",
  turn: "TURN",
  river: "RIVER",
  action: "ACTION",
  showdown: "SHOWDOWN",
  fold_win: "FOLD WIN",
};

const STEP_COLORS: Record<ReplayStep["kind"], string> = {
  deal: "#3498db",
  flop: "#2ecc71",
  turn: "#f39c12",
  river: "#e74c3c",
  action: "#95a5a6",
  showdown: "#9b59b6",
  fold_win: "#e67e22",
};

// ── Step description renderer ─────────────────────────────────────────────────

function StepDesc({ step }: { step: ReplayStep }) {
  switch (step.kind) {
    case "deal":
      return (
        <div>
          <div className="text-[9px]" style={{ color: "#c8e6ff" }}>
            Deck committed on-chain.
            {step.deckRoot && (
              <span style={{ color: "#7f8c8d" }}> ROOT: {step.deckRoot.slice(0, 16)}…</span>
            )}
          </div>
          {step.txHash && (
            <a
              href={stellarExpertUrl("tx", step.txHash)}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[8px]"
              style={{ color: "#ffc078" }}
            >
              VIEW DEAL PROOF ↗
            </a>
          )}
        </div>
      );

    case "flop":
      return (
        <div>
          <div className="flex gap-1 my-1">
            {step.cards.map((c, i) => (
              <Card key={i} value={c} size="sm" flip flipDelay={i * 0.12} />
            ))}
          </div>
          {step.txHash && (
            <a
              href={stellarExpertUrl("tx", step.txHash)}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[8px]"
              style={{ color: "#ffc078" }}
            >
              VIEW FLOP PROOF ↗
            </a>
          )}
        </div>
      );

    case "turn":
      return (
        <div>
          <div className="flex gap-1 my-1">
            <Card value={step.card} size="sm" flip />
          </div>
          {step.txHash && (
            <a
              href={stellarExpertUrl("tx", step.txHash)}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[8px]"
              style={{ color: "#ffc078" }}
            >
              VIEW TURN PROOF ↗
            </a>
          )}
        </div>
      );

    case "river":
      return (
        <div>
          <div className="flex gap-1 my-1">
            <Card value={step.card} size="sm" flip />
          </div>
          {step.txHash && (
            <a
              href={stellarExpertUrl("tx", step.txHash)}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[8px]"
              style={{ color: "#ffc078" }}
            >
              VIEW RIVER PROOF ↗
            </a>
          )}
        </div>
      );

    case "action":
      return (
        <div className="text-[9px]" style={{ color: "#f5e6c8" }}>
          <span style={{ color: "#c47d2e" }}>{shortAddr(step.player)}</span>
          {" → "}
          <span style={{ color: "#27ae60", fontWeight: "bold" }}>{step.action.toUpperCase()}</span>
          {step.amount != null && step.amount > 0 && (
            <span style={{ color: "#f1c40f" }}> {step.amount.toLocaleString()}</span>
          )}
          <span style={{ color: "#7f8c8d" }}> ({step.street})</span>
        </div>
      );

    case "showdown":
      return (
        <div>
          <div className="text-[9px]" style={{ color: "#27ae60" }}>
            WINNER: {shortAddr(step.winner)}
          </div>
          {Object.entries(step.holecards).map(([addr, cards]) => (
            <div key={addr} className="flex items-center gap-2 mt-1">
              <span className="text-[8px]" style={{ color: "#95a5a6" }}>{shortAddr(addr)}</span>
              <Card value={cards[0]} size="sm" flip flipDelay={0} />
              <Card value={cards[1]} size="sm" flip flipDelay={0.1} />
            </div>
          ))}
          {step.txHash && (
            <a
              href={stellarExpertUrl("tx", step.txHash)}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[8px] block mt-1"
              style={{ color: "#ffc078" }}
            >
              VIEW SHOWDOWN PROOF ↗
            </a>
          )}
        </div>
      );

    case "fold_win":
      return (
        <div>
          <div className="text-[9px]" style={{ color: "#e67e22" }}>
            {shortAddr(step.winner)} wins pot of{" "}
            <span style={{ color: "#f1c40f" }}>{step.pot.toLocaleString()}</span>{" "}
            (everyone else folded)
          </div>
          {step.txHash && (
            <a
              href={stellarExpertUrl("tx", step.txHash)}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[8px] block mt-1"
              style={{ color: "#ffc078" }}
            >
              VIEW TX ↗
            </a>
          )}
        </div>
      );
  }
}

// ── Board state at a given step index ────────────────────────────────────────

function boardCardsAtStep(steps: ReplayStep[], upTo: number): number[] {
  const cards: number[] = [];
  for (let i = 0; i <= upTo; i++) {
    const s = steps[i];
    if (!s) break;
    if (s.kind === "flop") cards.push(...s.cards);
    if (s.kind === "turn") cards.push(s.card);
    if (s.kind === "river") cards.push(s.card);
  }
  return cards;
}

function holeCardsAtStep(
  steps: ReplayStep[],
  upTo: number
): Record<string, [number, number]> {
  // deal step sets initial hole cards (may be empty if private).
  // showdown step reveals them.
  let latest: Record<string, [number, number]> = {};
  for (let i = 0; i <= upTo; i++) {
    const s = steps[i];
    if (!s) break;
    if (s.kind === "deal" && Object.keys(s.holeCards).length > 0) {
      latest = { ...s.holeCards };
    }
    if (s.kind === "showdown" && Object.keys(s.holecards).length > 0) {
      latest = { ...s.holecards };
    }
  }
  return latest;
}

// ── Main component ────────────────────────────────────────────────────────────

interface ReplayViewerProps {
  hand: ReplayHand;
}

export function ReplayViewer({ hand }: ReplayViewerProps) {
  const [stepIndex, setStepIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const totalSteps = hand.steps.length;

  const advance = useCallback(() => {
    setStepIndex((prev) => {
      if (prev >= totalSteps - 1) {
        setPlaying(false);
        return prev;
      }
      return prev + 1;
    });
  }, [totalSteps]);

  // Auto-play ticker
  useEffect(() => {
    if (playing) {
      intervalRef.current = setInterval(advance, 1400);
    } else {
      if (intervalRef.current) clearInterval(intervalRef.current);
    }
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [playing, advance]);

  const handlePlay = () => {
    if (stepIndex >= totalSteps - 1) {
      setStepIndex(0);
    }
    setPlaying(true);
  };

  const handlePause = () => setPlaying(false);

  const handlePrev = () => {
    setPlaying(false);
    setStepIndex((p) => Math.max(0, p - 1));
  };

  const handleNext = () => {
    setPlaying(false);
    setStepIndex((p) => Math.min(totalSteps - 1, p + 1));
  };

  const handleScrub = (e: React.ChangeEvent<HTMLInputElement>) => {
    setPlaying(false);
    setStepIndex(Number(e.target.value));
  };

  const currentStep = hand.steps[stepIndex];
  const board = boardCardsAtStep(hand.steps, stepIndex);
  const holeCards = holeCardsAtStep(hand.steps, stepIndex);

  return (
    <div
      className="flex flex-col gap-4"
      style={{ maxWidth: 560, margin: "0 auto" }}
      role="main"
      aria-label={`Hand replay: table ${hand.tableId} hand #${hand.handNumber}`}
    >
      {/* Header */}
      <div
        className="pixel-border flex items-center justify-between px-4 py-3"
        style={{ borderColor: "#c47d2e", background: "rgba(12,10,24,0.95)" }}
      >
        <div>
          <div className="text-[10px]" style={{ color: "#f5e6c8" }}>
            TABLE #{hand.tableId} &middot; HAND #{hand.handNumber}
          </div>
          {hand.settledAt && (
            <div className="text-[8px]" style={{ color: "#7f8c8d" }}>
              {new Date(hand.settledAt).toLocaleString()}
            </div>
          )}
        </div>
        {hand.winner && (
          <div className="text-[9px]" style={{ color: "#27ae60" }}>
            WINNER: {shortAddr(hand.winner)}
          </div>
        )}
      </div>

      {/* Board */}
      <div
        className="pixel-border flex flex-col items-center gap-3 py-4 px-4"
        style={{ borderColor: "#2c5c2c", background: "rgba(10,30,10,0.7)" }}
        aria-label="Community cards"
      >
        <div className="text-[8px]" style={{ color: "#95a5a6" }}>COMMUNITY CARDS</div>
        <div className="flex gap-2 min-h-[62px] items-end">
          {board.length === 0 ? (
            <>
              {[0, 1, 2, 3, 4].map((i) => (
                <Card key={i} faceDown size="sm" />
              ))}
            </>
          ) : (
            <>
              {board.map((c, i) => (
                <Card key={i} value={c} size="sm" />
              ))}
              {Array.from({ length: Math.max(0, 5 - board.length) }, (_, i) => (
                <Card key={`fd-${i}`} faceDown size="sm" />
              ))}
            </>
          )}
        </div>

        {/* Hole cards (when known) */}
        {Object.keys(holeCards).length > 0 && (
          <div className="w-full mt-2" aria-label="Player hole cards">
            <div className="text-[8px] mb-1" style={{ color: "#95a5a6" }}>HOLE CARDS</div>
            <div className="flex flex-wrap gap-3">
              {Object.entries(holeCards).map(([addr, cards]) => (
                <div key={addr} className="flex items-center gap-1">
                  <span className="text-[7px]" style={{ color: "#7f8c8d" }}>
                    {shortAddr(addr)}
                  </span>
                  <Card value={cards[0]} size="sm" />
                  <Card value={cards[1]} size="sm" />
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Current step display */}
      {currentStep && (
        <div
          className="pixel-border px-4 py-3 min-h-[72px]"
          style={{
            borderColor: STEP_COLORS[currentStep.kind],
            background: "rgba(12,10,24,0.92)",
          }}
          aria-live="polite"
          aria-label={`Current step: ${STEP_LABELS[currentStep.kind]}`}
        >
          <div
            className="text-[9px] mb-2"
            style={{ color: STEP_COLORS[currentStep.kind] }}
          >
            STEP {stepIndex + 1}/{totalSteps} — {STEP_LABELS[currentStep.kind]}
          </div>
          <StepDesc step={currentStep} />
        </div>
      )}

      {/* Scrubber */}
      <div className="flex flex-col gap-1">
        <input
          type="range"
          min={0}
          max={Math.max(0, totalSteps - 1)}
          value={stepIndex}
          onChange={handleScrub}
          className="w-full"
          aria-label="Replay scrubber"
          style={{ accentColor: "#c47d2e" }}
        />
        <div className="flex justify-between text-[7px]" style={{ color: "#7f8c8d" }}>
          <span>START</span>
          <span>STEP {stepIndex + 1} / {totalSteps}</span>
          <span>END</span>
        </div>
      </div>

      {/* Playback controls */}
      <div className="flex items-center justify-center gap-3">
        <button
          onClick={handlePrev}
          disabled={stepIndex === 0}
          className="pixel-btn text-[10px]"
          style={{
            padding: "6px 14px",
            opacity: stepIndex === 0 ? 0.4 : 1,
            background: "#2c3e50",
            color: "white",
          }}
          aria-label="Previous step"
        >
          ◀
        </button>

        {playing ? (
          <button
            onClick={handlePause}
            className="pixel-btn text-[10px]"
            style={{ padding: "6px 18px", background: "#e67e22", color: "white" }}
            aria-label="Pause replay"
          >
            ❚❚ PAUSE
          </button>
        ) : (
          <button
            onClick={handlePlay}
            disabled={totalSteps === 0}
            className="pixel-btn text-[10px]"
            style={{
              padding: "6px 18px",
              background: "#27ae60",
              color: "white",
              opacity: totalSteps === 0 ? 0.4 : 1,
            }}
            aria-label="Play replay"
          >
            ▶ PLAY
          </button>
        )}

        <button
          onClick={handleNext}
          disabled={stepIndex >= totalSteps - 1}
          className="pixel-btn text-[10px]"
          style={{
            padding: "6px 14px",
            opacity: stepIndex >= totalSteps - 1 ? 0.4 : 1,
            background: "#2c3e50",
            color: "white",
          }}
          aria-label="Next step"
        >
          ▶
        </button>
      </div>

      {/* Step timeline (scrollable list) */}
      <div
        className="pixel-border"
        style={{
          borderColor: "#2a2a4a",
          background: "rgba(12,10,24,0.88)",
          maxHeight: 220,
          overflowY: "auto",
          padding: "8px 4px",
        }}
        aria-label="Hand timeline"
      >
        <div className="text-[8px] px-2 mb-2" style={{ color: "#95a5a6" }}>
          TIMELINE
        </div>
        {hand.steps.map((step, i) => (
          <button
            key={i}
            onClick={() => {
              setPlaying(false);
              setStepIndex(i);
            }}
            className="w-full text-left px-2 py-1 flex items-center gap-2"
            style={{
              background: i === stepIndex ? "rgba(196,125,46,0.18)" : "transparent",
              border: "none",
              cursor: "pointer",
              borderLeft: `3px solid ${i === stepIndex ? "#c47d2e" : "transparent"}`,
            }}
            aria-label={`Go to step ${i + 1}: ${STEP_LABELS[step.kind]}`}
            aria-current={i === stepIndex ? "step" : undefined}
          >
            <span
              className="text-[8px] min-w-[70px]"
              style={{ color: STEP_COLORS[step.kind] }}
            >
              {STEP_LABELS[step.kind]}
            </span>
            <span className="text-[7px]" style={{ color: "#7f8c8d" }}>
              {step.kind === "action"
                ? `${shortAddr(step.player)}: ${step.action}`
                : step.kind === "showdown"
                  ? `winner: ${shortAddr(step.winner)}`
                  : step.kind === "fold_win"
                    ? `${shortAddr(step.winner)} wins`
                    : step.kind === "deal"
                      ? `${Object.keys(step.holeCards).length} players`
                      : step.kind === "flop"
                        ? step.cards.map((c) => c.toString()).join(", ")
                        : step.kind === "turn" || step.kind === "river"
                          ? String(step.card)
                          : ""}
            </span>
          </button>
        ))}
      </div>

      {/* Proof links */}
      {Object.keys(hand.proofLinks).length > 0 && (
        <div
          className="pixel-border px-4 py-3"
          style={{
            borderColor: "#4a4a6a",
            background: "rgba(12,10,24,0.88)",
          }}
          aria-label="On-chain proof links"
        >
          <div className="text-[8px] mb-2" style={{ color: "#95a5a6" }}>
            ON-CHAIN PROOF VERIFICATION STEPS
          </div>
          <div className="flex flex-wrap gap-2">
            {(
              Object.entries(hand.proofLinks) as [
                keyof typeof hand.proofLinks,
                string,
              ][]
            ).map(([key, url]) => (
              <a
                key={key}
                href={url}
                target="_blank"
                rel="noopener noreferrer"
                className="pixel-border-thin text-[8px] px-2 py-1"
                style={{
                  color: "#ffc078",
                  borderColor: "#c47d2e",
                  background: "rgba(196,125,46,0.08)",
                  textDecoration: "none",
                }}
              >
                {key.toUpperCase()} PROOF ↗
              </a>
            ))}
          </div>
        </div>
      )}

      {/* Final summary */}
      <div
        className="pixel-border-thin px-4 py-2 text-[8px] flex items-center justify-between"
        style={{
          borderColor: "#4a4a6a",
          background: "rgba(12,10,24,0.7)",
          color: "#95a5a6",
        }}
      >
        <span>FINAL POT: {hand.finalPot.toLocaleString()}</span>
        {hand.winner && (
          <span style={{ color: "#27ae60" }}>WINNER: {shortAddr(hand.winner)}</span>
        )}
      </div>
    </div>
  );
}
