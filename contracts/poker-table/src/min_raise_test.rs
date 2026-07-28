//! Unit tests for minimum-raise rules (Issue #13).
//!
//! Standard poker rule: the raise *increment* must be at least as large as
//! the previous bet or raise in the same round.  A raise smaller than the
//! last raise size must be rejected with `RaiseTooSmall`.

#![cfg(test)]

extern crate std;

use crate::betting;
use crate::types::*;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

// ---------------------------------------------------------------------------
// Minimal game-hub stub so cross-contract settlement calls don't panic.
// ---------------------------------------------------------------------------

#[contract]
pub struct MinRaiseHubStub;

#[contractimpl]
impl MinRaiseHubStub {
    pub fn start_game(
        _env: Env,
        _game_id: Address,
        _session_id: u32,
        _p1: Address,
        _p2: Address,
        _p1_pts: i128,
        _p2_pts: i128,
    ) {
    }
    pub fn end_game(_env: Env, _session_id: u32, _p1_won: bool) {}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let poker = env.register(crate::PokerTableContract, ());
    let hub = env.register(MinRaiseHubStub, ());
    (env, poker, hub)
}

/// Build a minimal two-player table ready for a preflop betting round.
/// SB = seat 0, BB = seat 1.  Returns the table and the two player addresses
/// in seat order.
fn two_player_table(
    env: &Env,
    poker: &Address,
    hub: &Address,
    stack: i128,
    big_blind: i128,
) -> (TableState, Address, Address) {
    let small_blind = big_blind / 2;
    let p0 = Address::generate(env);
    let p1 = Address::generate(env);

    let mut players = Vec::new(env);
    for (i, addr) in [p0.clone(), p1.clone()].iter().enumerate() {
        players.push_back(PlayerState {
            address: addr.clone(),
            stack,
            bet_this_round: 0,
            committed: 0,
            folded: false,
            all_in: false,
            sitting_out: false,
            seat_index: i as u32,
            total_buy_in: stack,
            rebuy_count: 0,
        });
    }

    let admin = Address::generate(env);
    let mut table = TableState {
        id: 0,
        admin: admin.clone(),
        config: TableConfig {
            token: admin.clone(),
            min_buy_in: 0,
            max_buy_in: i128::MAX,
            blinds_schedule: BlindsSchedule::fixed(env, small_blind, big_blind),
            min_players: 2,
            max_players: 6,
            timeout_ledgers: 0,
            committee: admin.clone(),
            verifier: admin.clone(),
            game_hub: hub.clone(),
            rake_bps: 0,
            max_rebuys: 0,
            jackpot_rake_share_bps: 0,
            min_bad_beat_category: 7,
            min_bad_beat_rank: 12,
        },
        phase: GamePhase::Preflop,
        players,
        dealer_seat: 0,
        current_turn: 0, // SB acts first heads-up preflop
        pot: 0,
        side_pots: Vec::new(env),
        deck_root: BytesN::from_array(env, &[0u8; 32]),
        hand_commitments: Vec::new(env),
        board_cards: Vec::new(env),
        dealt_indices: Vec::new(env),
        hand_number: 1,
        last_action_ledger: env.ledger().sequence(),
        committee: admin.clone(),
        session_id: 0,
        rake_balance: 0,
        action_deadline: 0,
        hand_actions: Vec::new(env),
        jackpot_balance: 0,
        last_raise_size: big_blind,
        rit_state: None,
        current_blind_level: 0,
        level_started_at: 0,
    };

    // Post SB/BB manually so both players have placed their blinds.
    let mut sb = table.players.get(0).unwrap();
    let sb_amt = core::cmp::min(small_blind, sb.stack);
    sb.stack -= sb_amt;
    sb.bet_this_round = sb_amt;
    sb.committed += sb_amt;
    table.pot += sb_amt;
    table.players.set(0, sb);

    let mut bb = table.players.get(1).unwrap();
    let bb_amt = core::cmp::min(big_blind, bb.stack);
    bb.stack -= bb_amt;
    bb.bet_this_round = bb_amt;
    bb.committed += bb_amt;
    table.pot += bb_amt;
    table.players.set(1, bb);

    // SB acts first (seat 0), as it must call or raise to match BB.
    table.current_turn = 0;

    (table, p0, p1)
}

