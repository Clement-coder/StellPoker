"use client";

/**
 * Collapsible multi-table overview mini-map (Issue #53).
 * Shows all tables with seat counts and chip stacks; click navigates.
 */

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import {
  listTableOverview,
  type TableOverviewInfo,
} from "@/lib/api";
import { useT } from "@/lib/i18n/context";

interface TableMiniMapProps {
  currentTableId?: number;
  /** Start collapsed to avoid distracting active play. */
  defaultCollapsed?: boolean;
}

function formatChips(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

export function TableMiniMap({
  currentTableId,
  defaultCollapsed = true,
}: TableMiniMapProps) {
  const t = useT();
  const router = useRouter();
  const [collapsed, setCollapsed] = useState(defaultCollapsed);
  const [tables, setTables] = useState<TableOverviewInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await listTableOverview();
      setTables(res.tables);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load tables");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (collapsed) return;
    refresh();
    const id = setInterval(refresh, 15_000);
    return () => clearInterval(id);
  }, [collapsed, refresh]);

  if (collapsed) {
    return (
      <button
        type="button"
        onClick={() => setCollapsed(false)}
        className="pixel-btn pixel-btn-blue text-[9px] fixed bottom-4 left-4 z-40"
        title={t("minimap.title")}
        style={{ opacity: 0.85 }}
      >
        {t("minimap.expand")}
      </button>
    );
  }

  return (
    <div
      className="fixed bottom-4 left-4 z-40 pixel-border"
      style={{
        background: "rgba(12, 10, 24, 0.94)",
        borderColor: "#8b6914",
        width: "220px",
        maxHeight: "280px",
        display: "flex",
        flexDirection: "column",
        boxShadow: "0 4px 0 rgba(0,0,0,0.4)",
      }}
    >
      <div
        className="flex items-center justify-between px-2 py-1"
        style={{ borderBottom: "1px solid #8b6914" }}
      >
        <span className="text-[8px]" style={{ color: "#f5e6c8" }}>
          {t("minimap.title")}
        </span>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={refresh}
            disabled={loading}
            className="text-[7px]"
            style={{
              background: "none",
              border: "none",
              color: "#c8e6ff",
              cursor: "pointer",
              padding: 0,
              textDecoration: "underline",
            }}
          >
            {t("minimap.refresh")}
          </button>
          <button
            type="button"
            onClick={() => setCollapsed(true)}
            className="text-[7px]"
            style={{
              background: "none",
              border: "none",
              color: "#e74c3c",
              cursor: "pointer",
              padding: 0,
            }}
          >
            {t("minimap.collapse")}
          </button>
        </div>
      </div>

      <div
        style={{
          overflowY: "auto",
          padding: "6px",
          display: "flex",
          flexDirection: "column",
          gap: "4px",
        }}
      >
        {error && (
          <div className="text-[7px]" style={{ color: "#e74c3c" }}>
            {error}
          </div>
        )}
        {!error && tables.length === 0 && !loading && (
          <div className="text-[7px]" style={{ color: "#7f8c8d" }}>
            {t("minimap.empty")}
          </div>
        )}
        {loading && tables.length === 0 && (
          <div className="text-[7px]" style={{ color: "#7f8c8d" }}>
            {t("app.loading")}
          </div>
        )}
        {tables.map((table) => {
          const isCurrent = currentTableId === table.table_id;
          return (
            <button
              key={table.table_id}
              type="button"
              onClick={() => {
                if (!isCurrent) {
                  router.push(`/table/${table.table_id}`);
                }
              }}
              className="text-left pixel-border-thin"
              style={{
                padding: "6px",
                background: isCurrent
                  ? "rgba(241, 196, 15, 0.12)"
                  : "rgba(255,255,255,0.03)",
                borderColor: isCurrent ? "#f1c40f" : "#4a3a28",
                cursor: isCurrent ? "default" : "pointer",
                width: "100%",
              }}
            >
              <div className="flex items-center justify-between">
                <span
                  className="text-[8px]"
                  style={{ color: isCurrent ? "#f1c40f" : "#f5e6c8" }}
                >
                  #{table.table_id}
                </span>
                <span className="text-[7px]" style={{ color: "#95a5a6" }}>
                  {table.phase}
                </span>
              </div>
              <div
                className="flex items-center justify-between mt-1"
                style={{ color: "#c8e6ff" }}
              >
                <span className="text-[7px]">
                  {t("minimap.seats", {
                    seated: table.seated,
                    max: table.max_players,
                  })}
                </span>
                <span className="text-[7px]" style={{ color: "#27ae60" }}>
                  {t("minimap.chips", {
                    chips: formatChips(table.total_chips),
                  })}
                </span>
              </div>
              {isCurrent && (
                <div
                  className="text-[6px] mt-1"
                  style={{ color: "#f1c40f" }}
                >
                  {t("minimap.current")}
                </div>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
