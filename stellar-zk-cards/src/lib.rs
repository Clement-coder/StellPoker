//! # stellar-zk-cards
//!
//! Reusable ZK card-game primitives for [Stellar Soroban](https://soroban.stellar.org) smart contracts.
//!
//! This crate provides:
//!
//! - A compact 6-bit card encoding (`0`–`51`) compatible with BN254 field elements
//!   used in Noir ZK circuits.
//! - Best-of-seven hand evaluation for Texas Hold'em (2 hole cards + 5 board cards).
//! - [`HandCategory`] and [`HandRank`] types that are directly comparable on-chain.
//!
//! ## Card encoding
//!
//! Cards are encoded as `suit * 13 + rank`:
//!
//! | Suit | Value |
//! |------|-------|
//! | Clubs    | 0 |
//! | Diamonds | 1 |
//! | Hearts   | 2 |
//! | Spades   | 3 |
//!
//! | Rank | Value |
//! |------|-------|
//! | 2–10 | 0–8   |
//! | J    | 9     |
//! | Q    | 10    |
//! | K    | 11    |
//! | A    | 12    |
//!
//! ## Usage (Soroban contract)
//!
//! ```rust,ignore
//! use stellar_zk_cards::{Card, evaluate_hand, HandCategory};
//!
//! // Build hole cards and board
//! let hole1 = Card::new(3, 12); // Ace of Spades
//! let hole2 = Card::new(2, 12); // Ace of Hearts
//! let board = [
//!     Card::new(0, 12), // Ace of Clubs
//!     Card::new(1, 12), // Ace of Diamonds
//!     Card::new(0, 11), // King of Clubs
//!     Card::new(1, 11), // King of Diamonds
//!     Card::new(2, 11), // King of Hearts
//! ];
//!
//! let all_seven = [
//!     hole1.value, hole2.value,
//!     board[0].value, board[1].value, board[2].value,
//!     board[3].value, board[4].value,
//! ];
//!
//! let rank = evaluate_hand(&all_seven);
//! assert_eq!(rank.category(), HandCategory::FourOfAKind as u32);
//! ```

#![no_std]

use soroban_sdk::contracttype;

/// Total number of cards in a standard deck.
pub const DECK_SIZE: u32 = 52;

/// Number of suits (Clubs, Diamonds, Hearts, Spades).
pub const NUM_SUITS: u32 = 4;

/// Number of ranks per suit (2 through Ace).
pub const NUM_RANKS: u32 = 13;

/// A single playing card encoded as `suit * 13 + rank` in the range `0..=51`.
///
/// This compact encoding fits in 6 bits and is compatible with BN254 field
/// elements used by the Noir ZK circuits in the Stellar Poker MPC committee.
///
/// # Suit encoding
/// - `0` = Clubs
/// - `1` = Diamonds
/// - `2` = Hearts
/// - `3` = Spades
///
/// # Rank encoding
/// - `0`–`8` = 2–10
/// - `9` = Jack, `10` = Queen, `11` = King, `12` = Ace
///
/// # Examples
///
/// ```rust,ignore
/// use stellar_zk_cards::Card;
///
/// let ace_of_spades = Card::new(3, 12);
/// assert_eq!(ace_of_spades.value, 51);
/// assert_eq!(ace_of_spades.suit(), 3);
/// assert_eq!(ace_of_spades.rank(), 12);
/// ```
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Card {
    /// Raw card value in the range `0..=51` (`suit * 13 + rank`).
    pub value: u32,
}

impl Card {
    /// Construct a [`Card`] from a suit (`0`–`3`) and rank (`0`–`12`).
    ///
    /// # Panics
    ///
    /// Panics if `suit >= 4` or `rank >= 13`.
    pub fn new(suit: u32, rank: u32) -> Self {
        assert!(suit < NUM_SUITS, "invalid suit");
        assert!(rank < NUM_RANKS, "invalid rank");
        Card {
            value: suit * NUM_RANKS + rank,
        }
    }

    /// Returns the suit of this card (`0` = Clubs … `3` = Spades).
    pub fn suit(&self) -> u32 {
        self.value / NUM_RANKS
    }

    /// Returns the rank of this card (`0` = 2 … `12` = Ace).
    pub fn rank(&self) -> u32 {
        self.value % NUM_RANKS
    }

