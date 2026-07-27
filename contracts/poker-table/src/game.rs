use soroban_sdk::{Env, Symbol, Vec};

use crate::constant_time;
use crate::game_hub;
use crate::history;
use crate::pot;
use crate::types::*;

/// Initialize state for a new hand.
pub fn start_new_hand(env: &Env, table: &mut TableState) -> Result<(), PokerTableError> {
    table.hand_number += 1;

    // Rotate dealer button
    let num_players = table.players.len() as u32;
    if num_players < table.config.min_players {
        return Err(PokerTableError::NotEnoughPlayers);
    }
    table.dealer_seat = (table.dealer_seat + 1) % num_players;

    // Reset player states
    for i in 0..table.players.len() {
        let mut p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        p.folded = false;
        p.all_in = false;
        p.bet_this_round = 0;
        p.committed = 0;
        table.players.set(i, p);
    }

    // Post blinds
    let sb_seat = (table.dealer_seat + 1) % num_players;
    let bb_seat = (table.dealer_seat + 2) % num_players;

    post_blind(table, sb_seat, table.config.small_blind)?;
    post_blind(table, bb_seat, table.config.big_blind)?;

    // Clear board state
    table.board_cards = Vec::new(env);
    table.dealt_indices = Vec::new(env);
    table.hand_commitments = Vec::new(env);
    table.side_pots = Vec::new(env);
    history::reset_actions(env, table);

    // Transition to dealing phase (committee will shuffle + deal)
    table.phase = GamePhase::Dealing;
    table.last_action_ledger = env.ledger().sequence();
    table.action_deadline = 0; // No action deadline during Dealing phase
    Ok(())
}

fn post_blind(table: &mut TableState, seat: u32, amount: i128) -> Result<(), PokerTableError> {
    let mut player = table
        .players
        .get(seat)
        .ok_or(PokerTableError::InvalidPlayerIndex)?;
    let actual = if player.stack < amount {
        player.all_in = true;
        player.stack
    } else {
        amount
    };

    player.stack -= actual;
    player.bet_this_round = actual;
    player.committed += actual;
    table.pot += actual;
    table.players.set(seat, player);
    Ok(())
}

/// Count players still active (not folded).
pub fn active_player_count(table: &TableState) -> u32 {
    let mut count = 0u32;
    for i in 0..table.players.len() {
        if let Some(p) = table.players.get(i) {
            if !p.folded {
                count += 1;
            }
        }
    }
    count
}

/// Find the single remaining player (when all others folded).
pub fn last_player_standing(table: &TableState) -> Option<u32> {
    if active_player_count(table) != 1 {
        return None;
    }
    for i in 0..table.players.len() {
        if let Some(p) = table.players.get(i) {
            if !p.folded {
                return Some(p.seat_index);
            }
        }
    }
    None
}

