"use client";

import {
  useState,
  useEffect,
  useCallback,
  useRef,
} from "react";
import { Card } from "./Card";
import { PotChipPile } from "./PixelChip";
import { stellarExpertUrl } from "@/lib/explorer";
import { buildReplayFrames } from "@/lib/hand-history";
import type { HandHistoryEntry, ReplayFrame } from "@/lib/hand-history";

// ── Constants ──────────────────────────────────────────────────────────────

/** Milliseconds between frames during auto-play. */
const AUTO_PLAY_INTERVAL_MS = 2200;

const STREET_COLORS: Record<string, string> = {
  "PRE-FLOP": "#3498db",
  FLOP: "#27ae60",
  TURN: "#f39c12",
  RIVER: "#e74c3c",
  SHOWDOWN: "#f1c40f",
};

// ── Helpers ────────────────────────────────────────────────────────────────

function shortAddress(address: string): string {
  if (address.length < 12) return address;
  return `${address.slice(0, 6)}...${address.slice(-6)}`;
}

// ── Progress bar ───────────────────────────────────────────────────────────

interface ProgressBarProps {
  current: number;
  total: number;
  onSeek: (index: number) => void;
}

function ProgressBar({ current, total, onSeek }: ProgressBarProps) {
  const pct = total <= 1 ? 100 : (current / (total - 1)) * 100;

  // Click-to-seek on the track
  const trackRef = useRef<HTMLDivElement>(null);
  const handleTrackClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!trackRef.current || total <= 1) return;
    const rect = trackRef.current.getBoundingClientRect();
    const ratio = (e.clientX - rect.left) / rect.width;
    const idx = Math.round(ratio * (total - 1));
    onSeek(Math.max(0, Math.min(total - 1, idx)));
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "4px", width: "100%" }}>
      {/* Track */}
      <div
        ref={trackRef}
        onClick={handleTrackClick}
        role="slider"
        aria-label="Replay progress"
        aria-valuenow={current}
        aria-valuemin={0}
        aria-valuemax={total - 1}
        style={{
          width: "100%",
          height: "10px",
          background: "rgba(255,255,255,0.08)",
          border: "2px solid rgba(139,105,20,0.5)",
          cursor: "pointer",
          position: "relative",
          boxSizing: "border-box",
        }}
      >
        {/* Fill */}
        <div
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            height: "100%",
            width: `${pct}%`,
            background: "linear-gradient(90deg, #c47d2e 0%, #f1c40f 100%)",
            transition: "width 0.25s ease",
          }}
        />
        {/* Scrubber thumb */}
        <div
          style={{
            position: "absolute",
            top: "50%",
            left: `${pct}%`,
            transform: "translate(-50%, -50%)",
            width: "14px",
            height: "14px",
            background: "#f1c40f",
            border: "2px solid #b7950b",
            boxSizing: "border-box",
            pointerEvents: "none",
          }}
        />
      </div>

      {/* Step ticks + labels */}
      <div style={{ display: "flex", justifyContent: "space-between", position: "relative" }}>
        {Array.from({ length: total }).map((_, i) => {
          const tickPct = total <= 1 ? 50 : (i / (total - 1)) * 100;
          return (
            <button
              key={i}
              onClick={() => onSeek(i)}
              aria-label={`Jump to step ${i + 1}`}
              style={{
                position: "absolute",
                left: `${tickPct}%`,
                transform: "translateX(-50%)",
                background: "none",
                border: "none",
                cursor: "pointer",
                padding: 0,
                top: 0,
              }}
            >
              <div
                style={{
                  width: "6px",
                  height: "6px",
                  background: i <= current ? "#f1c40f" : "rgba(255,255,255,0.25)",
                  border: i === current ? "2px solid #f5e6c8" : "1px solid rgba(255,255,255,0.15)",
                  transition: "background 0.2s",
                }}
              />
            </button>
          );
        })}
      </div>
    </div>
  );
}

// ── Board display ──────────────────────────────────────────────────────────

interface ReplayBoardProps {
  boardCards: number[];
  previousCount: number;
}

