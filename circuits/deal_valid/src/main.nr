/// Deal Validity Circuit
///
/// The deck is derived in-circuit from per-party private contributions:
/// - party permutation shares
/// - party salt shares
///
/// This removes any single-node plaintext deck dependency during prepare/split.

use stellar_poker_lib::cards;
use stellar_poker_lib::commitments;
use stellar_poker_lib::merkle;
use stellar_poker_lib::shuffle;

/// Deck size (cards in deck, default 52 for standard poker)
/// Can be overridden at compile time for variant poker games
global DECK_SIZE: u32 = 52;

/// Tree leaf count must be >= DECK_SIZE and power of 2
/// For 52 cards: 64 leaves (6 levels)
/// For 104 cards: 128 leaves (7 levels)
/// For 200+ cards: 256 leaves (8 levels)
global TREE_LEAVES: u32 = 64;
global TREE_DEPTH: u32 = 6;

/// Maximum players supported (for fixed circuit size).
/// Override at compile time via PLAYER_COUNT environment variable for
/// per-table-size parameterisation (Issue #7).
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
    num_players: pub u32,
) -> pub (Field, [Field; MAX_PLAYERS], [u32; MAX_PLAYERS], [u32; MAX_PLAYERS]) {
    assert(num_players >= 2, "need at least 2 players");
    assert(num_players <= MAX_PLAYERS, "too many players");

    let (deck, salts) = shuffle::derive_shared_deck_and_salts(
        party0_permutation,
        party1_permutation,
        party2_permutation,
        party0_salts,
        party1_salts,
        party2_salts,
    );

    // 1. Verify all cards in derived deck are valid and unique.
    cards::assert_valid_deck(deck);

    // 2. Compute card commitments and Merkle root.
    let mut leaves: [Field; TREE_LEAVES] = [0; TREE_LEAVES];
    for i in 0..DECK_SIZE {
        leaves[i] = commitments::commit_card(deck[i], salts[i]);
    }
    // Remaining leaves are zero (padding).
    let deck_root = merkle::compute_merkle_root_generic(leaves);

    // 3. Deterministically assign two cards per player by seat order.
    // Optimization: reuse card commitment leaves computed in step 2.
    let mut hand_commitments: [Field; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut dealt_card1_indices: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut dealt_card2_indices: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];

    for p in 0..MAX_PLAYERS {
        if p < num_players {
            let idx1 = p * 2;
            let idx2 = idx1 + 1;
            assert(idx2 < 52, "dealt index out of range");

            dealt_card1_indices[p] = idx1;
            dealt_card2_indices[p] = idx2;

            // Use pre-computed leaf commitments from step 2 to reduce constraints.
            let c1_commit = leaves[idx1];
            let c2_commit = leaves[idx2];
            hand_commitments[p] = commitments::commit_hand(c1_commit, c2_commit);
        }
    }

    // 4. Sanity check: verify no duplicate dealt indices (O(n) instead of O(n²)).
    // Since indices are deterministic (player p gets cards 2*p and 2*p+1),
    // we just need to verify that 2*num_players <= DECK_SIZE.
    assert(2 * num_players <= 52, "not enough cards for players");

    (deck_root, hand_commitments, dealt_card1_indices, dealt_card2_indices)
}
