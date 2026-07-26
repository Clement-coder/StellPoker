// Card encoding for Stellar Poker.
// Card value = suit * 13 + rank
// suit: 0=Clubs, 1=Diamonds, 2=Hearts, 3=Spades
// rank: 0=2, 1=3, ..., 8=10, 9=J, 10=Q, 11=K, 12=A

pub global DECK_SIZE: u32 = 52;
pub global NUM_SUITS: u32 = 4;
pub global NUM_RANKS: u32 = 13;

// Validate that a card value is in range [0, 51].
pub fn assert_valid_card(card: Field) {
    let card_u32 = card as u32;
    assert(card_u32 < DECK_SIZE, "invalid card value");
}

// Extract suit from card value.
pub fn suit(card: Field) -> u32 {
    (card as u32) / NUM_RANKS
}

// Extract rank from card value.
pub fn rank(card: Field) -> u32 {
    (card as u32) % NUM_RANKS
}

// Verify that a set of cards are all unique (no duplicates).
pub fn assert_all_unique<let N: u32>(cards: [Field; N]) {
    for i in 0..N {
        for j in (i + 1)..N {
            assert(cards[i] != cards[j], "duplicate card");
        }
    }
}

// Verify that a set of cards forms a valid deck (all unique, all in range).
pub fn assert_valid_deck(deck: [Field; 52]) {
    for i in 0..52 {
        assert_valid_card(deck[i]);
    }
    assert_all_unique(deck);
}

// Hand ranking for showdown verification.
// Returns a score where higher = better hand.
//
// Input: 7 cards (2 hole + 5 board), each as u32 value [0, 51].
// Output: score (u32) where higher = better.
pub fn evaluate_hand_rank(cards: [u32; 7]) -> u32 {
    let mut ranks: [u32; 7] = [0; 7];
    let mut suits: [u32; 7] = [0; 7];
    for i in 0..7 {
        ranks[i] = cards[i] % NUM_RANKS;
        suits[i] = cards[i] / NUM_RANKS;
    }

    let mut best_score: u32 = 0;

    // Check all C(7,5) = 21 combinations by choosing which 2 to skip
    for skip1 in 0..7 {
        for skip2 in (skip1 + 1)..7 {
            let mut hand_ranks: [u32; 5] = [0; 5];
            let mut hand_suits: [u32; 5] = [0; 5];
            let mut idx: u32 = 0;
            for k in 0..7 {
                if (k != skip1) & (k != skip2) {
                    hand_ranks[idx] = ranks[k];
                    hand_suits[idx] = suits[k];
                    idx += 1;
                }
            }
            let score = score_five(hand_ranks, hand_suits);
            if score > best_score {
                best_score = score;
            }
        }
    }

    best_score
}

