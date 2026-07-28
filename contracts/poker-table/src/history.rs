use soroban_sdk::{Env, Symbol, Vec};

use crate::constant_time;
use crate::types::*;

/// Completed hands retained per table. The buffer is circular — once it is
/// full, archiving a new hand overwrites the oldest record. Keeping the window
/// small bounds both storage rent and the cost of a full history read.
pub const HAND_HISTORY_CAPACITY: u32 = 16;

/// Upper bound on the actions summarised for a single hand. A hand that runs
/// longer than this is truncated: the record stays a *summary*, so the write
/// cost of settling a hand never grows without bound.
pub const MAX_ACTIONS_PER_HAND: u32 = 64;

/// TTL for archived hand records — matched to the table's own TTL so history
/// stays readable for as long as the table itself does.
const HISTORY_TTL_THRESHOLD: u32 = 17_280; // ~1 day
const HISTORY_TTL_EXTEND: u32 = 518_400; // ~30 days

/// Append a betting action to the current hand's summary. Silently stops
/// recording past `MAX_ACTIONS_PER_HAND` so a pathologically long hand cannot
/// inflate the settlement write.
pub fn record_action(env: &Env, table: &mut TableState, seat: u32, action: &Action, amount: i128) {
    if table.hand_actions.len() >= MAX_ACTIONS_PER_HAND {
        return;
    }
    let _ = env;
    let kind = match action {
        Action::Fold => ActionKind::Fold,
        Action::Check => ActionKind::Check,
        Action::Call => ActionKind::Call,
        Action::Bet(_) => ActionKind::Bet,
        Action::Raise(_) => ActionKind::Raise,
        Action::AllIn => ActionKind::AllIn,
    };
    table.hand_actions.push_back(ActionRecord {
        seat,
        phase: table.phase.clone(),
        kind,
        amount,
    });
}

/// Clear the action summary at the start of a new hand.
pub fn reset_actions(env: &Env, table: &mut TableState) {
    table.hand_actions = Vec::new(env);
}

/// Archive a settled hand into the table's circular buffer.
///
/// `payouts` are the `(seat, amount)` credits produced by pot distribution;
/// they are resolved to addresses here so a history reader never has to
/// cross-reference the live table (whose seating may have changed since).
pub fn archive_hand(
    env: &Env,
    table: &TableState,
    payouts: &Vec<(u32, i128)>,
    total_pot: i128,
    rake: i128,
    showdown: bool,
) -> Result<(), PokerTableError> {
    let mut players: Vec<soroban_sdk::Address> = Vec::new(env);
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        players.push_back(p.address);
    }

    let mut resolved: Vec<Payout> = Vec::new(env);
    for i in 0..payouts.len() {
        let (seat, amount) = payouts
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        let player = table
            .players
            .get(seat)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        resolved.push_back(Payout {
            seat,
            address: player.address,
            amount,
        });
    }

    let record = HandRecord {
        hand_number: table.hand_number,
        players,
        board: table.board_cards.clone(),
        actions: table.hand_actions.clone(),
        payouts: resolved,
        total_pot,
        rake,
        showdown,
        settled_ledger: env.ledger().sequence(),
    };

    let mut meta = load_meta(env, table.id);
    let slot = meta.next_slot;
    let key = DataKey::HandRecord(table.id, slot);
    env.storage().persistent().set(&key, &record);
    env.storage()
        .persistent()
        .extend_ttl(&key, HISTORY_TTL_THRESHOLD, HISTORY_TTL_EXTEND);

    meta.next_slot = (slot + 1) % HAND_HISTORY_CAPACITY;
    if meta.stored < HAND_HISTORY_CAPACITY {
        meta.stored += 1;
    }
    meta.total_archived = meta.total_archived.saturating_add(1);
    save_meta(env, table.id, &meta);

    env.events().publish(
        (Symbol::new(env, "hand_archived"), table.id),
        (record.hand_number, slot, total_pot),
    );
    Ok(())
}

/// Read up to `limit` archived hands, newest first. A `limit` of 0 (or one
/// larger than the buffer) returns every stored record.
pub fn get_history(env: &Env, table_id: u32, limit: u32) -> Vec<HandRecord> {
    let meta = load_meta(env, table_id);
    let mut out: Vec<HandRecord> = Vec::new(env);
    if meta.stored == 0 {
        return out;
    }
    let take = if limit == 0 || limit > meta.stored {
        meta.stored
    } else {
        limit
    };

    // Walk backwards from the most recently written slot.
    let mut slot = (meta.next_slot + HAND_HISTORY_CAPACITY - 1) % HAND_HISTORY_CAPACITY;
    for _ in 0..take {
        if let Some(record) = load_record(env, table_id, slot) {
            out.push_back(record);
        }
        slot = (slot + HAND_HISTORY_CAPACITY - 1) % HAND_HISTORY_CAPACITY;
    }
    out
}

/// Look up a single archived hand by its hand number, if still in the window.
pub fn get_hand(env: &Env, table_id: u32, hand_number: u32) -> Option<HandRecord> {
    let meta = load_meta(env, table_id);
    for slot in 0..meta.stored {
        if let Some(record) = load_record(env, table_id, slot) {
            if constant_time::u32_eq(record.hand_number, hand_number) {
                return Some(record);
            }
        }
    }
    None
}

pub fn load_meta(env: &Env, table_id: u32) -> HandHistoryMeta {
    env.storage()
        .persistent()
        .get(&DataKey::HandHistoryMeta(table_id))
        .unwrap_or(HandHistoryMeta {
            next_slot: 0,
            stored: 0,
            total_archived: 0,
        })
}

fn save_meta(env: &Env, table_id: u32, meta: &HandHistoryMeta) {
    let key = DataKey::HandHistoryMeta(table_id);
    env.storage().persistent().set(&key, meta);
    env.storage()
        .persistent()
        .extend_ttl(&key, HISTORY_TTL_THRESHOLD, HISTORY_TTL_EXTEND);
}

fn load_record(env: &Env, table_id: u32, slot: u32) -> Option<HandRecord> {
    let key = DataKey::HandRecord(table_id, slot);
    let record: Option<HandRecord> = env.storage().persistent().get(&key);
    if record.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, HISTORY_TTL_THRESHOLD, HISTORY_TTL_EXTEND);
    }
    record
}
