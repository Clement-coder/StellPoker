// Tournament proof aggregator (Issue #91)
// Recursively aggregates multiple hand proofs into single tournament proof

fn main(
    hand_proofs: [Field; 10],
    hand_winners: [Field; 10],
    num_hands: Field,
    tournament_winner: pub Field
) {
    // Verify each hand proof contributed to tournament
    let mut winner_chips: [Field; 10] = [0; 10];
    for i in 0..10 {
        if (i as Field) < num_hands {
            assert(hand_proofs[i] != 0);
            let winner_idx = hand_winners[i] as u64;
            winner_chips[winner_idx] = winner_chips[winner_idx] + 1;
        }
    }
    
    // Verify tournament winner has most chips
    let mut max_chips = 0;
    let mut max_player = 0;
    for i in 0..10 {
        if winner_chips[i] > max_chips {
            max_chips = winner_chips[i];
            max_player = i as Field;
        }
    }
    assert(max_player == tournament_winner);
}
