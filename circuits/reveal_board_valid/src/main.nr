/// Board Reveal Validity Circuit
///
/// Derives the same secret deck from party contributions and reveals the
/// next deterministic board card indices not yet used.

use stellar_poker_lib::cards;
use stellar_poker_lib::commitments;
use stellar_poker_lib::merkle;
use stellar_poker_lib::shuffle;

/// Deck size (cards in deck, default 52 for standard poker)
global DECK_SIZE: u32 = 52;

/// Tree leaf count must be >= DECK_SIZE and power of 2
/// For 52 cards: 64 leaves (6 levels)
/// For 104 cards: 128 leaves (7 levels)
global TREE_LEAVES: u32 = 64;
global TREE_DEPTH: u32 = 6;

/// Maximum cards that can be revealed at once (flop = 3)
global MAX_REVEAL: u32 = 3;
/// Maximum previously used indices (2 per player * 6 players + 4 board = 16)
global MAX_USED: u32 = 16;

fn main(
    // Private inputs (secret-shared in MPC)
    party0_permutation: [u32; 52],
    party1_permutation: [u32; 52],
    party2_permutation: [u32; 52],
    party0_salts: [Field; 52],
    party1_salts: [Field; 52],
    party2_salts: [Field; 52],

    // Public inputs
    deck_root: pub Field,
    num_revealed: pub u32,
    num_previously_used: pub u32,
    previously_used_indices: pub [u32; MAX_USED],
) -> pub ([Field; MAX_REVEAL], [u32; MAX_REVEAL]) {
    assert(num_revealed >= 1, "must reveal at least one card");
    assert(num_revealed <= MAX_REVEAL, "too many cards requested");
    assert(num_previously_used <= MAX_USED, "too many used indices");

    let (deck, salts) = shuffle::derive_shared_deck_and_salts(
        party0_permutation,
        party1_permutation,
        party2_permutation,
        party0_salts,
        party1_salts,
        party2_salts,
    );

    // 1. Recompute deck root and verify consistency with prior phases.
    let mut leaves: [Field; TREE_LEAVES] = [0; TREE_LEAVES];
    for i in 0..DECK_SIZE {
        leaves[i] = commitments::commit_card(deck[i], salts[i]);
    }
    let computed_root = merkle::compute_merkle_root_generic(leaves);
    assert(computed_root == deck_root, "deck root mismatch");

    // 2. Mark all previously used deck indices.
    let mut used: [bool; 52] = [false; 52];
    for i in 0..MAX_USED {
        if i < num_previously_used {
            let idx = previously_used_indices[i];
            assert(idx < 52, "used index out of range");
            assert(!used[idx], "duplicate used index");
            used[idx] = true;
        }
    }

    // 3. Pick next unused indices in ascending order.
    let mut revealed_indices: [u32; MAX_REVEAL] = [0; MAX_REVEAL];
    let mut found: u32 = 0;
    for idx in 0..52 {
        if (!used[idx]) & (found < num_revealed) {
            revealed_indices[found] = idx;
            found += 1;
        }
    }
    assert(found == num_revealed, "not enough undealt cards");

    // 4. Resolve plaintext card values for those indices.
    let mut revealed_cards: [Field; MAX_REVEAL] = [0; MAX_REVEAL];
    for i in 0..MAX_REVEAL {
        if i < num_revealed {
            revealed_cards[i] = deck[revealed_indices[i]];
            cards::assert_valid_card(revealed_cards[i]);
        }
    }

    (revealed_cards, revealed_indices)
}
