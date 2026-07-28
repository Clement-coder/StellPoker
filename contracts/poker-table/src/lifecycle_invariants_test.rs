//! Property-based invariant tests for the full poker-table lifecycle.
//!
//! These tests drive the public contract API through randomized hand flows and
//! assert the conservation/accounting invariants after every successful action.

#![cfg(test)]

extern crate std;

use crate::pot::MAX_RAKE_BPS;
use crate::types::*;
use crate::{PokerTableContract, PokerTableContractClient};
use proptest::prelude::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    Address, Bytes, BytesN, Env, Vec,
};
use std::format;

#[contract]
pub struct GameHubContract;

#[contractimpl]
impl GameHubContract {
    pub fn start_game(
        _env: Env,
        _game_id: Address,
        _session_id: u32,
        _player1: Address,
        _player2: Address,
        _player1_points: i128,
        _player2_points: i128,
    ) {
    }

    pub fn end_game(_env: Env, _session_id: u32, _player1_won: bool) {}
}

#[derive(Clone, Debug)]
enum FuzzMove {
    Conservative,
    Aggressive,
    AllIn,
    FoldIfSafe,
}

fn moves_strategy() -> impl Strategy<Value = std::vec::Vec<FuzzMove>> {
    prop::collection::vec(0u8..=3, 8..=48).prop_map(|raw| {
        raw.into_iter()
            .map(|n| match n {
                0 => FuzzMove::Conservative,
                1 => FuzzMove::Aggressive,
                2 => FuzzMove::AllIn,
                _ => FuzzMove::FoldIfSafe,
            })
            .collect()
    })
}

struct Setup<'a> {
    env: Env,
    client: PokerTableContractClient<'a>,
    token: TokenClient<'a>,
    token_admin: StellarAssetClient<'a>,
    admin: Address,
    committee: Address,
    verifier: Address,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();

    let contract_id = env.register(PokerTableContract, ());
    let client = PokerTableContractClient::new(&env, &contract_id);

    let token_admin_addr = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin_addr);
    let token = TokenClient::new(&env, &sac.address());
    let token_admin = StellarAssetClient::new(&env, &sac.address());

    let admin = Address::generate(&env);
    let committee = Address::generate(&env);
    let verifier = env.register(crate::verifier::ZkVerifierContract, ());

    Setup {
        env,
        client,
        token,
        token_admin,
        admin,
        committee,
        verifier,
    }
}

fn config(s: &Setup, player_count: u32, rake_bps: u32) -> TableConfig {
    let game_hub = s.env.register(GameHubContract, ());
    TableConfig {
        token: s.token.address.clone(),
        min_buy_in: 100,
        max_buy_in: 2_000,
        small_blind: 5,
        big_blind: 10,
        min_players: 2,
        max_players: player_count,
        timeout_ledgers: 100,
        committee: s.committee.clone(),
        verifier: s.verifier.clone(),
        game_hub,
        rake_bps,
        max_rebuys: 0,
        jackpot_rake_share_bps: 0,
        min_bad_beat_category: 7,
        min_bad_beat_rank: 12,
    }
}

fn join_players(s: &Setup, table_id: u32, buy_ins: &[i128]) -> std::vec::Vec<Address> {
    let mut players = std::vec::Vec::new();
    for buy_in in buy_ins {
        let player = Address::generate(&s.env);
        s.token_admin.mint(&player, buy_in);
        s.client.join_table(&table_id, &player, buy_in);
        players.push(player);
    }
    players
}

fn commit_deal(s: &Setup, table_id: u32, players: u32) {
    let deck_root = BytesN::from_array(&s.env, &[7u8; 32]);
    let mut commitments: Vec<BytesN<32>> = Vec::new(&s.env);
    for i in 0..players {
        commitments.push_back(BytesN::from_array(&s.env, &[10u8 + i as u8; 32]));
    }
    let mut dealt_indices: Vec<u32> = Vec::new(&s.env);
    for i in 0..(players * 2) {
        dealt_indices.push_back(i);
    }
    s.client.commit_deal(
        &table_id,
        &s.committee,
        &deck_root,
        &commitments,
        &dealt_indices,
        &Bytes::new(&s.env),
        &Bytes::new(&s.env),
    );
}

