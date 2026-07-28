//! Sit-and-go tournament manager (off-chain coordinator extension).
//!
//! A tournament is a fixed buy-in, single-elimination event:
//!   1. Players register (tokens escrowed in per-table PokerTable contracts).
//!   2. The coordinator seats players across one or more tables.
//!   3. Blinds escalate on a schedule tracked here; the coordinator calls
//!      `set_blinds` on each PokerTable between hands.
//!   4. Eliminated players (stack == 0 after settlement) are recorded in
//!      finish order. Their escrowed tokens remain in the PokerTable contract
//!      until `finalize_tournament` distributes payouts via `leave_table`.
//!   5. When one player remains, the tournament is finalised and prizes are
//!      paid out according to the configured payout schedule.
//!
//! # Design
//!
//! Tournament state lives entirely in the coordinator. Each table is a
//! regular `PokerTable` Soroban contract. The only new on-chain footprint is
//! the existing escrow in those contracts — no new Soroban contract is needed.
//!
//! Table balancing uses a simple max-diff algorithm: after each elimination
//! the coordinator moves a player from the largest table to the smallest when
//! the seat counts differ by more than 1.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use utoipa::ToSchema;
use uuid::Uuid;

// ── Blind schedule ────────────────────────────────────────────────────────────

/// One level of the blind schedule.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BlindLevel {
    /// Small blind in stroops (1 XLM = 10_000_000 stroops).
    pub small_blind: i128,
    /// Big blind in stroops.
    pub big_blind: i128,
    /// How many hands are played at this level before escalating.
    pub hands: u32,
}

/// Returns the default 9-level blind schedule for a micro sit-and-go.
pub fn default_blind_schedule() -> Vec<BlindLevel> {
    vec![
        BlindLevel { small_blind: 500_000,   big_blind: 1_000_000,   hands: 6  },
        BlindLevel { small_blind: 1_000_000,  big_blind: 2_000_000,   hands: 6  },
        BlindLevel { small_blind: 1_500_000,  big_blind: 3_000_000,   hands: 5  },
        BlindLevel { small_blind: 2_500_000,  big_blind: 5_000_000,   hands: 5  },
        BlindLevel { small_blind: 5_000_000,  big_blind: 10_000_000,  hands: 4  },
        BlindLevel { small_blind: 10_000_000, big_blind: 20_000_000,  hands: 4  },
        BlindLevel { small_blind: 20_000_000, big_blind: 40_000_000,  hands: 3  },
        BlindLevel { small_blind: 40_000_000, big_blind: 80_000_000,  hands: 3  },
        BlindLevel { small_blind: 80_000_000, big_blind: 160_000_000, hands: 99 }, // final level
    ]
}

// ── Prize schedule ────────────────────────────────────────────────────────────

/// Percentage of the prize pool awarded to each finishing position (1-indexed).
/// Must sum to 100. Positions beyond the slice length receive 0.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PayoutSchedule {
    /// `shares[0]` = 1st place %, `shares[1]` = 2nd place %, etc.
    pub shares: Vec<u32>,
}

impl PayoutSchedule {
    /// Standard top-3 payout: 50 / 30 / 20.
    pub fn top_three() -> Self {
        Self { shares: vec![50, 30, 20] }
    }

    /// Winner-takes-all.
    pub fn winner_takes_all() -> Self {
        Self { shares: vec![100] }
    }

    /// Compute payout amounts from a prize pool in stroops.
    /// Returns a vec of `(finish_position_1indexed, amount_stroops)`.
    pub fn compute(&self, prize_pool: i128) -> Vec<(u32, i128)> {
        let mut out = Vec::new();
        let mut distributed: i128 = 0;
        for (i, &pct) in self.shares.iter().enumerate() {
            let place = (i + 1) as u32;
            let amount = if i + 1 == self.shares.len() {
                // Give remainder to last paid place to avoid rounding loss.
                prize_pool - distributed
            } else {
                prize_pool * pct as i128 / 100
            };
            distributed += amount;
            out.push((place, amount));
        }
        out
    }
}

// ── Tournament state machine ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TournamentStatus {
    /// Accepting registrations, not yet started.
    Registration,
    /// Tables are running, eliminations in progress.
    Running,
    /// One player remains; payouts computed, awaiting on-chain settlement.
    Finalizing,
    /// All payouts distributed, tournament over.
    Completed,
    /// Tournament was cancelled (e.g. not enough registrations).
    Cancelled,
}

