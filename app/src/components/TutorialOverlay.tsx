"use client";

import { useEffect, useCallback } from "react";
import type { TutorialStep } from "@/lib/use-tutorial";

/* ── Step definitions ───────────────────────────────────────────────────── */

interface StepContent {
  title: string;
  icon: string;
  body: string[];
  /** Highlight target ID in the DOM (optional, for spotlight effect) */
  highlightId?: string;
  tip?: string;
}

const STEPS: Record<Exclude<TutorialStep, "done">, StepContent> = {
  welcome: {
    title: "WELCOME TO STELLPOKER",
    icon: "🃏",
    body: [
      "StellPoker is an on-chain Texas Hold'em poker game built on the Stellar blockchain.",
      "Every shuffle and deal is provably fair — backed by zero-knowledge (ZK) proofs that anyone can verify.",
      "This quick tour covers the table layout, betting, proof explorer, and wallet connection.",
    ],
    tip: "Press ESC or click outside at any time to dismiss.",
  },
  "table-layout": {
    title: "THE TABLE LAYOUT",
    icon: "🎴",
    body: [
      "OPPONENTS sit at the top of the table. Their cards are hidden — only the cat avatar and chip stack are visible.",
      "COMMUNITY CARDS appear in the centre once dealt. The pot size is shown above them.",
      "YOUR SEAT is at the bottom. Your two hole cards are visible to you only.",
      "The DEALER BUTTON (D) rotates each hand. The player with (D) posts the button blind.",
    ],
    tip: "Hover an opponent's avatar to see their VPIP / PFR / Aggression stats.",
    highlightId: "poker-table",
  },
  "betting-actions": {
    title: "BETTING ACTIONS",
    icon: "💰",
    body: [
      "When it's YOUR TURN the action panel activates below the board.",
      "FOLD — surrender your hand and exit the pot.",
      "CHECK — pass the action without betting (only when no bet is outstanding).",
      "CALL — match the current bet to stay in the hand.",
      "BET / RAISE — increase the stakes using the amount shown or set your own.",
      "ALL IN — put your entire stack into the pot.",
    ],
    tip: "Keyboard shortcuts: F=fold, C=check, B=call/bet, R=raise, A=all-in. Press ? to see all shortcuts.",
    highlightId: "action-panel",
  },
  "chip-tray": {
    title: "CHIP TRAY & BET SIZING",
    icon: "🔵",
    body: [
      "Use the BET SLIDER to choose a precise amount between the minimum and your stack.",
      "Quick preset buttons — 50%, 75%, MAX — let you size common bets with a single click.",
      "The CHIP TRAY below the slider shows four denominations: 25 · 100 · 500 · 1K.",
      "CLICK or DRAG chips into the drop zone to build your bet visually.",
      "Hit CLEAR to reset the selected chips and start over.",
    ],
    tip: "Keys 1–5 set pot-relative sizes: ½ pot, ⅔ pot, ¾ pot, 1× pot, 2× pot.",
  },
  "proof-explorer": {
    title: "ZK PROOF EXPLORER",
    icon: "🔬",
    body: [
      "StellPoker uses zero-knowledge proofs to guarantee a fair shuffle without a trusted dealer.",
      "Every deal, flop, turn, river, and showdown generates an on-chain ZK proof.",
      "The PROOF EXPLORER (inside the GameBoy settings icon) shows whether each proof is VERIFIED or PENDING.",
      "VERIFIED means the Soroban smart contract confirmed the proof on-chain — no one can cheat undetected.",
      "Tap the ⓘ icon beside each phase to learn exactly what that proof guarantees.",
    ],
    tip: "Open the GameBoy icon (⚙) in the header → ZK PROOFS tab to view live proof status.",
    highlightId: "gameboy-btn",
  },
  "wallet-connection": {
    title: "WALLET CONNECTION",
    icon: "🔗",
    body: [
      "StellPoker supports Freighter and Lobstr wallets for the Stellar network.",
      "Install the browser extension, unlock it, then click CONNECT on the home screen.",
      "Your wallet signs every action — deal, bet, reveal, showdown — to prove ownership.",
      "Chips are XLM-denominated. The buy-in amount is set when creating a multiplayer table.",
      "Solo vs AI mode uses fake chips and requires no real XLM, perfect for practice.",
    ],
    tip: "Your session reconnects silently next time if the wallet extension is still unlocked.",
  },
};