function ReplayBoard({ boardCards, previousCount }: ReplayBoardProps) {
  return (
    <div
      style={{
        display: "flex",
        gap: "6px",
        alignItems: "center",
        justifyContent: "center",
        flexWrap: "wrap",
        minHeight: "88px",
      }}
    >
      {boardCards.map((card, i) => (
        <Card
          key={i}
          value={card}
          size="md"
          flip={i >= previousCount}
          flipDelay={Math.max(0, i - previousCount) * 0.1}
        />
      ))}
      {/* Empty slots */}
      {Array.from({ length: 5 - boardCards.length }).map((_, i) => (
        <div
          key={`empty-${i}`}
          style={{
            width: "56px",
            height: "80px",
            border: "3px dashed rgba(139,105,20,0.25)",
            background: "rgba(0,0,0,0.1)",
          }}
        />
      ))}
    </div>
  );
}

// ── Frame display ──────────────────────────────────────────────────────────

interface FrameDisplayProps {
  frame: ReplayFrame;
  previousBoardCount: number;
  frameIndex: number;
  totalFrames: number;
}

function FrameDisplay({ frame, previousBoardCount, frameIndex, totalFrames }: FrameDisplayProps) {
  const streetColor = STREET_COLORS[frame.label] ?? "#f5e6c8";

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "12px", alignItems: "center" }}>

      {/* Street badge + frame counter */}
      <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
        <div
          style={{
            fontFamily: "'Press Start 2P', monospace",
            fontSize: "11px",
            color: streetColor,
            textShadow: `0 0 8px ${streetColor}60`,
            letterSpacing: "1px",
          }}
        >
          {frame.label}
        </div>
        <div
          style={{
            fontFamily: "'Press Start 2P', monospace",
            fontSize: "7px",
            color: "#7f8c8d",
          }}
        >
          {frameIndex + 1} / {totalFrames}
        </div>
      </div>

      {/* Pot */}
      <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
        <PotChipPile amount={frame.pot} size={2} />
        <span
          style={{
            fontFamily: "'Press Start 2P', monospace",
            fontSize: "10px",
            color: "#f1c40f",
            textShadow: "1px 1px 0 rgba(0,0,0,0.5)",
          }}
        >
          POT: {frame.pot.toLocaleString()}
        </span>
      </div>

      {/* Board */}
      <ReplayBoard boardCards={frame.boardCards} previousCount={previousBoardCount} />

      {/* Hole cards */}
      {frame.holeCards && (
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "6px" }}>
          <div
            style={{
              fontFamily: "'Press Start 2P', monospace",
              fontSize: "7px",
              color: "#95a5a6",
              letterSpacing: "1px",
            }}
          >
            YOUR HOLE CARDS
          </div>
          <div style={{ display: "flex", gap: "6px" }}>
            <Card value={frame.holeCards[0]} size="md" />
            <Card value={frame.holeCards[1]} size="md" />
          </div>
          {frame.handRankName && (
            <div
              style={{
                fontFamily: "'Press Start 2P', monospace",
                fontSize: "8px",
                color: "#27ae60",
                background: "rgba(39,174,96,0.1)",
                border: "1px solid rgba(39,174,96,0.3)",
                padding: "3px 8px",
              }}
            >
              {frame.handRankName.toUpperCase()}
            </div>
          )}
        </div>
      )}

      {/* Settlement result */}
      {frame.isSettlement && (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: "6px",
            background: "rgba(241,196,15,0.06)",
            border: "1px solid rgba(241,196,15,0.2)",
            padding: "10px 16px",
            width: "100%",
          }}
        >
          <div
            style={{
              fontFamily: "'Press Start 2P', monospace",
              fontSize: "9px",
              color: "#f1c40f",
            }}
          >
            🏆 HAND COMPLETE
          </div>
          {frame.winnerAddress && (
            <div
              style={{
                fontFamily: "'Press Start 2P', monospace",
                fontSize: "7px",
                color: "#27ae60",
              }}
            >
              WINNER: {shortAddress(frame.winnerAddress)}
            </div>
          )}
          <div
            style={{
              fontFamily: "'Press Start 2P', monospace",
              fontSize: "7px",
              color: "#f5e6c8",
            }}
          >
            FINAL POT: {frame.pot.toLocaleString()}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Control buttons ────────────────────────────────────────────────────────

interface ControlsProps {
  onPrev: () => void;
  onNext: () => void;
  onToggleAutoPlay: () => void;
  onRestart: () => void;
  isPlaying: boolean;
  canPrev: boolean;
  canNext: boolean;
  speed: number;
  onSpeedChange: (speed: number) => void;
}