fn reveal_if_needed(s: &Setup, table_id: u32, phase: &GamePhase, next_index: &mut u32) {
    let count = match phase {
        GamePhase::DealingFlop => 3,
        GamePhase::DealingTurn | GamePhase::DealingRiver => 1,
        _ => return,
    };
    let mut cards: Vec<u32> = Vec::new(&s.env);
    let mut indices: Vec<u32> = Vec::new(&s.env);
    for _ in 0..count {
        cards.push_back(20 + *next_index);
        indices.push_back(*next_index);
        *next_index += 1;
    }
    s.client.reveal_board(
        &table_id,
        &s.committee,
        &cards,
        &indices,
        &Bytes::new(&s.env),
        &Bytes::new(&s.env),
    );
}

fn showdown_inputs(env: &Env, table: &TableState, winner_seat: u32, tie_mask: u32) -> Bytes {
    let mut bytes = Bytes::new(env);
    for _ in 0..(27 * 32) {
        bytes.push_back(0);
    }
    for i in 0..table.players.len() {
        let p = table.players.get(i).unwrap();
        if p.folded {
            continue;
        }
        write_u32_field(&mut bytes, 13 + p.seat_index, 30 + p.seat_index);
        write_u32_field(&mut bytes, 19 + p.seat_index, 40 + p.seat_index);
    }
    write_u32_field(&mut bytes, 25, winner_seat);
    write_u32_field(&mut bytes, 26, tie_mask);
    bytes
}

fn write_u32_field(bytes: &mut Bytes, field_index: u32, value: u32) {
    let start = field_index * 32 + 28;
    bytes.set(start, ((value >> 24) & 0xff) as u8);
    bytes.set(start + 1, ((value >> 16) & 0xff) as u8);
    bytes.set(start + 2, ((value >> 8) & 0xff) as u8);
    bytes.set(start + 3, (value & 0xff) as u8);
}

fn active_seats(table: &TableState) -> std::vec::Vec<u32> {
    let mut seats = std::vec::Vec::new();
    for i in 0..table.players.len() {
        let p = table.players.get(i).unwrap();
        if !p.folded {
            seats.push(p.seat_index);
        }
    }
    seats
}

fn assert_invariants(s: &Setup, table_id: u32, initial_total: i128, rake_bps: u32) {
    let table = s.client.get_table(&table_id);
    let mut stacks = 0i128;
    let mut committed = 0i128;
    for i in 0..table.players.len() {
        let p = table.players.get(i).unwrap();
        assert!(p.stack >= 0);
        assert!(p.committed >= 0);
        assert!(p.bet_this_round >= 0);
        stacks += p.stack;
        committed += p.committed;
    }

    assert!(table.pot >= 0);
    assert!(table.rake_balance >= 0);
    assert_eq!(stacks + table.pot + table.rake_balance, initial_total);
    assert_eq!(s.token.balance(&s.client.address), initial_total);

    if table.phase == GamePhase::Settlement {
        assert_eq!(table.pot, 0);
        let max_rake = (committed * rake_bps as i128) / 10_000;
        assert!(table.rake_balance <= max_rake);

        let mut net_side_pots = 0i128;
        for i in 0..table.side_pots.len() {
            let pot = table.side_pots.get(i).unwrap();
            assert!(pot.amount >= 0);
            net_side_pots += pot.amount;
        }
        if table.side_pots.len() > 0 {
            assert_eq!(net_side_pots + table.rake_balance, committed);
        }
    }
}