// Score exactly 5 cards. Returns category << 20 | tiebreaker.
fn score_five(mut ranks: [u32; 5], suits: [u32; 5]) -> u32 {
    // Sort ranks descending (bubble sort for 5 elements)
    for i in 0..4 {
        for j in 0..(4 - i) {
            if ranks[j] < ranks[j + 1] {
                let tmp = ranks[j];
                ranks[j] = ranks[j + 1];
                ranks[j + 1] = tmp;
            }
        }
    }

    let is_flush = (suits[0] == suits[1])
        & (suits[1] == suits[2])
        & (suits[2] == suits[3])
        & (suits[3] == suits[4]);

    let is_straight = (ranks[0] == ranks[1] + 1)
        & (ranks[1] == ranks[2] + 1)
        & (ranks[2] == ranks[3] + 1)
        & (ranks[3] == ranks[4] + 1);

    let is_wheel = (ranks[0] == 12)
        & (ranks[1] == 3)
        & (ranks[2] == 2)
        & (ranks[3] == 1)
        & (ranks[4] == 0);

    let eq0 = ranks[0] == ranks[1];
    let eq1 = ranks[1] == ranks[2];
    let eq2 = ranks[2] == ranks[3];
    let eq3 = ranks[3] == ranks[4];

    let has_four = (eq0 & eq1 & eq2) | (eq1 & eq2 & eq3);
    let four_rank = if eq0 & eq1 & eq2 { ranks[0] } else { ranks[4] };

    let has_three = (eq0 & eq1) | (eq1 & eq2) | (eq2 & eq3);
    let three_rank = if eq0 & eq1 { ranks[0] } else if eq1 & eq2 { ranks[1] } else { ranks[2] };

    let is_full_house = ((eq0 & eq1) & eq3) | ((eq2 & eq3) & eq0);
    let full_house_pair_rank = if eq0 & eq1 { ranks[4] } else { ranks[0] };

    let has_two_pairs = ((eq0 & eq2) | (eq0 & eq3) | (eq1 & eq3)) & !has_four;
    let pair_rank_hi = if eq0 { ranks[0] } else { ranks[1] };
    let pair_rank_lo = if eq3 { ranks[4] } else { ranks[2] };

    let has_pair = (eq0 | eq1 | eq2 | eq3) & !has_three & !has_two_pairs & !has_four;
    let one_pair_rank = if eq0 { ranks[0] } else if eq1 { ranks[1] } else if eq2 { ranks[2] } else { ranks[3] };

    let tb = (ranks[0] << 16) | (ranks[1] << 12) | (ranks[2] << 8) | (ranks[3] << 4) | ranks[4];

    // Use conditional accumulation instead of early returns
    let mut score: u32 = 0;
    let mut categorized = false;

    // Royal/straight flush
    if !categorized & is_flush & is_straight & (ranks[0] == 12) {
        score = (9 << 20) | tb;
        categorized = true;
    }
    if !categorized & is_flush & (is_straight | is_wheel) {
        let high = if is_wheel { 3 << 16 } else { tb };
        score = (8 << 20) | high;
        categorized = true;
    }
    if !categorized & has_four {
        score = (7 << 20) | (four_rank << 16);
        categorized = true;
    }
    if !categorized & is_full_house {
        score = (6 << 20) | (three_rank << 8) | full_house_pair_rank;
        categorized = true;
    }
    if !categorized & is_flush {
        score = (5 << 20) | tb;
        categorized = true;
    }
    if !categorized & (is_straight | is_wheel) {
        let high = if is_wheel { 3 << 16 } else { ranks[0] << 16 };
        score = (4 << 20) | high;
        categorized = true;
    }
    if !categorized & has_three {
        score = (3 << 20) | (three_rank << 16);
        categorized = true;
    }
    if !categorized & has_two_pairs {
        score = (2 << 20) | (pair_rank_hi << 12) | (pair_rank_lo << 8);
        categorized = true;
    }
    if !categorized & has_pair {
        score = (1 << 20) | (one_pair_rank << 16);
        categorized = true;
    }
    if !categorized {
        score = tb;
    }

    score
}

#[test]
fn test_card_encoding() {
    assert(suit(0) == 0);
    assert(rank(0) == 0);
    assert(suit(51) == 3);
    assert(rank(51) == 12);
    assert(suit(37) == 2);
    assert(rank(37) == 11);
}

#[test]
fn test_valid_card() {
    assert_valid_card(0);
    assert_valid_card(51);
    assert_valid_card(25);
}

#[test]
fn test_hand_ranking_pair_beats_high_card() {
    // Pair of aces vs high card king
    let pair_hand = evaluate_hand_rank([12, 25, 0, 1, 3, 5, 7]);
    let high_card = evaluate_hand_rank([11, 23, 0, 1, 3, 5, 7]);
    assert(pair_hand > high_card);
}

#[test]
fn test_flush_beats_straight() {
    // Flush (all hearts): 2♥, 3♥, 5♥, 7♥, 9♥, K♦, Q♠
    let flush = evaluate_hand_rank([13, 26, 32, 45, 48, 9, 22]);
    // Straight (K-Q-J-10-9): K♠, Q♠, J♠, 10♣, 9♣, 2♥, 3♥
    let straight = evaluate_hand_rank([11, 24, 37, 8, 21, 0, 1]);
    assert(flush > straight);
}

#[test]
fn test_straight_beats_three_of_a_kind() {
    // Straight (5-4-3-2-A wheel): A♠, K♦, Q♦, 5♣, 4♣, 3♥, 2♥
    let straight = evaluate_hand_rank([12, 10, 23, 30, 29, 15, 14]);
    // Three of a kind (Kings): K♠, K♦, K♥, Q♠, J♠
    let three_kind = evaluate_hand_rank([11, 24, 37, 9, 22, 0, 1]);
    assert(straight > three_kind);
}

