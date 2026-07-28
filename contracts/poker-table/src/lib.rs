#![no_std]
#![allow(deprecated)]

use soroban_sdk::{contract, contractimpl, token, Address, Bytes, BytesN, Env, Symbol, Vec};

mod betting;
mod constant_time;
mod game;
mod game_hub;
#[cfg(test)]
mod gas_regression_test;
mod history;
#[cfg(test)]
mod invariants_test;
#[cfg(test)]
mod lifecycle_invariants_test;
mod pot;
#[cfg(test)]
mod state_machine_test;
mod test;
mod timeout;
#[cfg(test)]
mod tournament_lifecycle_test;
#[cfg(test)]
mod min_raise_test;
mod types;
mod verifier;

use types::*;

/// TTL for table storage (30 days in ledgers, ~5 seconds per ledger)
const TABLE_TTL_THRESHOLD: u32 = 17_280; // ~1 day — trigger extension when below this
const TABLE_TTL_EXTEND: u32 = 518_400; // ~30 days
const BOARD_INDICES_COUNT: u32 = 5; // flop(3) + turn(1) + river(1)
const MAX_PLAYERS_PER_TABLE: u32 = 6;

#[contract]
pub struct PokerTableContract;

fn require_not_paused(env: &Env, table_id: u32) -> Result<(), PokerTableError> {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused(table_id))
        .unwrap_or(false)
    {
        return Err(PokerTableError::ContractPaused);
    }
    Ok(())
}

fn load_table(env: &Env, table_id: u32) -> Result<TableState, PokerTableError> {
    let key = DataKey::Table(table_id);
    let table: TableState = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(PokerTableError::TableNotFound)?;
    // Extend TTL on every read to keep active tables alive
    env.storage()
        .persistent()
        .extend_ttl(&key, TABLE_TTL_THRESHOLD, TABLE_TTL_EXTEND);
    Ok(table)
}

fn save_table(env: &Env, table: &TableState) {
    let key = DataKey::Table(table.id);
    env.storage().persistent().set(&key, table);
    env.storage()
        .persistent()
        .extend_ttl(&key, TABLE_TTL_THRESHOLD, TABLE_TTL_EXTEND);
    // Keep instance storage alive too
    env.storage()
        .instance()
        .extend_ttl(TABLE_TTL_THRESHOLD, TABLE_TTL_EXTEND);
}

/// Extract a u32 from a BN254 field element at `field_index` in public_inputs.
/// Small integers are encoded big-endian in the last 4 bytes.
fn extract_u32_from_public_inputs(public_inputs: &Bytes, field_index: u32) -> u32 {
    let start = field_index * 32 + 28;
    let b0 = public_inputs.get(start).unwrap_or(0);
    let b1 = public_inputs.get(start + 1).unwrap_or(0);
    let b2 = public_inputs.get(start + 2).unwrap_or(0);
    let b3 = public_inputs.get(start + 3).unwrap_or(0);
    (b0 as u32) << 24 | (b1 as u32) << 16 | (b2 as u32) << 8 | b3 as u32
}

/// Verify that the committee-submitted hole_cards (in active-player order, seat
/// order skipping folded) match the card values in the proof's public outputs.
///
/// Public output layout for showdown:
///   [13..19)  hole_card1[0..6] — seat-indexed
///   [19..25)  hole_card2[0..6] — seat-indexed
fn verify_hole_cards_against_proof(
    _env: &Env,
    table: &TableState,
    public_inputs: &Bytes,
    hole_cards: &Vec<(u32, u32)>,
) -> Result<(), PokerTableError> {
    let mut active_idx: u32 = 0;
    for i in 0..table.players.len() {
        let player = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if player.folded {
            continue;
        }
        let seat = player.seat_index;
        let proof_c1 = extract_u32_from_public_inputs(public_inputs, 13 + seat);
        let proof_c2 = extract_u32_from_public_inputs(public_inputs, 19 + seat);
        let (submitted_c1, submitted_c2) = hole_cards
            .get(active_idx)
            .ok_or(PokerTableError::InvalidHoleCards)?;
        if constant_time::u32_ne(proof_c1, submitted_c1)
            || constant_time::u32_ne(proof_c2, submitted_c2)
        {
            return Err(PokerTableError::HoleCardMismatch);
        }
        active_idx += 1;
    }
    if active_idx == 0 {
        return Err(PokerTableError::InvalidHoleCards);
    }
    Ok(())
}

/// Record that `player` now holds a seat at `table_id`.
///
/// The index is a convenience for multi-table clients, not a limit: nothing
/// here rejects a wallet for sitting at too many tables. Its length is the
/// number of live seats the wallet holds, which is bounded in practice by the
/// buy-in each seat costs.
fn index_player_table(env: &Env, player: &Address, table_id: u32) {
    let key = DataKey::PlayerTables(player.clone());
    let mut tables: Vec<u32> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));

    for i in 0..tables.len() {
        if let Some(existing) = tables.get(i) {
            if constant_time::u32_eq(existing, table_id) {
                return; // already indexed
            }
        }
    }

    tables.push_back(table_id);
    env.storage().persistent().set(&key, &tables);
    env.storage()
        .persistent()
        .extend_ttl(&key, TABLE_TTL_THRESHOLD, TABLE_TTL_EXTEND);
}