fn act(
    env: &Env,
    poker: &Address,
    table: &mut TableState,
    player: &Address,
    action: &Action,
) -> Result<(), PokerTableError> {
    env.as_contract(poker, || betting::process_action(env, table, player, action))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A raise smaller than the big blind (the initial `last_raise_size`) must
/// be rejected even when there has been no prior raise in the round.
#[test]
fn raise_below_big_blind_rejected() {
    let (env, poker, hub) = make_env();
    let big_blind = 20i128;
    let (mut table, p0, _p1) = two_player_table(&env, &poker, &hub, 1_000, big_blind);

    // p0 is SB and must at least call BB (20). A raise of 5 (< big_blind=20) must fail.
    let err = act(&env, &poker, &mut table, &p0, &Action::Raise(5)).unwrap_err();
    assert_eq!(err, PokerTableError::RaiseTooSmall);
}

/// A raise exactly equal to the big blind must be accepted (minimum valid raise).
#[test]
fn raise_equal_to_big_blind_accepted() {
    let (env, poker, hub) = make_env();
    let big_blind = 20i128;
    let (mut table, p0, _p1) = two_player_table(&env, &poker, &hub, 1_000, big_blind);

    // p0 raises exactly the big blind — that's the minimum legal raise.
    act(&env, &poker, &mut table, &p0, &Action::Raise(big_blind))
        .expect("raise equal to big blind must succeed");
    // last_raise_size must now reflect the raise.
    assert_eq!(table.last_raise_size, big_blind);
}

/// After a raise of size R, the next re-raise increment must be >= R.
/// A re-raise smaller than the prior raise must be rejected.
#[test]
fn re_raise_must_match_prior_raise_size() {
    let (env, poker, hub) = make_env();
    let big_blind = 10i128;
    let (mut table, p0, p1) = two_player_table(&env, &poker, &hub, 2_000, big_blind);

    // p0 (SB) raises 40 (> BB=10).
    act(&env, &poker, &mut table, &p0, &Action::Raise(40)).expect("initial raise");
    assert_eq!(table.last_raise_size, 40);

    // p1 (BB) attempts a re-raise of only 20 — smaller than the prior raise of 40.
    let err = act(&env, &poker, &mut table, &p1, &Action::Raise(20)).unwrap_err();
    assert_eq!(err, PokerTableError::RaiseTooSmall);
}

/// After a raise of size R, a re-raise increment >= R must be accepted.
#[test]
fn re_raise_matching_prior_size_accepted() {
    let (env, poker, hub) = make_env();
    let big_blind = 10i128;
    let (mut table, p0, p1) = two_player_table(&env, &poker, &hub, 2_000, big_blind);

    // p0 raises 40.
    act(&env, &poker, &mut table, &p0, &Action::Raise(40)).expect("initial raise");

    // p1 re-raises exactly 40 — exactly matching prior raise. Must be accepted.
    act(&env, &poker, &mut table, &p1, &Action::Raise(40))
        .expect("re-raise equal to prior raise must succeed");
    assert_eq!(table.last_raise_size, 40);
}

/// After `reset_round`, `last_raise_size` is reset to the big blind.
#[test]
fn reset_round_restores_last_raise_size_to_big_blind() {
    let (env, poker, hub) = make_env();
    let big_blind = 10i128;
    let (mut table, p0, _p1) = two_player_table(&env, &poker, &hub, 2_000, big_blind);

    // Simulate a large raise having happened.
    table.last_raise_size = 200;

    // reset_round is called at the start of each post-flop round.
    env.as_contract(&poker, || betting::reset_round(&env, &mut table))
        .expect("reset_round");

    assert_eq!(
        table.last_raise_size, big_blind,
        "last_raise_size must be restored to big_blind after reset_round"
    );
}

/// A bet (no prior outstanding bet) must also update `last_raise_size`.
#[test]
fn bet_sets_last_raise_size() {
    let (env, poker, hub) = make_env();
    let big_blind = 10i128;
    // Use a post-flop scenario where current_bet == 0.
    let (mut table, p0, _p1) = two_player_table(&env, &poker, &hub, 2_000, big_blind);

    // Manually clear bets to simulate a post-flop round where no one has bet yet.
    for i in 0..table.players.len() {
        let mut p = table.players.get(i).unwrap();
        p.bet_this_round = 0;
        table.players.set(i, p);
    }
    table.pot = 0;
    table.current_turn = 0;

    let bet_amount = 50i128;
    act(&env, &poker, &mut table, &p0, &Action::Bet(bet_amount)).expect("bet");
    assert_eq!(table.last_raise_size, bet_amount);
}
