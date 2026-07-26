"use client";

import { Card } from "./Card";
import { stellarExpertUrl } from "@/lib/explorer";
import type { HandHistoryEntry, Street } from "@/lib/hand-history";

interface HandHistoryPanelProps {
  open: boolean;
  onClose: () => void;
  entries: HandHistoryEntry[];
}

function shortAddress(address: string): string {
  return `${address.slice(0, 6)}...${address.slice(-6)}`;
}

const STREET_LABEL: Record<Street, string> = {
  preflop: "PRE-FLOP",
  flop: "FLOP",
  turn: "TURN",
  river: "RIVER",
};

export function HandHistoryPanel({ open, onClose, entries }: HandHistoryPanelProps) {
  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)", backdropFilter: "blur(2px)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="pixel-border"
        style={{
          background: "rgba(12, 10, 24, 0.97)",
          borderColor: "#c47d2e",
          width: "420px",
          maxHeight: "80vh",
          overflowY: "auto",
          padding: "16px",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-3">
          <span className="text-[11px]" style={{ color: "#f5e6c8" }}>
            HAND HISTORY (THIS SESSION)
          </span>
          <button
            onClick={onClose}
            className="text-[11px]"
            style={{ background: "none", border: "none", color: "#e74c3c", cursor: "pointer" }}
          >
            ✕
          </button>
        </div>

        {entries.length === 0 && (
          <div className="text-[9px]" style={{ color: "#7f8c8d" }}>
            No completed hands yet this session.
          </div>
        )}

        <div className="flex flex-col gap-3">
          {entries.map((entry) => (
            <div
              key={`${entry.tableId}-${entry.handNumber}-${entry.timestamp}`}
              className="pixel-border-thin"
              style={{ padding: "8px", background: "rgba(255,255,255,0.03)" }}
            >
              <div className="flex items-center justify-between">
                <span className="text-[9px]" style={{ color: "#f1c40f" }}>
                  HAND #{entry.handNumber}
                </span>
                <span className="text-[8px]" style={{ color: "#7f8c8d" }}>
                  {new Date(entry.timestamp).toLocaleTimeString()}
                </span>
              </div>

              {entry.streets.length > 0 && (
                <div className="flex flex-col gap-1 mt-2">
                  {entry.streets.map((s) => (
                    <div
                      key={s.street}
                      className="flex items-center justify-between text-[8px]"
                      style={{ color: "#c8e6ff" }}
                    >
                      <span>{STREET_LABEL[s.street]}</span>
                      <span>POT: {s.pot.toLocaleString()}</span>
                    </div>
                  ))}
                </div>
              )}

              {entry.boardCards.length > 0 && (
                <div className="flex gap-1 mt-2">
                  {entry.boardCards.map((c, i) => (
                    <Card key={i} value={c} size="sm" />
                  ))}
                </div>
              )}

              {entry.holeCards && (
                <div className="flex items-center gap-2 mt-2">
                  <div className="flex gap-1">
                    <Card value={entry.holeCards[0]} size="sm" />
                    <Card value={entry.holeCards[1]} size="sm" />
                  </div>
                  {entry.handRankName && (
                    <span className="text-[8px]" style={{ color: "#27ae60" }}>
                      {entry.handRankName}
                    </span>
                  )}
                </div>
              )}

              <div className="text-[8px] mt-2" style={{ color: "#95a5a6" }}>
                FINAL POT: {entry.finalPot.toLocaleString()}
                {entry.winnerAddress && <> &middot; WINNER: {shortAddress(entry.winnerAddress)}</>}
              </div>

              {entry.txHash && (
                <a
                  href={stellarExpertUrl("tx", entry.txHash)}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[8px]"
                  style={{ color: "#ffc078", textDecoration: "none" }}
                >
                  VIEW PROOF TX ↗
                </a>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
