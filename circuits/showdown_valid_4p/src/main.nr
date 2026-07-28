/// Showdown Validity Circuit
///
/// Derives hole cards from shared deck contributions, verifies commitments,
/// evaluates all active hands, and outputs the winner index plus a bit mask of
/// every active player tied for the best score.

use stellar_poker_lib::cards;
use stellar_poker_lib::commitments;
use stellar_poker_lib::merkle;
use stellar_poker_lib::shuffle;

global MAX_PLAYERS: u32 = 6;

fn main(
    // Private inputs (secret-shared in MPC)
    party0_permutation: [u32; 52],
    party1_permutation: [u32; 52],
    party2_permutation: [u32; 52],
    party0_salts: [Field; 52],
    party1_salts: [Field; 52],
    party2_salts: [Field; 52],

    // Public inputs
    num_active_players: pub u32,
    hand_commitments: pub [Field; MAX_PLAYERS],
    board_indices: pub [u32; 5],
    deck_root: pub Field,
) -> pub ([u32; MAX_PLAYERS], [u32; MAX_PLAYERS], u32, u32) {
    assert(num_active_players >= 2, "need at least 2 players");
    assert(num_active_players <= MAX_PLAYERS, "too many active players");

    let (deck, salts) = shuffle::derive_shared_deck_and_salts(
        party0_permutation,
        party1_permutation,
        party2_permutation,
        party0_salts,
        party1_salts,
        party2_salts,
    );

    // 1. Recompute deck root and verify consistency with deal/reveal phases.
    let mut leaves: [Field; 64] = [0; 64];
    for i in 0..52 {
        leaves[i] = commitments::commit_card(deck[i], salts[i]);
    }
    let computed_root = merkle::compute_merkle_root(leaves);
    assert(computed_root == deck_root, "deck root mismatch");

    // 2. Resolve board cards and check index uniqueness against dealt cards.
    let mut used_indices: [bool; 52] = [false; 52];
    for p in 0..MAX_PLAYERS {
        if p < num_active_players {
            let idx1 = p * 2;
            let idx2 = idx1 + 1;
            assert(idx2 < 52, "hole card index out of range");
            assert(!used_indices[idx1], "duplicate dealt index");
            used_indices[idx1] = true;
            assert(!used_indices[idx2], "duplicate dealt index");
            used_indices[idx2] = true;
        }
    }

    let mut board_cards: [u32; 5] = [0; 5];
    for i in 0..5 {
        let idx = board_indices[i];
        assert(idx < 52, "board index out of range");
        assert(!used_indices[idx], "board index already used");
        used_indices[idx] = true;

        board_cards[i] = deck[idx] as u32;
    }

    // 3. Verify per-player commitments and evaluate hand ranks.
    let mut hole_card1: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut hole_card2: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut hand_scores: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];

    for p in 0..MAX_PLAYERS {
        if p < num_active_players {
            let idx1 = p * 2;
            let idx2 = idx1 + 1;

            hole_card1[p] = deck[idx1] as u32;
            hole_card2[p] = deck[idx2] as u32;

            let c1_commit = commitments::commit_card(deck[idx1], salts[idx1]);
            let c2_commit = commitments::commit_card(deck[idx2], salts[idx2]);
            let expected_commit = commitments::commit_hand(c1_commit, c2_commit);
            assert(expected_commit == hand_commitments[p], "hand commitment mismatch");

            let seven_cards: [u32; 7] = [
                hole_card1[p],
                hole_card2[p],
                board_cards[0],
                board_cards[1],
                board_cards[2],
                board_cards[3],
                board_cards[4],
            ];
            hand_scores[p] = cards::evaluate_hand_rank(seven_cards);
        }
    }

    // 4. Compute winner as first seat with maximal score and mark all tied
    // seats. The contract uses the mask to split tied pots.
    let mut winner_index: u32 = 0;
    let mut winner_score: u32 = 0;
    for p in 0..MAX_PLAYERS {
        if p < num_active_players {
            if (p == 0) | (hand_scores[p] > winner_score) {
                winner_index = p;
                winner_score = hand_scores[p];
            }
        }
    }

    for p in 0..MAX_PLAYERS {
        if p < num_active_players {
            assert(winner_score >= hand_scores[p], "declared winner is not maximal");
        }
    }

    let bit_values: [u32; MAX_PLAYERS] = [1, 2, 4, 8, 16, 32];
    let mut tie_mask: u32 = 0;
    for p in 0..MAX_PLAYERS {
        if p < num_active_players {
            if hand_scores[p] == winner_score {
                tie_mask = tie_mask + bit_values[p];
            }
        }
    }

    (hole_card1, hole_card2, winner_index, tie_mask)
}
