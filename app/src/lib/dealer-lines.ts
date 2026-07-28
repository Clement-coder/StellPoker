import type { GamePhase } from "@/lib/game-state";
import type { TableLobbyResponse } from "@/lib/api";
import { translate, type Locale, DEFAULT_LOCALE } from "@/lib/i18n";

type ActiveRequest = "deal" | "flop" | "turn" | "river" | "showdown" | null;
type PlayMode = "single" | "headsup" | "multi";

function shortAddr(address: string): string {
  return `${address.slice(0, 6)}...${address.slice(-6)}`;
}

export function getDealerLine(opts: {
  loading: boolean;
  elapsed: number;
  activeRequest: ActiveRequest;
  playMode: PlayMode;
  botLine: string | null;
  onChainPhase: string;
  gamePhase: GamePhase;
  wallet: boolean;
  isWalletSeated: boolean;
  seatedAddresses: string[];
  tableSeatLabel: string;
  winnerAddress: string | null;
  userAddress: string | undefined;
  lobby: TableLobbyResponse | null;
  /** Active UI locale (Issue #60). */
  locale?: Locale;
}): string {
  const locale = opts.locale ?? DEFAULT_LOCALE;
  const t = (key: string, vars?: Record<string, string | number>) =>
    translate(locale, key, vars);

  const formatElapsed = (s: number) => {
    const m = Math.floor(s / 60);
    const sec = s % 60;
    return m > 0 ? `${m}m ${sec}s` : `${sec}s`;
  };

  if (opts.loading) {
    const timer = ` [${formatElapsed(opts.elapsed)}]`;
    switch (opts.activeRequest) {
      case "deal":
        return t("dealer.shuffling", { timer });
      case "flop":
      case "turn":
      case "river":
        return t("dealer.revealProof", { timer });
      case "showdown":
        return t("dealer.showdown", { timer });
      default:
        return t("dealer.oneMoment", { timer });
    }
  }

  if (opts.playMode === "single" && opts.botLine && opts.gamePhase !== "waiting") {
    return `${opts.botLine}`;
  }

  if (opts.onChainPhase === "DealingFlop") {
    return t("dealer.dealingFlop");
  }
  if (opts.onChainPhase === "DealingTurn") {
    return t("dealer.dealingTurn");
  }
  if (opts.onChainPhase === "DealingRiver") {
    return t("dealer.dealingRiver");
  }
  if (opts.onChainPhase === "Showdown") {
    return t("dealer.resolvingShowdown");
  }

  if (opts.playMode !== "single" && opts.wallet && !opts.isWalletSeated && opts.seatedAddresses.length > 0) {
    return t("dealer.joinPrompt", { seats: opts.tableSeatLabel });
  }

  switch (opts.gamePhase) {
    case "waiting":
      if (opts.playMode === "single") {
        return t("dealer.soloStart");
      }
      if (opts.playMode === "headsup") {
        if ((opts.lobby?.joined_wallets ?? 0) < 2) {
          return t("dealer.headsUpWait");
        }
        return t("dealer.headsUpReady");
      }
      if ((opts.lobby?.joined_wallets ?? 0) < 3) {
        return t("dealer.multiWait");
      }
      return t("dealer.multiReady");
    case "dealing":
      return t("dealer.dealing");
    case "preflop":
      return t("dealer.preflop");
    case "flop":
      return t("dealer.flop");
    case "turn":
      return t("dealer.turn");
    case "river":
      return t("dealer.river");
    case "showdown":
      return t("dealer.showdownLive");
    case "settlement":
      if (opts.winnerAddress) {
        if (opts.userAddress && opts.winnerAddress === opts.userAddress) {
          return t("dealer.youWin");
        }
        if (opts.playMode === "single" && opts.userAddress && opts.winnerAddress !== opts.userAddress) {
          return t("dealer.aiWins");
        }
        return t("dealer.winner", { addr: shortAddr(opts.winnerAddress) });
      }
      return t("dealer.handComplete");
    default:
      return t("dealer.ready");
  }
}
