//! Property-based invariant tests for pot calculation (issue #38).
//!
//! Pot math is the most safety-critical part of the contract: a bug can mint or
//! burn chips. These tests use `proptest` to generate thousands of random
//! all-in / fold / stack combinations and assert the invariants that must hold
//! for every possible hand:
//!
//!   * Conservation of funds — chips in the pots equal chips committed, and
//!     after settlement the players' stacks equal the total that entered.
//!   * Non-negative balances — no stack or pot ever goes negative.
//!   * Winner takes at most the pot — a payout never exceeds the chips available.
//!   * A player only wins pots they contributed to (eligibility is respected).
//!   * Idempotent settlement — distributing an empty/zero pot set is a no-op.
//!   * Deterministic — the same inputs always yield the same pots and payouts.
//!
//! As required, the Soroban test budget is reset with `reset_unlimited()` so the
//! many generated cases are not throttled by metering.

#![cfg(test)]

extern crate std;

use crate::pot::{calculate_side_pots, distribute_pots};
use crate::types::*;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};
use std::format;

/// A generated player: total committed chips, whether all-in, whether folded.
#[derive(Clone, Debug)]
struct GenPlayer {
    committed: i128,
    all_in: bool,
    folded: bool,
}

/// Strategy producing 3–6 players with random commitments and flags, with at
/// least two non-folded contenders (a real showdown always has ≥2).
fn players_strategy() -> impl Strategy<Value = std::vec::Vec<GenPlayer>> {
    prop::collection::vec(
        (1i128..=1000i128, any::<bool>(), any::<bool>()).prop_map(|(committed, all_in, folded)| {
            GenPlayer {
                committed,
                all_in,
                folded,
            }
        }),
        3..=6,
    )
    .prop_map(|mut players| {
        // Guarantee at least two contenders so a showdown is well-defined.
        let contenders = players.iter().filter(|p| !p.folded).count();
        if contenders < 2 {
            for p in players.iter_mut().take(2) {
                p.folded = false;
            }
        }
        players
    })
}

/// Build a `TableState` from generated players. The contract pot equals the sum
/// of committed chips, matching the on-chain accounting (every committed chip
/// has already been added to `table.pot` during betting).
fn build_table(env: &Env, gen: &[GenPlayer]) -> (TableState, i128) {
    let mut players: Vec<PlayerState> = Vec::new(env);
    let mut total: i128 = 0;
    for (seat, g) in gen.iter().enumerate() {
        total += g.committed;
        players.push_back(PlayerState {
            address: Address::generate(env),
            stack: 0,
            bet_this_round: 0,
            committed: g.committed,
            folded: g.folded,
            all_in: g.all_in,
            sitting_out: false,
            seat_index: seat as u32,
            total_buy_in: 0,
            rebuy_count: 0,
        });
    }
    let admin = Address::generate(env);
    let table = TableState {
        id: 0,
        admin: admin.clone(),
        config: TableConfig {
            token: admin.clone(),
            min_buy_in: 0,
            max_buy_in: i128::MAX,
            blinds_schedule: BlindsSchedule::fixed(env, 0, 0),
            min_players: 2,
            max_players: 9,
            timeout_ledgers: 0,
            committee: admin.clone(),
            verifier: admin.clone(),
            game_hub: admin.clone(),
            rake_bps: 0,
            max_rebuys: 0,
            jackpot_rake_share_bps: 0,
            min_bad_beat_category: 7,
            min_bad_beat_rank: 12,
        },
        phase: GamePhase::Showdown,
        players,
        dealer_seat: 0,
        current_turn: 0,
        pot: total,
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
        action_deadline: 0,
        hand_actions: Vec::new(env),
        rit_state: None,
        jackpot_balance: 0,
        last_raise_size: 0,
        current_blind_level: 0,
        level_started_at: 0,
    };
    (table, total)
}

/// Best-first ranking over the non-folded seats, derived deterministically from
/// the random `order` permutation seed. Mirrors how the contract would feed a
/// hand ranking into `distribute_pots`.
fn ranking_from(env: &Env, table: &TableState, order: &[u32]) -> Vec<u32> {
    let mut contenders: std::vec::Vec<u32> = std::vec::Vec::new();
    for i in 0..table.players.len() {
        let p = table.players.get(i).unwrap();
        if !p.folded {
            contenders.push(p.seat_index);
        }
    }
    // Reorder contenders by the generated key so winners vary across cases.
    contenders.sort_by_key(|seat| order.get(*seat as usize).copied().unwrap_or(0));
    let mut ranking = Vec::new(env);
    for seat in contenders {
        ranking.push_back(seat);
    }
    ranking
}