    /// Returns `true` if the card value is within the valid range `0..=51`.
    pub fn is_valid(&self) -> bool {
        self.value < DECK_SIZE
    }
}

/// Hand ranking categories for Texas Hold'em, ordered from worst to best.
///
/// The numeric `repr` values match the tiebreaker encoding used in [`HandRank`]:
/// higher is always better, so a simple integer comparison determines the winner.
///
/// | Category       | Example           |
/// |----------------|-------------------|
/// | `HighCard`     | A K Q J 9         |
/// | `OnePair`      | A A K Q J         |
/// | `TwoPair`      | A A K K Q         |
/// | `ThreeOfAKind` | A A A K Q         |
/// | `Straight`     | A K Q J T         |
/// | `Flush`        | A K Q J 9 (same suit) |
/// | `FullHouse`    | A A A K K         |
/// | `FourOfAKind`  | A A A A K         |
/// | `StraightFlush`| 9 8 7 6 5 (same suit) |
/// | `RoyalFlush`   | A K Q J T (same suit) |
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum HandCategory {
    /// No pair; winner determined by highest card.
    HighCard = 0,
    /// Exactly two cards of the same rank.
    OnePair = 1,
    /// Two distinct pairs.
    TwoPair = 2,
    /// Three cards of the same rank.
    ThreeOfAKind = 3,
    /// Five consecutive ranks (ace-low or ace-high).
    Straight = 4,
    /// Five cards of the same suit.
    Flush = 5,
    /// Three of a kind plus a pair.
    FullHouse = 6,
    /// Four cards of the same rank.
    FourOfAKind = 7,
    /// Five consecutive ranks of the same suit.
    StraightFlush = 8,
    /// A-K-Q-J-T of the same suit.
    RoyalFlush = 9,
}

/// A comparable hand strength value produced by [`evaluate_hand`].
///
/// Internally encoded as:
/// ```text
/// score = category (top 4 bits) | tiebreaker (bottom 28 bits)
/// ```
///
/// This means any two [`HandRank`] values from valid 5-card hands can be
/// compared with a single integer comparison — the higher score wins.
///
/// Use [`HandRank::beats`] for a readable comparison, or compare
/// `score` fields directly in Soroban contract logic.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandRank {
    /// Packed score: `(category << 28) | tiebreaker`.
    pub score: u32,
}

impl HandRank {
    /// Construct a [`HandRank`] from a category index (`0`–`9`) and a
    /// tiebreaker value (at most 28 bits).
    pub fn new(category: u32, tiebreaker: u32) -> Self {
        HandRank {
            score: (category << 28) | (tiebreaker & 0x0FFF_FFFF),
        }
    }

    /// Extract the [`HandCategory`] index from this rank (`0`–`9`).
    ///
    /// Compare against [`HandCategory`] variant values to identify the hand.
    pub fn category(&self) -> u32 {
        self.score >> 28
    }

    /// Returns `true` if this hand is strictly stronger than `other`.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use stellar_zk_cards::evaluate_hand;
    ///
    /// let royal  = evaluate_hand(&[8, 9, 10, 11, 12, 13, 14]);
    /// let sf     = evaluate_hand(&[3, 4,  5,  6,  7, 13, 14]);
    /// assert!(royal.beats(&sf));
    /// ```
    pub fn beats(&self, other: &HandRank) -> bool {
        self.score > other.score
    }
}

/// Evaluate the best 5-card hand from a 7-card array (2 hole + 5 board).
///
/// Iterates all C(7,5) = 21 five-card combinations and returns the highest
/// [`HandRank`]. The result is deterministic and requires no heap allocation,
/// making it suitable for on-chain use inside Soroban contracts.
///
/// # Arguments
///
/// * `cards` — seven card values in the range `0..=51` in any order.
///
/// # Returns
///
/// The [`HandRank`] of the best possible 5-card hand. Higher `score` = stronger hand.
///
/// # Examples
///
/// ```rust,ignore
/// use stellar_zk_cards::{evaluate_hand, HandCategory};
///
/// // Pocket aces + board gives four aces
/// let rank = evaluate_hand(&[12, 25, 0, 13, 26, 39, 11]);
/// assert_eq!(rank.category(), HandCategory::FourOfAKind as u32);
/// ```
pub fn evaluate_hand(cards: &[u32; 7]) -> HandRank {
    let mut best_score: u32 = 0;

    // Check all C(7,5) = 21 combinations
    for i in 0..7 {
        for j in (i + 1)..7 {
            // Skip cards at indices i and j (use the other 5)
            let mut hand = [0u32; 5];
            let mut idx = 0;
            for (k, card) in cards.iter().enumerate().take(7) {
                if k != i && k != j {
                    hand[idx] = *card;
                    idx += 1;
                }
            }
            let rank = evaluate_five(&hand);
            if rank.score > best_score {
                best_score = rank.score;
            }
        }
    }

    HandRank { score: best_score }
}