/// A single seated player within the tournament.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TournamentPlayer {
    pub address: String,
    /// Which PokerTable contract address this player is currently seated at.
    pub table_contract: String,
    /// Current chip stack (updated after each hand settlement).
    pub stack: i128,
    /// Finish position (1 = winner). None while still playing.
    pub finish_position: Option<u32>,
    /// Payout amount in stroops. None until finalised.
    pub payout: Option<i128>,
}

/// Tracks hands played at the current blind level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindState {
    pub level_index: usize,
    pub hands_at_level: u32,
}

impl BlindState {
    fn new() -> Self {
        Self { level_index: 0, hands_at_level: 0 }
    }

    /// Record a completed hand and advance the level if due.
    /// Returns true if the level changed.
    pub fn record_hand(&mut self, schedule: &[BlindLevel]) -> bool {
        self.hands_at_level += 1;
        let current = &schedule[self.level_index];
        if self.hands_at_level >= current.hands
            && self.level_index + 1 < schedule.len()
        {
            self.level_index += 1;
            self.hands_at_level = 0;
            return true;
        }
        false
    }

    pub fn current<'a>(&self, schedule: &'a [BlindLevel]) -> &'a BlindLevel {
        &schedule[self.level_index]
    }
}

/// Full tournament record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tournament {
    pub id: String,
    pub name: String,
    /// Buy-in per player in stroops (escrowed in their PokerTable contract).
    pub buy_in: i128,
    /// Max seats across all tables (= max registrations).
    pub max_players: u32,
    /// Min registrations needed to start.
    pub min_players: u32,
    /// Players seated per table (2–6).
    pub players_per_table: u32,
    /// PokerTable contract addresses, one per active table.
    pub table_contracts: Vec<String>,
    /// All registered players.
    pub players: HashMap<String, TournamentPlayer>,
    /// Elimination order: address of each busted player, earliest first.
    pub eliminations: Vec<String>,
    pub status: TournamentStatus,
    pub blind_state: BlindState,
    pub blind_schedule: Vec<BlindLevel>,
    pub payout_schedule: PayoutSchedule,
    /// Total prize pool = buy_in × registered players. Rake not deducted here
    /// (table rake is already taken by the PokerTable contract per hand).
    pub prize_pool: i128,
    pub created_at: std::time::SystemTime,
    pub started_at: Option<std::time::SystemTime>,
    pub finished_at: Option<std::time::SystemTime>,
}

impl Tournament {
    pub fn new(req: CreateTournamentRequest) -> Self {
        let id = Uuid::new_v4().to_string();
        let blind_schedule = req.blind_schedule.unwrap_or_else(default_blind_schedule);
        let payout_schedule = req.payout_schedule.unwrap_or_else(PayoutSchedule::top_three);
        Tournament {
            id,
            name: req.name,
            buy_in: req.buy_in,
            max_players: req.max_players,
            min_players: req.min_players.unwrap_or(2),
            players_per_table: req.players_per_table.unwrap_or(6).min(6).max(2),
            table_contracts: Vec::new(),
            players: HashMap::new(),
            eliminations: Vec::new(),
            status: TournamentStatus::Registration,
            blind_state: BlindState::new(),
            blind_schedule,
            payout_schedule,
            prize_pool: 0,
            created_at: std::time::SystemTime::now(),
            started_at: None,
            finished_at: None,
        }
    }

    /// Register a player. Returns Err if registration is closed or full.
    pub fn register(&mut self, address: String, table_contract: String) -> Result<(), TournamentError> {
        if self.status != TournamentStatus::Registration {
            return Err(TournamentError::RegistrationClosed);
        }
        if self.players.len() as u32 >= self.max_players {
            return Err(TournamentError::TournamentFull);
        }
        if self.players.contains_key(&address) {
            return Err(TournamentError::AlreadyRegistered);
        }
        self.players.insert(address.clone(), TournamentPlayer {
            address,
            table_contract,
            stack: self.buy_in,
            finish_position: None,
            payout: None,
        });
        self.prize_pool += self.buy_in;
        Ok(())
    }

    /// Start the tournament. Assigns tables; returns Err if not enough players.
    pub fn start(&mut self) -> Result<(), TournamentError> {
        if self.status != TournamentStatus::Registration {
            return Err(TournamentError::InvalidState);
        }
        if (self.players.len() as u32) < self.min_players {
            return Err(TournamentError::NotEnoughPlayers);
        }
        self.status = TournamentStatus::Running;
        self.started_at = Some(std::time::SystemTime::now());
        Ok(())
    }

