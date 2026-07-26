use soroban_sdk::{contracterror, contracttype, Address, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug)]
pub struct TableConfig {
    pub token: Address, // Payment token (e.g., USDC)
    pub min_buy_in: i128,
    pub max_buy_in: i128,
    pub small_blind: i128,
    pub big_blind: i128,
    /// Minimum seated players required to start a hand.
    pub min_players: u32,
    /// Maximum seated players allowed at the table. Capped at 6.
    pub max_players: u32,
    pub timeout_ledgers: u32, // Ledgers before timeout (~5 sec each)
    pub committee: Address,   // MPC committee address
    pub verifier: Address,    // ZK verifier contract address
    pub game_hub: Address,    // Game hub contract for start_game/end_game
    /// Rake taken from every pot, in basis points (100 = 1%). Capped at
    /// `MAX_RAKE_BPS` (500 = 5%); enforced on table creation.
    pub rake_bps: u32,
}

#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PokerTableError {
    TableNotFound = 1,
    TableNotAcceptingPlayers = 2,
    TableFull = 3,
    InvalidBuyIn = 4,
    AlreadySeated = 5,
    PlayerNotAtTable = 6,
    CannotLeaveDuringActiveHand = 7,
    HandAlreadyInProgress = 8,
    NotEnoughPlayers = 9,
    InvalidPlayerIndex = 10,
    NotYourTurn = 11,
    PlayerAlreadyFolded = 12,
    PlayerAlreadyAllIn = 13,
    MustCallOrFold = 14,
    NothingToCall = 15,
    CannotBetWhenOutstandingBet = 16,
    BetTooSmall = 17,
    RaiseTooSmall = 18,
    NotEnoughChips = 19,
    NotInBettingPhase = 20,
    NotInDealingPhase = 21,
    NotInRevealPhase = 22,
    NotInShowdownPhase = 23,
    WrongCommitmentCount = 24,
    WrongCardCount = 25,
    NotAuthorizedCommittee = 26,
    DealProofVerificationFailed = 27,
    RevealProofVerificationFailed = 28,
    ShowdownProofVerificationFailed = 29,
    BoardNotComplete = 30,
    InvalidHoleCards = 31,
    TimeoutNotReached = 32,
    TimeoutNotApplicable = 33,
    HoleCardMismatch = 34,
    WinnerNotEligibleForPot = 35,
    RakeBpsExceedsMax = 36,
    InvalidPlayerCount = 37,
    CannotChangeMinPlayersMidHand = 38,
    ContractPaused = 39,
    ForceFoldNotAvailable = 40,
    TargetNotActive = 41,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PlayerState {
    pub address: Address,
    pub stack: i128,
    pub bet_this_round: i128,
    /// Total chips this player has committed to the pot across every betting
    /// round of the current hand. Used to compute multi-way side pots, since a
    /// player can only win the chips they themselves have contributed to.
    pub committed: i128,
    pub folded: bool,
    pub all_in: bool,
    pub sitting_out: bool,
    pub seat_index: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum GamePhase {
    Waiting,      // Waiting for players
    Dealing,      // Committee is dealing
    Preflop,      // Betting round: preflop
    DealingFlop,  // Committee revealing flop
    Flop,         // Betting round: flop
    DealingTurn,  // Committee revealing turn
    Turn,         // Betting round: turn
    DealingRiver, // Committee revealing river
    River,        // Betting round: river
    Showdown,     // Revealing hands and determining winner
    Settlement,   // Pot distributed, ready for next hand
    Dispute,      // Something went wrong; funds frozen
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum Action {
    Fold,
    Check,
    Call,
    Bet(i128),
    Raise(i128),
    AllIn,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct SidePot {
    pub amount: i128,
    pub eligible_players: Vec<u32>, // seat indices
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct TableState {
    pub id: u32,
    pub admin: Address,
    pub config: TableConfig,
    pub phase: GamePhase,
    pub players: Vec<PlayerState>,
    pub dealer_seat: u32,
    pub current_turn: u32,
    pub pot: i128,
    pub side_pots: Vec<SidePot>,
    pub deck_root: BytesN<32>,
    pub hand_commitments: Vec<BytesN<32>>,
    pub board_cards: Vec<u32>,   // Revealed community cards
    pub dealt_indices: Vec<u32>, // Deck indices already dealt
    pub hand_number: u32,
    pub last_action_ledger: u32, // For timeout calculation
    pub committee: Address,
    pub session_id: u32, // Game hub session ID for current hand
    /// Accumulated rake collected from settled hands, withdrawable by `admin`.
    pub rake_balance: i128,
    /// Ledger sequence by which the current player must act. Any other seated
    /// player may call `force_fold` after this deadline is reached.
    pub action_deadline: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Table(u32),
    Paused(u32), // per-table pause flag
}