const STEP_ORDER: Exclude<TutorialStep, "done">[] = [
  "welcome",
  "table-layout",
  "betting-actions",
  "chip-tray",
  "proof-explorer",
  "wallet-connection",
];

/* ── Progress dots ──────────────────────────────────────────────────────── */

function ProgressDots({
  total,
  current,
  onDotClick,
}: {
  total: number;
  current: number;
  onDotClick: (idx: number) => void;
}) {
  return (
    <div
      role="tablist"
      aria-label="Tutorial progress"
      style={{ display: "flex", gap: "6px", alignItems: "center" }}
    >
      {Array.from({ length: total }).map((_, i) => (
        <button
          key={i}
          role="tab"
          aria-selected={i === current}
          aria-label={`Step ${i + 1}`}
          onClick={() => onDotClick(i)}
          style={{
            width: i === current ? "14px" : "8px",
            height: "8px",
            borderRadius: "4px",
            background: i === current ? "#f1c40f" : i < current ? "#27ae60" : "rgba(255,255,255,0.25)",
            border: "none",
            cursor: "pointer",
            padding: 0,
            transition: "width 0.2s ease, background 0.2s ease",
          }}
        />
      ))}
    </div>
  );
}

/* ── Step card ──────────────────────────────────────────────────────────── */

function StepCard({ content }: { content: StepContent }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
      {/* Icon + title */}
      <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
        <span style={{ fontSize: "28px", lineHeight: 1 }} aria-hidden="true">
          {content.icon}
        </span>
        <h2
          style={{
            fontFamily: "'Press Start 2P', monospace",
            fontSize: "11px",
            color: "#f1c40f",
            textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
            margin: 0,
            lineHeight: 1.4,
          }}
        >
          {content.title}
        </h2>
      </div>

      {/* Body paragraphs */}
      <ul
        style={{
          listStyle: "none",
          margin: 0,
          padding: 0,
          display: "flex",
          flexDirection: "column",
          gap: "8px",
        }}
      >
        {content.body.map((line, i) => {
          // Bold the first word before the em-dash for action labels
          const dashIdx = line.indexOf(" — ");
          if (dashIdx > -1) {
            const label = line.slice(0, dashIdx);
            const rest = line.slice(dashIdx);
            return (
              <li key={i} style={{ display: "flex", gap: "6px", alignItems: "flex-start" }}>
                <span
                  aria-hidden="true"
                  style={{
                    color: "#c47d2e",
                    fontSize: "9px",
                    marginTop: "1px",
                    flexShrink: 0,
                  }}
                >
                  ▶
                </span>
                <span
                  style={{
                    fontFamily: "'Press Start 2P', monospace",
                    fontSize: "8px",
                    color: "#f5e6c8",
                    lineHeight: 1.7,
                  }}
                >
                  <span style={{ color: "#f1c40f" }}>{label}</span>
                  <span>{rest}</span>
                </span>
              </li>
            );
          }
          return (
            <li key={i} style={{ display: "flex", gap: "6px", alignItems: "flex-start" }}>
              <span
                aria-hidden="true"
                style={{
                  color: "#c47d2e",
                  fontSize: "9px",
                  marginTop: "1px",
                  flexShrink: 0,
                }}
              >
                ▶
              </span>
              <span
                style={{
                  fontFamily: "'Press Start 2P', monospace",
                  fontSize: "8px",
                  color: "#f5e6c8",
                  lineHeight: 1.7,
                }}
              >
                {line}
              </span>
            </li>
          );
        })}
      </ul>

      {/* Tip */}
      {content.tip && (
        <div
          style={{
            background: "rgba(241,196,15,0.08)",
            border: "1px solid rgba(241,196,15,0.25)",
            borderRadius: "3px",
            padding: "8px 10px",
          }}
        >
          <span
            style={{
              fontFamily: "'Press Start 2P', monospace",
              fontSize: "7px",
              color: "#f1c40f",
              lineHeight: 1.7,
            }}
          >
            💡 {content.tip}
          </span>
        </div>
      )}
    </div>
  );
}