#[test]
fn test_identical_hands() {
    // Two players with the same exact cards should have equal score
    let hand1 = evaluate_hand_rank([12, 25, 38, 0, 1, 2, 3]);
    let hand2 = evaluate_hand_rank([12, 25, 38, 0, 1, 2, 3]);
    assert(hand1 == hand2);
}

#[test]
fn test_high_card_kicker_tie_breaker() {
    // Pair of kings with different kickers
    // K♠, K♦, A♠, Q♠, J♠
    let high_kicker = evaluate_hand_rank([11, 24, 12, 9, 22, 0, 1]);
    // K♠, K♦, A♠, Q♠, 10♠
    let low_kicker = evaluate_hand_rank([11, 24, 12, 9, 22, 7, 1]);
    assert(high_kicker > low_kicker);
}

#[test]
fn test_two_pair_kicker_comparison() {
    // Two pair (Aces and Kings) with Queen kicker: A♠, A♦, K♠, K♦, Q♠, J♠, 2♥
    let two_pair_q = evaluate_hand_rank([12, 25, 11, 24, 9, 22, 14]);
    // Two pair (Aces and Kings) with Jack kicker: A♠, A♦, K♠, K♦, J♠, 10♠, 2♥
    let two_pair_j = evaluate_hand_rank([12, 25, 11, 24, 22, 8, 14]);
    assert(two_pair_q > two_pair_j);
}

#[test]
fn test_full_house_ranking() {
    // Full house (Aces full of Kings): A♠, A♦, A♥, K♠, K♦, Q♠, J♠
    let full_house_aaa_kk = evaluate_hand_rank([12, 25, 38, 11, 24, 9, 22]);
    // Full house (Kings full of Aces): K♠, K♦, K♥, A♠, A♦, Q♠, J♠
    let full_house_kkk_aa = evaluate_hand_rank([11, 24, 37, 12, 25, 9, 22]);
    assert(full_house_aaa_kk > full_house_kkk_aa);
}

#[test]
fn test_four_of_a_kind_beats_full_house() {
    // Four of a kind (Aces): A♠, A♦, A♥, A♣, K♠, K♦, Q♠
    let four_kind = evaluate_hand_rank([12, 25, 38, 0, 11, 24, 9]);
    // Full house (Kings full of Aces): K♠, K♦, K♥, A♠, A♦, Q♠, J♠
    let full_house = evaluate_hand_rank([11, 24, 37, 12, 25, 9, 22]);
    assert(four_kind > full_house);
}

#[test]
fn test_straight_flush_beats_four_of_a_kind() {
    // Straight flush (9-8-7-6-5 all hearts): 9♥, 8♥, 7♥, 6♥, 5♥, K♠, Q♠
    let straight_flush = evaluate_hand_rank([48, 47, 46, 45, 44, 11, 24]);
    // Four of a kind (Aces): A♠, A♦, A♥, A♣, K♠, K♦, Q♠
    let four_kind = evaluate_hand_rank([12, 25, 38, 0, 11, 24, 9]);
    assert(straight_flush > four_kind);
}

#[test]
fn test_royal_flush_is_best() {
    // Royal flush (A-K-Q-J-10 all spades): A♠, K♠, Q♠, J♠, 10♠, 2♥, 3♥
    let royal = evaluate_hand_rank([12, 11, 10, 9, 8, 13, 26]);
    // Straight flush (9-8-7-6-5): 9♠, 8♠, 7♠, 6♠, 5♠, K♥, Q♥
    let straight_flush = evaluate_hand_rank([8, 7, 6, 5, 4, 37, 50]);
    assert(royal > straight_flush);
}

#[test]
fn test_wheel_straight() {
    // Wheel (A-2-3-4-5): A♠, K♦, Q♦, 5♣, 4♣, 3♥, 2♥
    let wheel = evaluate_hand_rank([12, 10, 23, 30, 29, 15, 14]);
    // Six-high (6-5-4-3-2): 6♠, 5♠, 4♠, 3♠, 2♠, K♥, Q♥
    let six_high = evaluate_hand_rank([5, 4, 3, 2, 1, 37, 50]);
    assert(wheel > six_high);
}