/// Evaluate exactly 5 cards and return their [`HandRank`].
fn evaluate_five(cards: &[u32; 5]) -> HandRank {
    let mut ranks = [0u32; 5];
    let mut suits = [0u32; 5];
    for i in 0..5 {
        ranks[i] = cards[i] % NUM_RANKS;
        suits[i] = cards[i] / NUM_RANKS;
    }

    // Sort ranks descending
    sort_desc(&mut ranks);

    let is_flush = suits[0] == suits[1]
        && suits[1] == suits[2]
        && suits[2] == suits[3]
        && suits[3] == suits[4];

    let is_straight = is_straight_hand(&ranks);

    // Also check A-2-3-4-5 (wheel)
    let is_wheel =
        ranks[0] == 12 && ranks[1] == 3 && ranks[2] == 2 && ranks[3] == 1 && ranks[4] == 0;

    // Count rank frequencies
    let mut freq = [0u32; NUM_RANKS as usize];
    for &r in ranks.iter() {
        freq[r as usize] += 1;
    }

    // Find groups
    let mut quads = 0u32;
    let mut trips = 0u32;
    let mut pairs = 0u32;
    let mut quad_rank = 0u32;
    let mut trip_rank = 0u32;
    let mut pair_ranks = [0u32; 2];

    for r in (0..NUM_RANKS).rev() {
        match freq[r as usize] {
            4 => {
                quads += 1;
                quad_rank = r;
            }
            3 => {
                trips += 1;
                trip_rank = r;
            }
            2 => {
                if pairs < 2 {
                    pair_ranks[pairs as usize] = r;
                }
                pairs += 1;
            }
            _ => {}
        }
    }

    if is_flush && is_straight {
        if ranks[0] == 12 && ranks[1] == 11 {
            // Royal flush (A-K-Q-J-10)
            return HandRank::new(9, ranks[0]);
        }
        return HandRank::new(8, if is_wheel { 3 } else { ranks[0] });
    }

    if is_flush && is_wheel {
        return HandRank::new(8, 3); // Straight flush, 5-high
    }

    if quads == 1 {
        let kicker = ranks
            .iter()
            .find(|&&r| r != quad_rank)
            .copied()
            .unwrap_or(0);
        return HandRank::new(7, (quad_rank << 4) | kicker);
    }

    if trips == 1 && pairs >= 1 {
        return HandRank::new(6, (trip_rank << 4) | pair_ranks[0]);
    }

    if is_flush {
        let tb = (ranks[0] << 16) | (ranks[1] << 12) | (ranks[2] << 8) | (ranks[3] << 4) | ranks[4];
        return HandRank::new(5, tb);
    }

    if is_straight || is_wheel {
        return HandRank::new(4, if is_wheel { 3 } else { ranks[0] });
    }

    if trips == 1 {
        let mut kickers = [0u32; 2];
        let mut ki = 0;
        for &r in ranks.iter() {
            if r != trip_rank && ki < 2 {
                kickers[ki] = r;
                ki += 1;
            }
        }
        return HandRank::new(3, (trip_rank << 8) | (kickers[0] << 4) | kickers[1]);
    }

    if pairs == 2 {
        let high_pair = if pair_ranks[0] > pair_ranks[1] {
            pair_ranks[0]
        } else {
            pair_ranks[1]
        };
        let low_pair = if pair_ranks[0] > pair_ranks[1] {
            pair_ranks[1]
        } else {
            pair_ranks[0]
        };
        let kicker = ranks
            .iter()
            .find(|&&r| r != high_pair && r != low_pair)
            .copied()
            .unwrap_or(0);
        return HandRank::new(2, (high_pair << 8) | (low_pair << 4) | kicker);
    }

    if pairs == 1 {
        let pr = pair_ranks[0];
        let mut kickers = [0u32; 3];
        let mut ki = 0;
        for &r in ranks.iter() {
            if r != pr && ki < 3 {
                kickers[ki] = r;
                ki += 1;
            }
        }
        return HandRank::new(
            1,
            (pr << 12) | (kickers[0] << 8) | (kickers[1] << 4) | kickers[2],
        );
    }

    // High card
    let tb = (ranks[0] << 16) | (ranks[1] << 12) | (ranks[2] << 8) | (ranks[3] << 4) | ranks[4];
    HandRank::new(0, tb)
}

