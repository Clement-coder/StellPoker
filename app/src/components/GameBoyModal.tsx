"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { useRouter } from "next/navigation";
import { getMasterVolume, setMasterVolume } from "./PixelWorld";
import { clearSavedWallet } from "@/lib/freighter";
import { ProofExplorer } from "./ProofExplorer";
import { LanguageSelector } from "./LanguageSelector";
import { useT } from "@/lib/i18n/context";
import { getSfxMuted, setSfxMuted, getSfxVolume, setSfxVolume, playSound } from "@/lib/sound-engine";

type Tab = "settings" | "flappy" | "proofs";

/* ─────────────────────────────────────────────
   FLAPPY BIRD ENGINE
   ───────────────────────────────────────────── */

interface Pipe {
  x: number;
  gapY: number;
  scored: boolean;
}

interface FlappyState {
  birdY: number;
  birdV: number;
  pipes: Pipe[];
  score: number;
  alive: boolean;
  started: boolean;
  frame: number;
}

const FB = {
  W: 160,
  H: 144, // Game Boy resolution
  GRAVITY: 0.35,
  JUMP: -3.5,
  BIRD_X: 30,
  BIRD_SIZE: 8,
  PIPE_W: 16,
  GAP: 40,
  PIPE_SPEED: 1.2,
  SPAWN_INTERVAL: 60,
} as const;

function initFlappy(): FlappyState {
  return {
    birdY: FB.H / 2,
    birdV: 0,
    pipes: [],
    score: 0,
    alive: true,
    started: false,
    frame: 0,
  };
}

function tickFlappy(state: FlappyState): FlappyState {
  if (!state.alive || !state.started) return state;
  const next = { ...state, frame: state.frame + 1 };

  // Bird physics
  next.birdV = state.birdV + FB.GRAVITY;
  next.birdY = state.birdY + next.birdV;

  // Spawn pipes
  let pipes = [...state.pipes];
  if (next.frame % FB.SPAWN_INTERVAL === 0) {
    const gapY = 28 + Math.random() * (FB.H - 28 - FB.GAP - 20);
    pipes.push({ x: FB.W, gapY, scored: false });
  }

  // Move pipes & score
  pipes = pipes
    .map((p) => {
      const moved = { ...p, x: p.x - FB.PIPE_SPEED };
      if (!moved.scored && moved.x + FB.PIPE_W < FB.BIRD_X) {
        moved.scored = true;
        next.score++;
      }
      return moved;
    })
    .filter((p) => p.x > -FB.PIPE_W);

  next.pipes = pipes;

  // Collision
  const bTop = next.birdY;
  const bBot = next.birdY + FB.BIRD_SIZE;
  const bLeft = FB.BIRD_X;
  const bRight = FB.BIRD_X + FB.BIRD_SIZE;

  if (bTop < 0 || bBot > FB.H) {
    next.alive = false;
  }

  for (const p of pipes) {
    if (bRight > p.x && bLeft < p.x + FB.PIPE_W) {
      if (bTop < p.gapY || bBot > p.gapY + FB.GAP) {
        next.alive = false;
      }
    }
  }

  return next;
}

