export type GamePhase =
  | "waiting"
  | "dealing"
  | "preflop"
  | "flop"
  | "turn"
  | "river"
  | "showdown"
  | "settlement"
  | "awaiting_run_it_twice"
  | "showdown_run1"
  | "showdown_run2"
  | "rit_settlement";

export interface Player {
  address: string;
  seat: number;
  stack: number;
  betThisRound: number;
  folded: boolean;
  allIn: boolean;
  cards?: [number, number];
}

export interface RitState {
  active: boolean;
  player1Seat: number;
  player2Seat: number;
  player1OptedIn: boolean;
  player2OptedIn: boolean;
  sharedBoardCount: number;
  currentRun: number;
  run1BoardIndices: number[];
  run2BoardIndices: number[];
  run1Winner: number;
  run2Winner: number;
}

export interface GameState {
  tableId: number;
  phase: GamePhase;
  players: Player[];
  pot: number;
  boardCards: number[];
  boardCardsRun2?: number[];
  currentTurn: number;
  dealerSeat: number;
  handNumber: number;
  lastTxHash?: string;
  proofSize?: number;
  onChainConfirmed: boolean;
  ritState?: RitState;
}

export function createInitialState(tableId: number): GameState {
  return {
    tableId,
    phase: "waiting",
    players: [],
    pot: 0,
    boardCards: [],
    currentTurn: 0,
    dealerSeat: 0,
    handNumber: 0,
    onChainConfirmed: false,
  };
}
