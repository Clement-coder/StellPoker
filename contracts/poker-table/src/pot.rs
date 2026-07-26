use soroban_sdk::{Env, Vec};

use crate::constant_time;
use crate::types::*;

/// Maximum allowed rake, in basis points (5%). Enforced at table creation.
pub const MAX_RAKE_BPS: u32 = 500;

/// Deduct rake from each pot before distribution. Rake is computed per pot as
/// `floor(pot.amount * rake_bps / 10_000)`, so the fee is predictable and
/// proportional to each pot's own size — side pots are raked independently of
/// the main pot, exactly like the main pot. Eligibility is unaffected.
///
/// Returns the rake-adjusted pots (same eligibility, reduced amount) and the
/// total rake collected across all pots.
pub fn apply_rake(
    env: &Env,
    pots: &Vec<SidePot>,
    rake_bps: u32,
) -> Result<(Vec<SidePot>, i128), PokerTableError> {
    let mut net_pots: Vec<SidePot> = Vec::new(env);
    let mut total_rake: i128 = 0;

    for i in 0..pots.len() {
        let pot = pots.get(i).ok_or(PokerTableError::InvalidPlayerIndex)?;
        let rake = (pot.amount * rake_bps as i128) / 10_000;
        total_rake += rake;
        net_pots.push_back(SidePot {
            amount: pot.amount - rake,
            eligible_players: pot.eligible_players,
        });
    }

    Ok((net_pots, total_rake))
}

/// Compute the main pot and any side pots for the current hand.
///
/// Side pots are required whenever players go all-in for different total
/// amounts. Each pot is formed from a "layer" of contributions between two
/// consecutive distinct all-in (or final) commitment levels. A player is only
/// eligible to win a pot if they contributed the full amount of that layer —
/// i.e. their total `committed` for the hand is at least that layer's level —
/// which is exactly the Texas Hold'em rule that a player can only win the chips
/// they have matched.
///
/// The pots are returned in order, main pot first, followed by side pots from
/// the lowest all-in level upward. Folded players still contribute their chips
/// (dead money stays in the pots they helped build) but are never eligible to
/// win. The sum of all returned pot amounts always equals `table.pot`.
///
/// Reference: <https://en.wikipedia.org/wiki/Side_pot>
pub fn calculate_side_pots(env: &Env, table: &TableState) -> Result<Vec<SidePot>, PokerTableError> {
    let mut pots: Vec<SidePot> = Vec::new(env);

    // Distinct, ascending contribution levels at which a pot boundary occurs.
    // We only need a boundary where a *contender* (non-folded player) is capped
    // (all-in). Players who bet more than the highest such cap form the top
    // layer, which is handled by the trailing pass below.
    let levels = distinct_all_in_levels(env, table)?;

    // The top boundary is the highest amount committed by any *non-folded*
    // player. Everything between the last all-in level and this cap forms the
    // top pot, contested by players who still had chips behind. Folded players
    // never extend the eligible ceiling (they can only add dead money).
    let top_level = max_contender_committed(env, table)?;

    let mut prev_level: i128 = 0;
    for li in 0..levels.len() {
        let level = levels.get(li).ok_or(PokerTableError::InvalidPlayerIndex)?;
        if level <= prev_level || level > top_level {
            continue;
        }
        let pot = build_layer(env, table, prev_level, level)?;
        if pot.amount > 0 {
            pots.push_back(pot);
        }
        prev_level = level;
    }

    // Top layer: every remaining chip above the last all-in level. This sweeps
    // contributions up to the highest contender commitment plus any "dead money"
    // a folded player committed beyond that cap, so no chips are ever orphaned.
    // Eligibility is still capped at `top_level` (only players who reached the
    // top contender level can win it).
    if top_level <= prev_level {
        sweep_excess_dead_money_into_last_pot(&mut pots, table, prev_level)?;
    } else {
        let top = build_top_layer(env, table, prev_level, top_level)?;
        if top.amount > 0 {
            pots.push_back(top);
        }
    }

    Ok(pots)
}