fn is_straight_hand(sorted_ranks: &[u32; 5]) -> bool {
    sorted_ranks[0] == sorted_ranks[1] + 1
        && sorted_ranks[1] == sorted_ranks[2] + 1
        && sorted_ranks[2] == sorted_ranks[3] + 1
        && sorted_ranks[3] == sorted_ranks[4] + 1
}

fn sort_desc(arr: &mut [u32; 5]) {
    // Simple insertion sort for 5 elements
    for i in 1..5 {
        let mut j = i;
        while j > 0 && arr[j] > arr[j - 1] {
            arr.swap(j, j - 1);
            j -= 1;
        }
    }
}

#[cfg(test)]
mod fuzz_hand_eval;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_card_encoding() {
        let card = Card::new(0, 0); // 2 of clubs
        assert_eq!(card.value, 0);
        assert_eq!(card.suit(), 0);
        assert_eq!(card.rank(), 0);

        let card = Card::new(3, 12); // Ace of spades
        assert_eq!(card.value, 51);
        assert_eq!(card.suit(), 3);
        assert_eq!(card.rank(), 12);
    }

    #[test]
    fn test_royal_flush_beats_straight_flush() {
        // Royal flush: 10♣ J♣ Q♣ K♣ A♣ + 2♦ 3♦
        let royal = evaluate_hand(&[8, 9, 10, 11, 12, 13, 14]);
        // Straight flush: 5♣ 6♣ 7♣ 8♣ 9♣ + 2♦ 3♦
        let sf = evaluate_hand(&[3, 4, 5, 6, 7, 13, 14]);
        assert!(royal.beats(&sf));
    }

    #[test]
    fn test_four_of_a_kind_beats_full_house() {
        // Four 2s: 2♣ 2♦ 2♥ 2♠ + K♣ Q♣ J♣
        let quads = evaluate_hand(&[0, 13, 26, 39, 11, 10, 9]);
        // Full house: 3♣ 3♦ 3♥ + K♣ K♦ + Q♣ J♣
        let fh = evaluate_hand(&[1, 14, 27, 11, 24, 10, 9]);
        assert!(quads.beats(&fh));
    }

    #[test]
    fn test_flush_beats_straight() {
        // Flush: 2♣ 4♣ 6♣ 8♣ K♣ + 2♦ 3♦
        let flush = evaluate_hand(&[0, 2, 4, 6, 11, 13, 14]);
        // Straight: 5♣ 6♦ 7♥ 8♠ 9♣ + 2♦ 3♦
        let straight = evaluate_hand(&[3, 17, 31, 45, 7, 13, 14]);
        assert!(flush.beats(&straight));
    }

    #[test]
    fn test_pair_beats_high_card() {
        // Pair: 2♣ 2♦ + 5♣ 7♣ 9♣ K♣ A♣
        let pair = evaluate_hand(&[0, 13, 3, 5, 7, 11, 12]);
        // High card: A♣ K♦ Q♥ J♠ 9♣ + 2♦ 3♦
        let high = evaluate_hand(&[12, 24, 36, 48, 7, 13, 14]);
        assert!(pair.beats(&high));
    }

    #[test]
    fn test_wheel_straight() {
        // A-2-3-4-5 (wheel): A♣ 2♦ 3♥ 4♠ 5♣ + K♦ Q♦
        let wheel = evaluate_hand(&[12, 13, 27, 41, 3, 24, 23]);
        assert_eq!(wheel.category(), 4); // Straight
    }
}