/* ── "Done" card ────────────────────────────────────────────────────────── */

function DoneCard({ onClose }: { onClose: () => void }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "16px", alignItems: "center", textAlign: "center" }}>
      <span style={{ fontSize: "40px", lineHeight: 1 }}>🎉</span>
      <h2
        style={{
          fontFamily: "'Press Start 2P', monospace",
          fontSize: "11px",
          color: "#f1c40f",
          textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
          margin: 0,
          lineHeight: 1.5,
        }}
      >
        YOU'RE READY TO PLAY!
      </h2>
      <p
        style={{
          fontFamily: "'Press Start 2P', monospace",
          fontSize: "8px",
          color: "#f5e6c8",
          lineHeight: 1.8,
          margin: 0,
        }}
      >
        Connect your Stellar wallet, join or create a table, and let the ZK proofs keep the game honest. Good luck!
      </p>
      <div
        style={{
          background: "rgba(39,174,96,0.1)",
          border: "1px solid rgba(39,174,96,0.3)",
          borderRadius: "3px",
          padding: "8px 10px",
        }}
      >
        <span
          style={{
            fontFamily: "'Press Start 2P', monospace",
            fontSize: "7px",
            color: "#27ae60",
            lineHeight: 1.7,
          }}
        >
          💡 You can reopen this tutorial any time via the{" "}
          <span style={{ color: "#f1c40f" }}>? HELP</span> button in the header.
        </span>
      </div>
      <button
        onClick={onClose}
        className="pixel-btn pixel-btn-green"
        style={{
          fontSize: "11px",
          padding: "10px 24px",
          marginTop: "4px",
        }}
        autoFocus
      >
        LET'S PLAY!
      </button>
    </div>
  );
}

/* ── Main component ─────────────────────────────────────────────────────── */

export interface TutorialOverlayProps {
  isOpen: boolean;
  currentStep: TutorialStep;
  currentIndex: number;
  totalSteps: number;
  isLastStep: boolean;
  isFirstStep: boolean;
  onClose: () => void;
  onNext: () => void;
  onPrev: () => void;
  onGoTo: (step: TutorialStep) => void;
}