/// Collect the distinct total-commitment levels of non-folded all-in players,
/// sorted ascending. These are the boundaries between the main pot and side
/// pots. Folded players never create a boundary (they cannot win), though their
/// chips are still swept into whichever layers they reach.
fn distinct_all_in_levels(env: &Env, table: &TableState) -> Result<Vec<i128>, PokerTableError> {
    let mut levels: Vec<i128> = Vec::new(env);
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.folded || !p.all_in || p.committed <= 0 {
            continue;
        }
        insert_sorted_unique(&mut levels, p.committed);
    }
    Ok(levels)
}

/// Insert `value` into an ascending vector, keeping it sorted and de-duplicated.
fn insert_sorted_unique(levels: &mut Vec<i128>, value: i128) {
    for j in 0..levels.len() {
        let existing = levels.get(j).unwrap_or(0);
        if constant_time::i128_eq(value, existing) {
            return; // already present
        }
        if value < existing {
            levels.insert(j, value);
            return;
        }
    }
    levels.push_back(value);
}

/// Build a single pot layer covering contributions in the half-open band
/// `(lower, upper]`. Every player (including folded ones) contributes
/// `clamp(committed, lower, upper) - lower` chips to this layer. A non-folded
/// player is eligible to win it only if their total commitment reaches `upper`
/// (the top of the band), i.e. they fully covered this layer.
fn build_layer(
    env: &Env,
    table: &TableState,
    lower: i128,
    upper: i128,
) -> Result<SidePot, PokerTableError> {
    let mut amount: i128 = 0;
    let mut eligible: Vec<u32> = Vec::new(env);

    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.committed <= lower {
            continue;
        }
        let capped = core::cmp::min(p.committed, upper);
        amount += capped - lower;
        if !p.folded && p.committed >= upper {
            eligible.push_back(p.seat_index);
        }
    }

    Ok(SidePot {
        amount,
        eligible_players: eligible,
    })
}

/// The highest amount committed by any non-folded (contender) player. This is
/// the ceiling of the top pot's eligibility — only players who reached it can
/// win the final layer.
fn max_contender_committed(env: &Env, table: &TableState) -> Result<i128, PokerTableError> {
    let _ = env;
    let mut max = 0i128;
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if !p.folded && p.committed > max {
            max = p.committed;
        }
    }
    Ok(max)
}

/// Build the final pot layer. Its amount sweeps every chip committed above
/// `lower` (including dead money from folded players who committed past the
/// contender ceiling), guaranteeing no chips are orphaned. Eligibility is
/// limited to non-folded players who committed up to `eligible_ceiling`.
fn build_top_layer(
    env: &Env,
    table: &TableState,
    lower: i128,
    eligible_ceiling: i128,
) -> Result<SidePot, PokerTableError> {
    let mut amount: i128 = 0;
    let mut eligible: Vec<u32> = Vec::new(env);

    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.committed <= lower {
            continue;
        }
        amount += p.committed - lower;
        if !p.folded && p.committed >= eligible_ceiling && eligible_ceiling > lower {
            eligible.push_back(p.seat_index);
        }
    }

    Ok(SidePot {
        amount,
        eligible_players: eligible,
    })
}

fn sweep_excess_dead_money_into_last_pot(
    pots: &mut Vec<SidePot>,
    table: &TableState,
    lower: i128,
) -> Result<(), PokerTableError> {
    if pots.len() == 0 {
        return Ok(());
    }
    let mut excess: i128 = 0;
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.folded && p.committed > lower {
            excess += p.committed - lower;
        }
    }
    if excess > 0 {
        let last_index = pots.len() - 1;
        let mut last = pots
            .get(last_index)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        last.amount += excess;
        pots.set(last_index, last);
    }
    Ok(())
}

