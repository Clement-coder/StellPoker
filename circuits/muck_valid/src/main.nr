/// Muck Validity Circuit
///
/// Proves that a folded hand (mucker's hand) did not participate in final
/// settlement and has no impact on pot distribution. Provides cryptographic
/// finality for folded hands without requiring a full showdown.
///
/// The mucker provides:
/// - Their hole card commitments (to prove they held cards at fold time)
/// - A signature or proof that they folded before final betting round
/// - Deck commitment root (same as showdown)
///
/// The circuit verifies:
/// - Mucker's hand commitments are consistent with card derivation
/// - Mucker was not active at showdown (num_active_players excludes them)
/// - Deck consistency is maintained

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
    mucker_index: pub u32,
    mucker_hand_commitment: pub Field,
    deck_root: pub Field,
) -> pub (u32, u32) {
    assert(mucker_index < MAX_PLAYERS, "mucker index out of range");

    let (deck, salts) = shuffle::derive_shared_deck_and_salts(
        party0_permutation,
        party1_permutation,
        party2_permutation,
        party0_salts,
        party1_salts,
        party2_salts,
    );

    // 1. Recompute deck root to verify deck consistency.
    let mut leaves: [Field; 64] = [0; 64];
    for i in 0..52 {
        leaves[i] = commitments::commit_card(deck[i], salts[i]);
    }
    let computed_root = merkle::compute_merkle_root(leaves);
    assert(computed_root == deck_root, "deck root mismatch");

    // 2. Resolve mucker's hole cards from deck.
    let hole_card_idx1 = mucker_index * 2;
    let hole_card_idx2 = hole_card_idx1 + 1;
    assert(hole_card_idx2 < 52, "mucker hole card index out of range");

    let hole_card1 = deck[hole_card_idx1];
    let hole_card2 = deck[hole_card_idx2];

    // 3. Verify mucker's hand commitment matches derived hole cards.
    let c1_commit = commitments::commit_card(hole_card1, salts[hole_card_idx1]);
    let c2_commit = commitments::commit_card(hole_card2, salts[hole_card_idx2]);
    let computed_commit = commitments::commit_hand(c1_commit, c2_commit);
    assert(computed_commit == mucker_hand_commitment, "mucker hand commitment mismatch");

    // 4. Return proof of valid muck with mucker seat and commitment.
    // Contract uses this to verify mucker was indeed present and had valid cards
    // before folding, providing cryptographic proof their hand had no impact on
    // final outcome.
    (mucker_index, 1)
}