export function TutorialOverlay({
  isOpen,
  currentStep,
  currentIndex,
  totalSteps,
  isLastStep,
  isFirstStep,
  onClose,
  onNext,
  onPrev,
  onGoTo,
}: TutorialOverlayProps) {
  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!isOpen) return;
      switch (e.key) {
        case "Escape":
          onClose();
          break;
        case "ArrowRight":
        case "ArrowDown":
          e.preventDefault();
          if (!isLastStep) onNext();
          break;
        case "ArrowLeft":
        case "ArrowUp":
          e.preventDefault();
          if (!isFirstStep) onPrev();
          break;
        case "Enter":
          if (isLastStep) onClose();
          else onNext();
          break;
      }
    },
    [isOpen, isLastStep, isFirstStep, onClose, onNext, onPrev]
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (!isOpen) return null;

  const stepKey = currentStep as Exclude<TutorialStep, "done">;
  const stepContent = currentStep !== "done" ? STEPS[stepKey] : null;
  const displayIndex = Math.min(currentIndex, STEP_ORDER.length - 1);

  return (
    <>
      {/* Backdrop */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Tutorial walkthrough"
        className="fixed inset-0 z-[200] flex items-center justify-center p-4"
        style={{
          background: "rgba(0,0,0,0.72)",
          backdropFilter: "blur(3px)",
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget) onClose();
        }}
      >
        {/* Panel */}
        <div
          className="pixel-border"
          style={{
            background: "rgba(12, 8, 24, 0.98)",
            borderColor: "#c47d2e",
            width: "100%",
            maxWidth: "480px",
            maxHeight: "90vh",
            overflowY: "auto",
            padding: "20px 24px 24px",
            position: "relative",
            animation: "gameboySlideIn 0.28s ease-out",
            boxShadow: "0 0 40px rgba(196, 125, 46, 0.15), 0 8px 24px rgba(0,0,0,0.6)",
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {/* Close button */}
          <button
            onClick={onClose}
            aria-label="Close tutorial"
            style={{
              position: "absolute",
              top: "14px",
              right: "14px",
              background: "none",
              border: "none",
              cursor: "pointer",
              color: "#7f8c8d",
              fontSize: "14px",
              fontFamily: "'Press Start 2P', monospace",
              lineHeight: 1,
              padding: "2px 6px",
              transition: "color 0.15s",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.color = "#e74c3c")}
            onMouseLeave={(e) => (e.currentTarget.style.color = "#7f8c8d")}
          >
            ✕
          </button>

          {/* Step label */}
          {currentStep !== "done" && (
            <div
              style={{
                fontFamily: "'Press Start 2P', monospace",
                fontSize: "7px",
                color: "#7f8c8d",
                marginBottom: "16px",
                letterSpacing: "1px",
              }}
            >
              STEP {displayIndex + 1} OF {STEP_ORDER.length}
            </div>
          )}

          {/* Content */}
          {currentStep === "done" ? (
            <DoneCard onClose={onClose} />
          ) : stepContent ? (
            <StepCard content={stepContent} />
          ) : null}

          {/* Footer: progress dots + nav buttons */}
          {currentStep !== "done" && (
            <div
              style={{
                marginTop: "20px",
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "12px",
              }}
            >
              {/* Progress dots */}
              <ProgressDots
                total={STEP_ORDER.length}
                current={displayIndex}
                onDotClick={(i) => onGoTo(STEP_ORDER[i])}
              />

              {/* Nav buttons */}
              <div style={{ display: "flex", gap: "8px" }}>
                {!isFirstStep && (
                  <button
                    onClick={onPrev}
                    aria-label="Previous step"
                    className="pixel-btn pixel-btn-dark"
                    style={{ fontSize: "9px", padding: "6px 12px" }}
                  >
                    ← BACK
                  </button>
                )}

                {isFirstStep && (
                  <button
                    onClick={onClose}
                    aria-label="Skip tutorial"
                    style={{
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      fontFamily: "'Press Start 2P', monospace",
                      fontSize: "8px",
                      color: "#7f8c8d",
                      padding: "6px 0",
                      textDecoration: "underline",
                    }}
                  >
                    SKIP
                  </button>
                )}

                <button
                  onClick={onNext}
                  aria-label="Next step"
                  className="pixel-btn pixel-btn-green"
                  style={{ fontSize: "9px", padding: "6px 14px" }}
                  autoFocus={isFirstStep}
                >
                  NEXT →
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </>
  );
}

/* ── Help trigger icon ──────────────────────────────────────────────────── */

/**
 * Small "?" button that reopens the tutorial from anywhere in the header.
 */
export function TutorialHelpButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      aria-label="Open tutorial walkthrough"
      title="Help / Tutorial"
      style={{
        background: "rgba(196,125,46,0.15)",
        border: "2px solid #c47d2e",
        borderRadius: "3px",
        cursor: "pointer",
        fontFamily: "'Press Start 2P', monospace",
        fontSize: "9px",
        color: "#ffc078",
        padding: "3px 7px",
        lineHeight: 1,
        transition: "background 0.15s, color 0.15s",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "rgba(196,125,46,0.30)";
        e.currentTarget.style.color = "#f1c40f";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "rgba(196,125,46,0.15)";
        e.currentTarget.style.color = "#ffc078";
      }}
    >
      ? HELP
    </button>
  );
}