function drawFlappy(ctx: CanvasRenderingContext2D, state: FlappyState) {
  const { W, H } = FB;
  // BG — bright sky blue
  ctx.fillStyle = "#4ec0ca";
  ctx.fillRect(0, 0, W, H);

  // Ground
  ctx.fillStyle = "#8b6914";
  ctx.fillRect(0, H - 8, W, 8);
  // Ground detail (sandy)
  ctx.fillStyle = "#c4a24c";
  for (let x = 0; x < W; x += 4) {
    ctx.fillRect(x, H - 8, 2, 2);
  }
  // Grass strip on top of ground
  ctx.fillStyle = "#4cae4c";
  ctx.fillRect(0, H - 10, W, 2);

  // Pipes (green)
  for (const p of state.pipes) {
    // Top pipe body
    ctx.fillStyle = "#27ae60";
    ctx.fillRect(p.x, 0, FB.PIPE_W, p.gapY);
    // Top pipe cap
    ctx.fillStyle = "#1e8449";
    ctx.fillRect(p.x - 2, p.gapY - 4, FB.PIPE_W + 4, 4);
    // Bottom pipe body
    ctx.fillStyle = "#27ae60";
    ctx.fillRect(p.x, p.gapY + FB.GAP, FB.PIPE_W, H - (p.gapY + FB.GAP));
    // Bottom pipe cap
    ctx.fillStyle = "#1e8449";
    ctx.fillRect(p.x - 2, p.gapY + FB.GAP, FB.PIPE_W + 4, 4);
    // Pipe highlights (lighter green)
    ctx.fillStyle = "#58d68d";
    ctx.fillRect(p.x + 2, 0, 2, p.gapY - 4);
    ctx.fillRect(p.x + 2, p.gapY + FB.GAP + 4, 2, H - (p.gapY + FB.GAP + 4));
  }

  // Bird
  const bx = FB.BIRD_X;
  const by = Math.round(state.birdY);
  // Body outline
  ctx.fillStyle = "#784212";
  ctx.fillRect(bx, by, FB.BIRD_SIZE, FB.BIRD_SIZE);
  // Inner body (yellow)
  ctx.fillStyle = "#f1c40f";
  ctx.fillRect(bx + 1, by + 1, FB.BIRD_SIZE - 2, FB.BIRD_SIZE - 2);
  // Belly (lighter)
  ctx.fillStyle = "#f9e547";
  ctx.fillRect(bx + 1, by + 4, FB.BIRD_SIZE - 2, 2);
  // Eye (white + pupil)
  ctx.fillStyle = "#fff";
  ctx.fillRect(bx + 5, by + 1, 2, 2);
  ctx.fillStyle = "#1a1a1a";
  ctx.fillRect(bx + 6, by + 1, 1, 1);
  // Wing
  const wingOff = state.frame % 6 < 3 ? 0 : 1;
  ctx.fillStyle = "#e67e22";
  ctx.fillRect(bx + 1, by + 3 + wingOff, 3, 2);
  // Beak (red-orange)
  ctx.fillStyle = "#e74c3c";
  ctx.fillRect(bx + FB.BIRD_SIZE, by + 3, 2, 2);

  // Score (white with shadow)
  ctx.fillStyle = "rgba(0,0,0,0.3)";
  ctx.font = "8px monospace";
  ctx.textAlign = "center";
  ctx.fillText(String(state.score), W / 2 + 1, 15);
  ctx.fillStyle = "#fff";
  ctx.fillText(String(state.score), W / 2, 14);

  // Start / Game Over
  if (!state.started) {
    ctx.fillStyle = "#fff";
    ctx.font = "7px monospace";
    ctx.textAlign = "center";
    ctx.fillText("PRESS A TO START", W / 2, H / 2 - 8);
    ctx.fillText("A = FLAP", W / 2, H / 2 + 8);
  } else if (!state.alive) {
    ctx.fillStyle = "#fff";
    ctx.font = "8px monospace";
    ctx.textAlign = "center";
    ctx.fillText("GAME OVER", W / 2, H / 2 - 12);
    ctx.fillText(`SCORE: ${state.score}`, W / 2, H / 2);
    ctx.font = "6px monospace";
    ctx.fillText("PRESS A TO RETRY", W / 2, H / 2 + 14);
  }
}

/* ─────────────────────────────────────────────
   GAME BOY ICON (pixel art via box-shadow)
   ───────────────────────────────────────────── */

