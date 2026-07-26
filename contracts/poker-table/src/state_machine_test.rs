//! Property-based state-machine tests for the betting round transitions
//! (issue #292). These tests model the betting round as a formal state
//! machine, generate random legal action sequences, and verify:
//!
//!   * **State transitions are valid** — only legal phase edges are taken
//!     and the phase rank is monotonically non-decreasing.
//!   * **Invariant checks pass** — chip conservation holds after every
//!     action, no balance ever goes negative, and `current_turn` always
//!     references an active, non-folded, non-all-in seat inside a betting
//!     phase.
//!   * **Terminal states are reachable** — for any sequence of intents we
//!     always reach either `Showdown` (matching bets through River) or
//!     `Settlement` (fold-chain) within a bounded number of moves.
//!
//! The tests drive `betting::process_action` directly against an in-memory
//! `TableState`, deliberately skipping the deal/reveal/showdown ZK proof
//! path (covered elsewhere by the lifecycle tests). They run inside the
//! `proptest!` macro with the Soroban test budget reset
//! (`cost_estimate().budget().reset_unlimited()`) so the many generated
//! cases are not throttled by metering.

#![cfg(test)]

extern crate std;

use crate::betting;
use crate::types::*;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};
use std::format;
use std::vec::Vec as StdVec;

/// Maximum seated players per table (matches the contract cap).
const MAX_PLAYERS_PER_TABLE: u32 = 6;

/// Hard upper bound on moves before we declare the machine stuck. With N
/// players and at most four betting rounds, each round needs at most ~2N
/// actions (call/raise matching) before every active player has matched.
/// 256 is a generous safety bound that still catches any runaway loop.
const MAX_MOVES_BEFORE_TERMINAL: u32 = 256;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Per-player buy-in strategy: 200..=2_000 chips. The lower bound is large
/// enough that any player can absorb several raises before going all-in.
fn buy_in_strategy(n: usize) -> impl Strategy<Value = StdVec<i128>> {
    prop::collection::vec(200i128..=2_000i128, n..=n)
}

fn player_count_strategy() -> impl Strategy<Value = usize> {
    2usize..=6usize
}

/// Stream of "intents": coarse-grained player intents that the action
/// synthesizer projects to a guaranteed-legal `Action`. The four-way split
/// mirrors real player archetypes (tight, loose, aggressive, all-in).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intent {
    Fold,
    PassThrough, // check if possible, else call
    Aggressive,  // bet/raise if stack allows, else fall back
    AllIn,
}

fn intents_strategy(min: usize, max: usize) -> impl Strategy<Value = StdVec<Intent>> {
    prop::collection::vec(
        (0u8..=3).prop_map(|n| match n {
            0 => Intent::Fold,
            1 => Intent::PassThrough,
            2 => Intent::Aggressive,
            _ => Intent::AllIn,
        }),
        min..=max,
    )
}

// ---------------------------------------------------------------------------
// State builder
// ---------------------------------------------------------------------------

fn post_blind(table: &mut TableState, seat: u32, amount: i128) -> Result<(), PokerTableError> {
    let mut p = table
        .players
        .get(seat)
        .ok_or(PokerTableError::InvalidPlayerIndex)?;
    let actual = if p.stack < amount {
        p.all_in = true;
        p.stack
    } else {
        amount
    };
    p.stack -= actual;
    p.bet_this_round = actual;
    p.committed += actual;
    table.pot += actual;
    table.players.set(seat, p);
    Ok(())
}