#[test]
fn test_kicker_matters_in_flush() {
    // Flush with ace high kicker: A♥, K♥, Q♥, J♥, 9♥, 2♠, 3♠
    let ace_flush = evaluate_hand_rank([12, 11, 10, 9, 7, 26, 39]);
    // Flush with king high kicker: K♥, Q♥, J♥, 10♥, 8♥, 2♠, 3♠
    let king_flush = evaluate_hand_rank([11, 10, 9, 8, 6, 26, 39]);
    assert(ace_flush > king_flush);
}

#[test]
fn test_all_unique_cards_validation() {
    let unique_cards = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51];
    assert_valid_deck(unique_cards);
}

#[test]
fn test_five_card_vs_seven_card_evaluation() {
    // The evaluate_hand_rank function should return the best 5-card hand
    // Seven cards: A♠, K♠, Q♠, J♠, 10♠, 9♠, 2♥ (royal flush + extra)
    let seven_card_hand = evaluate_hand_rank([12, 11, 10, 9, 8, 7, 14]);
    // Five cards: A♠, K♠, Q♠, J♠, 10♠ (royal flush)
    let five_card_hand = evaluate_hand_rank([12, 11, 10, 9, 8, 0, 1]);
    // Both should have same best hand score (royal flush)
    assert(seven_card_hand == five_card_hand);
}

#[test]
fn test_high_card_category() {
    // 7♠, 9♦, J♥, Q♣, K♠, 2♥, 4♣ — no pairs, no flush, no straight
    let score = evaluate_hand_rank([5, 21, 35, 9, 11, 14, 28]);
    let category = score >> 20;
    assert(category == 0);
}

#[test]
fn test_one_pair_category() {
    // Pair of 2s: 2♣, 2♦, 5♥, 7♠, 9♣, J♦, K♠
    let score = evaluate_hand_rank([0, 13, 18, 33, 7, 22, 11]);
    let category = score >> 20;
    assert(category == 1);
}

#[test]
fn test_two_pair_category() {
    // Two pair: A♠, A♦, K♥, K♣, Q♠, J♦, 2♥
    let score = evaluate_hand_rank([12, 25, 11, 24, 9, 22, 14]);
    let category = score >> 20;
    assert(category == 2);
}

#[test]
fn test_three_of_a_kind_category() {
    // Trips: K♠, K♦, K♥, Q♣, J♠, 2♦, 3♥
    let score = evaluate_hand_rank([11, 24, 37, 9, 22, 14, 15]);
    let category = score >> 20;
    assert(category == 3);
}

#[test]
fn test_straight_category() {
    // Straight 10-J-Q-K-A (ace-high): 10♣, J♦, Q♥, K♠, A♣, 2♦, 3♥
    let score = evaluate_hand_rank([8, 22, 9, 11, 12, 14, 15]);
    let category = score >> 20;
    assert(category == 4);
}

#[test]
fn test_flush_category() {
    // Flush: all hearts — 2♥, 5♥, 8♥, J♥, K♥, 3♣, 7♠
    let score = evaluate_hand_rank([14, 18, 46, 35, 37, 15, 33]);
    let category = score >> 20;
    assert(category == 5);
}

#[test]
fn test_full_house_category() {
    // Full house: K♠, K♦, K♥, Q♣, Q♠, 2♦, 3♥
    let score = evaluate_hand_rank([11, 24, 37, 9, 22, 14, 15]);
    let category = score >> 20;
    assert(category == 6);
}

#[test]
fn test_four_of_a_kind_category() {
    // Quads: K♠, K♦, K♥, K♣, Q♠, 2♦, 3♥
    let score = evaluate_hand_rank([11, 24, 37, 0, 9, 14, 15]);
    let category = score >> 20;
    assert(category == 7);
}

#[test]
fn test_straight_flush_category() {
    // Straight flush 5-6-7-8-9 all hearts: 5♥, 6♥, 7♥, 8♥, 9♥, 2♣, 3♠
    let score = evaluate_hand_rank([18, 19, 20, 46, 47, 14, 15]);
    let category = score >> 20;
    assert(category == 8);
}

#[test]
fn test_royal_flush_category() {
    // Royal flush: A♠, K♠, Q♠, J♠, 10♠, 2♥, 3♥
    let score = evaluate_hand_rank([12, 11, 10, 9, 8, 13, 26]);
    let category = score >> 20;
    assert(category == 9);
}