/// Distribute every pot to its winner. `winner_seats` lists the seat indices of
/// the hand's ranked contenders best-first (as proved by the showdown circuit).
/// For each pot, the chips go to the highest-ranked contender that is eligible
/// for that pot (the best hand among players who contributed to it). Returns the
/// per-seat winnings credited so callers can emit a payout breakdown.
///
/// This guarantees the core invariant that a player can only win pots they have
/// contributed to, and that the total distributed equals the sum of the pots.
#[cfg(test)]
pub fn distribute_pots(
    env: &Env,
    table: &mut TableState,
    pots: &Vec<SidePot>,
    winner_seats: &Vec<u32>,
) -> Result<Vec<(u32, i128)>, PokerTableError> {
    let mut payouts: Vec<(u32, i128)> = Vec::new(env);

    for pi in 0..pots.len() {
        let pot = pots.get(pi).ok_or(PokerTableError::InvalidPlayerIndex)?;
        if pot.amount <= 0 {
            continue;
        }
        let winner_seat = best_eligible_winner(winner_seats, &pot.eligible_players)
            .ok_or(PokerTableError::WinnerNotEligibleForPot)?;

        let mut winner = table
            .players
            .get(winner_seat)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        winner.stack += pot.amount;
        table.players.set(winner_seat, winner);

        accumulate_payout(&mut payouts, winner_seat, pot.amount);
    }

    Ok(payouts)
}

/// Distribute pots when the best hand is shared by multiple tied seats. For
/// each pot, every tied winner eligible for that pot receives an equal share;
/// any odd chip remainder is awarded one chip at a time to the earliest seat.
/// If none of the tied winners can win a side pot, the fallback ranking is used
/// to award that side pot to the best eligible non-tied contender.
pub fn distribute_pots_with_ties(
    env: &Env,
    table: &mut TableState,
    pots: &Vec<SidePot>,
    tied_winner_seats: &Vec<u32>,
    fallback_ranking: &Vec<u32>,
) -> Result<Vec<(u32, i128)>, PokerTableError> {
    let mut payouts: Vec<(u32, i128)> = Vec::new(env);

    for pi in 0..pots.len() {
        let pot = pots.get(pi).ok_or(PokerTableError::InvalidPlayerIndex)?;
        if pot.amount <= 0 {
            continue;
        }

        let recipients = eligible_tied_winners(env, tied_winner_seats, &pot.eligible_players);
        if recipients.is_empty() {
            let winner_seat = best_eligible_winner(fallback_ranking, &pot.eligible_players)
                .ok_or(PokerTableError::WinnerNotEligibleForPot)?;
            credit_player(table, &mut payouts, winner_seat, pot.amount)?;
            continue;
        }

        let share = pot.amount / recipients.len() as i128;
        let mut remainder = pot.amount % recipients.len() as i128;
        for i in 0..recipients.len() {
            let seat = recipients
                .get(i)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;
            let odd_chip = if remainder > 0 {
                remainder -= 1;
                1
            } else {
                0
            };
            credit_player(table, &mut payouts, seat, share + odd_chip)?;
        }
    }

    Ok(payouts)
}

fn eligible_tied_winners(env: &Env, tied: &Vec<u32>, eligible: &Vec<u32>) -> Vec<u32> {
    let mut recipients = Vec::new(env);
    for ei in 0..eligible.len() {
        if let Some(seat) = eligible.get(ei) {
            for ti in 0..tied.len() {
                if tied
                    .get(ti)
                    .map(|candidate| constant_time::u32_eq(candidate, seat))
                    .unwrap_or(false)
                {
                    recipients.push_back(seat);
                    break;
                }
            }
        }
    }
    recipients
}

fn credit_player(
    table: &mut TableState,
    payouts: &mut Vec<(u32, i128)>,
    seat: u32,
    amount: i128,
) -> Result<(), PokerTableError> {
    let mut player = table
        .players
        .get(seat)
        .ok_or(PokerTableError::InvalidPlayerIndex)?;
    player.stack += amount;
    table.players.set(seat, player);
    accumulate_payout(payouts, seat, amount);
    Ok(())
}

/// Find the best-ranked seat (earliest in `ranked`) that is eligible for a pot.
fn best_eligible_winner(ranked: &Vec<u32>, eligible: &Vec<u32>) -> Option<u32> {
    for ri in 0..ranked.len() {
        let seat = ranked.get(ri)?;
        for ei in 0..eligible.len() {
            if eligible
                .get(ei)
                .map(|candidate| constant_time::u32_eq(candidate, seat))
                .unwrap_or(false)
            {
                return Some(seat);
            }
        }
    }
    None
}