    /// Record a completed hand's stack changes and handle eliminations.
    ///
    /// `stack_updates` maps player address → new stack after settlement.
    /// Returns the list of newly eliminated player addresses.
    pub fn record_hand_result(
        &mut self,
        stack_updates: HashMap<String, i128>,
    ) -> Vec<String> {
        let mut newly_eliminated = Vec::new();

        for (addr, new_stack) in &stack_updates {
            if let Some(player) = self.players.get_mut(addr) {
                player.stack = *new_stack;
            }
        }

        // Advance blind schedule.
        self.blind_state.record_hand(&self.blind_schedule);

        // Detect eliminations (stack == 0, not yet assigned a finish position).
        let active_count = self.active_players().len() as u32;
        let mut next_finish = active_count + self.eliminations.len() as u32 + 1;

        for (addr, player) in self.players.iter_mut() {
            if player.stack == 0 && player.finish_position.is_none() {
                player.finish_position = Some(next_finish);
                next_finish -= 1; // earlier eliminations finish lower
                newly_eliminated.push(addr.clone());
            }
        }

        for addr in &newly_eliminated {
            self.eliminations.push(addr.clone());
        }

        // Check if the tournament is over.
        let remaining = self.active_players();
        if remaining.len() == 1 {
            let winner = remaining[0].clone();
            if let Some(p) = self.players.get_mut(&winner) {
                p.finish_position = Some(1);
            }
            self.status = TournamentStatus::Finalizing;
            self.finished_at = Some(std::time::SystemTime::now());
            self.distribute_prizes();
        }

        newly_eliminated
    }

    /// Distribute prizes according to the payout schedule.
    fn distribute_prizes(&mut self) {
        let payouts = self.payout_schedule.compute(self.prize_pool);

        // Build finish-position → payout map.
        let payout_map: HashMap<u32, i128> = payouts.into_iter().collect();

        for player in self.players.values_mut() {
            if let Some(pos) = player.finish_position {
                player.payout = Some(*payout_map.get(&pos).unwrap_or(&0));
            }
        }
    }

    /// Returns addresses of players still in the tournament (stack > 0).
    pub fn active_players(&self) -> Vec<String> {
        self.players
            .values()
            .filter(|p| p.stack > 0 && p.finish_position.is_none())
            .map(|p| p.address.clone())
            .collect()
    }

    /// Table balancing: returns (source_player, from_table, to_table) moves
    /// needed to keep table sizes within 1 of each other.
    pub fn balancing_moves(&self) -> Vec<TableMove> {
        if self.table_contracts.len() < 2 {
            return Vec::new();
        }

        // Count active players per table.
        let mut counts: HashMap<String, Vec<String>> = HashMap::new();
        for contract in &self.table_contracts {
            counts.insert(contract.clone(), Vec::new());
        }
        for p in self.players.values() {
            if p.stack > 0 && p.finish_position.is_none() {
                counts
                    .entry(p.table_contract.clone())
                    .or_default()
                    .push(p.address.clone());
            }
        }

        let mut moves = Vec::new();
        loop {
            let max_table = counts
                .iter()
                .max_by_key(|(_, ps)| ps.len())
                .map(|(t, ps)| (t.clone(), ps.len()));
            let min_table = counts
                .iter()
                .min_by_key(|(_, ps)| ps.len())
                .map(|(t, ps)| (t.clone(), ps.len()));

            match (max_table, min_table) {
                (Some((from, from_len)), Some((to, to_len)))
                    if from != to && from_len > to_len + 1 =>
                {
                    // Move the last player from the largest table to the smallest.
                    let player = counts.get_mut(&from).unwrap().pop().unwrap();
                    counts.get_mut(&to).unwrap().push(player.clone());
                    moves.push(TableMove { player, from_table: from, to_table: to });
                }
                _ => break,
            }
        }
        moves
    }

    /// Collapse empty tables: returns contracts that now have 0 active players.
    pub fn empty_tables(&self) -> Vec<String> {
        let mut active_per_table: HashMap<String, u32> = HashMap::new();
        for contract in &self.table_contracts {
            active_per_table.insert(contract.clone(), 0);
        }
        for p in self.players.values() {
            if p.stack > 0 && p.finish_position.is_none() {
                *active_per_table.entry(p.table_contract.clone()).or_default() += 1;
            }
        }
        active_per_table
            .into_iter()
            .filter(|(_, count)| *count == 0)
            .map(|(t, _)| t)
            .collect()
    }

    pub fn current_blind_level(&self) -> &BlindLevel {
        self.blind_state.current(&self.blind_schedule)
    }
}

/// Instruction to move a player from one table to another for balancing.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TableMove {
    pub player: String,
    pub from_table: String,
    pub to_table: String,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TournamentError {
    NotFound,
    RegistrationClosed,
    TournamentFull,
    AlreadyRegistered,
    NotEnoughPlayers,
    InvalidState,
}

