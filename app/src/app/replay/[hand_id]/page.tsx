"use client";

import { use, useEffect, useState } from "react";
import Link from "next/link";
import { PixelWorld } from "@/components/PixelWorld";
import { ReplayViewer } from "@/components/ReplayViewer";
import { fetchReplayHand, parseHandId, type ReplayHand } from "@/lib/replay";

export default function ReplayPage({
  params,
}: {
  params: Promise<{ hand_id: string }>;
}) {
  const { hand_id } = use(params);
  const parsed = parseHandId(hand_id);

  const [hand, setHand] = useState<ReplayHand | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!parsed) {
      setError("Invalid hand ID format. Expected <table_id>-<hand_number>.");
      setLoading(false);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    fetchReplayHand(parsed.tableId, parsed.handNumber)
      .then((result) => {
        if (cancelled) return;
        if (!result) {
          setError(
            "No events found for this hand. It may still be in progress, or the hand ID is incorrect."
          );
        } else {
          setHand(result);
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setError(
          e instanceof Error ? e.message : "Failed to load replay data."
        );
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [hand_id]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <PixelWorld>
      <div className="min-h-screen flex flex-col items-center p-6 gap-6">
        {/* Nav */}
        <div className="w-full" style={{ maxWidth: 560 }}>
          <div className="flex items-center justify-between">
            <Link
              href="/"
              className="text-[9px]"
              style={{
                color: "#c47d2e",
                textDecoration: "none",
                fontFamily: "'Press Start 2P', monospace",
              }}
            >
              ← HOME
            </Link>
            <div className="text-[9px]" style={{ color: "#f5e6c8" }}>
              HAND REPLAY
            </div>
            {parsed && (
              <Link
                href={`/table/${parsed.tableId}`}
                className="text-[9px]"
                style={{
                  color: "#3498db",
                  textDecoration: "none",
                  fontFamily: "'Press Start 2P', monospace",
                }}
              >
                TABLE #{parsed.tableId} →
              </Link>
            )}
          </div>
        </div>

        {/* Loading state */}
        {loading && (
          <div
            className="pixel-border px-6 py-4 text-[10px]"
            style={{
              borderColor: "#c47d2e",
              background: "rgba(12,10,24,0.92)",
              color: "#f5e6c8",
              animation: "textPulse 1.5s ease-in-out infinite",
            }}
            aria-live="polite"
            aria-busy="true"
          >
            LOADING HAND DATA FROM CHAIN…
          </div>
        )}

        {/* Error state */}
        {!loading && error && (
          <div
            className="pixel-border px-6 py-4"
            style={{
              borderColor: "#e74c3c",
              background: "rgba(12,10,24,0.92)",
              maxWidth: 560,
              width: "100%",
            }}
            role="alert"
          >
            <div className="text-[10px] mb-2" style={{ color: "#e74c3c" }}>
              REPLAY UNAVAILABLE
            </div>
            <div className="text-[9px]" style={{ color: "#95a5a6" }}>
              {error}
            </div>
            <div className="text-[8px] mt-3" style={{ color: "#7f8c8d" }}>
              Hand ID: <span style={{ color: "#f5e6c8" }}>{hand_id}</span>
            </div>
            <div className="text-[8px] mt-2" style={{ color: "#7f8c8d" }}>
              Tip: Replays are available for hands that have already been settled
              on-chain. The coordinator must be running and the hand must be
              complete.
            </div>
          </div>
        )}

        {/* Replay viewer */}
        {!loading && hand && (
          <div style={{ width: "100%", maxWidth: 560 }}>
            <ReplayViewer hand={hand} />
          </div>
        )}

        {/* ZK explanation footer */}
        {!loading && hand && (
          <div
            className="pixel-border-thin px-4 py-3 text-[8px]"
            style={{
              maxWidth: 560,
              width: "100%",
              borderColor: "#2a2a4a",
              background: "rgba(12,10,24,0.7)",
              color: "#7f8c8d",
              lineHeight: 1.8,
            }}
          >
            <div style={{ color: "#c47d2e", marginBottom: 4 }}>
              ZK + MPC PROPERTIES
            </div>
            All deal, reveal, and showdown steps shown above were verified
            on-chain via UltraHonk ZK proofs using Soroban&#39;s native BN254
            host functions. No single party — including the coordinator — ever
            held the complete deck. Cards were secret-shared across 3 independent
            MPC nodes using REP3 sharing.{" "}
            <a
              href="https://github.com/HitEmPoka/StellPoker"
              target="_blank"
              rel="noopener noreferrer"
              style={{ color: "#3498db" }}
            >
              Learn more ↗
            </a>
          </div>
        )}
      </div>
    </PixelWorld>
  );
}