/// Add `amount` to the running total for `seat`, merging into an existing entry
/// when a seat wins more than one pot.
fn accumulate_payout(payouts: &mut Vec<(u32, i128)>, seat: u32, amount: i128) {
    for i in 0..payouts.len() {
        if let Some((s, existing)) = payouts.get(i) {
            if constant_time::u32_eq(s, seat) {
                payouts.set(i, (seat, existing + amount));
                return;
            }
        }
    }
    payouts.push_back((seat, amount));
}

#[cfg(test)]
mod pot_test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    /// Build a player with a given total commitment and all-in / folded flags.
    fn player(env: &Env, seat: u32, committed: i128, all_in: bool, folded: bool) -> PlayerState {
        PlayerState {
            address: Address::generate(env),
            stack: 0,
            bet_this_round: 0,
            committed,
            folded,
            all_in,
            sitting_out: false,
            seat_index: seat,
        }
    }

    /// Assemble a minimal TableState whose `pot` equals the sum of commitments.
    fn table_with(env: &Env, players: Vec<PlayerState>) -> TableState {
        let mut pot: i128 = 0;
        for i in 0..players.len() {
            pot += players.get(i).unwrap().committed;
        }
        let admin = Address::generate(env);
        TableState {
            id: 0,
            admin: admin.clone(),
            config: TableConfig {
                token: Address::generate(env),
                min_buy_in: 0,
                max_buy_in: i128::MAX,
                small_blind: 0,
                big_blind: 0,
                min_players: 2,
                max_players: 9,
                timeout_ledgers: 0,
                committee: admin.clone(),
                verifier: admin.clone(),
                game_hub: admin.clone(),
                rake_bps: 0,
            },
            phase: GamePhase::Showdown,
            players,
            dealer_seat: 0,
            current_turn: 0,
            pot,
            side_pots: Vec::new(env),
            deck_root: BytesN::from_array(env, &[0u8; 32]),
            hand_commitments: Vec::new(env),
            board_cards: Vec::new(env),
            dealt_indices: Vec::new(env),
            hand_number: 1,
            last_action_ledger: 0,
            committee: admin,
            session_id: 0,
            rake_balance: 0,
        }
    }

    fn sum_pots(pots: &Vec<SidePot>) -> i128 {
        let mut total = 0i128;
        for i in 0..pots.len() {
            total += pots.get(i).unwrap().amount;
        }
        total
    }

    fn seats(env: &Env, ids: &[u32]) -> Vec<u32> {
        let mut v = Vec::new(env);
        for id in ids {
            v.push_back(*id);
        }
        v
    }

    // -----------------------------------------------------------------------
    // calculate_side_pots
    // -----------------------------------------------------------------------

    #[test]
    fn no_all_in_single_main_pot() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 100, false, false),
                player(&env, 1, 100, false, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        assert_eq!(pots.len(), 1);
        let main = pots.get(0).unwrap();
        assert_eq!(main.amount, 300);
        assert_eq!(main.eligible_players, seats(&env, &[0, 1, 2]));
    }

    #[test]
    fn three_way_unequal_all_ins() {
        // Classic case: stacks 50 / 100 / 100.
        // Main pot = 3*50 = 150 (all three eligible).
        // Side pot = 2*50 = 100 (seats 1 and 2 eligible).
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 50, true, false),
                player(&env, 1, 100, true, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        assert_eq!(sum_pots(&pots), 250);
        assert_eq!(pots.len(), 2);

        let main = pots.get(0).unwrap();
        assert_eq!(main.amount, 150);
        assert_eq!(main.eligible_players, seats(&env, &[0, 1, 2]));

        let side = pots.get(1).unwrap();
        assert_eq!(side.amount, 100);
        assert_eq!(side.eligible_players, seats(&env, &[1, 2]));
    }

    #[test]
    fn four_way_three_distinct_all_in_levels() {
        // Commitments 20 / 50 / 80 / 80.
        // Layer (0,20]:  4*20 = 80   -> seats 0,1,2,3
        // Layer (20,50]: 3*30 = 90   -> seats 1,2,3
        // Layer (50,80]: 2*30 = 60   -> seats 2,3
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 20, true, false),
                player(&env, 1, 50, true, false),
                player(&env, 2, 80, true, false),
                player(&env, 3, 80, false, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        assert_eq!(sum_pots(&pots), 230);
        assert_eq!(pots.len(), 3);
        assert_eq!(pots.get(0).unwrap().amount, 80);
        assert_eq!(
            pots.get(0).unwrap().eligible_players,
            seats(&env, &[0, 1, 2, 3])
        );
        assert_eq!(pots.get(1).unwrap().amount, 90);
        assert_eq!(
            pots.get(1).unwrap().eligible_players,
            seats(&env, &[1, 2, 3])
        );
        assert_eq!(pots.get(2).unwrap().amount, 60);
        assert_eq!(pots.get(2).unwrap().eligible_players, seats(&env, &[2, 3]));
    }

    #[test]
    fn folded_player_is_dead_money_but_not_eligible() {
        // Seat 1 folded after committing 30. Seats 0 and 2 are all-in at the
        // SAME level (100), so there is a single pot; the folded 30 is dead
        // money swept into it, and only the two contenders are eligible.
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 100, true, false),
                player(&env, 1, 30, false, true), // folded
                player(&env, 2, 100, true, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        assert_eq!(sum_pots(&pots), 230);
        assert_eq!(pots.len(), 1);
        let p0 = pots.get(0).unwrap();
        assert_eq!(p0.amount, 230);
        assert_eq!(p0.eligible_players, seats(&env, &[0, 2]));
    }

    #[test]
    fn folded_dead_money_with_distinct_all_in_levels() {
        // Seat 1 folded after committing 80 (a large dead-money contribution).
        // Contenders: seat 0 all-in at 40, seat 2 all-in at 100.
        // Main pot (0,40]: 40+40+40 = 120 -> eligible 0,2
        // Side pot (40,100]: seat 1 adds 40 dead money + seat 2 adds 60 = 100
        //                    -> eligible 2 only (seat 0 capped at 40)
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 40, true, false),
                player(&env, 1, 80, false, true), // folded, dead money
                player(&env, 2, 100, true, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        // No chips orphaned: 40 + 80 + 100 = 220.
        assert_eq!(sum_pots(&pots), 220);
        assert_eq!(pots.len(), 2);
        assert_eq!(pots.get(0).unwrap().amount, 120);
        assert_eq!(pots.get(0).unwrap().eligible_players, seats(&env, &[0, 2]));
        assert_eq!(pots.get(1).unwrap().amount, 100);
        assert_eq!(pots.get(1).unwrap().eligible_players, seats(&env, &[2]));
    }

    #[test]
    fn six_way_mixed_stacks() {
        // Commitments: 10, 25, 25, 60, 100, 100 (last two not all-in).
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 10, true, false),
                player(&env, 1, 25, true, false),
                player(&env, 2, 25, true, false),
                player(&env, 3, 60, true, false),
                player(&env, 4, 100, false, false),
                player(&env, 5, 100, false, false),
            ],
        );
        let table = table_with(&env, players.clone());
        let pots = calculate_side_pots(&env, &table).unwrap();

        // Total chips committed.
        let mut total = 0i128;
        for i in 0..players.len() {
            total += players.get(i).unwrap().committed;
        }
        assert_eq!(sum_pots(&pots), total);

        // Distinct all-in levels: 10, 25, 60, plus top layer at 100 -> 4 pots.
        assert_eq!(pots.len(), 4);
        // First (main) pot eligible to everyone.
        assert_eq!(
            pots.get(0).unwrap().eligible_players,
            seats(&env, &[0, 1, 2, 3, 4, 5])
        );
        // Top layer only seats 4 and 5.
        assert_eq!(pots.get(3).unwrap().eligible_players, seats(&env, &[4, 5]));
    }

    // -----------------------------------------------------------------------
    // distribute_pots
    // -----------------------------------------------------------------------

    #[test]
    fn distribute_short_stack_wins_only_main_pot() {
        // Seat 0 all-in 50 has the best hand; seats 1,2 cover 100.
        // Seat 0 wins the 150 main pot; seat 1 (next in ranking) wins the
        // 100 side pot it is eligible for.
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 50, true, false),
                player(&env, 1, 100, true, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        // Ranking: seat 0 best, then 1, then 2.
        let ranking = seats(&env, &[0, 1, 2]);
        let payouts = distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        assert_eq!(table.players.get(0).unwrap().stack, 150); // main pot
        assert_eq!(table.players.get(1).unwrap().stack, 100); // side pot
        assert_eq!(table.players.get(2).unwrap().stack, 0);

        // Conservation: everything distributed.
        let mut paid = 0i128;
        for i in 0..payouts.len() {
            paid += payouts.get(i).unwrap().1;
        }
        assert_eq!(paid, 250);
    }

    #[test]
    fn distribute_big_stack_wins_everything() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 50, true, false),
                player(&env, 1, 100, true, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        // Seat 2 has the best hand and contributed to every pot -> wins all 250.
        let ranking = seats(&env, &[2, 1, 0]);
        distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        assert_eq!(table.players.get(2).unwrap().stack, 250);
        assert_eq!(table.players.get(0).unwrap().stack, 0);
        assert_eq!(table.players.get(1).unwrap().stack, 0);
    }

    #[test]
    fn distribute_tied_winners_split_pot_evenly() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 100, false, false),
                player(&env, 1, 100, false, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let tied = seats(&env, &[0, 1]);
        let ranking = seats(&env, &[0, 1, 2]);
        distribute_pots_with_ties(&env, &mut table, &pots, &tied, &ranking).unwrap();

        assert_eq!(table.players.get(0).unwrap().stack, 150);
        assert_eq!(table.players.get(1).unwrap().stack, 150);
        assert_eq!(table.players.get(2).unwrap().stack, 0);
    }

    #[test]
    fn distribute_tied_winners_odd_chip_to_earliest_seat() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 101, false, false),
                player(&env, 1, 101, false, false),
                player(&env, 2, 101, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let tied = seats(&env, &[1, 2]);
        let ranking = seats(&env, &[1, 2, 0]);
        distribute_pots_with_ties(&env, &mut table, &pots, &tied, &ranking).unwrap();

        assert_eq!(table.players.get(0).unwrap().stack, 0);
        assert_eq!(table.players.get(1).unwrap().stack, 152);
        assert_eq!(table.players.get(2).unwrap().stack, 151);
    }

    #[test]
    fn distribute_tied_winners_board_plays_for_all() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 25, false, false),
                player(&env, 1, 25, false, false),
                player(&env, 2, 25, false, false),
                player(&env, 3, 25, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let tied = seats(&env, &[0, 1, 2, 3]);
        let ranking = tied.clone();
        distribute_pots_with_ties(&env, &mut table, &pots, &tied, &ranking).unwrap();

        for seat in 0..4 {
            assert_eq!(table.players.get(seat).unwrap().stack, 25);
        }
    }

    #[test]
    fn distribute_conserves_all_chips_six_way() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 10, true, false),
                player(&env, 1, 25, true, false),
                player(&env, 2, 25, true, false),
                player(&env, 3, 60, true, false),
                player(&env, 4, 100, false, false),
                player(&env, 5, 100, false, false),
            ],
        );
        let total: i128 = 10 + 25 + 25 + 60 + 100 + 100;
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let ranking = seats(&env, &[3, 5, 4, 2, 1, 0]);
        distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        let mut paid = 0i128;
        for i in 0..table.players.len() {
            paid += table.players.get(i).unwrap().stack;
        }
        assert_eq!(paid, total);
    }

    // -----------------------------------------------------------------------
    // apply_rake
    // -----------------------------------------------------------------------

    #[test]
    fn rake_is_floor_division_of_pot() {
        let env = Env::default();
        let pots: Vec<SidePot> = Vec::from_array(
            &env,
            [SidePot {
                amount: 999,
                eligible_players: seats(&env, &[0, 1]),
            }],
        );
        // 5% of 999 = 49.95 -> floor to 49.
        let (net, rake) = apply_rake(&env, &pots, 500).unwrap();
        assert_eq!(rake, 49);
        assert_eq!(net.get(0).unwrap().amount, 950);
    }

    #[test]
    fn rake_on_small_pot_can_round_to_zero() {
        let env = Env::default();
        let pots: Vec<SidePot> = Vec::from_array(
            &env,
            [SidePot {
                amount: 3,
                eligible_players: seats(&env, &[0, 1]),
            }],
        );
        // 1% of 3 = 0.03 -> floor to 0; the whole pot is preserved.
        let (net, rake) = apply_rake(&env, &pots, 100).unwrap();
        assert_eq!(rake, 0);
        assert_eq!(net.get(0).unwrap().amount, 3);
    }

    #[test]
    fn rake_zero_bps_takes_nothing() {
        let env = Env::default();
        let pots: Vec<SidePot> = Vec::from_array(
            &env,
            [SidePot {
                amount: 10_000,
                eligible_players: seats(&env, &[0, 1]),
            }],
        );
        let (net, rake) = apply_rake(&env, &pots, 0).unwrap();
        assert_eq!(rake, 0);
        assert_eq!(net.get(0).unwrap().amount, 10_000);
    }

    #[test]
    fn rake_is_deducted_independently_per_side_pot() {
        // Multi-way all-in: main pot 150, side pot 100. Rake (5%) is taken
        // from each pot separately, not from the combined total.
        let env = Env::default();
        let pots: Vec<SidePot> = Vec::from_array(
            &env,
            [
                SidePot {
                    amount: 150,
                    eligible_players: seats(&env, &[0, 1, 2]),
                },
                SidePot {
                    amount: 100,
                    eligible_players: seats(&env, &[1, 2]),
                },
            ],
        );
        let (net, rake) = apply_rake(&env, &pots, 500).unwrap();
        // floor(150*0.05) = 7, floor(100*0.05) = 5 -> total 12.
        assert_eq!(rake, 12);
        assert_eq!(net.get(0).unwrap().amount, 143);
        assert_eq!(net.get(1).unwrap().amount, 95);
        // Eligibility is preserved.
        assert_eq!(
            net.get(0).unwrap().eligible_players,
            seats(&env, &[0, 1, 2])
        );
        assert_eq!(net.get(1).unwrap().eligible_players, seats(&env, &[1, 2]));
    }

    #[test]
    fn rake_capped_at_max_does_not_overflow_pot() {
        let env = Env::default();
        let pots: Vec<SidePot> = Vec::from_array(
            &env,
            [SidePot {
                amount: 1,
                eligible_players: seats(&env, &[0, 1]),
            }],
        );
        // Even at the max cap (500 bps), a 1-chip pot rounds the rake to 0,
        // and the pot is never driven negative.
        let (net, rake) = apply_rake(&env, &pots, MAX_RAKE_BPS).unwrap();
        assert_eq!(rake, 0);
        assert_eq!(net.get(0).unwrap().amount, 1);
    }

    #[test]
    fn rake_conserves_chips_with_distribution() {
        // Full flow: all-in pots -> rake -> distribute. Total chips paid out
        // plus rake collected must equal the original pot total.
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 50, true, false),
                player(&env, 1, 100, true, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let gross_total = sum_pots(&pots);

        let (net_pots, rake) = apply_rake(&env, &pots, 200).unwrap(); // 2%
        let ranking = seats(&env, &[2, 1, 0]);
        let payouts = distribute_pots(&env, &mut table, &net_pots, &ranking).unwrap();

        let mut paid = 0i128;
        for i in 0..payouts.len() {
            paid += payouts.get(i).unwrap().1;
        }
        assert_eq!(paid + rake, gross_total);
    }
}
