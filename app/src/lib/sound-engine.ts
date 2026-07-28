/**
 * sound-engine.ts
 * ─────────────────────────────────────────────────────────────────────────────
 * Web Audio API sound-effect engine for StellPoker.
 *
 * All sounds are procedurally synthesised — no audio files required.
 *
 * Public surface
 * ──────────────
 *  playSound(name)          — fire a one-shot effect
 *  setSfxMuted(muted)       — silence / restore all SFX
 *  getSfxMuted()            — current mute state
 *  setSfxVolume(0-1)        — master SFX gain (independent of music volume)
 *  getSfxVolume()
 *
 * Sounds
 * ──────
 *  "shuffle"  — quick burst of short white-noise "whoosh" pulses (card riffle)
 *  "chip"     — percussive plastic click + brief tonal ring (chip stack)
 *  "flip"     — thin snap + high sine blip (single card flip)
 *  "winner"   — ascending arpeggio + shimmering release (hand won)
 */

export type SoundName = "shuffle" | "chip" | "flip" | "winner" | "deal" | "bet" | "fold" | "win";

// ── Module-level singleton ───────────────────────────────────────────────────

let _ctx: AudioContext | null = null;
let _masterGain: GainNode | null = null;
let _muted = false;
let _volume = 0.55; // default SFX level — quieter than music
const SFX_MUTE_KEY = "stellpoker-sfx-muted";
const SFX_VOL_KEY = "stellpoker-sfx-volume";

function getContext(): AudioContext | null {
  if (typeof window === "undefined") return null;
  if (_ctx) return _ctx;

  try {
    _ctx = new AudioContext();
    _masterGain = _ctx.createGain();
    _masterGain.gain.value = _muted ? 0 : _volume;
    _masterGain.connect(_ctx.destination);
  } catch {
    _ctx = null;
  }
  return _ctx;
}

function getMasterGain(): GainNode | null {
  getContext(); // ensure init
  return _masterGain;
}

/** Resume a suspended context (browsers require user gesture before audio). */
async function ensureRunning(): Promise<AudioContext | null> {
  const ctx = getContext();
  if (!ctx) return null;
  if (ctx.state === "suspended") {
    try {
      await ctx.resume();
    } catch {
      return null;
    }
  }
  return ctx;
}

// ── State helpers ────────────────────────────────────────────────────────────

export function getSfxMuted(): boolean {
  if (typeof window === "undefined") return false;
  // Read from localStorage on first call so pref survives page reload
  const stored = localStorage.getItem(SFX_MUTE_KEY);
  if (stored !== null) _muted = stored === "true";
  return _muted;
}

export function setSfxMuted(muted: boolean): void {
  _muted = muted;
  if (typeof window !== "undefined") {
    try { localStorage.setItem(SFX_MUTE_KEY, String(muted)); } catch {}
  }
  if (_masterGain) {
    try {
      _masterGain.gain.setTargetAtTime(muted ? 0 : _volume, _masterGain.context.currentTime, 0.02);
    } catch {}
  }
}

export function getSfxVolume(): number {
  if (typeof window !== "undefined") {
    try {
      const stored = localStorage.getItem(SFX_VOL_KEY);
      if (stored !== null) _volume = Math.max(0, Math.min(1, parseFloat(stored)));
    } catch {}
  }
  return _volume;
}

export function setSfxVolume(v: number): void {
  _volume = Math.max(0, Math.min(1, v));
  if (typeof window !== "undefined") {
    try { localStorage.setItem(SFX_VOL_KEY, String(_volume)); } catch {}
  }
  if (_masterGain && !_muted) {
    try {
      _masterGain.gain.setTargetAtTime(_volume, _masterGain.context.currentTime, 0.02);
    } catch {}
  }
}

// ── Sound synthesis helpers ──────────────────────────────────────────────────

/** Connect a node chain to the master gain, then disconnect after `durMs`. */
function autoDisconnect(node: AudioNode, durMs: number) {
  setTimeout(() => {
    try { node.disconnect(); } catch { /* already gone */ }
  }, durMs + 100);
}

/** Short white-noise burst — single "whip" sound for one card. */
function playNoiseWhip(
  ctx: AudioContext,
  out: GainNode,
  startTime: number,
  durationSec: number,
  peak: number,
  highpassHz: number,
) {
  const bufLen = Math.ceil(ctx.sampleRate * durationSec);
  const buf = ctx.createBuffer(1, bufLen, ctx.sampleRate);
  const data = buf.getChannelData(0);
  for (let i = 0; i < bufLen; i++) {
    data[i] = (Math.random() * 2 - 1);
  }

  const src = ctx.createBufferSource();
  src.buffer = buf;

  // High-pass to make it feel more papery
  const hp = ctx.createBiquadFilter();
  hp.type = "highpass";
  hp.frequency.value = highpassHz;
  hp.Q.value = 0.7;

  const env = ctx.createGain();
  env.gain.setValueAtTime(0, startTime);
  env.gain.linearRampToValueAtTime(peak, startTime + durationSec * 0.1);
  env.gain.exponentialRampToValueAtTime(0.0001, startTime + durationSec);

  src.connect(hp);
  hp.connect(env);
  env.connect(out);
  src.start(startTime);
  src.stop(startTime + durationSec + 0.01);

  autoDisconnect(env, (durationSec + 0.15) * 1000);
}