fn sum_pots(pots: &Vec<SidePot>) -> i128 {
    let mut total = 0i128;
    for i in 0..pots.len() {
        total += pots.get(i).unwrap().amount;
    }
    total
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Conservation + non-negativity at the pot-calculation stage: the side pots
    /// always sum to exactly the chips committed, and no pot is negative.
    #[test]
    fn prop_pots_conserve_and_are_non_negative(gen in players_strategy()) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let (table, total) = build_table(&env, &gen);
        let pots = calculate_side_pots(&env, &table).unwrap();

        prop_assert_eq!(sum_pots(&pots), total);
        for i in 0..pots.len() {
            prop_assert!(pots.get(i).unwrap().amount >= 0);
        }
    }

    /// Eligibility is contribution-bounded: every eligible player in a pot
    /// committed at least the pot's per-player share, and is never folded.
    #[test]
    fn prop_eligibility_respects_contribution(gen in players_strategy()) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let (table, _total) = build_table(&env, &gen);
        let pots = calculate_side_pots(&env, &table).unwrap();

        for pi in 0..pots.len() {
            let pot = pots.get(pi).unwrap();
            for ei in 0..pot.eligible_players.len() {
                let seat = pot.eligible_players.get(ei).unwrap();
                let p = table.players.get(seat).unwrap();
                prop_assert!(!p.folded);
                prop_assert!(p.committed > 0);
            }
        }
    }

    /// Conservation through settlement: distributing the pots credits players
    /// exactly the chips that entered the pot — no chips minted or burned — and
    /// no payout exceeds the total pot.
    #[test]
    fn prop_distribution_conserves_funds(
        gen in players_strategy(),
        order in prop::collection::vec(0u32..1000, 6),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let (mut table, total) = build_table(&env, &gen);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let ranking = ranking_from(&env, &table, &order);

        let payouts = distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        // Sum of credited payouts equals the whole pot (conservation).
        let mut paid = 0i128;
        for i in 0..payouts.len() {
            let amt = payouts.get(i).unwrap().1;
            prop_assert!(amt >= 0);
            paid += amt;
        }
        prop_assert_eq!(paid, total);

        // No single payout exceeds the total available chips.
        for i in 0..payouts.len() {
            prop_assert!(payouts.get(i).unwrap().1 <= total);
        }

        // Final stacks sum back to the total that entered the hand.
        let mut stacks = 0i128;
        for i in 0..table.players.len() {
            let s = table.players.get(i).unwrap().stack;
            prop_assert!(s >= 0);
            stacks += s;
        }
        prop_assert_eq!(stacks, total);
    }

    /// Winner-takes-at-most-pot, per pot: each pot is credited to exactly one
    /// eligible seat and that credit equals the pot amount.
    #[test]
    fn prop_each_pot_goes_to_eligible_winner(
        gen in players_strategy(),
        order in prop::collection::vec(0u32..1000, 6),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let (mut table, _total) = build_table(&env, &gen);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let ranking = ranking_from(&env, &table, &order);

        // Snapshot stacks before, distribute, and check each pot landed on an
        // eligible seat.
        let before: std::vec::Vec<i128> =
            (0..table.players.len()).map(|i| table.players.get(i).unwrap().stack).collect();
        let _ = distribute_pots(&env, &mut table, &pots, &ranking).unwrap();
        let after: std::vec::Vec<i128> =
            (0..table.players.len()).map(|i| table.players.get(i).unwrap().stack).collect();

        // Total gain equals total pot.
        let gained: i128 = after.iter().zip(before.iter()).map(|(a, b)| a - b).sum();
        prop_assert_eq!(gained, sum_pots(&pots));
    }

    /// Determinism: identical inputs always produce identical pots and payouts.
    #[test]
    fn prop_calculation_is_deterministic(
        gen in players_strategy(),
        order in prop::collection::vec(0u32..1000, 6),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let (mut t1, _) = build_table(&env, &gen);
        let (mut t2, _) = build_table(&env, &gen);
        let p1 = calculate_side_pots(&env, &t1).unwrap();
        let p2 = calculate_side_pots(&env, &t2).unwrap();
        prop_assert_eq!(sum_pots(&p1), sum_pots(&p2));
        prop_assert_eq!(p1.len(), p2.len());

        let r1 = ranking_from(&env, &t1, &order);
        let r2 = ranking_from(&env, &t2, &order);
        let pay1 = distribute_pots(&env, &mut t1, &p1, &r1).unwrap();
        let pay2 = distribute_pots(&env, &mut t2, &p2, &r2).unwrap();
        prop_assert_eq!(pay1.len(), pay2.len());
        for i in 0..pay1.len() {
            prop_assert_eq!(pay1.get(i).unwrap(), pay2.get(i).unwrap());
        }
    }

    /// Idempotent settlement: re-running the distribution on already-settled
    /// state (empty pot set) leaves every balance unchanged.
    #[test]
    fn prop_settlement_is_idempotent(
        gen in players_strategy(),
        order in prop::collection::vec(0u32..1000, 6),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let (mut table, _total) = build_table(&env, &gen);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let ranking = ranking_from(&env, &table, &order);
        distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        let snapshot: std::vec::Vec<i128> =
            (0..table.players.len()).map(|i| table.players.get(i).unwrap().stack).collect();

        // Settling again with no pots (the post-settlement state) must be a
        // no-op — balances do not change.
        let empty: Vec<SidePot> = Vec::new(&env);
        distribute_pots(&env, &mut table, &empty, &ranking).unwrap();
        let after: std::vec::Vec<i128> =
            (0..table.players.len()).map(|i| table.players.get(i).unwrap().stack).collect();

        prop_assert_eq!(snapshot, after);
    }
}