/// Drop `table_id` from `player`'s seat index.
fn unindex_player_table(env: &Env, player: &Address, table_id: u32) {
    let key = DataKey::PlayerTables(player.clone());
    let tables: Vec<u32> = match env.storage().persistent().get(&key) {
        Some(t) => t,
        None => return,
    };

    let mut remaining: Vec<u32> = Vec::new(env);
    for i in 0..tables.len() {
        if let Some(existing) = tables.get(i) {
            if constant_time::u32_ne(existing, table_id) {
                remaining.push_back(existing);
            }
        }
    }

    if remaining.is_empty() {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &remaining);
        env.storage()
            .persistent()
            .extend_ttl(&key, TABLE_TTL_THRESHOLD, TABLE_TTL_EXTEND);
    }
}

/// Find a seated player's index, or `PlayerNotAtTable`.
fn find_seat(env: &Env, table: &TableState, player: &Address) -> Result<u32, PokerTableError> {
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if constant_time::address_eq(env, &p.address, player) {
            return Ok(i);
        }
    }
    Err(PokerTableError::PlayerNotAtTable)
}

/// Compute the 5 board indices for each RIT run.
/// Run 1 uses the next 5 unused positions starting from current dealt_indices.
/// Run 2 uses the 5 positions after Run 1's.
fn compute_rit_board_indices(
    _env: &Env,
    table: &TableState,
    rit: &mut RitState,
) -> Result<(), PokerTableError> {
    let mut used: [bool; 52] = [false; 52];
    for i in 0..table.dealt_indices.len() {
        if let Some(idx) = table.dealt_indices.get(i) {
            if idx < 52 {
                used[idx as usize] = true;
            }
        }
    }

    // Find the next 5 unused indices for Run 1
    let mut run1_indices: [u32; 5] = [0; 5];
    let mut found: u32 = 0;
    for idx in 0..52 {
        if !used[idx] && found < 5 {
            run1_indices[found as usize] = idx as u32;
            found += 1;
            used[idx] = true;
        }
    }

    // Find the next 5 unused indices for Run 2
    let mut run2_indices: [u32; 5] = [0; 5];
    let mut found: u32 = 0;
    for idx in 0..52 {
        if !used[idx] && found < 5 {
            run2_indices[found as usize] = idx as u32;
            found += 1;
            used[idx] = true;
        }
    }

    // For shared board cards, replace the first N indices with the actual shared indices
    let shared_count = rit.shared_board_count as usize;
    for i in 0..shared_count {
        if let Some(idx) = table.dealt_indices.get(
            table.dealt_indices.len() - shared_count as u32 + i as u32,
        ) {
            run1_indices[i] = idx;
            run2_indices[i] = idx;
        }
    }

    rit.run1_board_indices = Vec::new(_env);
    for i in 0..5 {
        rit.run1_board_indices.push_back(run1_indices[i]);
    }
    rit.run2_board_indices = Vec::new(_env);
    for i in 0..5 {
        rit.run2_board_indices.push_back(run2_indices[i]);
    }

    Ok(())
}

fn derive_session_id(table_id: u32, hand_number: u32) -> u32 {
    // Deterministic 32-bit hash of (table_id, hand_number).
    let mut x = table_id ^ hand_number.rotate_left(16);
    x = x.wrapping_mul(0x9E37_79B1);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x
}

