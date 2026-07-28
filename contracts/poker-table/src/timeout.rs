use soroban_sdk::{Address, Env, Symbol};

use crate::game;
use crate::game_hub;
use crate::types::*;

/// Process a timeout claim.
/// Anyone can call this if enough ledgers have passed since the last action.
pub fn process_timeout(
    env: &Env,
    table: &mut TableState,
    _claimer: &Address,
) -> Result<(), PokerTableError> {
    let current_ledger = env.ledger().sequence();
    let elapsed = current_ledger - table.last_action_ledger;

    if elapsed < table.config.timeout_ledgers {
        return Err(PokerTableError::TimeoutNotReached);
    }

    match table.phase {
        // Player timeout during betting — auto-fold the stalling player
        GamePhase::Preflop | GamePhase::Flop | GamePhase::Turn | GamePhase::River => {
            let seat = table.current_turn;
            let mut p = table
                .players
                .get(seat)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;

            if !p.folded && !p.all_in {
                p.folded = true;
                table.players.set(seat, p.clone());

                env.events().publish(
                    (Symbol::new(env, "timeout_fold"), table.id),
                    p.address.clone(),
                );

                // Check if only one player remains
                if game::active_player_count(table) == 1 {
                    game::settle_fold_win(env, table)?;
                } else {
                    // Advance to next player
                    let num_players = table.players.len() as u32;
                    let mut next = (seat + 1) % num_players;
                    for _ in 0..num_players {
                        let np = table
                            .players
                            .get(next)
                            .ok_or(PokerTableError::InvalidPlayerIndex)?;
                        if !np.folded && !np.all_in {
                            break;
                        }
                        next = (next + 1) % num_players;
                    }
                    table.current_turn = next;
                    table.last_action_ledger = current_ledger;
                }
            }
        }

        // RIT decision timeout — default to normal play (no RIT)
        GamePhase::AwaitingRunItTwice => {
            // Transition to the next normal phase as if RIT was declined
            table.phase = match table.board_cards.len() {
                0 => GamePhase::DealingFlop,
                3 => GamePhase::DealingTurn,
                4 => GamePhase::DealingRiver,
                _ => GamePhase::Showdown,
            };
            table.last_action_ledger = current_ledger;
            table.action_deadline = 0;
            env.events().publish(
                (Symbol::new(env, "rit_timeout"), table.id),
                (),
            );
        }

        // Committee timeout during dealing/reveal — dispute, return funds
        GamePhase::Dealing
        | GamePhase::DealingFlop
        | GamePhase::DealingTurn
        | GamePhase::DealingRiver
        | GamePhase::Showdown => {
            // Committee failed to act — enter dispute phase
            table.phase = GamePhase::Dispute;
            table.last_action_ledger = current_ledger;

            env.events().publish(
                (Symbol::new(env, "committee_timeout"), table.id),
                table.hand_number,
            );

            // Return all funds to players (emergency settlement)
            emergency_refund(env, table)?;

            // Notify Game Hub that the game ended (player1_won = true as default for dispute)
            game_hub::notify_end(env, &table.config.game_hub, table.session_id, true);
        }

        _ => {
            return Err(PokerTableError::TimeoutNotApplicable);
        }
    }
    Ok(())
}

/// Force-fold a stalling player after the action deadline has passed.
///
/// Any seated player (except the one whose turn it is) may call this once
/// the on-chain `action_deadline` ledger has been reached. The target player
/// is folded, the turn advances, and the deadline is reset for the new
/// active player.
pub fn force_fold(
    env: &Env,
    table: &mut TableState,
    caller: &Address,
    target_seat: u32,
) -> Result<(), PokerTableError> {
    let current_ledger = env.ledger().sequence();

    // Must be in a betting phase
    if !matches!(
        table.phase,
        GamePhase::Preflop | GamePhase::Flop | GamePhase::Turn | GamePhase::River
    ) {
        return Err(PokerTableError::ForceFoldNotAvailable);
    }

    // Deadline must have passed
    if table.action_deadline == 0 || current_ledger < table.action_deadline {
        return Err(PokerTableError::TimeoutNotReached);
    }

    // The target must be the current player to act
    if target_seat != table.current_turn {
        return Err(PokerTableError::TargetNotActive);
    }

    // Verify caller is a seated player (not the target)
    let caller_seat = find_seat_by_address(env, table, caller)?;
    if caller_seat == target_seat {
        return Err(PokerTableError::NotYourTurn);
    }

    // Target must be active (not folded, not all-in)
    let mut target = table
        .players
        .get(target_seat)
        .ok_or(PokerTableError::InvalidPlayerIndex)?;
    if target.folded || target.all_in {
        return Err(PokerTableError::TargetNotActive);
    }

    // Force fold the target
    target.folded = true;
    table.players.set(target_seat, target.clone());

    env.events().publish(
        (Symbol::new(env, "force_fold"), table.id),
        (target.address.clone(), caller.clone()),
    );

    // Check if only one player remains
    if game::active_player_count(table) == 1 {
        game::settle_fold_win(env, table)?;
    } else {
        // Advance to next player
        let num_players = table.players.len() as u32;
        let mut next = (target_seat + 1) % num_players;
        for _ in 0..num_players {
            let np = table
                .players
                .get(next)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;
            if !np.folded && !np.all_in {
                break;
            }
            next = (next + 1) % num_players;
        }
        table.current_turn = next;
        table.last_action_ledger = current_ledger;
        table.action_deadline = current_ledger + table.config.timeout_ledgers;
    }

    Ok(())
}

/// Find the seat index of a player by their address.
fn find_seat_by_address(
    _env: &Env,
    table: &TableState,
    address: &Address,
) -> Result<u32, PokerTableError> {
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if crate::constant_time::address_eq(_env, &p.address, address) {
            return Ok(p.seat_index);
        }
    }
    Err(PokerTableError::PlayerNotAtTable)
}

/// Emergency refund: return all player stacks + pot split equally
/// among non-folded players. Used when committee fails.
fn emergency_refund(_env: &Env, table: &mut TableState) -> Result<(), PokerTableError> {
    let active = game::active_player_count(table);
    if active == 0 {
        return Ok(());
    }

    let share = table.pot / (active as i128);
    let mut distributed: i128 = 0;

    for i in 0..table.players.len() {
        let mut p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if !p.folded {
            p.stack += share;
            distributed += share;
        }
        table.players.set(i, p);
    }

    // Handle remainder (give to first active player)
    let remainder = table.pot - distributed;
    if remainder > 0 {
        for i in 0..table.players.len() {
            let mut p = table
                .players
                .get(i)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;
            if !p.folded {
                p.stack += remainder;
                table.players.set(i, p);
                break;
            }
        }
    }

    table.pot = 0;
    table.phase = GamePhase::Settlement;
    Ok(())
}
