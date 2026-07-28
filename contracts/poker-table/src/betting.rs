use soroban_sdk::{Address, Env, Symbol};

use crate::constant_time;
use crate::game;
use crate::history;
use crate::types::*;

/// Process a player's betting action.
pub fn process_action(
    env: &Env,
    table: &mut TableState,
    player: &Address,
    action: &Action,
) -> Result<(), PokerTableError> {
    // Find the player
    let seat = find_player_seat(env, table, player)?;
    if constant_time::u32_ne(seat, table.current_turn) {
        return Err(PokerTableError::NotYourTurn);
    }

    let mut p = table
        .players
        .get(seat)
        .ok_or(PokerTableError::InvalidPlayerIndex)?;
    if p.folded {
        return Err(PokerTableError::PlayerAlreadyFolded);
    }
    if p.all_in {
        return Err(PokerTableError::PlayerAlreadyAllIn);
    }

    let current_bet = max_bet_this_round(table)?;
    let pot_before = table.pot;

    match action {
        Action::Fold => {
            p.folded = true;
            table.players.set(seat, p);

            // Check if only one player remains
            if game::active_player_count(table) == 1 {
                emit_action(env, table, player, action, 0);
                history::record_action(env, table, seat, action, 0);
                game::settle_fold_win(env, table)?;
                return Ok(());
            }
        }
        Action::Check => {
            if p.bet_this_round != current_bet {
                return Err(PokerTableError::MustCallOrFold);
            }
        }
        Action::Call => {
            let to_call = current_bet - p.bet_this_round;
            if to_call <= 0 {
                return Err(PokerTableError::NothingToCall);
            }
            let actual = core::cmp::min(to_call, p.stack);

            p.stack -= actual;
            p.bet_this_round += actual;
            p.committed += actual;
            table.pot += actual;

            if p.stack == 0 {
                p.all_in = true;
            }
            table.players.set(seat, p);
        }
        Action::Bet(amount) => {
            if current_bet != 0 {
                return Err(PokerTableError::CannotBetWhenOutstandingBet);
            }
            if *amount < table.config.big_blind {
                return Err(PokerTableError::BetTooSmall);
            }
            if *amount > p.stack {
                return Err(PokerTableError::NotEnoughChips);
            }

            p.stack -= *amount;
            p.bet_this_round += *amount;
            p.committed += *amount;
            table.pot += *amount;

            if p.stack == 0 {
                p.all_in = true;
            }
            table.players.set(seat, p);
        }
        Action::Raise(amount) => {
            let to_call = current_bet - p.bet_this_round;
            let total_needed = to_call + *amount;
            if *amount < table.config.big_blind {
                return Err(PokerTableError::RaiseTooSmall);
            }
            if total_needed > p.stack {
                return Err(PokerTableError::NotEnoughChips);
            }

            p.stack -= total_needed;
            p.bet_this_round += total_needed;
            p.committed += total_needed;
            table.pot += total_needed;

            if p.stack == 0 {
                p.all_in = true;
            }
            table.players.set(seat, p);
        }
        Action::AllIn => {
            let amount = p.stack;
            p.bet_this_round += amount;
            p.committed += amount;
            table.pot += amount;
            p.stack = 0;
            p.all_in = true;
            table.players.set(seat, p);
        }
    }

    // Emit a per-action event so the frontend can react without polling. The
    // amount is the chips added to the pot by this action (0 for fold/check).
    emit_action(env, table, player, action, table.pot - pot_before);
    history::record_action(env, table, seat, action, table.pot - pot_before);

    table.last_action_ledger = env.ledger().sequence();
    // Reset action deadline for the next player
    table.action_deadline = env.ledger().sequence() + table.config.timeout_ledgers;

    // Advance turn
    advance_turn(env, table)
}

/// Publish a `player_action` event describing a betting move. Topic carries the
/// action type symbol; data is `(player, amount_added)`.
fn emit_action(env: &Env, table: &TableState, player: &Address, action: &Action, amount: i128) {
    let label = match action {
        Action::Fold => "fold",
        Action::Check => "check",
        Action::Call => "call",
        Action::Bet(_) => "bet",
        Action::Raise(_) => "raise",
        Action::AllIn => "all_in",
    };
    env.events().publish(
        (
            Symbol::new(env, "player_action"),
            table.id,
            Symbol::new(env, label),
        ),
        (player.clone(), amount),
    );
}

/// Reset betting state for a new round.
pub fn reset_round(env: &Env, table: &mut TableState) -> Result<(), PokerTableError> {
    for i in 0..table.players.len() {
        let mut p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        p.bet_this_round = 0;
        table.players.set(i, p);
    }

    // First active player after dealer acts first post-flop
    let num_players = table.players.len() as u32;
    if num_players == 0 {
        return Err(PokerTableError::NotEnoughPlayers);
    }
    let mut seat = (table.dealer_seat + 1) % num_players;
    for _ in 0..num_players {
        let p = table
            .players
            .get(seat)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if !p.folded && !p.all_in {
            table.current_turn = seat;
            return Ok(());
        }
        seat = (seat + 1) % num_players;
    }

    // All players are all-in or folded — skip to next deal phase
    advance_to_next_phase(env, table)
}