/// Settle the showdown using the winner_index proved by the ZK circuit.
///
/// The winner_index is a 0-based seat index determined by the showdown_valid
/// circuit, which evaluates all active hands against the secret deck and
/// commitments.  The committee-submitted hole_cards have already been verified
/// against the proof outputs by the caller.
///
/// `bad_beat_scores` is a vector of `(seat_index, hand_score)` pairs for every
/// non-folded player at showdown, submitted by the committee.  The contract
/// checks these against the bad-beat qualifying threshold and, if triggered,
/// pays the jackpot pool to the losing player with a qualifying hand (see
/// [`process_bad_beat_jackpot`]).  Pass an empty vec to skip the bad-beat
/// check (e.g. when the jackpot is not configured).
pub fn settle_showdown(
    env: &Env,
    table: &mut TableState,
    winner_seat: u32,
    tie_mask: u32,
    bad_beat_scores: &Vec<(u32, u32)>,
) -> Result<(), PokerTableError> {
    let total_pot = table.pot;

    // Compute the main pot and any side pots from cumulative contributions,
    // then deduct rake from each pot independently before awarding it to its
    // best eligible contributor. The proved winner is ranked first; the
    // remaining non-folded contenders follow in seat order so that side pots
    // the proved winner cannot win still go to an eligible player.
    let pots = pot::calculate_side_pots(env, table)?;
    let (net_pots, rake) = pot::apply_rake(env, &pots, table.config.rake_bps)?;
    table.side_pots = net_pots.clone();

    // Split rake between house and jackpot pool.
    let (house_rake, jackpot_rake) =
        pot::split_jackpot_rake(rake, table.config.jackpot_rake_share_bps);
    table.rake_balance += house_rake;
    table.jackpot_balance += jackpot_rake;

    let ranking = build_winner_ranking(env, table, winner_seat)?;
    let tied_winners = build_tied_winners(env, table, winner_seat, tie_mask)?;
    let payouts = pot::distribute_pots_with_ties(env, table, &net_pots, &tied_winners, &ranking)?;

    // Check for a qualifying bad beat before finalising.
    if !bad_beat_scores.is_empty() && table.config.jackpot_rake_share_bps > 0 {
        process_bad_beat_jackpot(env, table, winner_seat, bad_beat_scores)?;
    }

    table.pot = 0;
    table.phase = GamePhase::Settlement;
    table.last_action_ledger = env.ledger().sequence();

    history::archive_hand(env, table, &payouts, total_pot, rake, true)?;

    // Notify game hub: player1_won = true if the proved winner is seat 0.
    let player1_won = constant_time::u32_eq(winner_seat, 0);
    game_hub::notify_end(env, &table.config.game_hub, table.session_id, player1_won);

    let winner = table
        .players
        .get(winner_seat)
        .ok_or(PokerTableError::InvalidPlayerIndex)?;
    env.events().publish(
        (Symbol::new(env, "hand_settled"), table.id),
        (winner.address.clone(), total_pot, payouts),
    );
    if rake > 0 {
        env.events().publish(
            (Symbol::new(env, "rake_collected"), table.id),
            (table.hand_number, house_rake, jackpot_rake, table.rake_balance, table.jackpot_balance),
        );
    }
    Ok(())
}

/// Default split ratios for the bad-beat jackpot payout (in basis points):
///   - Loser (qualifying hand that lost): 6000 bps = 60%
///   - Winner:                             2000 bps = 20%
///   - Other dealt-in players (shared):    2000 bps = 20%
const JACKPOT_LOSER_SHARE_BPS: i128 = 6000;
const JACKPOT_WINNER_SHARE_BPS: i128 = 2000;
#[allow(dead_code)]
const JACKPOT_OTHERS_SHARE_BPS: i128 = 2000;

/// Check whether the submitted hand scores contain a qualifying bad beat and,
/// if so, pay out the current jackpot balance.
///
/// A bad beat is triggered when at least one non-winner player at showdown has
/// a hand score >= the qualifying threshold (computed from
/// `min_bad_beat_category` and `min_bad_beat_rank`).  The *best* such
 /// qualifying losing hand receives the largest share (60%), the winner
/// receives 20%, and the remaining dealt-in players split 20% equally.
///
/// The jackpot balance is reset to zero after payout.
fn process_bad_beat_jackpot(
    env: &Env,
    table: &mut TableState,
    winner_seat: u32,
    scores: &Vec<(u32, u32)>,
) -> Result<(), PokerTableError> {
    if table.jackpot_balance <= 0 {
        return Ok(()); // nothing to pay out
    }

    let threshold = pot::min_bad_beat_qualifying_score(
        table.config.min_bad_beat_category,
        table.config.min_bad_beat_rank,
    );

    // Find the best qualifying losing hand (highest score that is not the
    // winner and meets the threshold).
    let mut best_loser_seat: Option<u32> = None;
    let mut best_loser_score: u32 = 0;

    for i in 0..scores.len() {
        let (seat, score) = scores
            .get(i)
            .ok_or(PokerTableError::BadBeatHandDataInvalid)?;
        if constant_time::u32_eq(seat, winner_seat) {
            continue;
        }
        if score >= threshold && score > best_loser_score {
            best_loser_score = score;
            best_loser_seat = Some(seat);
        }
    }

    let loser_seat = match best_loser_seat {
        Some(s) => s,
        None => return Ok(()), // no qualifying loser
    };

    // Payout the jackpot.
    let jackpot = table.jackpot_balance;
    table.jackpot_balance = 0;

    // 60% to the losing player with the qualifying hand.
    let loser_share = (jackpot * JACKPOT_LOSER_SHARE_BPS) / 10_000;
    let mut remaining = jackpot - loser_share;
    credit_player_by_seat(table, loser_seat, loser_share)?;

    // 20% to the winner.
    let winner_share = (jackpot * JACKPOT_WINNER_SHARE_BPS) / 10_000;
    remaining -= winner_share;
    credit_player_by_seat(table, winner_seat, winner_share)?;

    // 20% split equally among other dealt-in (non-folded, non-winner,
    // non-loser) players.
    let mut other_seats: Vec<u32> = Vec::new(env);
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if !p.folded
            && constant_time::u32_ne(p.seat_index, winner_seat)
            && constant_time::u32_ne(p.seat_index, loser_seat)
        {
            other_seats.push_back(p.seat_index);
        }
    }

    let others_total = remaining;
    if !other_seats.is_empty() && others_total > 0 {
        let share = others_total / other_seats.len() as i128;
        let mut remainder = others_total % other_seats.len() as i128;
        for i in 0..other_seats.len() {
            let seat = other_seats
                .get(i)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;
            let odd = if remainder > 0 {
                remainder -= 1;
                1
            } else {
                0
            };
            credit_player_by_seat(table, seat, share + odd)?;
        }
    } else if others_total > 0 {
        // No other players – give the leftover to the loser.
        credit_player_by_seat(table, loser_seat, others_total)?;
    }

    env.events().publish(
        (Symbol::new(env, "bad_beat_jackpot"), table.id),
        (winner_seat, loser_seat, jackpot, loser_share, winner_share),
    );

    Ok(())
}

