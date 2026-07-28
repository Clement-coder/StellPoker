/**
 * Tournament API client.
 *
 * Talks to the coordinator's /api/tournaments endpoints.
 * All monetary values are in stroops (1 XLM = 10_000_000 stroops).
 */

// ── Types ─────────────────────────────────────────────────────────────────────

export type TournamentStatus =
  | "registration"
  | "running"
  | "finalizing"
  | "completed"
  | "cancelled";

export interface BlindLevel {
  small_blind: number;
  big_blind: number;
  /** Hands played at this level before escalating. */
  hands: number;
}

export interface PayoutSchedule {
  /** shares[0] = 1st place %, shares[1] = 2nd place %, etc. Must sum to 100. */
  shares: number[];
}

export interface TournamentPlayer {
  address: string;
  table_contract: string;
  stack: number;
  finish_position: number | null;
  payout: number | null;
}

export interface TournamentSummary {
  id: string;
  name: string;
  buy_in: number;
  max_players: number;
  registered: number;
  status: TournamentStatus;
  prize_pool: number;
  current_small_blind: number;
  current_big_blind: number;
  blind_level: number;
}

export interface TournamentDetail extends TournamentSummary {
  min_players: number;
  players_per_table: number;
  players: TournamentPlayer[];
  eliminations: string[];
  table_contracts: string[];
  blind_schedule: BlindLevel[];
  payout_schedule: PayoutSchedule;
}

export interface TableMove {
  player: string;
  from_table: string;
  to_table: string;
}

export interface HandResultResponse {
  tournament: TournamentDetail;
  newly_eliminated: string[];
  balancing_moves: TableMove[];
  empty_tables: string[];
}

export interface CreateTournamentParams {
  name: string;
  buy_in: number;
  max_players: number;
  min_players?: number;
  players_per_table?: number;
  blind_schedule?: BlindLevel[];
  payout_schedule?: PayoutSchedule;
}

// ── Base URL ──────────────────────────────────────────────────────────────────

function coordinatorBase(): string {
  return (
    process.env.NEXT_PUBLIC_COORDINATOR_URL?.replace(/\/+$/, "") ??
    "http://localhost:8080"
  );
}

async function apiFetch<T>(
  path: string,
  init?: RequestInit
): Promise<T> {
  const res = await fetch(`${coordinatorBase()}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...init,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText);
    throw new Error(`Tournament API ${path}: ${res.status} ${text}`);
  }
  return res.json() as Promise<T>;
}

// ── API functions ─────────────────────────────────────────────────────────────

export async function listTournaments(): Promise<TournamentSummary[]> {
  return apiFetch<TournamentSummary[]>("/api/tournaments");
}

export async function getTournament(id: string): Promise<TournamentDetail> {
  return apiFetch<TournamentDetail>(`/api/tournaments/${id}`);
}

export async function createTournament(
  params: CreateTournamentParams
): Promise<TournamentDetail> {
  return apiFetch<TournamentDetail>("/api/tournaments", {
    method: "POST",
    body: JSON.stringify(params),
  });
}

export async function registerPlayer(
  tournamentId: string,
  address: string,
  tableContract: string
): Promise<TournamentDetail> {
  return apiFetch<TournamentDetail>(
    `/api/tournaments/${tournamentId}/register`,
    {
      method: "POST",
      body: JSON.stringify({ address, table_contract: tableContract }),
    }
  );
}

export async function startTournament(
  tournamentId: string
): Promise<TournamentDetail> {
  return apiFetch<TournamentDetail>(
    `/api/tournaments/${tournamentId}/start`,
    { method: "POST" }
  );
}

export async function cancelTournament(
  tournamentId: string
): Promise<TournamentDetail> {
  return apiFetch<TournamentDetail>(
    `/api/tournaments/${tournamentId}/cancel`,
    { method: "POST" }
  );
}

export async function recordHandResult(
  tournamentId: string,
  stacks: Record<string, number>
): Promise<HandResultResponse> {
  return apiFetch<HandResultResponse>(
    `/api/tournaments/${tournamentId}/hand-result`,
    {
      method: "POST",
      body: JSON.stringify({ stacks }),
    }
  );
}

export async function getBalancingMoves(
  tournamentId: string
): Promise<{ balancing_moves: TableMove[]; current_small_blind: number; current_big_blind: number; blind_level: number }> {
  return apiFetch(`/api/tournaments/${tournamentId}/balancing`);
}

// ── Formatting helpers ────────────────────────────────────────────────────────

const STROOPS = 10_000_000;

export function stroopsToXlm(stroops: number): string {
  const xlm = stroops / STROOPS;
  return xlm % 1 === 0 ? xlm.toFixed(0) : xlm.toFixed(2);
}

export function statusLabel(status: TournamentStatus): string {
  const labels: Record<TournamentStatus, string> = {
    registration: "REGISTRATION",
    running: "IN PROGRESS",
    finalizing: "FINALIZING",
    completed: "COMPLETED",
    cancelled: "CANCELLED",
  };
  return labels[status] ?? status.toUpperCase();
}

export function statusColor(status: TournamentStatus): string {
  const colors: Record<TournamentStatus, string> = {
    registration: "#3498db",
    running: "#27ae60",
    finalizing: "#f39c12",
    completed: "#9b59b6",
    cancelled: "#e74c3c",
  };
  return colors[status] ?? "#95a5a6";
}

export function placeLabel(pos: number): string {
  if (pos === 1) return "1ST";
  if (pos === 2) return "2ND";
  if (pos === 3) return "3RD";
  return `${pos}TH`;
}

export function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}