impl std::fmt::Display for TournamentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TournamentError::NotFound => write!(f, "tournament not found"),
            TournamentError::RegistrationClosed => write!(f, "registration is closed"),
            TournamentError::TournamentFull => write!(f, "tournament is full"),
            TournamentError::AlreadyRegistered => write!(f, "player already registered"),
            TournamentError::NotEnoughPlayers => write!(f, "not enough players to start"),
            TournamentError::InvalidState => write!(f, "invalid tournament state for this action"),
        }
    }
}

// ── In-memory store ───────────────────────────────────────────────────────────

pub type TournamentStore = Arc<RwLock<HashMap<String, Tournament>>>;

pub fn new_store() -> TournamentStore {
    Arc::new(RwLock::new(HashMap::new()))
}

// ── API request / response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTournamentRequest {
    pub name: String,
    /// Buy-in in stroops.
    pub buy_in: i128,
    pub max_players: u32,
    pub min_players: Option<u32>,
    /// Seats per table (2–6, default 6).
    pub players_per_table: Option<u32>,
    pub blind_schedule: Option<Vec<BlindLevel>>,
    pub payout_schedule: Option<PayoutSchedule>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterPlayerRequest {
    /// Stellar address of the player.
    pub address: String,
    /// PokerTable contract address where the player has escrowed their buy-in.
    pub table_contract: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct HandResultRequest {
    /// Map of player address → stack after settlement.
    pub stacks: HashMap<String, i128>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentSummary {
    pub id: String,
    pub name: String,
    pub buy_in: i128,
    pub max_players: u32,
    pub registered: u32,
    pub status: TournamentStatus,
    pub prize_pool: i128,
    pub current_small_blind: i128,
    pub current_big_blind: i128,
    pub blind_level: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentDetail {
    pub id: String,
    pub name: String,
    pub buy_in: i128,
    pub max_players: u32,
    pub min_players: u32,
    pub players_per_table: u32,
    pub registered: u32,
    pub status: TournamentStatus,
    pub prize_pool: i128,
    pub players: Vec<TournamentPlayer>,
    pub eliminations: Vec<String>,
    pub table_contracts: Vec<String>,
    pub current_small_blind: i128,
    pub current_big_blind: i128,
    pub blind_level: usize,
    pub blind_schedule: Vec<BlindLevel>,
    pub payout_schedule: PayoutSchedule,
}

impl From<&Tournament> for TournamentSummary {
    fn from(t: &Tournament) -> Self {
        let blind = t.current_blind_level();
        TournamentSummary {
            id: t.id.clone(),
            name: t.name.clone(),
            buy_in: t.buy_in,
            max_players: t.max_players,
            registered: t.players.len() as u32,
            status: t.status.clone(),
            prize_pool: t.prize_pool,
            current_small_blind: blind.small_blind,
            current_big_blind: blind.big_blind,
            blind_level: t.blind_state.level_index,
        }
    }
}

impl From<&Tournament> for TournamentDetail {
    fn from(t: &Tournament) -> Self {
        let blind = t.current_blind_level();
        let mut players: Vec<TournamentPlayer> = t.players.values().cloned().collect();
        players.sort_by(|a, b| {
            // Sort by finish position (winners first), then by stack descending.
            match (a.finish_position, b.finish_position) {
                (None, None) => b.stack.cmp(&a.stack),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(pa), Some(pb)) => pa.cmp(&pb),
            }
        });
        TournamentDetail {
            id: t.id.clone(),
            name: t.name.clone(),
            buy_in: t.buy_in,
            max_players: t.max_players,
            min_players: t.min_players,
            players_per_table: t.players_per_table,
            registered: t.players.len() as u32,
            status: t.status.clone(),
            prize_pool: t.prize_pool,
            players,
            eliminations: t.eliminations.clone(),
            table_contracts: t.table_contracts.clone(),
            current_small_blind: blind.small_blind,
            current_big_blind: blind.big_blind,
            blind_level: t.blind_state.level_index,
            blind_schedule: t.blind_schedule.clone(),
            payout_schedule: t.payout_schedule.clone(),
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tournament(max_players: u32) -> Tournament {
        Tournament::new(CreateTournamentRequest {
            name: "Test SNG".to_string(),
            buy_in: 10_000_000,
            max_players,
            min_players: Some(2),
            players_per_table: Some(6),
            blind_schedule: None,
            payout_schedule: Some(PayoutSchedule::top_three()),
        })
    }

    #[test]
    fn test_register_and_prize_pool() {
        let mut t = make_tournament(6);
        t.register("ALICE".to_string(), "TABLE1".to_string()).unwrap();
        t.register("BOB".to_string(), "TABLE1".to_string()).unwrap();
        assert_eq!(t.prize_pool, 20_000_000);
        assert_eq!(t.players.len(), 2);
    }

    #[test]
    fn test_register_full_returns_error() {
        let mut t = make_tournament(2);
        t.register("A".to_string(), "T".to_string()).unwrap();
        t.register("B".to_string(), "T".to_string()).unwrap();
        let err = t.register("C".to_string(), "T".to_string());
        assert!(matches!(err, Err(TournamentError::TournamentFull)));
    }

    #[test]
    fn test_duplicate_register_returns_error() {
        let mut t = make_tournament(6);
        t.register("ALICE".to_string(), "T".to_string()).unwrap();
        let err = t.register("ALICE".to_string(), "T".to_string());
        assert!(matches!(err, Err(TournamentError::AlreadyRegistered)));
    }

    #[test]
    fn test_start_requires_min_players() {
        let mut t = make_tournament(6);
        t.register("A".to_string(), "T".to_string()).unwrap();
        assert!(matches!(t.start(), Err(TournamentError::NotEnoughPlayers)));
        t.register("B".to_string(), "T".to_string()).unwrap();
        assert!(t.start().is_ok());
    }

    #[test]
    fn test_elimination_recorded() {
        let mut t = make_tournament(3);
        t.register("A".to_string(), "T".to_string()).unwrap();
        t.register("B".to_string(), "T".to_string()).unwrap();
        t.register("C".to_string(), "T".to_string()).unwrap();
        t.start().unwrap();

        let mut updates = HashMap::new();
        updates.insert("A".to_string(), 0i128);
        updates.insert("B".to_string(), 15_000_000i128);
        updates.insert("C".to_string(), 15_000_000i128);
        let eliminated = t.record_hand_result(updates);

        assert_eq!(eliminated, vec!["A"]);
        assert_eq!(t.eliminations, vec!["A"]);
        assert_eq!(t.active_players().len(), 2);
    }

    #[test]
    fn test_winner_triggers_finalizing() {
        let mut t = make_tournament(2);
        t.register("A".to_string(), "T".to_string()).unwrap();
        t.register("B".to_string(), "T".to_string()).unwrap();
        t.start().unwrap();

        let mut updates = HashMap::new();
        updates.insert("A".to_string(), 20_000_000i128);
        updates.insert("B".to_string(), 0i128);
        t.record_hand_result(updates);

        assert_eq!(t.status, TournamentStatus::Finalizing);
        assert_eq!(
            t.players["A"].finish_position,
            Some(1)
        );
    }

    #[test]
    fn test_payout_schedule_compute() {
        let sched = PayoutSchedule::top_three();
        let payouts = sched.compute(100_000_000);
        assert_eq!(payouts[0], (1, 50_000_000));
        assert_eq!(payouts[1], (2, 30_000_000));
        // Remainder goes to 3rd
        assert_eq!(payouts[2], (3, 20_000_000));
        let total: i128 = payouts.iter().map(|(_, v)| v).sum();
        assert_eq!(total, 100_000_000);
    }

    #[test]
    fn test_blind_escalation() {
        let schedule = default_blind_schedule();
        let mut state = BlindState::new();
        // Play through all hands in level 0
        let level_0_hands = schedule[0].hands;
        for i in 0..level_0_hands - 1 {
            let changed = state.record_hand(&schedule);
            assert!(!changed, "should not advance on hand {}", i);
        }
        let changed = state.record_hand(&schedule);
        assert!(changed, "should advance after level_0_hands");
        assert_eq!(state.level_index, 1);
    }

    #[test]
    fn test_table_balancing_moves() {
        let mut t = make_tournament(6);
        t.table_contracts = vec!["T1".to_string(), "T2".to_string()];
        // 4 players on T1, 1 on T2
        for i in 0..4u32 {
            t.players.insert(format!("P{}", i), TournamentPlayer {
                address: format!("P{}", i),
                table_contract: "T1".to_string(),
                stack: 10_000_000,
                finish_position: None,
                payout: None,
            });
        }
        t.players.insert("P4".to_string(), TournamentPlayer {
            address: "P4".to_string(),
            table_contract: "T2".to_string(),
            stack: 10_000_000,
            finish_position: None,
            payout: None,
        });

        let moves = t.balancing_moves();
        // Needs at least one move to equalize 4 vs 1 → 3 vs 2 → 2 vs 3... stop at diff<=1
        assert!(!moves.is_empty());
        for m in &moves {
            assert_eq!(m.from_table, "T1");
            assert_eq!(m.to_table, "T2");
        }
    }
}