#[test]
fn test_straight_flush_beats_four_of_a_kind_explicit() {
    let sf = evaluate_hand_rank([48, 47, 46, 45, 44, 11, 24]);
    let quads = evaluate_hand_rank([12, 25, 38, 0, 11, 24, 9]);
    assert(sf >> 20 == 8);
    assert(quads >> 20 == 7);
    assert(sf > quads);
}

#[test]
fn test_four_of_a_kind_beats_full_house_explicit() {
    let quads = evaluate_hand_rank([12, 25, 38, 0, 11, 24, 9]);
    let fh = evaluate_hand_rank([11, 24, 37, 12, 25, 9, 22]);
    assert(quads >> 20 == 7);
    assert(fh >> 20 == 6);
    assert(quads > fh);
}

#[test]
fn test_full_house_beats_flush_explicit() {
    let fh = evaluate_hand_rank([11, 24, 37, 12, 25, 9, 22]);
    let flush = evaluate_hand_rank([13, 26, 32, 45, 48, 9, 22]);
    assert(fh >> 20 == 6);
    assert(flush >> 20 == 5);
    assert(fh > flush);
}

#[test]
fn test_flush_beats_straight_explicit() {
    let flush = evaluate_hand_rank([13, 26, 32, 45, 48, 9, 22]);
    let straight = evaluate_hand_rank([11, 24, 37, 8, 21, 0, 1]);
    assert(flush >> 20 == 5);
    assert(straight >> 20 == 4);
    assert(flush > straight);
}

#[test]
fn test_straight_beats_three_of_a_kind_explicit() {
    let straight = evaluate_hand_rank([12, 10, 23, 30, 29, 15, 14]);
    let trips = evaluate_hand_rank([11, 24, 37, 9, 22, 0, 1]);
    assert(straight >> 20 == 4);
    assert(trips >> 20 == 3);
    assert(straight > trips);
}

#[test]
fn test_three_of_a_kind_beats_two_pair_explicit() {
    let trips = evaluate_hand_rank([11, 24, 37, 9, 22, 0, 1]);
    let two_pair = evaluate_hand_rank([12, 25, 11, 24, 9, 22, 14]);
    assert(trips >> 20 == 3);
    assert(two_pair >> 20 == 2);
    assert(trips > two_pair);
}

#[test]
fn test_two_pair_beats_one_pair_explicit() {
    let two_pair = evaluate_hand_rank([12, 25, 11, 24, 9, 22, 14]);
    let one_pair = evaluate_hand_rank([12, 25, 0, 1, 3, 5, 7]);
    assert(two_pair >> 20 == 2);
    assert(one_pair >> 20 == 1);
    assert(two_pair > one_pair);
}

#[test]
fn test_one_pair_beats_high_card_explicit() {
    let one_pair = evaluate_hand_rank([12, 25, 0, 1, 3, 5, 7]);
    let high_card = evaluate_hand_rank([5, 21, 35, 9, 11, 14, 28]);
    assert(one_pair >> 20 == 1);
    assert(high_card >> 20 == 0);
    assert(one_pair > high_card);
}

#[test]
fn test_higher_pair_wins() {
    // Pair of aces: A♠, A♦, 2♥, 3♣, 5♠, 7♦, 9♣
    let pair_aces = evaluate_hand_rank([12, 25, 14, 15, 18, 33, 7]);
    // Pair of kings: K♠, K♦, 2♥, 3♣, 5♠, 7♦, 9♣
    let pair_kings = evaluate_hand_rank([11, 24, 14, 15, 18, 33, 7]);
    assert(pair_aces > pair_kings);
}

#[test]
fn test_ace_high_straight_beats_king_high_straight() {
    // Ace-high straight: 10-J-Q-K-A (mixed suits)
    let ace_high = evaluate_hand_rank([8, 22, 9, 11, 12, 0, 1]);
    // King-high straight: 9-10-J-Q-K (mixed suits)
    let king_high = evaluate_hand_rank([7, 8, 22, 9, 11, 0, 1]);
    assert(ace_high > king_high);
}

#[test]
fn test_wheel_straight_category() {
    // Wheel: A-2-3-4-5 (mixed suits, no flush)
    let wheel = evaluate_hand_rank([12, 14, 15, 29, 30, 10, 23]);
    let category = wheel >> 20;
    assert(category == 4);
}