/// Advance to the next player's turn, or end the betting round.
fn advance_turn(env: &Env, table: &mut TableState) -> Result<(), PokerTableError> {
    let num_players = table.players.len() as u32;
    if num_players == 0 {
        return Err(PokerTableError::NotEnoughPlayers);
    }
    let mut next = (table.current_turn + 1) % num_players;

    // Find next active player
    for _ in 0..num_players {
        let p = table
            .players
            .get(next)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if !p.folded && !p.all_in {
            break;
        }
        next = (next + 1) % num_players;
    }

    // Check if betting round is complete
    if is_round_complete(table)? {
        advance_to_next_phase(env, table)?;
    } else {
        table.current_turn = next;
    }
    Ok(())
}

/// Check if all active players have matched the current bet.
fn is_round_complete(table: &TableState) -> Result<bool, PokerTableError> {
    let current_bet = max_bet_this_round(table)?;
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.folded || p.all_in {
            continue;
        }
        if p.bet_this_round != current_bet {
            return Ok(false);
        }
    }

    // All active non-all-in players have matched the current bet
    Ok(true)
}

/// Check if exactly 2 non-folded players are both all-in (heads-up all-in).
fn is_heads_up_all_in(table: &TableState) -> bool {
    let mut non_folded: u32 = 0;
    let mut all_in_count: u32 = 0;
    for i in 0..table.players.len() {
        if let Some(p) = table.players.get(i) {
            if !p.folded {
                non_folded += 1;
                if p.all_in {
                    all_in_count += 1;
                }
            }
        }
    }
    non_folded == 2 && all_in_count == 2
}

/// Advance to the next game phase, checking for Run-It-Twice condition.
fn advance_to_next_phase(env: &Env, table: &mut TableState) -> Result<(), PokerTableError> {
    // If only one player left, settle immediately
    if game::active_player_count(table) == 1 {
        game::settle_fold_win(env, table)?;
        return Ok(());
    }

    // Check for RIT condition: 2 players heads-up both all-in, RIT not yet decided
    if table.rit_state.is_none()
        && is_heads_up_all_in(table)
        && !matches!(table.phase, GamePhase::River)
    {
        table.phase = GamePhase::AwaitingRunItTwice;
        table.last_action_ledger = env.ledger().sequence();
        // No action deadline during RIT decision (committee phases)
        table.action_deadline = 0;
        env.events().publish(
            (Symbol::new(env, "phase_change"), table.id),
            table.phase.clone(),
        );
        return Ok(());
    }

    // If RIT is active, handle special phase transitions
    if let Some(ref rit) = table.rit_state {
        if rit.active {
            if matches!(table.phase, GamePhase::River) {
                // River completes -> go to appropriate showdown phase
                table.phase = if rit.current_run == 2 {
                    GamePhase::ShowdownRun2
                } else {
                    GamePhase::ShowdownRun1
                };
                table.last_action_ledger = env.ledger().sequence();
                table.action_deadline = 0;
                env.events().publish(
                    (Symbol::new(env, "phase_change"), table.id),
                    table.phase.clone(),
                );
                return Ok(());
            }
            // For RIT phases, skip betting (all-in) and go straight to next deal
            table.phase = match table.phase {
                GamePhase::Preflop => GamePhase::DealingFlop,
                GamePhase::Flop => GamePhase::DealingTurn,
                GamePhase::Turn => GamePhase::DealingRiver,
                _ => return Ok(()),
            };
            table.last_action_ledger = env.ledger().sequence();
            table.action_deadline = 0;
            env.events().publish(
                (Symbol::new(env, "phase_change"), table.id),
                table.phase.clone(),
            );
            return Ok(());
        }
    }

    table.phase = match table.phase {
        GamePhase::Preflop => GamePhase::DealingFlop,
        GamePhase::Flop => GamePhase::DealingTurn,
        GamePhase::Turn => GamePhase::DealingRiver,
        GamePhase::River => GamePhase::Showdown,
        _ => return Ok(()),
    };
    table.last_action_ledger = env.ledger().sequence();
    // No action deadline during committee phases (Dealing/Reveal/Showdown)
    table.action_deadline = 0;

    env.events().publish(
        (Symbol::new(env, "phase_change"), table.id),
        table.phase.clone(),
    );
    Ok(())
}

fn find_player_seat(
    env: &Env,
    table: &TableState,
    player: &Address,
) -> Result<u32, PokerTableError> {
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if constant_time::address_eq(env, &p.address, player) {
            return Ok(p.seat_index);
        }
    }
    Err(PokerTableError::PlayerNotAtTable)
}

fn max_bet_this_round(table: &TableState) -> Result<i128, PokerTableError> {
    let mut max_bet: i128 = 0;
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.bet_this_round > max_bet {
            max_bet = p.bet_this_round;
        }
    }
    Ok(max_bet)
}