function GameBoyIcon({ size = 3 }: { size?: number }) {
  const px = size;
  // 8×12 pixel Game Boy shape
  const pixels: [number, number, string][] = [
    // Shell outline (light grey)
    [1,0,"#b0b0b0"],[2,0,"#b0b0b0"],[3,0,"#b0b0b0"],[4,0,"#b0b0b0"],[5,0,"#b0b0b0"],[6,0,"#b0b0b0"],
    [0,1,"#b0b0b0"],[1,1,"#c8c8c8"],[2,1,"#c8c8c8"],[3,1,"#c8c8c8"],[4,1,"#c8c8c8"],[5,1,"#c8c8c8"],[6,1,"#c8c8c8"],[7,1,"#b0b0b0"],
    // Screen area (dark green)
    [0,2,"#b0b0b0"],[1,2,"#2a2a2a"],[2,2,"#2a2a2a"],[3,2,"#2a2a2a"],[4,2,"#2a2a2a"],[5,2,"#2a2a2a"],[6,2,"#2a2a2a"],[7,2,"#b0b0b0"],
    [0,3,"#b0b0b0"],[1,3,"#2a2a2a"],[2,3,"#6b7a60"],[3,3,"#b8c4a0"],[4,3,"#b8c4a0"],[5,3,"#6b7a60"],[6,3,"#2a2a2a"],[7,3,"#b0b0b0"],
    [0,4,"#b0b0b0"],[1,4,"#2a2a2a"],[2,4,"#6b7a60"],[3,4,"#b8c4a0"],[4,4,"#b8c4a0"],[5,4,"#6b7a60"],[6,4,"#2a2a2a"],[7,4,"#b0b0b0"],
    [0,5,"#b0b0b0"],[1,5,"#2a2a2a"],[2,5,"#2a2a2a"],[3,5,"#2a2a2a"],[4,5,"#2a2a2a"],[5,5,"#2a2a2a"],[6,5,"#2a2a2a"],[7,5,"#b0b0b0"],
    // Body
    [0,6,"#b0b0b0"],[1,6,"#c8c8c8"],[2,6,"#c8c8c8"],[3,6,"#c8c8c8"],[4,6,"#c8c8c8"],[5,6,"#c8c8c8"],[6,6,"#c8c8c8"],[7,6,"#b0b0b0"],
    // D-pad & buttons
    [0,7,"#b0b0b0"],[1,7,"#c8c8c8"],[2,7,"#555"],[3,7,"#c8c8c8"],[4,7,"#c8c8c8"],[5,7,"#8b2252"],[6,7,"#c8c8c8"],[7,7,"#b0b0b0"],
    [0,8,"#b0b0b0"],[1,8,"#555"],[2,8,"#555"],[3,8,"#555"],[4,8,"#c8c8c8"],[5,8,"#c8c8c8"],[6,8,"#8b2252"],[7,8,"#b0b0b0"],
    [0,9,"#b0b0b0"],[1,9,"#c8c8c8"],[2,9,"#555"],[3,9,"#c8c8c8"],[4,9,"#c8c8c8"],[5,9,"#c8c8c8"],[6,9,"#c8c8c8"],[7,9,"#b0b0b0"],
    // Bottom
    [0,10,"#b0b0b0"],[1,10,"#c8c8c8"],[2,10,"#c8c8c8"],[3,10,"#888"],[4,10,"#888"],[5,10,"#c8c8c8"],[6,10,"#c8c8c8"],[7,10,"#b0b0b0"],
    [1,11,"#b0b0b0"],[2,11,"#b0b0b0"],[3,11,"#b0b0b0"],[4,11,"#b0b0b0"],[5,11,"#b0b0b0"],[6,11,"#b0b0b0"],
  ];

  const shadows = pixels.map(([x, y, c]) => `${x * px}px ${y * px}px 0 ${c}`).join(", ");

  return (
    <div style={{
      width: `${8 * px}px`,
      height: `${12 * px}px`,
      position: "relative",
    }}>
      <div style={{
        position: "absolute",
        width: `${px}px`,
        height: `${px}px`,
        boxShadow: shadows,
      }} />
    </div>
  );
}

/* ─────────────────────────────────────────────
   GAME BOY MODAL
   ───────────────────────────────────────────── */

interface GameBoyModalProps {
  open: boolean;
  onClose: () => void;
  onLogout: () => void;
}

export function GameBoyButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      title="Settings"
      style={{
        background: "none",
        border: "none",
        cursor: "pointer",
        padding: "4px",
        transition: "transform 0.15s ease",
      }}
      onMouseEnter={(e) => (e.currentTarget.style.transform = "scale(1.15)")}
      onMouseLeave={(e) => (e.currentTarget.style.transform = "scale(1)")}
    >
      <GameBoyIcon size={3} />
    </button>
  );
}