function Controls({
  onPrev,
  onNext,
  onToggleAutoPlay,
  onRestart,
  isPlaying,
  canPrev,
  canNext,
  speed,
  onSpeedChange,
}: ControlsProps) {
  const btnBase: React.CSSProperties = {
    fontFamily: "'Press Start 2P', monospace",
    border: "3px solid",
    cursor: "pointer",
    fontSize: "10px",
    padding: "7px 12px",
    transition: "transform 0.05s",
    userSelect: "none",
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "8px", alignItems: "center", width: "100%" }}>
      {/* Main transport row */}
      <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
        {/* Restart */}
        <button
          onClick={onRestart}
          aria-label="Restart replay"
          title="Restart (R)"
          style={{
            ...btnBase,
            background: "#2c3e50",
            borderColor: "#4a6a8a",
            color: "#c8e6ff",
          }}
          onMouseEnter={(e) => (e.currentTarget.style.transform = "scale(1.05)")}
          onMouseLeave={(e) => (e.currentTarget.style.transform = "scale(1)")}
        >
          ⏮
        </button>

        {/* Prev */}
        <button
          onClick={onPrev}
          disabled={!canPrev}
          aria-label="Previous step"
          title="Previous (← / J)"
          style={{
            ...btnBase,
            background: canPrev ? "#1a5276" : "#2c3e50",
            borderColor: canPrev ? "#2980b9" : "#4a6a8a",
            color: canPrev ? "white" : "#4a6a8a",
            opacity: canPrev ? 1 : 0.5,
          }}
          onMouseEnter={(e) => canPrev && (e.currentTarget.style.transform = "scale(1.05)")}
          onMouseLeave={(e) => (e.currentTarget.style.transform = "scale(1)")}
        >
          ◀ PREV
        </button>

        {/* Play / Pause */}
        <button
          onClick={onToggleAutoPlay}
          disabled={!canNext && !isPlaying}
          aria-label={isPlaying ? "Pause auto-play" : "Start auto-play"}
          title={isPlaying ? "Pause (Space)" : "Auto-play (Space)"}
          style={{
            ...btnBase,
            background: isPlaying ? "#6c1f0f" : "#145a32",
            borderColor: isPlaying ? "#c0392b" : "#1e8449",
            color: "white",
            padding: "7px 16px",
            minWidth: "80px",
            opacity: !canNext && !isPlaying ? 0.4 : 1,
          }}
          onMouseEnter={(e) => (e.currentTarget.style.transform = "scale(1.05)")}
          onMouseLeave={(e) => (e.currentTarget.style.transform = "scale(1)")}
        >
          {isPlaying ? "⏸ PAUSE" : "▶ PLAY"}
        </button>

        {/* Next */}
        <button
          onClick={onNext}
          disabled={!canNext}
          aria-label="Next step"
          title="Next (→ / L)"
          style={{
            ...btnBase,
            background: canNext ? "#1a5276" : "#2c3e50",
            borderColor: canNext ? "#2980b9" : "#4a6a8a",
            color: canNext ? "white" : "#4a6a8a",
            opacity: canNext ? 1 : 0.5,
          }}
          onMouseEnter={(e) => canNext && (e.currentTarget.style.transform = "scale(1.05)")}
          onMouseLeave={(e) => (e.currentTarget.style.transform = "scale(1)")}
        >
          NEXT ▶
        </button>
      </div>

      {/* Speed selector */}
      <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
        <span
          style={{
            fontFamily: "'Press Start 2P', monospace",
            fontSize: "7px",
            color: "#7f8c8d",
          }}
        >
          SPEED:
        </span>
        {[
          { label: "0.5×", value: 0.5 },
          { label: "1×", value: 1 },
          { label: "2×", value: 2 },
        ].map((opt) => (
          <button
            key={opt.value}
            onClick={() => onSpeedChange(opt.value)}
            aria-pressed={speed === opt.value}
            style={{
              fontFamily: "'Press Start 2P', monospace",
              fontSize: "7px",
              padding: "3px 7px",
              background: speed === opt.value ? "rgba(241,196,15,0.18)" : "rgba(255,255,255,0.04)",
              border: speed === opt.value ? "1px solid #f1c40f" : "1px solid rgba(255,255,255,0.1)",
              color: speed === opt.value ? "#f1c40f" : "#7f8c8d",
              cursor: "pointer",
            }}
          >
            {opt.label}
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Main HandReplayer component ────────────────────────────────────────────

export interface HandReplayerProps {
  entry: HandHistoryEntry | null;
  onClose: () => void;
}

export function HandReplayer({ entry, onClose }: HandReplayerProps) {
  const [frameIndex, setFrameIndex] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const autoPlayRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Build frames when entry changes
  const frames: ReplayFrame[] = entry ? buildReplayFrames(entry) : [];
  const totalFrames = frames.length;
  const currentFrame = frames[frameIndex] ?? null;
  const previousFrame = frameIndex > 0 ? frames[frameIndex - 1] : null;
  const previousBoardCount = previousFrame?.boardCards.length ?? 0;

  // Reset state when a new entry is loaded
  useEffect(() => {
    setFrameIndex(0);
    setIsPlaying(false);
  }, [entry]);

  // Auto-play ticker
  const stopAutoPlay = useCallback(() => {
    if (autoPlayRef.current !== null) {
      clearInterval(autoPlayRef.current);
      autoPlayRef.current = null;
    }
    setIsPlaying(false);
  }, []);

  const startAutoPlay = useCallback(() => {
    if (autoPlayRef.current !== null) clearInterval(autoPlayRef.current);
    const intervalMs = AUTO_PLAY_INTERVAL_MS / speed;
    autoPlayRef.current = setInterval(() => {
      setFrameIndex((prev) => {
        if (prev >= totalFrames - 1) {
          stopAutoPlay();
          return prev;
        }
        return prev + 1;
      });
    }, intervalMs);
    setIsPlaying(true);
  }, [speed, totalFrames, stopAutoPlay]);

  const toggleAutoPlay = useCallback(() => {
    if (isPlaying) {
      stopAutoPlay();
    } else {
      if (frameIndex >= totalFrames - 1) {
        setFrameIndex(0);
      }
      startAutoPlay();
    }
  }, [isPlaying, frameIndex, totalFrames, stopAutoPlay, startAutoPlay]);

  // Restart auto-play when speed changes while playing
  useEffect(() => {
    if (isPlaying) {
      stopAutoPlay();
      startAutoPlay();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [speed]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (autoPlayRef.current !== null) clearInterval(autoPlayRef.current);
    };
  }, []);

  // Keyboard controls
  useEffect(() => {
    if (!entry) return;
    const handler = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;

      switch (e.key) {
        case "Escape":
          onClose();
          break;
        case "ArrowLeft":
        case "j":
        case "J":
          e.preventDefault();
          stopAutoPlay();
          setFrameIndex((prev) => Math.max(0, prev - 1));
          break;
        case "ArrowRight":
        case "l":
        case "L":
          e.preventDefault();
          stopAutoPlay();
          setFrameIndex((prev) => Math.min(totalFrames - 1, prev + 1));
          break;
        case " ":
          e.preventDefault();
          toggleAutoPlay();
          break;
        case "r":
        case "R":
          e.preventDefault();
          stopAutoPlay();
          setFrameIndex(0);
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [entry, totalFrames, stopAutoPlay, toggleAutoPlay, onClose]);

  if (!entry) return null;

  const canPrev = frameIndex > 0;
  const canNext = frameIndex < totalFrames - 1;

  const handlePrev = () => {
    stopAutoPlay();
    setFrameIndex((i) => Math.max(0, i - 1));
  };

  const handleNext = () => {
    stopAutoPlay();
    setFrameIndex((i) => Math.min(totalFrames - 1, i + 1));
  };

  const handleSeek = (idx: number) => {
    stopAutoPlay();
    setFrameIndex(idx);
  };

  const handleRestart = () => {
    stopAutoPlay();
    setFrameIndex(0);
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`Hand #${entry.handNumber} replayer`}
      className="fixed inset-0 z-[150] flex items-center justify-center p-4"
      style={{
        background: "rgba(0,0,0,0.75)",
        backdropFilter: "blur(3px)",
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="pixel-border"
        style={{
          background: "rgba(8, 6, 20, 0.98)",
          borderColor: "#c47d2e",
          width: "100%",
          maxWidth: "520px",
          maxHeight: "92vh",
          overflowY: "auto",
          padding: "0",
          display: "flex",
          flexDirection: "column",
          boxShadow: "0 0 40px rgba(196,125,46,0.2), 0 8px 32px rgba(0,0,0,0.7)",
          animation: "gameboySlideIn 0.25s ease-out",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* ── Header ── */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "12px 16px 10px",
            borderBottom: "2px solid rgba(196,125,46,0.3)",
            background: "rgba(196,125,46,0.06)",
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: "3px" }}>
            <span
              style={{
                fontFamily: "'Press Start 2P', monospace",
                fontSize: "11px",
                color: "#f1c40f",
                textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
              }}
            >
              ▶ REPLAY — HAND #{entry.handNumber}
            </span>
            <span
              style={{
                fontFamily: "'Press Start 2P', monospace",
                fontSize: "7px",
                color: "#7f8c8d",
              }}
            >
              {new Date(entry.timestamp).toLocaleString()} · TABLE #{entry.tableId}
            </span>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            {entry.txHash && (
              <a
                href={stellarExpertUrl("tx", entry.txHash)}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  fontFamily: "'Press Start 2P', monospace",
                  fontSize: "7px",
                  color: "#ffc078",
                  textDecoration: "none",
                }}
              >
                TX ↗
              </a>
            )}
            <button
              onClick={onClose}
              aria-label="Close replayer"
              style={{
                background: "none",
                border: "none",
                cursor: "pointer",
                fontFamily: "'Press Start 2P', monospace",
                fontSize: "14px",
                color: "#7f8c8d",
                lineHeight: 1,
                padding: "2px 4px",
                transition: "color 0.15s",
              }}
              onMouseEnter={(e) => (e.currentTarget.style.color = "#e74c3c")}
              onMouseLeave={(e) => (e.currentTarget.style.color = "#7f8c8d")}
            >
              ✕
            </button>
          </div>
        </div>

        {/* ── No frames guard ── */}
        {totalFrames === 0 && (
          <div
            style={{
              padding: "40px 20px",
              textAlign: "center",
              fontFamily: "'Press Start 2P', monospace",
              fontSize: "9px",
              color: "#7f8c8d",
            }}
          >
            NO REPLAY DATA AVAILABLE FOR THIS HAND.
          </div>
        )}

        {/* ── Frame display ── */}
        {totalFrames > 0 && currentFrame && (
          <>
            {/* Felt area */}
            <div
              style={{
                background:
                  "radial-gradient(ellipse at center, var(--felt-light) 0%, var(--felt-mid) 45%, var(--felt-dark) 100%)",
                padding: "20px 16px",
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                gap: "4px",
                minHeight: "280px",
                position: "relative",
              }}
            >
              {/* Inner rail */}
              <div
                style={{
                  position: "absolute",
                  inset: "6px",
                  border: "2px solid rgba(139,105,20,0.2)",
                  pointerEvents: "none",
                }}
              />
              <FrameDisplay
                frame={currentFrame}
                previousBoardCount={previousBoardCount}
                frameIndex={frameIndex}
                totalFrames={totalFrames}
              />
            </div>

            {/* ── Progress + controls ── */}
            <div
              style={{
                padding: "14px 16px 16px",
                display: "flex",
                flexDirection: "column",
                gap: "12px",
                borderTop: "2px solid rgba(196,125,46,0.2)",
                background: "rgba(0,0,0,0.25)",
              }}
            >
              <ProgressBar
                current={frameIndex}
                total={totalFrames}
                onSeek={handleSeek}
              />

              <Controls
                onPrev={handlePrev}
                onNext={handleNext}
                onToggleAutoPlay={toggleAutoPlay}
                onRestart={handleRestart}
                isPlaying={isPlaying}
                canPrev={canPrev}
                canNext={canNext}
                speed={speed}
                onSpeedChange={(s) => setSpeed(s)}
              />

              {/* Keyboard hint */}
              <div
                style={{
                  fontFamily: "'Press Start 2P', monospace",
                  fontSize: "6px",
                  color: "#4a4a6a",
                  textAlign: "center",
                  lineHeight: 1.8,
                }}
              >
                ← / → STEP &nbsp;·&nbsp; SPACE PLAY/PAUSE &nbsp;·&nbsp; R RESTART &nbsp;·&nbsp; ESC CLOSE
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
