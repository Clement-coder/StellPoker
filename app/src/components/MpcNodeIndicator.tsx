"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import {
  getMpcStatus,
  type MpcNodeProgress,
  type TableMpcStatusResponse,
} from "@/lib/api";

interface MpcNodeIndicatorProps {
  tableId: number;
  phase: string;
  className?: string;
}

const POLL_INTERVAL_MS = 2000;
const TIMEOUT_THRESHOLD_SECS = 30;

function nodeLabel(endpoint: string, index: number): string {
  const match = endpoint.match(/:(\d+)$/);
  if (match) return `Node ${match[1].slice(-2)}`;
  return `Node ${index}`;
}

function phaseLabel(phase: string): string {
  switch (phase) {
    case "dealing":
      return "Dealing";
    case "preflop":
    case "flop":
    case "turn":
    case "river":
      return "Betting";
    case "dealingflop":
    case "dealingturn":
    case "dealingriver":
      return "Revealing";
    case "showdown":
      return "Showdown";
    default:
      return phase;
  }
}

function isMpcPhase(phase: string): boolean {
  return (
    phase === "dealing" ||
    phase === "dealingflop" ||
    phase === "dealingturn" ||
    phase === "dealingriver" ||
    phase === "showdown"
  );
}

export function MpcNodeIndicator({
  tableId,
  phase,
  className,
}: MpcNodeIndicatorProps) {
  const [status, setStatus] = useState<TableMpcStatusResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchStatus = useCallback(async () => {
    try {
      const data = await getMpcStatus(tableId);
      setStatus(data);
      setError(null);
    } catch {
      setError("Failed to fetch MPC status");
    }
  }, [tableId]);

  useEffect(() => {
    if (!isMpcPhase(phase)) {
      setStatus(null);
      if (intervalRef.current) clearInterval(intervalRef.current);
      return;
    }

    fetchStatus();
    intervalRef.current = setInterval(fetchStatus, POLL_INTERVAL_MS);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [phase, fetchStatus]);

  if (!isMpcPhase(phase) || !status) return null;

  const nodes = status.nodes;
  if (nodes.length === 0) return null;

  return (
    <div
      className={`flex items-center gap-1.5 text-[9px] ${className ?? ""}`}
      style={{ color: "#b2bec3" }}
    >
      <span className="opacity-60">{phaseLabel(phase)}</span>
      <span className="opacity-30">|</span>
      {nodes.map((node, i) => (
        <NodeDot key={i} node={node} index={i} />
      ))}
    </div>
  );
}

function NodeDot({ node, index }: { node: MpcNodeProgress; index: number }) {
  const isTimedOut =
    !node.healthy || node.elapsed_secs > TIMEOUT_THRESHOLD_SECS;
  const isActive = node.phase !== "idle" && node.phase !== "";

  let color = "#636e72";
  if (isTimedOut) {
    color = "#d63031";
  } else if (isActive && node.healthy) {
    color = "#00b894";
  }

  return (
    <span
      className="inline-flex items-center gap-0.5"
      title={`${nodeLabel(node.endpoint, index)}: ${node.phase}${isTimedOut ? " (timeout)" : ""}`}
    >
      <span
        className="inline-block rounded-full"
        style={{
          width: 5,
          height: 5,
          backgroundColor: color,
          boxShadow: isActive && !isTimedOut ? `0 0 4px ${color}` : "none",
        }}
      />
      <span
        className="opacity-60"
        style={{ fontSize: 8 }}
      >
        {nodeLabel(node.endpoint, index).slice(-1)}
      </span>
    </span>
  );
}