export function GameBoyModal({ open, onClose, onLogout }: GameBoyModalProps) {
  const router = useRouter();
  const t = useT();
  const [tab, setTab] = useState<Tab>("settings");
  const [volume, setVolume] = useState(() => Math.round(getMasterVolume() * 100));
  const [sfxMuted, setSfxMutedState] = useState(() => getSfxMuted());
  const [sfxVolume, setSfxVolumeState] = useState(() => Math.round(getSfxVolume() * 100));
  const [flappy, setFlappy] = useState<FlappyState>(initFlappy);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const loopRef = useRef<number>(0);
  const flappyRef = useRef(flappy);
  flappyRef.current = flappy;

  // Sync volume slider to shared master volume
  useEffect(() => {
    setMasterVolume(volume / 100);
  }, [volume]);

  // Sync SFX settings to sound engine
  useEffect(() => {
    setSfxMuted(sfxMuted);
  }, [sfxMuted]);

  useEffect(() => {
    setSfxVolume(sfxVolume / 100);
  }, [sfxVolume]);

  // Flappy Bird game loop
  useEffect(() => {
    if (!open || tab !== "flappy") {
      cancelAnimationFrame(loopRef.current);
      return;
    }
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.imageSmoothingEnabled = false;

    let lastTime = 0;
    const targetMs = 1000 / 30; // 30 FPS, retro feel

    const loop = (timestamp: number) => {
      const delta = timestamp - lastTime;
      if (delta >= targetMs) {
        lastTime = timestamp;
        const next = tickFlappy(flappyRef.current);
        flappyRef.current = next;
        setFlappy(next);
        drawFlappy(ctx, next);
      }
      loopRef.current = requestAnimationFrame(loop);
    };

    // Initial draw
    drawFlappy(ctx, flappyRef.current);
    loopRef.current = requestAnimationFrame(loop);

    return () => cancelAnimationFrame(loopRef.current);
  }, [open, tab]);

  const handleFlap = useCallback(() => {
    setFlappy((prev) => {
      if (!prev.alive && prev.started) {
        // Restart
        return { ...initFlappy(), started: true };
      }
      if (!prev.started) {
        return { ...prev, started: true, birdV: FB.JUMP };
      }
      return { ...prev, birdV: FB.JUMP };
    });
  }, []);

  const handleLogout = () => {
    clearSavedWallet();
    onLogout();
    onClose();
    router.push("/");
  };

  // Keyboard controls for flappy
  useEffect(() => {
    if (!open) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.code === "Space" || e.code === "ArrowUp" || e.key === "a" || e.key === "A") {
        e.preventDefault();
        if (tab === "flappy") handleFlap();
      }
      if (e.code === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, tab, handleFlap, onClose]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)", backdropFilter: "blur(2px)" }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      {/* GAME BOY SHELL */}
      <div
        style={{
          background: "linear-gradient(160deg, #d0ccd8 0%, #b8b4c4 40%, #a8a4b4 100%)",
          borderRadius: "14px 14px 14px 70px",
          padding: "20px 18px 24px",
          width: "380px",
          boxShadow:
            "inset 2px 2px 0 rgba(255,255,255,0.4), inset -2px -2px 0 rgba(0,0,0,0.2), 0 8px 24px rgba(0,0,0,0.5), 0 2px 0 #888",
          position: "relative",
          animation: "gameboySlideIn 0.3s ease-out",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Top ridge line */}
        <div style={{
          position: "absolute",
          top: "6px",
          left: "20px",
          right: "20px",
          height: "2px",
          background: "linear-gradient(90deg, transparent, rgba(255,255,255,0.3), transparent)",
        }} />

        {/* Screen bezel */}
        <div style={{
          background: "linear-gradient(180deg, #3a3a5c 0%, #2a2a40 100%)",
          borderRadius: "6px",
          padding: "10px 10px 8px",
          margin: "0 12px",
          boxShadow: "inset 2px 2px 4px rgba(0,0,0,0.5), inset -1px -1px 2px rgba(255,255,255,0.05)",
        }}>
          {/* Power LED */}
          <div style={{
            display: "flex",
            alignItems: "center",
            gap: "4px",
            marginBottom: "4px",
          }}>
            <div style={{
              width: "6px",
              height: "6px",
              borderRadius: "50%",
              background: "#e74c3c",
              boxShadow: "0 0 4px #e74c3c, 0 0 8px rgba(231,76,60,0.4)",
            }} />
            <span style={{ fontSize: "5px", color: "#888", letterSpacing: "1px" }}>BATTERY</span>
          </div>

          {/* Tab switcher */}
          <div style={{
            display: "flex",
            gap: "2px",
            marginBottom: "2px",
          }}>
            {(["settings", "proofs", "flappy"] as Tab[]).map((t) => (
              <button
                key={t}
                onClick={() => setTab(t)}
                style={{
                  flex: 1,
                  background: tab === t ? "#b8c4a0" : "#6b7a60",
                  color: tab === t ? "#3a4438" : "#a0b090",
                  border: "none",
                  padding: "3px 0",
                  fontSize: "7px",
                  fontFamily: "'Press Start 2P', monospace",
                  cursor: "pointer",
                  letterSpacing: "0.5px",
                }}
              >
                {t === "settings" ? "SETTINGS" : t === "proofs" ? "ZK PROOFS" : "FLAPPY BIRD"}
              </button>
            ))}
          </div>

          {/* LCD Screen */}
          <div style={{
            background: "#b8c4a0",
            width: "100%",
            aspectRatio: `${FB.W}/${FB.H}`,
            position: "relative",
            overflow: "hidden",
            imageRendering: "pixelated",
            boxShadow: "inset 1px 1px 3px rgba(0,0,0,0.3)",
          }}>
            {tab === "proofs" ? (
              /* ── ZK PROOF EXPLORER SCREEN ── */
              <div style={{
                padding: "10px 12px",
                height: "100%",
                overflowY: "auto",
                background: "#b8c4a0",
              }}>
                <ProofExplorer />
              </div>
            ) : tab === "settings" ? (
              /* ── SETTINGS SCREEN ── */
              <div style={{
                padding: "10px 12px",
                display: "flex",
                flexDirection: "column",
                gap: "8px",
                height: "100%",
              }}>
                {/* Volume */}
                <div>
                  <div style={{
                    fontSize: "7px",
                    fontFamily: "'Press Start 2P', monospace",
                    color: "#3a4438",
                    marginBottom: "5px",
                  }}>
                    VOLUME
                  </div>
                  <div style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "6px",
                  }}>
                    {/* Speaker icon */}
                    <div style={{
                      fontSize: "7px",
                      fontFamily: "'Press Start 2P', monospace",
                      color: "#6b7a60",
                    }}>
                      {volume === 0 ? "x" : "#"}
                    </div>
                    {/* Custom pixel slider track */}
                    <div style={{ flex: 1, position: "relative", height: "12px" }}>
                      <div style={{
                        position: "absolute",
                        top: "5px",
                        left: 0,
                        right: 0,
                        height: "3px",
                        background: "#6b7a60",
                      }} />
                      <div style={{
                        position: "absolute",
                        top: "5px",
                        left: 0,
                        width: `${volume}%`,
                        height: "3px",
                        background: "#3a4438",
                      }} />
                      <input
                        type="range"
                        min={0}
                        max={100}
                        value={volume}
                        onChange={(e) => setVolume(Number(e.target.value))}
                        style={{
                          position: "absolute",
                          top: 0,
                          left: 0,
                          width: "100%",
                          height: "100%",
                          opacity: 0,
                          cursor: "pointer",
                          margin: 0,
                        }}
                      />
                    </div>
                    <div style={{
                      fontSize: "6px",
                      fontFamily: "'Press Start 2P', monospace",
                      color: "#3a4438",
                      minWidth: "24px",
                      textAlign: "right",
                    }}>
                      {volume}%
                    </div>
                  </div>
                </div>

                {/* Divider */}
                <div style={{ borderTop: "1px dashed #6b7a60" }} />

                {/* SFX Toggle */}
                <div>
                  <div style={{
                    fontSize: "7px",
                    fontFamily: "'Press Start 2P', monospace",
                    color: "#3a4438",
                    marginBottom: "5px",
                  }}>
                    SFX
                  </div>
                  {/* Mute toggle row */}
                  <div style={{ display: "flex", alignItems: "center", gap: "6px", marginBottom: "6px" }}>
                    <button
                      onClick={() => setSfxMutedState((m) => !m)}
                      title={sfxMuted ? "Unmute sound effects" : "Mute sound effects"}
                      style={{
                        fontFamily: "'Press Start 2P', monospace",
                        fontSize: "7px",
                        padding: "3px 6px",
                        background: sfxMuted ? "#3a2a2a" : "#2a3a2a",
                        border: sfxMuted ? "1px solid #7f3333" : "1px solid #3a4438",
                        color: sfxMuted ? "#c06060" : "#6b7a60",
                        cursor: "pointer",
                        minWidth: "24px",
                        textAlign: "center",
                      }}
                    >
                      {sfxMuted ? "✕" : "♪"}
                    </button>
                    {/* SFX volume slider */}
                    <div style={{ flex: 1, position: "relative", height: "12px", opacity: sfxMuted ? 0.4 : 1 }}>
                      <div style={{
                        position: "absolute",
                        top: "5px",
                        left: 0,
                        right: 0,
                        height: "3px",
                        background: "#6b7a60",
                      }} />
                      <div style={{
                        position: "absolute",
                        top: "5px",
                        left: 0,
                        width: `${sfxVolume}%`,
                        height: "3px",
                        background: "#3a4438",
                      }} />
                      <input
                        type="range"
                        min={0}
                        max={100}
                        value={sfxVolume}
                        disabled={sfxMuted}
                        onChange={(e) => setSfxVolumeState(Number(e.target.value))}
                        onMouseUp={() => { if (!sfxMuted) void playSound("chip"); }}
                        style={{
                          position: "absolute",
                          top: 0,
                          left: 0,
                          width: "100%",
                          height: "100%",
                          opacity: 0,
                          cursor: sfxMuted ? "not-allowed" : "pointer",
                          margin: 0,
                        }}
                      />
                    </div>
                    <div style={{
                      fontSize: "6px",
                      fontFamily: "'Press Start 2P', monospace",
                      color: "#3a4438",
                      minWidth: "24px",
                      textAlign: "right",
                    }}>
                      {sfxMuted ? "OFF" : `${sfxVolume}%`}
                    </div>
                  </div>
                  {/* Quick test buttons */}
                  <div style={{ display: "flex", gap: "3px", flexWrap: "wrap" }}>
                    {(["shuffle", "chip", "flip", "winner"] as const).map((name) => (
                      <button
                        key={name}
                        onClick={() => { if (!sfxMuted) void playSound(name); }}
                        disabled={sfxMuted}
                        style={{
                          fontFamily: "'Press Start 2P', monospace",
                          fontSize: "6px",
                          padding: "2px 5px",
                          background: sfxMuted ? "#2a2a2a" : "#3a4438",
                          border: "1px solid #6b7a60",
                          color: sfxMuted ? "#555" : "#b8c4a0",
                          cursor: sfxMuted ? "not-allowed" : "pointer",
                          opacity: sfxMuted ? 0.5 : 1,
                        }}
                        title={`Test ${name} sound`}
                      >
                        {name === "shuffle" ? "SHFL" : name === "chip" ? "CHIP" : name === "flip" ? "FLIP" : "WIN"}
                      </button>
                    ))}
                  </div>
                </div>

                {/* Divider */}
                <div style={{ borderTop: "1px dashed #6b7a60" }} />

                {/* Language (Issue #60) */}
                <LanguageSelector variant="settings" />

                {/* Divider */}
                <div style={{ borderTop: "1px dashed #6b7a60" }} />

                {/* Logout */}
                <button
                  onClick={handleLogout}
                  style={{
                    background: "#6b7a60",
                    color: "#b8c4a0",
                    border: "2px solid #3a4438",
                    padding: "5px 8px",
                    fontSize: "7px",
                    fontFamily: "'Press Start 2P', monospace",
                    cursor: "pointer",
                    textAlign: "center",
                    transition: "background 0.1s",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "#3a4438";
                    e.currentTarget.style.color = "#b8c4a0";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "#6b7a60";
                    e.currentTarget.style.color = "#b8c4a0";
                  }}
                >
                  {t("settings.logout")}
                </button>

              </div>
            ) : (
              /* ── FLAPPY BIRD SCREEN ── */
              <canvas
                ref={canvasRef}
                width={FB.W}
                height={FB.H}
                onClick={handleFlap}
                style={{
                  width: "100%",
                  height: "100%",
                  imageRendering: "pixelated",
                  cursor: "pointer",
                }}
              />
            )}
          </div>
        </div>

        {/* Label */}
        <div style={{
          textAlign: "center",
          margin: "8px 0 4px",
          fontSize: "8px",
          fontFamily: "'Press Start 2P', monospace",
          color: "#5a5668",
          letterSpacing: "3px",
        }}>
          GAMEBOY
        </div>

        {/* Controls area */}
        <div style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          padding: "0 20px",
          marginTop: "6px",
        }}>
          {/* D-PAD */}
          <div style={{ position: "relative", width: "68px", height: "68px" }}>
            {/* Horizontal bar */}
            <div style={{
              position: "absolute",
              top: "22px",
              left: "0",
              width: "68px",
              height: "24px",
              background: "#2a2a3a",
              borderRadius: "3px",
              boxShadow: "inset 1px 1px 0 rgba(255,255,255,0.1), inset -1px -1px 0 rgba(0,0,0,0.3), 0 2px 0 #1a1a28",
            }} />
            {/* Vertical bar */}
            <div style={{
              position: "absolute",
              top: "0",
              left: "22px",
              width: "24px",
              height: "68px",
              background: "#2a2a3a",
              borderRadius: "3px",
              boxShadow: "inset 1px 1px 0 rgba(255,255,255,0.1), inset -1px -1px 0 rgba(0,0,0,0.3)",
            }} />
            {/* Center circle */}
            <div style={{
              position: "absolute",
              top: "27px",
              left: "27px",
              width: "14px",
              height: "14px",
              borderRadius: "50%",
              background: "#1e1e2e",
            }} />
          </div>

          {/* A / B BUTTONS */}
          <div style={{ position: "relative", width: "100px", height: "68px", transform: "rotate(-25deg)" }}>
            {/* B button */}
            <button
              onClick={onClose}
              style={{
                position: "absolute",
                bottom: "4px",
                left: "0",
                width: "36px",
                height: "36px",
                borderRadius: "50%",
                background: "linear-gradient(135deg, #c0392b 0%, #962d22 100%)",
                border: "2px solid #7b241c",
                cursor: "pointer",
                boxShadow: "0 3px 0 #6b1e18, inset 0 2px 0 rgba(255,255,255,0.2)",
              }}
              title="B — Close"
            />
            <span style={{
              position: "absolute",
              bottom: "-4px",
              left: "12px",
              fontSize: "7px",
              fontFamily: "'Press Start 2P', monospace",
              color: "#5a5668",
              transform: "rotate(25deg)",
            }}>B</span>

            {/* A button */}
            <button
              onClick={handleFlap}
              style={{
                position: "absolute",
                top: "0",
                right: "0",
                width: "36px",
                height: "36px",
                borderRadius: "50%",
                background: "linear-gradient(135deg, #c0392b 0%, #962d22 100%)",
                border: "2px solid #7b241c",
                cursor: "pointer",
                boxShadow: "0 3px 0 #6b1e18, inset 0 2px 0 rgba(255,255,255,0.2)",
              }}
              title="A — Flap / Select"
            />
            <span style={{
              position: "absolute",
              top: "-4px",
              right: "12px",
              fontSize: "7px",
              fontFamily: "'Press Start 2P', monospace",
              color: "#5a5668",
              transform: "rotate(25deg)",
            }}>A</span>
          </div>
        </div>

        {/* START / SELECT */}
        <div style={{
          display: "flex",
          justifyContent: "center",
          gap: "20px",
          marginTop: "10px",
        }}>
          <div style={{ textAlign: "center" }}>
            <div style={{
              width: "28px",
              height: "7px",
              background: "#7a7888",
              borderRadius: "4px",
              margin: "0 auto 3px",
              boxShadow: "inset 0 1px 0 rgba(255,255,255,0.15), 0 1px 0 rgba(0,0,0,0.2)",
            }} />
            <span style={{ fontSize: "5px", color: "#5a5668", fontFamily: "'Press Start 2P', monospace" }}>
              SELECT
            </span>
          </div>
          <div style={{ textAlign: "center" }}>
            <div style={{
              width: "28px",
              height: "7px",
              background: "#7a7888",
              borderRadius: "4px",
              margin: "0 auto 3px",
              boxShadow: "inset 0 1px 0 rgba(255,255,255,0.15), 0 1px 0 rgba(0,0,0,0.2)",
            }} />
            <span style={{ fontSize: "5px", color: "#5a5668", fontFamily: "'Press Start 2P', monospace" }}>
              START
            </span>
          </div>
        </div>

        {/* Speaker grille (bottom-right) */}
        <div style={{
          position: "absolute",
          bottom: "20px",
          right: "22px",
          display: "grid",
          gridTemplateColumns: "repeat(5, 4px)",
          gap: "3px",
          transform: "rotate(-25deg)",
        }}>
          {Array.from({ length: 15 }).map((_, i) => (
            <div
              key={i}
              style={{
                width: "4px",
                height: "4px",
                borderRadius: "50%",
                background: "#9a98a8",
                boxShadow: "inset 0 1px 0 rgba(0,0,0,0.25)",
              }}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