#[test]
fn test_flush_high_card_tiebreak() {
    // Flush A-K-Q-J-9 all hearts: A♥, K♥, Q♥, J♥, 9♥, 2♠, 3♠
    let flush_ace = evaluate_hand_rank([12, 11, 10, 9, 7, 26, 39]);
    // Flush K-Q-J-10-8 all hearts: K♥, Q♥, J♥, 10♥, 8♥, 2♠, 3♠
    let flush_king = evaluate_hand_rank([11, 10, 9, 8, 6, 26, 39]);
    assert(flush_ace > flush_king);
}

#[test]
fn test_different_suit_flush() {
    // Flush in diamonds: 2♦, 5♦, 8♦, J♦, K♦, 3♣, 7♠
    let diamond_flush = evaluate_hand_rank([1, 17, 34, 22, 24, 15, 33]);
    // Flush in spades: 2♠, 5♠, 8♠, J♠, K♠, 3♣, 7♦
    let spade_flush = evaluate_hand_rank([12, 16, 29, 35, 37, 15, 17]);
    // Both should be category 5 (flush)
    assert(diamond_flush >> 20 == 5);
    assert(spade_flush >> 20 == 5);
}

#[test]
fn test_two_different_quads() {
    // Quad aces: A♠, A♦, A♥, A♣, 2♠, 3♦, 5♥
    let quads_aces = evaluate_hand_rank([12, 25, 38, 0, 14, 15, 18]);
    // Quad kings: K♠, K♦, K♥, K♣, 2♠, 3♦, 5♥
    let quads_kings = evaluate_hand_rank([11, 24, 37, 0, 14, 15, 18]);
    assert(quads_aces > quads_kings);
}

#[test]
fn test_flush_loses_to_full_house() {
    // Full house: 5♠, 5♦, 5♥, 3♣, 3♠, 2♥, 7♣
    let fh = evaluate_hand_rank([4, 17, 30, 1, 14, 28, 33]);
    // Flush: 2♥, 4♥, 6♥, 8♥, K♥, 3♣, 7♠
    let flush = evaluate_hand_rank([14, 16, 18, 46, 37, 15, 33]);
    assert(fh >> 20 == 6);
    assert(flush >> 20 == 5);
    assert(fh > flush);
}

#[test]
fn test_three_of_a_kind_beats_high_pair() {
    // Trip twos: 2♣, 2♦, 2♥, 5♠, 7♣, 9♦, J♠
    let trips = evaluate_hand_rank([0, 13, 26, 4, 33, 7, 22]);
    // Pair of aces: A♠, A♦, 2♣, 3♦, 5♥, 7♣, 9♠
    let pair_aces = evaluate_hand_rank([12, 25, 0, 15, 18, 33, 7]);
    assert(trips >> 20 == 3);
    assert(pair_aces >> 20 == 1);
    assert(trips > pair_aces);
}

#[test]
fn test_hand_ordering_full_spectrum() {
    // Verify the complete ordering: Royal > SF > 4K > FH > Flush > Straight > Trips > 2P > 1P > HC
    let royal = evaluate_hand_rank([12, 11, 10, 9, 8, 13, 26]);
    let sf = evaluate_hand_rank([48, 47, 46, 45, 44, 11, 24]);
    let quads = evaluate_hand_rank([12, 25, 38, 0, 11, 24, 9]);
    let fh = evaluate_hand_rank([11, 24, 37, 12, 25, 9, 22]);
    let flush = evaluate_hand_rank([13, 26, 32, 45, 48, 9, 22]);
    let straight = evaluate_hand_rank([11, 24, 37, 8, 21, 0, 1]);
    let trips = evaluate_hand_rank([11, 24, 37, 9, 22, 0, 1]);
    let two_pair = evaluate_hand_rank([12, 25, 11, 24, 9, 22, 14]);
    let one_pair = evaluate_hand_rank([12, 25, 0, 1, 3, 5, 7]);
    let high_card = evaluate_hand_rank([5, 21, 35, 9, 11, 14, 28]);

    assert(royal > sf);
    assert(sf > quads);
    assert(quads > fh);
    assert(fh > flush);
    assert(flush > straight);
    assert(straight > trips);
    assert(trips > two_pair);
    assert(two_pair > one_pair);
    assert(one_pair > high_card);
}