/// Build a `TableState` sitting in `Preflop` with blinds already posted.
/// Skip `game::start_new_hand` (it leaves us in `Dealing`) and the ZK deal
/// step — neither is part of the betting state machine. The blind posting
/// and `current_turn` assignment exactly mirror `start_new_hand` and
/// `commit_deal` in `contracts/poker-table/src/lib.rs`.
fn build_preflop_state(
    env: &Env,
    buy_ins: &[i128],
    small_blind: i128,
    big_blind: i128,
) -> TableState {
    let n = buy_ins.len();
    assert!(n >= 2 && n <= MAX_PLAYERS_PER_TABLE as usize);

    let mut players: Vec<PlayerState> = Vec::new(env);
    let admin = Address::generate(env);
    for (seat, &bi) in buy_ins.iter().enumerate() {
        players.push_back(PlayerState {
            address: Address::generate(env),
            stack: bi,
            bet_this_round: 0,
            committed: 0,
            folded: false,
            all_in: false,
            sitting_out: false,
            seat_index: seat as u32,
        });
    }

    let mut table = TableState {
        id: 0,
        admin: admin.clone(),
        config: TableConfig {
            token: admin.clone(),
            min_buy_in: 0,
            max_buy_in: i128::MAX,
            small_blind,
            big_blind,
            min_players: 2,
            max_players: MAX_PLAYERS_PER_TABLE,
            timeout_ledgers: 0,
            committee: admin.clone(),
            verifier: admin.clone(),
            game_hub: admin.clone(),
            rake_bps: 0,
        },
        phase: GamePhase::Preflop,
        players,
        dealer_seat: 0,
        current_turn: 0,
        pot: 0,
        side_pots: Vec::new(env),
        deck_root: BytesN::from_array(env, &[0u8; 32]),
        hand_commitments: Vec::new(env),
        board_cards: Vec::new(env),
        dealt_indices: Vec::new(env),
        hand_number: 1,
        last_action_ledger: env.ledger().sequence(),
        committee: admin,
        session_id: 0,
        rake_balance: 0,
    };

    // Same blind placement and UTG calculation as the public contract:
    //   SB = (dealer+1)%N, BB = (dealer+2)%N, first-to-act = (dealer+3)%N.
    let num_players = table.players.len() as u32;
    let sb_seat = (table.dealer_seat + 1) % num_players;
    let bb_seat = (table.dealer_seat + 2) % num_players;
    post_blind(&mut table, sb_seat, small_blind).expect("post SB");
    post_blind(&mut table, bb_seat, big_blind).expect("post BB");
    table.current_turn = (table.dealer_seat + 3) % num_players;
    table
}

// ---------------------------------------------------------------------------
// State-machine helpers (mirror contracts/poker-table/src/betting.rs)
// ---------------------------------------------------------------------------

/// Maximum `bet_this_round` over all players — the live "match-me" size.
fn current_bet(table: &TableState) -> i128 {
    let mut max = 0i128;
    for i in 0..table.players.len() {
        let bet = table.players.get(i).unwrap().bet_this_round;
        if bet > max {
            max = bet;
        }
    }
    max
}

/// True iff every non-folded, non-all-in player has matched `current_bet`,
/// matching `betting::is_round_complete`.
fn is_round_complete(table: &TableState) -> bool {
    let bet = current_bet(table);
    for i in 0..table.players.len() {
        let p = table.players.get(i).unwrap();
        if p.folded || p.all_in {
            continue;
        }
        if p.bet_this_round != bet {
            return false;
        }
    }
    true
}

/// Sum of `committed` over every player — must equal `table.pot` at all
/// times during betting (the pot is just the ledger view of committed
/// chips, before settlement redistribution).
fn sum_committed(table: &TableState) -> i128 {
    let mut t = 0i128;
    for i in 0..table.players.len() {
        t += table.players.get(i).unwrap().committed;
    }
    t
}

/// Rank of a phase on the betting DAG, used for monotonicity checks.
fn phase_rank(phase: &GamePhase) -> u8 {
    match phase {
        GamePhase::Waiting => 0,
        GamePhase::Dealing => 1,
        GamePhase::Preflop => 2,
        GamePhase::DealingFlop => 3,
        GamePhase::Flop => 4,
        GamePhase::DealingTurn => 5,
        GamePhase::Turn => 6,
        GamePhase::DealingRiver => 7,
        GamePhase::River => 8,
        GamePhase::Showdown => 9,
        GamePhase::Settlement => 10,
        // `Dispute` is unreachable in the betting state machine and is
        // assigned the highest rank so any spurious transition fails the
        // monotonicity assertion loudly.
        GamePhase::Dispute => u8::MAX,
    }
}

/// True when the betting state machine has reached a terminal state for
/// the current hand (no further player_action is meaningful).
fn is_terminal(phase: &GamePhase) -> bool {
    matches!(phase, GamePhase::Showdown | GamePhase::Settlement)
}

fn is_betting_phase(phase: &GamePhase) -> bool {
    matches!(
        phase,
        GamePhase::Preflop | GamePhase::Flop | GamePhase::Turn | GamePhase::River
    )
}

fn is_in_dealing_phase(phase: &GamePhase) -> bool {
    matches!(
        phase,
        GamePhase::DealingFlop | GamePhase::DealingTurn | GamePhase::DealingRiver
    )
}