fn credit_player_by_seat(
    table: &mut TableState,
    seat: u32,
    amount: i128,
) -> Result<(), PokerTableError> {
    let mut player = table
        .players
        .get(seat)
        .ok_or(PokerTableError::InvalidPlayerIndex)?;
    player.stack += amount;
    table.players.set(seat, player);
    Ok(())
}

fn build_tied_winners(
    env: &Env,
    table: &TableState,
    winner_seat: u32,
    tie_mask: u32,
) -> Result<Vec<u32>, PokerTableError> {
    let mut winners: Vec<u32> = Vec::new(env);
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.folded {
            continue;
        }
        let seat = p.seat_index;
        let tied = constant_time::u32_eq(seat, winner_seat)
            || constant_time::u32_ne(tie_mask & (1u32 << seat), 0);
        if tied {
            winners.push_back(seat);
        }
    }
    if winners.is_empty() {
        return Err(PokerTableError::WinnerNotEligibleForPot);
    }
    Ok(winners)
}

/// Build a best-first ranking of contenders for pot distribution. The ZK
/// showdown proof establishes the single overall winner; we place that seat
/// first and append the remaining non-folded players in seat order. For the
/// common case (no side pots, or the proved winner eligible everywhere) this
/// awards the entire pot to the proved winner. When side pots exist that the
/// proved winner did not contribute to, the next eligible contender wins them.
fn build_winner_ranking(
    env: &Env,
    table: &TableState,
    winner_seat: u32,
) -> Result<Vec<u32>, PokerTableError> {
    let mut ranking: Vec<u32> = Vec::new(env);
    ranking.push_back(winner_seat);
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.folded || constant_time::u32_eq(p.seat_index, winner_seat) {
            continue;
        }
        ranking.push_back(p.seat_index);
    }
    Ok(ranking)
}

/// Award pot to last player standing (all others folded).
pub fn settle_fold_win(env: &Env, table: &mut TableState) -> Result<(), PokerTableError> {
    if let Some(winner_seat) = last_player_standing(table) {
        let total_pot = table.pot;
        let rake = (total_pot * table.config.rake_bps as i128) / 10_000;
        let winnings = total_pot - rake;
        let mut winner = table
            .players
            .get(winner_seat)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        winner.stack += winnings;
        table.players.set(winner_seat, winner.clone());
        table.pot = 0;
        table.rake_balance += rake;
        table.phase = GamePhase::Settlement;
        table.last_action_ledger = env.ledger().sequence();

        let mut payouts: Vec<(u32, i128)> = Vec::new(env);
        payouts.push_back((winner_seat, winnings));
        history::archive_hand(env, table, &payouts, total_pot, rake, false)?;

        // Notify game hub
        let player1_won = constant_time::u32_eq(winner_seat, 0);
        game_hub::notify_end(env, &table.config.game_hub, table.session_id, player1_won);

        env.events().publish(
            (Symbol::new(env, "fold_win"), table.id),
            (winner.address.clone(), winnings),
        );
        if rake > 0 {
            env.events().publish(
                (Symbol::new(env, "rake_collected"), table.id),
                (table.hand_number, rake, table.rake_balance),
            );
        }
    }
    Ok(())
}