fn choose_action(table: &TableState, mv: &FuzzMove) -> Action {
    let p = table.players.get(table.current_turn).unwrap();
    let mut current_bet = 0i128;
    for i in 0..table.players.len() {
        let bet = table.players.get(i).unwrap().bet_this_round;
        if bet > current_bet {
            current_bet = bet;
        }
    }
    let to_call = current_bet - p.bet_this_round;
    match mv {
        FuzzMove::Conservative => {
            if to_call > 0 {
                Action::Call
            } else {
                Action::Check
            }
        }
        FuzzMove::Aggressive => {
            if to_call > 0 && p.stack > to_call + table.config.big_blind {
                Action::Raise(table.config.big_blind)
            } else if to_call > 0 {
                Action::Call
            } else if p.stack >= table.config.big_blind {
                Action::Bet(table.config.big_blind)
            } else {
                Action::AllIn
            }
        }
        FuzzMove::AllIn => Action::AllIn,
        FuzzMove::FoldIfSafe => {
            if active_seats(table).len() > 2 {
                Action::Fold
            } else if to_call > 0 {
                Action::Call
            } else {
                Action::Check
            }
        }
    }
}

fn settle_showdown(s: &Setup, table_id: u32) {
    let table = s.client.get_table(&table_id);
    if table.phase != GamePhase::Showdown {
        return;
    }
    let active = active_seats(&table);
    let winner = active.first().copied().unwrap_or(0);
    let public_inputs = showdown_inputs(&s.env, &table, winner, 0);
    let mut hole_cards: Vec<(u32, u32)> = Vec::new(&s.env);
    let mut salts: Vec<(BytesN<32>, BytesN<32>)> = Vec::new(&s.env);
    for seat in active {
        hole_cards.push_back((30 + seat, 40 + seat));
        salts.push_back((
            BytesN::from_array(&s.env, &[0u8; 32]),
            BytesN::from_array(&s.env, &[0u8; 32]),
        ));
    }
    let bad_beat_scores: Vec<(u32, u32)> = Vec::new(&s.env);
    s.client.submit_showdown(
        &table_id,
        &s.committee,
        &hole_cards,
        &salts,
        &Bytes::new(&s.env),
        &public_inputs,
        &bad_beat_scores,
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn prop_full_lifecycle_preserves_chip_accounting(
        player_count in 2u32..=6,
        rake_bps in 0u32..=MAX_RAKE_BPS,
        buy_in_seed in prop::collection::vec(100i128..=2_000i128, 6),
        moves in moves_strategy(),
    ) {
        let s = setup();
        let buy_ins = &buy_in_seed[..player_count as usize];
        for buy_in in buy_ins {
            prop_assert!(*buy_in >= 100 && *buy_in <= 2_000);
        }
        let initial_total: i128 = buy_ins.iter().sum();

        let table_id = s.client.create_table(&s.admin, &config(&s, player_count, rake_bps));
        let players = join_players(&s, table_id, buy_ins);
        assert_invariants(&s, table_id, initial_total, rake_bps);

        s.client.start_hand(&table_id);
        assert_invariants(&s, table_id, initial_total, rake_bps);
        commit_deal(&s, table_id, player_count);
        assert_invariants(&s, table_id, initial_total, rake_bps);

        let mut next_board_index = player_count * 2;
        for mv in &moves {
            let table = s.client.get_table(&table_id);
            match table.phase {
                GamePhase::Preflop | GamePhase::Flop | GamePhase::Turn | GamePhase::River => {
                    let seat = table.current_turn;
                    let player = players.get(seat as usize).unwrap();
                    let action = choose_action(&table, mv);
                    s.client.player_action(&table_id, player, &action);
                    assert_invariants(&s, table_id, initial_total, rake_bps);
                }
                GamePhase::DealingFlop | GamePhase::DealingTurn | GamePhase::DealingRiver => {
                    reveal_if_needed(&s, table_id, &table.phase, &mut next_board_index);
                    assert_invariants(&s, table_id, initial_total, rake_bps);
                }
                GamePhase::Showdown => {
                    settle_showdown(&s, table_id);
                    assert_invariants(&s, table_id, initial_total, rake_bps);
                    break;
                }
                GamePhase::Settlement => {
                    assert_invariants(&s, table_id, initial_total, rake_bps);
                    break;
                }
                _ => {}
            }
        }

        let table = s.client.get_table(&table_id);
        if table.phase == GamePhase::Showdown {
            settle_showdown(&s, table_id);
        }
        assert_invariants(&s, table_id, initial_total, rake_bps);
    }
}
