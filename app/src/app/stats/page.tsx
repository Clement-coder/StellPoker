"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import {
  getStats,
  getRatingLeaderboard,
  type StatsResponse,
  type RatingLeaderboardResponse,
} from "@/lib/api";
import { useT } from "@/lib/i18n/context";
import { LanguageSelector } from "@/components/LanguageSelector";

const STROOPS_PER_XLM = 10_000_000;

function formatXlm(stroops: number): string {
  if (!stroops) return "0 XLM";
  const xlm = stroops / STROOPS_PER_XLM;
  return `${xlm.toLocaleString(undefined, { maximumFractionDigits: 2 })} XLM`;
}

function shortAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

export default function StatsPage() {
  const t = useT();
  const [stats, setStats] = useState<StatsResponse | null>(null);
  const [ratings, setRatings] = useState<RatingLeaderboardResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const [data, ratingData] = await Promise.all([
          getStats(),
          getRatingLeaderboard(0, 20).catch(() => null),
        ]);
        if (!cancelled) {
          setStats(data);
          setRatings(ratingData);
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : "Failed to load stats");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    load();
    const id = setInterval(load, 30_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  return (
    <main className="min-h-screen bg-gray-950 text-gray-100 p-6 font-mono">
      <div className="max-w-2xl mx-auto">
        <div className="flex items-center justify-between mb-6">
          <h1 className="text-2xl font-bold text-yellow-400">{t("stats.title")}</h1>
          <div className="flex items-center gap-3">
            <LanguageSelector variant="header" />
            <Link href="/" className="text-sm text-gray-400 hover:text-gray-200 underline">
              {t("app.back")}
            </Link>
          </div>
        </div>

        {loading && (
          <p className="text-gray-400 animate-pulse">{t("stats.loadingStats")}</p>
        )}

        {error && (
          <p className="text-red-400 bg-red-900/30 rounded px-4 py-2">{error}</p>
        )}

        {stats && (
          <>
            <section className="mb-8">
              <h2 className="text-lg font-semibold text-gray-300 mb-3 uppercase tracking-wide">
                {t("stats.global")}
              </h2>
              <div className="grid grid-cols-3 gap-4">
                <StatCard label={t("stats.handsPlayed")} value={stats.global.hands_played.toLocaleString()} />
                <StatCard label={t("stats.biggestPot")} value={formatXlm(stats.global.biggest_pot)} />
                <StatCard label={t("stats.playersJoined")} value={stats.global.total_players_joined.toLocaleString()} />
              </div>
            </section>

            <section className="mb-8">
              <h2 className="text-lg font-semibold text-gray-300 mb-3 uppercase tracking-wide">
                {t("stats.leaderboard")}
              </h2>
              {stats.leaderboard.length === 0 ? (
                <p className="text-gray-500 text-sm">{t("stats.noHands")}</p>
              ) : (
                <table className="w-full text-sm border-collapse">
                  <thead>
                    <tr className="text-gray-400 border-b border-gray-700">
                      <th className="text-left py-2 pr-4">#</th>
                      <th className="text-left py-2 pr-4">{t("stats.player")}</th>
                      <th className="text-right py-2 pr-4">{t("stats.handsWon")}</th>
                      <th className="text-right py-2 pr-4">{t("stats.handsPlayed")}</th>
                      <th className="text-right py-2">{t("stats.biggestPot")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {stats.leaderboard.map((p, i) => (
                      <tr
                        key={p.address}
                        className={`border-b border-gray-800 ${i === 0 ? "text-yellow-300" : "text-gray-200"}`}
                      >
                        <td className="py-2 pr-4 text-gray-500">{i + 1}</td>
                        <td className="py-2 pr-4 font-mono" title={p.address}>
                          {shortAddress(p.address)}
                        </td>
                        <td className="py-2 pr-4 text-right">{p.hands_won}</td>
                        <td className="py-2 pr-4 text-right text-gray-400">{p.hands_played}</td>
                        <td className="py-2 text-right text-gray-400">
                          {formatXlm(p.biggest_pot_won)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </section>

            {/* Issue #70 — on-chain ELO ratings */}
            <section>
              <h2 className="text-lg font-semibold text-gray-300 mb-3 uppercase tracking-wide">
                {t("stats.ratingLeaderboard")}
              </h2>
              <p className="text-xs text-gray-500 mb-3">{t("stats.minHandsNote")}</p>
              {!ratings || ratings.entries.length === 0 ? (
                <p className="text-gray-500 text-sm">{t("stats.noHands")}</p>
              ) : (
                <table className="w-full text-sm border-collapse">
                  <thead>
                    <tr className="text-gray-400 border-b border-gray-700">
                      <th className="text-left py-2 pr-4">{t("stats.rank")}</th>
                      <th className="text-left py-2 pr-4">{t("stats.player")}</th>
                      <th className="text-right py-2 pr-4">{t("stats.rating")}</th>
                      <th className="text-right py-2 pr-4">{t("stats.handsWon")}</th>
                      <th className="text-right py-2">{t("stats.handsPlayed")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {ratings.entries.map((p, i) => (
                      <tr
                        key={p.address}
                        className={`border-b border-gray-800 ${i === 0 ? "text-yellow-300" : "text-gray-200"}`}
                      >
                        <td className="py-2 pr-4 text-gray-500">{i + 1}</td>
                        <td className="py-2 pr-4 font-mono" title={p.address}>
                          {shortAddress(p.address)}
                        </td>
                        <td className="py-2 pr-4 text-right font-bold">{p.rating}</td>
                        <td className="py-2 pr-4 text-right text-gray-400">{p.hands_won}</td>
                        <td className="py-2 text-right text-gray-400">{p.hands_played}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </section>

            <p className="mt-6 text-xs text-gray-600">
              {t("stats.cachedAt", {
                time: new Date(stats.cached_at * 1000).toLocaleTimeString(),
              })}
            </p>
          </>
        )}
      </div>
    </main>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-gray-900 rounded-lg p-4 border border-gray-800">
      <p className="text-xs text-gray-500 mb-1">{label}</p>
      <p className="text-xl font-bold text-white">{value}</p>
    </div>
  );
}
