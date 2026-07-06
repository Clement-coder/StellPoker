"use client";

import { Card } from "./Card";
import { PixelCat, opponentSprite } from "./PixelCat";
import { PixelChip } from "./PixelChip";
import { Identicon } from "./Identicon";
import type { Player } from "@/lib/game-state";

interface PlayerSeatProps {
  player: Player;
  isCurrentTurn: boolean;
  isDealer: boolean;
  isUser: boolean;
  isWinner?: boolean;
  isBot?: boolean;
  labelOverride?: string;
  /** Client-side display alias for this seat's address, if one has been set. */
  alias?: string;
  /** Renders a small edit affordance next to the label (own seat only). */
  onEditAlias?: () => void;
  hideChipStats?: boolean;
  activeEmote?: string | null;
}

export function PlayerSeat({
  player,
  isCurrentTurn,
  isDealer,
  isUser,
  isWinner = false,
  isBot = false,
  labelOverride,
  alias,
  onEditAlias,
  hideChipStats = false,
  activeEmote = null,
}: PlayerSeatProps) {
  const sprite = isUser ? 18 : opponentSprite(player.seat);
  const cardSize = isUser ? "md" : "sm";
  const fallbackLabel = isUser
    ? "— YOU —"
    : isBot
      ? "— AI BOT —"
      : `${player.address.slice(0, 4)}...${player.address.slice(-4)}`;
  const displayLabel =
    labelOverride ?? (alias ? (isUser ? `${alias} (YOU)` : alias) : fallbackLabel);

  return (
    <div
      className="relative flex flex-col items-center gap-1"
      style={{
        opacity: player.folded ? 0.5 : 1,
        // Pulse glow on user's own seat when it's their turn (#47).
        boxShadow:
          isCurrentTurn && isUser && !player.folded
            ? "0 0 0 3px #f1c40f, 0 0 18px 6px rgba(241,196,15,0.45)"
            : undefined,
        borderRadius: isCurrentTurn && isUser ? 6 : undefined,
        animation:
          isCurrentTurn && isUser && !player.folded
            ? "seatPulse 1.2s ease-in-out infinite"
            : undefined,
        transition: "box-shadow 0.3s",
      }}
    >
      {activeEmote && (
        <div
          className="absolute z-50 bg-[#1a120c] border-2 border-[#8b6914] px-2 py-1 text-[16px] animate-float-up pointer-events-none text-center"
          style={{
            top: "-30px",
            boxShadow: "0 4px 0 rgba(0,0,0,0.3)",
          }}
        >
          {activeEmote}
          {/* Small Speech Bubble Tail */}
          <div
            className="absolute left-1/2 bottom-[-6px] translate-x-[-50%] w-0 h-0 border-l-[6px] border-l-transparent border-r-[6px] border-r-transparent border-t-[6px] border-t-[#1a120c]"
          />
          <div
            className="absolute left-1/2 bottom-[-8px] translate-x-[-50%] w-0 h-0 border-l-[6px] border-l-transparent border-r-[6px] border-r-transparent border-t-[6px] border-t-[#8b6914] z-[-1]"
          />
        </div>
      )}
      {/* Turn indicator */}
      {isCurrentTurn && !player.folded && (
        <div style={{
          animation: 'textPulse 1s ease-in-out infinite',
          fontSize: '9px',
          color: '#f1c40f',
          textShadow: '1px 1px 0 rgba(0,0,0,0.6)',
          whiteSpace: 'nowrap',
          marginBottom: '2px',
        }}>
          {isUser ? "▼ YOUR TURN ▼" : "▼ THEIR TURN ▼"}
        </div>
      )}

      {/* Winner badge */}
      {isWinner && (
        <div style={{
          fontSize: "9px",
          color: "#f1c40f",
          textShadow: "1px 1px 0 rgba(0,0,0,0.6)",
          marginBottom: '2px',
        }}>
          ★ WINNER ★
        </div>
      )}

      {/* Label */}
      <div className="text-[9px] mb-1 flex items-center gap-1" style={{
        color: isUser ? '#f1c40f' : '#95a5a6',
        textShadow: '1px 1px 0 rgba(0,0,0,0.5)',
      }}>
        <span>{displayLabel}</span>
        {isDealer && <span style={{ color: '#f1c40f' }}>[D]</span>}
        {onEditAlias && (
          <button
            onClick={onEditAlias}
            title="Change your alias"
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              color: '#95a5a6',
              fontSize: '8px',
              padding: 0,
            }}
          >
            [EDIT]
          </button>
        )}
      </div>

      {/* Avatar */}
      <div style={{ marginBottom: '4px', position: 'relative' }}>
        {isBot ? (
          <img
            src="/cat_sprites/bot.png"
            alt="AI Bot"
            width={48}
            height={48}
            style={{ imageRendering: "pixelated" }}
          />
        ) : (
          <>
            <PixelCat
              sprite={sprite}
              size={isUser ? 72 : 48}
              isUser={isUser}
            />
            {/* Deterministic identicon badge — a stable visual fingerprint of
                the seat's Stellar address, independent of the cat sprite
                (which is assigned by seat index, not identity). */}
            <div style={{ position: 'absolute', bottom: '-2px', right: '-2px' }}>
              <Identicon seed={player.address} size={5} cellSize={3} />
            </div>
          </>
        )}
      </div>

      {/* Cards */}
      <div className="flex gap-1">
        {player.cards ? (
          <>
            <Card value={player.cards[0]} size={cardSize} faceDown={!isUser} flip={isUser} />
            <Card value={player.cards[1]} size={cardSize} faceDown={!isUser} flip={isUser} flipDelay={0.08} />
          </>
        ) : (
          <>
            <Card faceDown size={cardSize} />
            <Card faceDown size={cardSize} />
          </>
        )}
      </div>

      {!hideChipStats && (
        <>
          {/* Stack */}
          <div className="flex items-center gap-1 mt-1">
            <PixelChip color={player.stack >= 5000 ? "gold" : player.stack >= 500 ? "blue" : "red"} size={isUser ? 2 : 1} />
            <span className="text-[10px]" style={{
              color: '#27ae60',
              textShadow: '1px 1px 0 rgba(0,0,0,0.4)',
            }}>
              {player.stack.toLocaleString()} CHIPS
            </span>
          </div>

          {/* Bet */}
          {player.betThisRound > 0 && (
            <div
              className="flex items-center gap-1"
              style={{
                animation: "chipBounce 0.4s ease-out",
              }}
            >
              <PixelChip color="gold" size={1} />
              <span className="text-[9px]" style={{ color: '#f1c40f' }}>
                BET: {player.betThisRound.toLocaleString()}
              </span>
            </div>
          )}
        </>
      )}

      {/* Status tags */}
      {player.folded && (
        <div className="text-[9px]" style={{ color: '#e74c3c' }}>FOLDED</div>
      )}
      {player.allIn && (
        <div className="text-[9px]" style={{
          color: '#e67e22',
          animation: 'textPulse 0.8s ease-in-out infinite',
        }}>
          ALL IN!
        </div>
      )}
    </div>
  );
}