#[contractimpl]
impl PokerTableContract {
    /// Initialize a new poker table with configuration.
    pub fn create_table(
        env: Env,
        admin: Address,
        config: TableConfig,
    ) -> Result<u32, PokerTableError> {
        admin.require_auth();

        if config.rake_bps > pot::MAX_RAKE_BPS {
            return Err(PokerTableError::RakeBpsExceedsMax);
        }
        if config.min_players < 2
            || config.max_players < config.min_players
            || config.max_players > MAX_PLAYERS_PER_TABLE
        {
            return Err(PokerTableError::InvalidPlayerCount);
        }

        let table_id = env
            .storage()
            .instance()
            .get::<Symbol, u32>(&Symbol::new(&env, "next_id"))
            .unwrap_or(0);

        let table = TableState {
            id: table_id,
            admin: admin.clone(),
            config: config.clone(),
            phase: GamePhase::Waiting,
            players: Vec::new(&env),
            dealer_seat: 0,
            current_turn: 0,
            pot: 0,
            side_pots: Vec::new(&env),
            deck_root: BytesN::from_array(&env, &[0u8; 32]),
            hand_commitments: Vec::new(&env),
            board_cards: Vec::new(&env),
            dealt_indices: Vec::new(&env),
            hand_number: 0,
            last_action_ledger: env.ledger().sequence(),
            committee: config.committee,
            session_id: 0,
            rake_balance: 0,
            action_deadline: 0,
            hand_actions: Vec::new(&env),
            rit_state: None,
            jackpot_balance: 0,
            last_raise_size: config.big_blind,
        };

        save_table(&env, &table);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "next_id"), &(table_id + 1));

        env.events()
            .publish((Symbol::new(&env, "table_created"), table_id), admin);

        Ok(table_id)
    }

    /// Join a table with a buy-in deposit.
    pub fn join_table(
        env: Env,
        table_id: u32,
        player: Address,
        buy_in: i128,
    ) -> Result<u32, PokerTableError> {
        player.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        if !matches!(table.phase, GamePhase::Waiting) {
            return Err(PokerTableError::TableNotAcceptingPlayers);
        }
        if (table.players.len() as u32) >= table.config.max_players {
            return Err(PokerTableError::TableFull);
        }
        if buy_in < table.config.min_buy_in || buy_in > table.config.max_buy_in {
            return Err(PokerTableError::InvalidBuyIn);
        }

        // Check player not already seated.
        for i in 0..table.players.len() {
            let p = table
                .players
                .get(i)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;
            if constant_time::address_eq(&env, &p.address, &player) {
                return Err(PokerTableError::AlreadySeated);
            }
        }

        // Transfer buy-in to contract.
        let token = token::Client::new(&env, &table.config.token);
        token.transfer(&player, &env.current_contract_address(), &buy_in);

        let seat = table.players.len() as u32;
        table.players.push_back(PlayerState {
            address: player.clone(),
            stack: buy_in,
            bet_this_round: 0,
            committed: 0,
            folded: false,
            all_in: false,
            sitting_out: false,
            seat_index: seat,
            total_buy_in: buy_in,
            rebuy_count: 0,
        });

        save_table(&env, &table);
        index_player_table(&env, &player, table_id);

        env.events().publish(
            (Symbol::new(&env, "player_joined"), table_id),
            (player, seat),
        );

        Ok(seat)
    }

    /// Tables a wallet is currently seated at.
    ///
    /// A wallet may sit at any number of tables at once — there is no per-player
    /// cap, only the per-table `max_players` and whatever capital the player is
    /// willing to put up. This index exists so a multi-table client can restore
    /// a player's open seats after a reload without scanning every table.
    ///
    /// The list is maintained on `join_table` and `leave_table`, so it holds
    /// only live seats.
    pub fn get_player_tables(env: Env, player: Address) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::PlayerTables(player))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Number of tables a wallet is currently seated at.
    pub fn get_player_table_count(env: Env, player: Address) -> u32 {
        Self::get_player_tables(env, player).len()
    }

    /// Top a seated player's stack up by a partial amount.
    ///
    /// A rebuy may be for any amount that leaves the player's stack inside the
    /// table's `[min_buy_in, max_buy_in]` band — it does not have to be a full
    /// buy-in. A player who has been ground down to 40 chips at a 100/1000
    /// table can add anywhere from 60 (back to the minimum) to 960 (up to the
    /// maximum). A single rebuy may never exceed one full buy-in.
    ///
    /// Rebuys are only allowed between hands, so the chips cannot appear
    /// mid-hand and change what an opponent is playing against. Each one is
    /// counted against `TableConfig::max_rebuys` (0 = unlimited).
    ///
    /// Returns the player's new stack.
    pub fn rebuy(
        env: Env,
        table_id: u32,
        player: Address,
        amount: i128,
    ) -> Result<i128, PokerTableError> {
        player.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        // Chips may only enter between hands — never while a hand is live.
        if !matches!(table.phase, GamePhase::Waiting | GamePhase::Settlement) {
            return Err(PokerTableError::CannotRebuyDuringActiveHand);
        }

        let seat = find_seat(&env, &table, &player)?;
        let mut p = table
            .players
            .get(seat)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;

        if table.config.max_rebuys > 0 && p.rebuy_count >= table.config.max_rebuys {
            return Err(PokerTableError::RebuyLimitReached);
        }

        let new_stack = p.stack + amount;
        if amount <= 0
            || amount > table.config.max_buy_in
            || new_stack > table.config.max_buy_in
            || new_stack < table.config.min_buy_in
        {
            return Err(PokerTableError::InvalidRebuyAmount);
        }

        // Take the chips before crediting the stack.
        let token = token::Client::new(&env, &table.config.token);
        token.transfer(&player, &env.current_contract_address(), &amount);

        p.stack = new_stack;
        p.total_buy_in += amount;
        p.rebuy_count += 1;
        let rebuy_count = p.rebuy_count;
        table.players.set(seat, p);

        save_table(&env, &table);

        env.events().publish(
            (Symbol::new(&env, "player_rebuy"), table_id),
            (player, amount, new_stack, rebuy_count),
        );

        Ok(new_stack)
    }

    /// Chips a seated player has deposited this session (initial buy-in plus
    /// every rebuy) and how many rebuys they have used.
    pub fn get_player_buy_in(
        env: Env,
        table_id: u32,
        player: Address,
    ) -> Result<(i128, u32), PokerTableError> {
        let table = load_table(&env, table_id)?;
        let seat = find_seat(&env, &table, &player)?;
        let p = table
            .players
            .get(seat)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        Ok((p.total_buy_in, p.rebuy_count))
    }

    /// Update the per-session rebuy limit (admin only). `0` means unlimited.
    /// Lowering the limit below what a player has already used simply stops
    /// them rebuying again; it never claws chips back.
    pub fn set_max_rebuys(
        env: Env,
        table_id: u32,
        max_rebuys: u32,
    ) -> Result<(), PokerTableError> {
        let mut table = load_table(&env, table_id)?;
        table.admin.require_auth();
        table.config.max_rebuys = max_rebuys;
        save_table(&env, &table);

        env.events().publish(
            (Symbol::new(&env, "max_rebuys_updated"), table_id),
            max_rebuys,
        );
        Ok(())
    }

    /// Leave the table and withdraw remaining stack.
    pub fn leave_table(env: Env, table_id: u32, player: Address) -> Result<i128, PokerTableError> {
        player.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        // Can only leave during Waiting phase or between hands.
        if !matches!(table.phase, GamePhase::Waiting | GamePhase::Settlement) {
            return Err(PokerTableError::CannotLeaveDuringActiveHand);
        }

        let mut withdrawn: i128 = 0;
        let mut found = false;
        let mut new_players: Vec<PlayerState> = Vec::new(&env);

        for i in 0..table.players.len() {
            let p = table
                .players
                .get(i)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;
            if constant_time::address_eq(&env, &p.address, &player) {
                found = true;
                withdrawn = p.stack;
                if withdrawn > 0 {
                    let token = token::Client::new(&env, &table.config.token);
                    token.transfer(&env.current_contract_address(), &player, &withdrawn);
                }
            } else {
                new_players.push_back(p);
            }
        }

        if !found {
            return Err(PokerTableError::PlayerNotAtTable);
        }

        // Renumber the remaining seats so `seat_index` keeps matching the
        // player's position in the vector. Betting, side-pot eligibility and
        // the showdown proof's seat-indexed public outputs all treat the two as
        // the same number, so a gap left by the departing player would send
        // actions and payouts to the wrong seat. Leaving is only allowed
        // between hands, so no pot state depends on the old numbering.
        for i in 0..new_players.len() {
            let mut p = new_players
                .get(i)
                .ok_or(PokerTableError::InvalidPlayerIndex)?;
            p.seat_index = i;
            new_players.set(i, p);
        }
        table.players = new_players;

        // Keep the button and the turn pointer inside the shrunken table.
        let remaining = table.players.len() as u32;
        if remaining == 0 {
            table.dealer_seat = 0;
            table.current_turn = 0;
        } else {
            table.dealer_seat %= remaining;
            table.current_turn %= remaining;
        }

        save_table(&env, &table);
        unindex_player_table(&env, &player, table_id);

        env.events().publish(
            (Symbol::new(&env, "player_left"), table_id),
            (player, withdrawn),
        );

        Ok(withdrawn)
    }

    /// Start a new hand. Called after enough players are seated.
    pub fn start_hand(env: Env, table_id: u32) -> Result<(), PokerTableError> {
        require_not_paused(&env, table_id)?;
        let mut table = load_table(&env, table_id)?;

        if !matches!(table.phase, GamePhase::Waiting | GamePhase::Settlement) {
            return Err(PokerTableError::HandAlreadyInProgress);
        }
        if table.players.len() < table.config.min_players {
            return Err(PokerTableError::NotEnoughPlayers);
        }

        game::start_new_hand(&env, &mut table)?;

        // Notify game hub: start_game with first 2 players.
        let p1 = table
            .players
            .get(0)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        let p2 = table
            .players
            .get(1)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;

        table.session_id = derive_session_id(table.id, table.hand_number);
        game_hub::notify_start(
            &env,
            &table.config.game_hub,
            &env.current_contract_address(),
            table.session_id,
            &p1.address,
            &p2.address,
            p1.stack,
            p2.stack,
        );

        save_table(&env, &table);

        env.events().publish(
            (Symbol::new(&env, "hand_started"), table_id),
            table.hand_number,
        );

        Ok(())
    }

    /// Committee submits deal commitment and proof.
    pub fn commit_deal(
        env: Env,
        table_id: u32,
        committee: Address,
        deck_root: BytesN<32>,
        hand_commitments: Vec<BytesN<32>>,
        dealt_indices: Vec<u32>,
        proof: Bytes,
        public_inputs: Bytes,
    ) -> Result<(), PokerTableError> {
        committee.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        if !matches!(table.phase, GamePhase::Dealing) {
            return Err(PokerTableError::NotInDealingPhase);
        }
        if constant_time::address_ne(&env, &committee, &table.committee) {
            return Err(PokerTableError::NotAuthorizedCommittee);
        }
        if hand_commitments.len() != table.players.len() {
            return Err(PokerTableError::WrongCommitmentCount);
        }

        // Verify deal proof via ZK verifier contract.
        let verifier_client = verifier::ZkVerifierClient::new(&env, &table.config.verifier);
        if !verifier_client.verify_deal(&proof, &public_inputs, &deck_root, &hand_commitments) {
            return Err(PokerTableError::DealProofVerificationFailed);
        }

        table.deck_root = deck_root;
        table.hand_commitments = hand_commitments;
        table.dealt_indices = dealt_indices;
        table.phase = GamePhase::Preflop;
        table.last_action_ledger = env.ledger().sequence();

        // Set first player to act (left of big blind).
        let num_players = table.players.len() as u32;
        if num_players < 2 {
            return Err(PokerTableError::NotEnoughPlayers);
        }
        table.current_turn = (table.dealer_seat + 3) % num_players;
        // Set action deadline for the first player to act
        table.action_deadline = env.ledger().sequence() + table.config.timeout_ledgers;

        save_table(&env, &table);

        env.events().publish(
            (Symbol::new(&env, "deal_committed"), table_id),
            (table.hand_number, table.hand_commitments.clone()),
        );

        Ok(())
    }

    /// Player submits a betting action.
    pub fn player_action(
        env: Env,
        table_id: u32,
        player: Address,
        action: Action,
    ) -> Result<(), PokerTableError> {
        player.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        if !matches!(
            table.phase,
            GamePhase::Preflop | GamePhase::Flop | GamePhase::Turn | GamePhase::River
        ) {
            return Err(PokerTableError::NotInBettingPhase);
        }
        // No betting actions during RIT (all players are all-in)
        if table.rit_state.as_ref().map(|r| r.active).unwrap_or(false) {
            return Err(PokerTableError::NotInBettingPhase);
        }

        betting::process_action(&env, &mut table, &player, &action)?;

        save_table(&env, &table);
        Ok(())
    }

    /// Opt into Run-It-Twice when two players are heads-up all-in.
    /// Both all-in players must call this with opt_in=true to enable RIT.
    /// If either player calls with opt_in=false or the phase times out,
    /// play continues normally.
    pub fn rit_opt_in(
        env: Env,
        table_id: u32,
        player: Address,
        opt_in: bool,
    ) -> Result<(), PokerTableError> {
        player.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        if !matches!(table.phase, GamePhase::AwaitingRunItTwice) {
            return Err(PokerTableError::NotInRitPhase);
        }
        if table.rit_state.is_some() {
            return Err(PokerTableError::RitAlreadyDecided);
        }

        let seat = find_seat(&env, &table, &player)?;

        // Verify exactly 2 non-folded all-in players
        let mut non_folded_all_in: Vec<u32> = Vec::new(&env);
        for i in 0..table.players.len() {
            if let Some(p) = table.players.get(i) {
                if !p.folded && p.all_in {
                    non_folded_all_in.push_back(p.seat_index);
                }
            }
        }
        if non_folded_all_in.len() != 2 {
            return Err(PokerTableError::NotHeadsUpAllIn);
        }

        let p1_seat = non_folded_all_in.get(0).ok_or(PokerTableError::InvalidPlayerIndex)?;
        let p2_seat = non_folded_all_in.get(1).ok_or(PokerTableError::InvalidPlayerIndex)?;

        if seat != p1_seat && seat != p2_seat {
            return Err(PokerTableError::NotHeadsUpAllIn);
        }

        if !opt_in {
            // Player declined RIT - continue normally
            let next_phase = match table.board_cards.len() {
                0 => GamePhase::DealingFlop,
                3 => GamePhase::DealingTurn,
                4 => GamePhase::DealingRiver,
                _ => GamePhase::Showdown,
            };
            table.phase = next_phase;
            table.last_action_ledger = env.ledger().sequence();
            table.action_deadline = 0;
            save_table(&env, &table);
            env.events().publish(
                (Symbol::new(&env, "rit_declined"), table_id),
                (player, seat),
            );
            return Ok(());
        }

        // Player opted in
        // Initialize or update RIT state
        let shared_board_count = table.board_cards.len() as u32;
        let mut rit = RitState {
            active: false,
            player1_seat: p1_seat,
            player2_seat: p2_seat,
            player1_opted_in: false,
            player2_opted_in: false,
            shared_board_count,
            current_run: 1,
            run1_board_indices: Vec::new(&env),
            run2_board_indices: Vec::new(&env),
            run1_winner: 0,
            run2_winner: 0,
        };

        if seat == p1_seat {
            rit.player1_opted_in = true;
        } else {
            rit.player2_opted_in = true;
        }

        // Check if both have opted in
        if rit.player1_opted_in && rit.player2_opted_in {
            rit.active = true;
            table.rit_state = Some(rit);

            // Pre-compute board indices for both runs
            let rit_state = table.rit_state.as_mut().unwrap();
            compute_rit_board_indices(&env, table, rit_state)?;

            // Transition to appropriate dealing phase based on how many shared cards
            table.phase = match shared_board_count {
                0 => GamePhase::DealingFlop,
                3 => GamePhase::DealingTurn,
                4 => GamePhase::DealingRiver,
                _ => GamePhase::DealingFlop,
            };
            table.last_action_ledger = env.ledger().sequence();
            table.action_deadline = 0;

            save_table(&env, &table);
            env.events().publish(
                (Symbol::new(&env, "rit_activated"), table_id),
                (p1_seat, p2_seat),
            );
        } else {
            // Waiting for other player
            table.rit_state = Some(rit);
            save_table(&env, &table);
            env.events().publish(
                (Symbol::new(&env, "rit_opted_in"), table_id),
                (player, seat),
            );
        }

        Ok(())
    }

    /// Committee reveals board cards (flop/turn/river) with proof.
    /// Handles RIT dual-board reveals when active.
    pub fn reveal_board(
        env: Env,
        table_id: u32,
        committee: Address,
        cards: Vec<u32>,
        indices: Vec<u32>,
        proof: Bytes,
        public_inputs: Bytes,
    ) -> Result<(), PokerTableError> {
        committee.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        if constant_time::address_ne(&env, &committee, &table.committee) {
            return Err(PokerTableError::NotAuthorizedCommittee);
        }

        let expected_cards: u32 = match table.phase {
            GamePhase::DealingFlop => 3,
            GamePhase::DealingTurn => 1,
            GamePhase::DealingRiver => 1,
            _ => return Err(PokerTableError::NotInRevealPhase),
        };

        if cards.len() != expected_cards || indices.len() != expected_cards {
            return Err(PokerTableError::WrongCardCount);
        }

        // Verify reveal proof via zk-verifier.
        let verifier_client = verifier::ZkVerifierClient::new(&env, &table.config.verifier);
        if !verifier_client.verify_reveal(
            &proof,
            &public_inputs,
            &table.deck_root,
            &cards,
            &indices,
        ) {
            return Err(PokerTableError::RevealProofVerificationFailed);
        }

        let rit_run = table
            .rit_state
            .as_ref()
            .map(|r| {
                if r.active {
                    if r.current_run == 2 {
                        2u32
                    } else {
                        1u32
                    }
                } else {
                    0u32
                }
            })
            .unwrap_or(0);

        // Add revealed cards to board tracking.
        for i in 0..cards.len() {
            let card = cards.get(i).ok_or(PokerTableError::WrongCardCount)?;
            let idx = indices.get(i).ok_or(PokerTableError::WrongCardCount)?;

            // Always track in dealt_indices for circuit consistency
            table.dealt_indices.push_back(idx);

            // Track in board_cards (both runs get their board here)
            table.board_cards.push_back(card);
            if let Some(ref mut rit) = table.rit_state {
                if rit.active && rit_run == 1 {
                    rit.run1_board_indices.push_back(idx);
                }
            }
        }

        // Transition to next phase (standard phases for both runs).
        table.phase = match table.phase {
            GamePhase::DealingFlop => GamePhase::Flop,
            GamePhase::DealingTurn => GamePhase::Turn,
            GamePhase::DealingRiver => GamePhase::River,
            _ => return Err(PokerTableError::NotInRevealPhase),
        };
        table.last_action_ledger = env.ledger().sequence();

        // Reset betting state for new round.
        betting::reset_round(&env, &mut table)?;

        save_table(&env, &table);

        env.events().publish(
            (Symbol::new(&env, "board_revealed"), table_id),
            (cards, indices),
        );

        Ok(())
    }

    /// Submit showdown: reveal hole cards, verify winner, settle.
    /// Handles Run-It-Twice showdowns for both runs when active.
    ///
    /// `bad_beat_scores` is a committee-submitted vector of `(seat, hand_score)`
    /// pairs for every non-folded player at showdown.  The contract checks
    /// these against the configured bad-beat threshold and, if a qualifying
    /// losing hand exists, pays out the jackpot pool.  Pass an empty vector
    /// to skip the check.
    pub fn submit_showdown(
        env: Env,
        table_id: u32,
        committee: Address,
        hole_cards: Vec<(u32, u32)>,
        _salts: Vec<(BytesN<32>, BytesN<32>)>,
        proof: Bytes,
        public_inputs: Bytes,
        bad_beat_scores: Vec<(u32, u32)>,
    ) -> Result<(), PokerTableError> {
        committee.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        let is_rit_run1 = matches!(table.phase, GamePhase::ShowdownRun1);
        let is_rit_run2 = matches!(table.phase, GamePhase::ShowdownRun2);
        let is_normal = matches!(table.phase, GamePhase::Showdown);

        if !is_normal && !is_rit_run1 && !is_rit_run2 {
            return Err(PokerTableError::NotInShowdownPhase);
        }
        if constant_time::address_ne(&env, &committee, &table.committee) {
            return Err(PokerTableError::NotAuthorizedCommittee);
        }

        // Extract board indices based on which run we're proving
        let board_indices: Vec<u32> = if is_rit_run1 {
            // Use Run 1's pre-computed board indices
            table
                .rit_state
                .as_ref()
                .map(|r| r.run1_board_indices.clone())
                .ok_or(PokerTableError::BoardNotComplete)?
        } else if is_rit_run2 {
            // Use Run 2's pre-computed board indices
            table
                .rit_state
                .as_ref()
                .map(|r| r.run2_board_indices.clone())
                .ok_or(PokerTableError::BoardNotComplete)?
        } else {
            // Normal: extract last 5 board indices from dealt_indices
            if table.dealt_indices.len() < BOARD_INDICES_COUNT {
                return Err(PokerTableError::BoardNotComplete);
            }
            let board_start = table.dealt_indices.len() - BOARD_INDICES_COUNT;
            let mut indices: Vec<u32> = Vec::new(&env);
            for i in board_start..table.dealt_indices.len() {
                indices.push_back(
                    table
                        .dealt_indices
                        .get(i)
                        .ok_or(PokerTableError::BoardNotComplete)?,
                );
            }
            indices
        };

        if board_indices.len() != 5 {
            return Err(PokerTableError::BoardNotComplete);
        }

        // Verify showdown proof via zk-verifier.
        let verifier_client = verifier::ZkVerifierClient::new(&env, &table.config.verifier);
        if !verifier_client.verify_showdown(
            &proof,
            &public_inputs,
            &table.hand_commitments,
            &board_indices,
            &table.deck_root,
        ) {
            return Err(PokerTableError::ShowdownProofVerificationFailed);
        }

        let winner_index = extract_u32_from_public_inputs(&public_inputs, 25);
        let tie_mask = extract_u32_from_public_inputs(&public_inputs, 26);

        verify_hole_cards_against_proof(&env, &table, &public_inputs, &hole_cards)?;

        if is_rit_run1 {
            // Record Run 1 winner, then transition to Run 2 dealing
            if let Some(ref mut rit) = table.rit_state {
                rit.run1_winner = winner_index;
                rit.current_run = 2;
            }
            // Reset board_cards to shared cards only for Run 2 reveals
            let shared_count = table
                .rit_state
                .as_ref()
                .map(|r| r.shared_board_count)
                .unwrap_or(0) as u32;
            let mut shared: Vec<u32> = Vec::new(&env);
            for i in 0..shared_count {
                if let Some(card) = table.board_cards.get(i) {
                    shared.push_back(card);
                }
            }
            table.board_cards = shared;
        // Settle using the proved winner and optional tie mask from the proof
        // (not re-evaluating hands on-chain).
        game::settle_showdown(&env, &mut table, winner_index, tie_mask, &bad_beat_scores)?;

            table.phase = GamePhase::DealingFlop;
            table.last_action_ledger = env.ledger().sequence();
            table.action_deadline = 0;
            save_table(&env, &table);
            env.events().publish(
                (Symbol::new(&env, "showdown_run1"), table_id),
                (winner_index, tie_mask),
            );
            Ok(())
        } else if is_rit_run2 {
            // Record Run 2 winner, then go to RIT settlement
            if let Some(ref mut rit) = table.rit_state {
                rit.run2_winner = winner_index;
            }
            table.phase = GamePhase::RitSettlement;
            table.last_action_ledger = env.ledger().sequence();
            // Settle RIT pot split
            game::settle_rit(&env, &mut table)?;
            save_table(&env, &table);
            env.events().publish(
                (Symbol::new(&env, "showdown_run2"), table_id),
                (winner_index, tie_mask),
            );
            Ok(())
        } else {
            // Normal showdown
            game::settle_showdown(&env, &mut table, winner_index, tie_mask)?;
            save_table(&env, &table);
            Ok(())
        }
    }

    /// Claim timeout when opponent or committee is stalling.
    pub fn claim_timeout(env: Env, table_id: u32, claimer: Address) -> Result<(), PokerTableError> {
        claimer.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        timeout::process_timeout(&env, &mut table, &claimer)?;

        save_table(&env, &table);
        Ok(())
    }

    /// Force-fold a stalling player after the action deadline has passed.
    ///
    /// Any seated player may call this once the `action_deadline` ledger has
    /// been reached. The target player (must be the current turn) is folded
    /// and the deadline is reset for the next active player.
    pub fn force_fold(
        env: Env,
        table_id: u32,
        caller: Address,
        target_seat: u32,
    ) -> Result<(), PokerTableError> {
        caller.require_auth();
        require_not_paused(&env, table_id)?;

        let mut table = load_table(&env, table_id)?;

        timeout::force_fold(&env, &mut table, &caller, target_seat)?;

        save_table(&env, &table);
        Ok(())
    }

    /// Read current table state (view function).
    pub fn get_table(env: Env, table_id: u32) -> Result<TableState, PokerTableError> {
        load_table(&env, table_id)
    }

    // ========================================================================
    // Hand History (read-only)
    // ========================================================================

    /// Read the most recently completed hands, newest first.
    ///
    /// The table keeps a circular buffer of the last
    /// `history::HAND_HISTORY_CAPACITY` settled hands. Pass `limit = 0` to read
    /// every retained record, or a smaller number to page in just the latest
    /// few. Hands older than the window have been overwritten and are only
    /// recoverable from the `hand_archived` event stream.
    pub fn get_hand_history(env: Env, table_id: u32, limit: u32) -> Vec<HandRecord> {
        history::get_history(&env, table_id, limit)
    }

    /// Read a single archived hand by its hand number. Returns `None` once the
    /// hand has been evicted from the circular buffer.
    pub fn get_hand(env: Env, table_id: u32, hand_number: u32) -> Option<HandRecord> {
        history::get_hand(&env, table_id, hand_number)
    }

    /// Bookkeeping for a table's hand-history buffer: how many records are
    /// retained right now and how many hands have been archived in total.
    pub fn get_hand_history_meta(env: Env, table_id: u32) -> HandHistoryMeta {
        history::load_meta(&env, table_id)
    }

    /// Number of hands the history buffer can hold before it starts evicting.
    pub fn hand_history_capacity() -> u32 {
        history::HAND_HISTORY_CAPACITY
    }

    // ========================================================================
    // Admin Functions (Stellar Game Studio pattern)
    // ========================================================================

    /// Pause a table (admin only). All non-admin operations will revert while paused.
    /// NOTE: for production consider a timelock or multi-sig for unpause.
    pub fn pause(env: Env, table_id: u32) -> Result<(), PokerTableError> {
        let table = load_table(&env, table_id)?;
        table.admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Paused(table_id), &true);
        env.events()
            .publish((Symbol::new(&env, "table_paused"), table_id), table.admin);
        Ok(())
    }

    /// Unpause a table (admin only).
    /// NOTE: for production consider a timelock or multi-sig here.
    pub fn unpause(env: Env, table_id: u32) -> Result<(), PokerTableError> {
        let table = load_table(&env, table_id)?;
        table.admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Paused(table_id), &false);
        env.events()
            .publish((Symbol::new(&env, "table_unpaused"), table_id), table.admin);
        Ok(())
    }

    /// Returns true if the table is currently paused.
    pub fn is_paused(env: Env, table_id: u32) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused(table_id))
            .unwrap_or(false)
    }

    /// Get the admin address for a table.
    pub fn get_admin(env: Env, table_id: u32) -> Result<Address, PokerTableError> {
        let table = load_table(&env, table_id)?;
        Ok(table.admin)
    }

    /// Get the Game Hub address for a table.
    pub fn get_hub(env: Env, table_id: u32) -> Result<Address, PokerTableError> {
        let table = load_table(&env, table_id)?;
        Ok(table.config.game_hub)
    }

    /// Update the Game Hub address for a table (admin only).
    pub fn set_hub(env: Env, table_id: u32, new_hub: Address) -> Result<(), PokerTableError> {
        let mut table = load_table(&env, table_id)?;
        table.admin.require_auth();
        table.config.game_hub = new_hub;
        save_table(&env, &table);
        Ok(())
    }

    /// Upgrade the contract WASM (admin only).
    pub fn upgrade(
        env: Env,
        table_id: u32,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), PokerTableError> {
        let table = load_table(&env, table_id)?;
        table.admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    /// Update the rake (admin only). Capped at `MAX_RAKE_BPS` (5%).
    pub fn set_rake_bps(env: Env, table_id: u32, rake_bps: u32) -> Result<(), PokerTableError> {
        if rake_bps > pot::MAX_RAKE_BPS {
            return Err(PokerTableError::RakeBpsExceedsMax);
        }
        let mut table = load_table(&env, table_id)?;
        table.admin.require_auth();
        table.config.rake_bps = rake_bps;
        save_table(&env, &table);

        env.events()
            .publish((Symbol::new(&env, "rake_bps_updated"), table_id), rake_bps);
        Ok(())
    }

    /// Update the minimum seated players required to start a hand. This can
    /// only be changed between hands and must stay within the table capacity.
    pub fn set_min_players(
        env: Env,
        table_id: u32,
        min_players: u32,
    ) -> Result<(), PokerTableError> {
        let mut table = load_table(&env, table_id)?;
        table.admin.require_auth();
        if !matches!(table.phase, GamePhase::Waiting | GamePhase::Settlement) {
            return Err(PokerTableError::CannotChangeMinPlayersMidHand);
        }
        if min_players < 2 || min_players > table.config.max_players {
            return Err(PokerTableError::InvalidPlayerCount);
        }
        table.config.min_players = min_players;
        save_table(&env, &table);

        env.events().publish(
            (Symbol::new(&env, "min_players_updated"), table_id),
            min_players,
        );
        Ok(())
    }

    /// Read the rake accumulated so far for a table (view function).
    pub fn get_rake_balance(env: Env, table_id: u32) -> Result<i128, PokerTableError> {
        let table = load_table(&env, table_id)?;
        Ok(table.rake_balance)
    }

    /// Read the bad-beat jackpot pool balance for a table (view function).
    pub fn get_jackpot_balance(env: Env, table_id: u32) -> Result<i128, PokerTableError> {
        let table = load_table(&env, table_id)?;
        Ok(table.jackpot_balance)
    }

    /// Read the jackpot configuration parameters for a table (view function).
    pub fn get_jackpot_config(
        env: Env,
        table_id: u32,
    ) -> Result<(u32, u32, u32), PokerTableError> {
        let table = load_table(&env, table_id)?;
        Ok((
            table.config.jackpot_rake_share_bps,
            table.config.min_bad_beat_category,
            table.config.min_bad_beat_rank,
        ))
    }

    /// Update the jackpot configuration (admin only).
    ///
    /// `jackpot_rake_share_bps` is the share of each hand's rake, in basis
    /// points, that feeds the jackpot pool (`0` disables the jackpot).
    /// `min_category` and `min_rank` define the qualifying hand threshold
    /// (defaults: category 7 = FourOfAKind, rank 12 = Ace).
    pub fn set_jackpot_config(
        env: Env,
        table_id: u32,
        jackpot_rake_share_bps: u32,
        min_category: u32,
        min_rank: u32,
    ) -> Result<(), PokerTableError> {
        let mut table = load_table(&env, table_id)?;
        table.admin.require_auth();
        table.config.jackpot_rake_share_bps = jackpot_rake_share_bps;
        table.config.min_bad_beat_category = min_category;
        table.config.min_bad_beat_rank = min_rank;
        save_table(&env, &table);

        env.events().publish(
            (Symbol::new(&env, "jackpot_config_updated"), table_id),
            (jackpot_rake_share_bps, min_category, min_rank),
        );
        Ok(())
    }

    /// Withdraw the accumulated rake to the table admin. Returns the amount
    /// withdrawn.
    pub fn withdraw_rake(env: Env, table_id: u32) -> Result<i128, PokerTableError> {
        let mut table = load_table(&env, table_id)?;
        table.admin.require_auth();

        let amount = table.rake_balance;
        if amount > 0 {
            let token = token::Client::new(&env, &table.config.token);
            token.transfer(&env.current_contract_address(), &table.admin, &amount);
            table.rake_balance = 0;
            save_table(&env, &table);
        }

        env.events().publish(
            (Symbol::new(&env, "rake_withdrawn"), table_id),
            (table.admin.clone(), amount),
        );
        Ok(amount)
    }
}