/** Short sine-wave blip. */
function playSineBlip(
  ctx: AudioContext,
  out: GainNode,
  startTime: number,
  freqHz: number,
  durationSec: number,
  peak: number,
) {
  const osc = ctx.createOscillator();
  osc.type = "sine";
  osc.frequency.setValueAtTime(freqHz, startTime);

  const env = ctx.createGain();
  env.gain.setValueAtTime(0, startTime);
  env.gain.linearRampToValueAtTime(peak, startTime + 0.005);
  env.gain.exponentialRampToValueAtTime(0.0001, startTime + durationSec);

  osc.connect(env);
  env.connect(out);
  osc.start(startTime);
  osc.stop(startTime + durationSec + 0.01);

  autoDisconnect(env, (durationSec + 0.15) * 1000);
}

/** Sharp transient click — useful for chip impact. */
function playClick(
  ctx: AudioContext,
  out: GainNode,
  startTime: number,
  freqStart: number,
  freqEnd: number,
  durationSec: number,
  peak: number,
) {
  const osc = ctx.createOscillator();
  osc.type = "triangle";
  osc.frequency.setValueAtTime(freqStart, startTime);
  osc.frequency.exponentialRampToValueAtTime(freqEnd, startTime + durationSec * 0.3);

  const env = ctx.createGain();
  env.gain.setValueAtTime(peak, startTime);
  env.gain.exponentialRampToValueAtTime(0.0001, startTime + durationSec);

  osc.connect(env);
  env.connect(out);
  osc.start(startTime);
  osc.stop(startTime + durationSec + 0.01);

  autoDisconnect(env, (durationSec + 0.15) * 1000);
}

// ── Sound definitions ────────────────────────────────────────────────────────

/**
 * shuffle — 6 rapid white-noise whips with slight pitch variation,
 * mimicking cards being riffled together.
 */
function synthesiseShuffle(ctx: AudioContext, out: GainNode) {
  const numCards = 6;
  const spacing = 0.055; // seconds between each whip
  for (let i = 0; i < numCards; i++) {
    const t = ctx.currentTime + i * spacing;
    const hp = 1800 + Math.random() * 1200;
    playNoiseWhip(ctx, out, t, 0.06, 0.35, hp);
    // Thin sine click to accentuate the "snap"
    playSineBlip(ctx, out, t, 900 + i * 80, 0.03, 0.12);
  }
}

/**
 * chip — plastic click followed by a brief metallic ring.
 * Sounds like a poker chip landing on a stack.
 */
function synthesiseChip(ctx: AudioContext, out: GainNode) {
  const t = ctx.currentTime;
  // Impact click — triangle sweep downward
  playClick(ctx, out, t, 1600, 240, 0.08, 0.5);
  // Short noise burst for the plastic "clack"
  playNoiseWhip(ctx, out, t, 0.045, 0.22, 2400);
  // Ring overtone — thin sine decay
  playSineBlip(ctx, out, t + 0.01, 1240, 0.18, 0.08);
}

/**
 * flip — thin high-frequency snap + tiny sine blip.
 * Used when a single card is revealed / turned over.
 */
function synthesiseFlip(ctx: AudioContext, out: GainNode) {
  const t = ctx.currentTime;
  playNoiseWhip(ctx, out, t, 0.045, 0.28, 3200);
  playSineBlip(ctx, out, t + 0.015, 1800, 0.06, 0.09);
}

/**
 * winner — a bright ascending pentatonic arpeggio (5 notes) plus a
 * shimmering release chord. Short and celebratory without being annoying.
 */
function synthesiseWinner(ctx: AudioContext, out: GainNode) {
  // Pentatonic scale starting on C5: 523, 587, 659, 784, 880 Hz
  const notes = [523.25, 587.33, 659.25, 783.99, 880.00];
  const step = 0.11; // seconds between notes

  notes.forEach((freq, i) => {
    const t = ctx.currentTime + i * step;
    playSineBlip(ctx, out, t, freq, 0.22, 0.3);
    // Fifth above for richness
    playSineBlip(ctx, out, t, freq * 1.5, 0.15, 0.1);
  });

  // Final shimmer chord — all notes together, softer
  const shimmerStart = ctx.currentTime + notes.length * step + 0.04;
  notes.forEach((freq) => {
    playSineBlip(ctx, out, shimmerStart, freq, 0.45, 0.12);
  });

  // Gold noise burst at the apex
  playNoiseWhip(ctx, out, shimmerStart, 0.12, 0.12, 4000);
}

/**
 * fold — soft downward swish for card folding.
 */
function synthesiseFold(ctx: AudioContext, out: GainNode) {
  const t = ctx.currentTime;
  playNoiseWhip(ctx, out, t, 0.08, 0.2, 1200);
  playSineBlip(ctx, out, t, 350, 0.08, 0.08);
}

// ── Public API ───────────────────────────────────────────────────────────────

const SYNTHS: Record<SoundName, (ctx: AudioContext, out: GainNode) => void> = {
  shuffle: synthesiseShuffle,
  chip: synthesiseChip,
  flip: synthesiseFlip,
  winner: synthesiseWinner,
  deal: synthesiseShuffle,
  bet: synthesiseChip,
  fold: synthesiseFold,
  win: synthesiseWinner,
};

/**
 * Fire a sound effect. Safe to call from any React event handler or useEffect.
 * Silently no-ops in SSR, when muted, or if Web Audio is unavailable.
 */
export async function playSound(name: SoundName): Promise<void> {
  if (_muted) return;
  // Also respect the persisted mute preference on first call
  if (typeof window !== "undefined") {
    const stored = localStorage.getItem(SFX_MUTE_KEY);
    if (stored === "true") { _muted = true; return; }
  }

  const ctx = await ensureRunning();
  if (!ctx) return;

  const out = getMasterGain();
  if (!out) return;

  try {
    SYNTHS[name](ctx, out);
  } catch {
    // Never crash the game because of a sound effect
  }
}