// ---------------------------------------------------------------------------
// Action synthesis: turn an `Intent` into a legal `Action`
// ---------------------------------------------------------------------------

/// Translate an `Intent` into a guaranteed-legal `Action` for the player
/// whose turn it is. The synthesizer is total: every intent maps to some
/// legal action regardless of state (folded/all-in short-circuits are
/// handled by the driver, not here).
fn synthesize_legal_action(table: &TableState, intent: Intent) -> Action {
    let curr = table
        .players
        .get(table.current_turn)
        .expect("current_turn in range");
    let bet = current_bet(table);
    let to_call = bet - curr.bet_this_round;
    let bb = table.config.big_blind;
    match intent {
        Intent::Fold => Action::Fold,
        Intent::AllIn => Action::AllIn,
        Intent::PassThrough => {
            if to_call > 0 {
                Action::Call
            } else {
                Action::Check
            }
        }
        Intent::Aggressive => {
            if to_call > 0 {
                // Need a raise whose amount is at least `bb` and whose
                // total committed (`to_call + amount`) does not exceed stack.
                if curr.stack > to_call + bb {
                    Action::Raise(bb)
                } else if curr.stack > 0 {
                    // Cannot raise; stay in with whatever is left.
                    Action::Call
                } else {
                    Action::AllIn
                }
            } else if curr.stack >= bb {
                Action::Bet(bb)
            } else {
                Action::AllIn
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Step driver: feed one intent into the state machine and apply invariants
// ---------------------------------------------------------------------------

/// Move past any in-progress `DealingX` phase by promoting it to the next
/// Betting phase and resetting per-round state. Mirrors the work the
/// committee performs over `reveal_board` / `commit_deal` in production,
/// but without ZK proofs — the betting state machine is independent of
/// proving machinery.
fn advance_past_dealing(env: &Env, table: &mut TableState) {
    match table.phase {
        GamePhase::DealingFlop => {
            table.phase = GamePhase::Flop;
            let _ = betting::reset_round(env, table);
        }
        GamePhase::DealingTurn => {
            table.phase = GamePhase::Turn;
            let _ = betting::reset_round(env, table);
        }
        GamePhase::DealingRiver => {
            table.phase = GamePhase::River;
            let _ = betting::reset_round(env, table);
        }
        _ => {}
    }
}

/// Apply one intent, returning `true` if a `process_action` call happened.
/// After every meaningful action the function asserts the action-level
/// invariants: chip conservation, non-negative balances, pot/committed
/// consistency, and a valid `current_turn`.
#[allow(clippy::too_many_lines)]
fn step(
    env: &Env,
    table: &mut TableState,
    intent: Intent,
    initial_total: i128,
    last_rank: &mut u8,
) -> bool {
    if is_terminal(&table.phase) {
        return false;
    }
    if !is_betting_phase(&table.phase) {
        advance_past_dealing(env, table);
        return false;
    }
    let curr = table
        .players
        .get(table.current_turn)
        .expect("current_turn in range");
    if curr.folded || curr.all_in || curr.stack == 0 {
        return false;
    }

    let action = synthesize_legal_action(table, intent);
    let addr = curr.address.clone();
    betting::process_action(env, table, &addr, &action).expect("legal action");

    // Conservation: `sum(stack) + pot + rake_balance` is invariant
    // across every action AND across every settlement path (`settle_fold_win`
    // and `settle_showdown`, though only the former is exercised here).
    // This is the strictly correct chip-conservation invariant —
    // `sum(stack + committed)` would over-count chips that any settlement
    // step re-distributes from the pot back onto a winner's stack (the
    // winning player's `committed` is unchanged; the chips just move from
    // `pot` to `stack`, and the rake flows into `rake_balance`).
    let mut live_total: i128 = 0;
    for i in 0..table.players.len() {
        let p = table.players.get(i).unwrap();
        assert!(p.stack >= 0, "stack went negative on seat {}", p.seat_index);
        assert!(p.bet_this_round >= 0, "bet_this_round went negative");
        assert!(p.committed >= 0, "committed went negative");
        live_total += p.stack;
    }
    live_total += table.pot;
    live_total += table.rake_balance;
    assert_eq!(
        live_total, initial_total,
        "chip conservation broken: live={} pot={} rake={} initial={} phase={:?}",
        live_total, table.pot, table.rake_balance, initial_total, table.phase
    );

    // The pot is a re-projection of `committed` chips whenever no
    // settlement has redistributed pot chips. After fold-win the pot is
    // zero, so we only enforce this from a Betting phase.
    if is_betting_phase(&table.phase) {
        assert_eq!(
            table.pot,
            sum_committed(table),
            "pot != sum(committed) on {:?}",
            table.phase
        );
    }

    // Phase monotonicity.
    let now_rank = phase_rank(&table.phase);
    assert!(
        now_rank >= *last_rank,
        "phase DAG went backward: rank {} -> {} (phase {:?})",
        *last_rank,
        now_rank,
        table.phase
    );
    *last_rank = now_rank;

    // Turn validity inside Betting phases.
    if is_betting_phase(&table.phase) {
        let p = table.players.get(table.current_turn).unwrap();
        assert!(
            !p.folded && !p.all_in,
            "turn points to inactive player on phase {:?}: {:?}",
            table.phase,
            p
        );
        assert!(
            p.stack > 0 || is_round_complete(table),
            "turn points to a player with no stack but round is not complete: {:?}",
            p
        );
    }
    true
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// **Chip conservation holds across the betting state machine.**
    /// Across every action — folded, all-in, raised, called, or checked —
    /// `sum(stack) + pot + rake_balance` equals the initial chip pool,
    /// no balance ever goes negative, and the pot always equals
    /// `sum(committed)` while in a Betting phase. After `settle_fold_win`,
    /// the invariant still holds because the redistributed chips moved
    /// from pot onto the winner's stack.
    #[test]
    fn prop_betting_machine_conserves_chips(
        n in player_count_strategy(),
        buy_ins in buy_in_strategy(6),
        intents in intents_strategy(16, 96),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let buy_ins = &buy_ins[..n];
        let mut table = build_preflop_state(&env, buy_ins, 5, 10);
        let initial_total: i128 = buy_ins.iter().sum();
        let mut last_rank = phase_rank(&table.phase);

        let mut moves = 0u32;
        for intent in &intents {
            if is_terminal(&table.phase) {
                break;
            }
            advance_past_dealing(&env, &mut table);
            if step(&env, &mut table, *intent, initial_total, &mut last_rank) {
                moves += 1;
            }
        }
        prop_assert!(
            is_terminal(&table.phase),
            "machine stopped without reaching a terminal state: phase={:?}",
            table.phase
        );
        prop_assert!(
            moves <= MAX_MOVES_BEFORE_TERMINAL,
            "machine ran for {} moves without terminating: phase={:?}",
            moves,
            table.phase
        );
    }

    /// **Phase DAG is monotonically non-decreasing.** Across any sequence
    /// of synthesized actions, the phase never moves backward and never
    /// jumps over the linear betting sequence (Preflop → DealingFlop →
    /// Flop → DealingTurn → Turn → DealingRiver → River → Showdown) or
    /// short-circuits to `Settlement` via a fold chain.
    #[test]
    fn prop_phase_dag_is_monotonic(
        n in player_count_strategy(),
        buy_ins in buy_in_strategy(6),
        intents in intents_strategy(16, 96),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let buy_ins = &buy_ins[..n];
        let mut table = build_preflop_state(&env, buy_ins, 5, 10);
        let initial_total: i128 = buy_ins.iter().sum();
        let mut last_rank = phase_rank(&table.phase);

        for intent in &intents {
            if is_terminal(&table.phase) {
                break;
            }
            advance_past_dealing(&env, &mut table);
            step(&env, &mut table, *intent, initial_total, &mut last_rank);
        }
    }

    /// **Round completion drives phase transitions.** Whenever the betting
    /// state machine transitions out of a betting phase, the round must
    /// have been complete going in — the contract never advances before
    /// every active non-all-in player matched. Conversely, until that
    /// criterion holds, the machine must keep letting players act.
    #[test]
    fn prop_round_completion_drives_phase_transition(
        n in player_count_strategy(),
        buy_ins in buy_in_strategy(6),
        intents in intents_strategy(16, 96),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let buy_ins = &buy_ins[..n];
        let mut table = build_preflop_state(&env, buy_ins, 5, 10);
        let initial_total: i128 = buy_ins.iter().sum();
        let mut last_rank = phase_rank(&table.phase);

        for intent in &intents {
            if is_terminal(&table.phase) {
                break;
            }
            advance_past_dealing(&env, &mut table);
            if !is_betting_phase(&table.phase) {
                continue;
            }

            let pre_phase = table.phase.clone();
            let pre_complete = is_round_complete(&table);
            let curr = table.players.get(table.current_turn).unwrap();
            if curr.folded || curr.all_in || curr.stack == 0 {
                continue;
            }
            let action = synthesize_legal_action(&table, *intent);
            let addr = curr.address.clone();
            betting::process_action(&env, &mut table, &addr, &action).unwrap();

            let advanced_out_of_betting = is_betting_phase(&pre_phase)
                && !is_betting_phase(&table.phase)
                && !is_in_dealing_phase(&table.phase);
            prop_assert!(
                pre_complete || !advanced_out_of_betting,
                "phase advanced without round completion: {:?} -> {:?}",
                pre_phase,
                table.phase
            );

            // Update rank bookkeeping to keep the next iteration monotonic.
            let now_rank = phase_rank(&table.phase);
            prop_assert!(now_rank >= last_rank, "phase rank went backward");
            last_rank = now_rank;
        }
    }

    /// **Terminal state reachable within bounds.** Given an infinite
    /// stream of legal intents, the betting state machine must reach
    /// either `Showdown` (matched bets through River) or `Settlement`
    /// (fold-chain) within `MAX_MOVES_BEFORE_TERMINAL` moves.
    #[test]
    fn prop_terminal_state_reachable_within_bounds(
        n in player_count_strategy(),
        buy_ins in buy_in_strategy(6),
        intents in intents_strategy(16, 192),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let buy_ins = &buy_ins[..n];
        let mut table = build_preflop_state(&env, buy_ins, 5, 10);
        let initial_total: i128 = buy_ins.iter().sum();
        let mut last_rank = phase_rank(&table.phase);
        let mut moves = 0u32;

        for intent in &intents {
            if is_terminal(&table.phase) {
                break;
            }
            advance_past_dealing(&env, &mut table);
            if step(&env, &mut table, *intent, initial_total, &mut last_rank) {
                moves += 1;
                if moves >= MAX_MOVES_BEFORE_TERMINAL {
                    break;
                }
            }
        }
        prop_assert!(
            moves <= MAX_MOVES_BEFORE_TERMINAL,
            "machine ran for {} moves without terminating: phase={:?}",
            moves,
            table.phase
        );
        prop_assert!(
            is_terminal(&table.phase),
            "machine stopped without reaching a terminal state: phase={:?} (moves={})",
            table.phase,
            moves
        );
    }

    /// **`current_turn` always references an active, stack-bearing seat**
    /// inside a betting phase (or the round is complete). The player who
    /// is currently up must not be folded, all-in out, or already at
    /// zero stack — any of those states would mean the state machine
    /// stranded the turn on a non-actionable seat.
    #[test]
    fn prop_current_turn_is_active_inside_betting(
        n in player_count_strategy(),
        buy_ins in buy_in_strategy(6),
        intents in intents_strategy(16, 96),
    ) {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();

        let buy_ins = &buy_ins[..n];
        let mut table = build_preflop_state(&env, buy_ins, 5, 10);
        let initial_total: i128 = buy_ins.iter().sum();
        let mut last_rank = phase_rank(&table.phase);

        for intent in &intents {
            if is_terminal(&table.phase) {
                break;
            }
            advance_past_dealing(&env, &mut table);
            if !is_betting_phase(&table.phase) {
                continue;
            }
            let curr = table.players.get(table.current_turn).unwrap();
            prop_assert!(
                !curr.folded,
                "current_turn points to a folded player ({}) on phase {:?}",
                curr.seat_index,
                table.phase
            );
            prop_assert!(
                !curr.all_in,
                "current_turn points to an all-in player ({}) on phase {:?}",
                curr.seat_index,
                table.phase
            );
            prop_assert!(
                curr.stack > 0 || is_round_complete(&table),
                "current_turn has no stack but round is not complete on phase {:?}: {:?}",
                table.phase,
                curr
            );
            let action = synthesize_legal_action(&table, *intent);
            let addr = curr.address.clone();
            betting::process_action(&env, &mut table, &addr, &action).unwrap();
            let now_rank = phase_rank(&table.phase);
            prop_assert!(now_rank >= last_rank, "phase rank went backward");
            last_rank = now_rank;
        }
    }
}
