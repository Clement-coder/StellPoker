# Noir Testing Guide

A comprehensive guide to writing tests for Noir circuits in the Stellar Poker project.

## Table of Contents

1. [Test Structure](#test-structure)
2. [Test Data Generation](#test-data-generation)
3. [Constraint Counting](#constraint-counting)
4. [Edge Case Coverage](#edge-case-coverage)
5. [Integration with nargo test](#integration-with-nargo-test)
6. [CI Integration](#ci-integration)
7. [Best Practices](#best-practices)
8. [Common Patterns](#common-patterns)

---

## 1. Test Structure

### Basic Test Anatomy

Noir tests are written using the `#[test]` attribute directly in your `.nr` files. Tests run in the same file as your implementation code.

```noir
// Basic test structure
#[test]
fn test_function_name() {
    // Arrange: Set up test data
    let input = 42;
    
    // Act: Call the function
    let result = my_function(input);
    
    // Assert: Verify the output
    assert(result == expected_value);
}
```

### Test Organization

Tests should be placed at the bottom of each module file:


```noir
// Implementation code
pub fn my_function(x: Field) -> Field {
    x + 1
}

// Tests section
#[test]
fn test_my_function_basic() {
    assert(my_function(5) == 6);
}

#[test]
fn test_my_function_zero() {
    assert(my_function(0) == 1);
}
```

### Test Naming Conventions

Follow these patterns for test names:

- `test_<function>_<scenario>` - For basic function tests
- `test_<function>_<edge_case>` - For edge case tests
- `test_<function>_should_fail` - For negative tests (using `#[test(should_fail)]`)

**Examples from the project:**

```noir
#[test]
fn test_card_encoding()

#[test]
fn test_hand_ranking_pair_beats_high_card()

#[test]
fn test_card_commitment_roundtrip()
```


---

## 2. Test Data Generation

### Manual Test Data

For simple tests, create data inline:

```noir
#[test]
fn test_suit_extraction() {
    // Clubs (suit 0)
    assert(suit(0) == 0);    // 2 of Clubs
    assert(suit(12) == 0);   // Ace of Clubs
    
    // Spades (suit 3)
    assert(suit(39) == 3);   // 2 of Spades
    assert(suit(51) == 3);   // Ace of Spades
}
```

### Array Generation Patterns

For larger datasets, use loops and computations:

```noir
#[test]
fn test_valid_deck_generation() {
    // Generate a sequential deck
    let mut deck: [Field; 52] = [0; 52];
    for i in 0..52 {
        deck[i] = i as Field;
    }
    
    // Verify it's valid
    assert_valid_deck(deck);
}
```

### Permutation Generation

For shuffle/permutation tests:

```noir
#[test]
fn test_simple_permutation() {
    // Identity permutation
    let mut identity: [u32; 52] = [0; 52];
    for i in 0..52 {
        identity[i] = i;
    }
    
    // Reverse permutation
    let mut reverse: [u32; 52] = [0; 52];
    for i in 0..52 {
        reverse[i] = 51 - i;
    }
    
    // Swap first two
    let mut swap_two: [u32; 52] = [0; 52];
    swap_two[0] = 1;
    swap_two[1] = 0;
    for i in 2..52 {
        swap_two[i] = i;
    }
}
```

### Salt/Random Field Generation

For commitment schemes and cryptographic tests:

```noir
#[test]
fn test_commitment_with_various_salts() {
    let card: Field = 25;
    
    // Test with different salts
    let salt1: Field = 123456789;
    let salt2: Field = 987654321;
    let salt3: Field = 0;
    
    let commit1 = commit_card(card, salt1);
    let commit2 = commit_card(card, salt2);
    let commit3 = commit_card(card, salt3);
    
    // Different salts should produce different commitments
    assert(commit1 != commit2);
    assert(commit2 != commit3);
    assert(commit1 != commit3);
}
```

### Helper Functions for Test Data

Create reusable helpers within test modules:

```noir
// Helper to create a specific hand
fn create_test_hand_pair_of_aces() -> [u32; 7] {
    // Two aces (rank 12): cards 12 and 25
    // Plus 5 random low cards
    [12, 25, 0, 1, 3, 5, 7]
}

fn create_test_hand_royal_flush() -> [u32; 7] {
    // Spades: 10, J, Q, K, A (suits all 3)
    // Card encoding: suit*13 + rank
    [
        (3 * 13) + 8,  // 10 of Spades
        (3 * 13) + 9,  // J of Spades
        (3 * 13) + 10, // Q of Spades
        (3 * 13) + 11, // K of Spades
        (3 * 13) + 12, // A of Spades
        0, 1           // Two irrelevant cards
    ]
}

#[test]
fn test_royal_flush_beats_pair() {
    let royal = evaluate_hand_rank(create_test_hand_royal_flush());
    let pair = evaluate_hand_rank(create_test_hand_pair_of_aces());
    assert(royal > pair);
}
```

---

## 3. Constraint Counting

### Understanding Constraints

Noir circuits compile to arithmetic constraints. Fewer constraints mean:
- Faster proving time
- Lower memory usage
- Smaller proof sizes

### Viewing Constraint Counts

Run `nargo info` to see constraint counts:

```bash
cd circuits/deal_valid
nargo info
```

**Output example:**
```
+---------+------------------------+---------------+
| Package | Function               | Constraints   |
+---------+------------------------+---------------+
| deal    | main                   | 15234         |
+---------+------------------------+---------------+
```

### Optimizing for Constraint Count

**1. Avoid Unnecessary Assertions**

```noir
// Bad: Extra assertions
fn validate_input(x: Field) {
    assert(x < 100);
    assert(x >= 0);  // Redundant if Field is always >= 0
}

// Good: Only necessary assertions
fn validate_input(x: Field) {
    assert(x < 100);
}
```

**2. Use Efficient Comparisons**

```noir
// Less efficient: Multiple checks
let is_valid = (x == 0) | (x == 1) | (x == 2);

// More efficient: Range check
let is_valid = x < 3;
```

**3. Minimize Loop Iterations**

```noir
// Optimize nested loops
for i in 0..n {
    for j in (i + 1)..n {  // Start from i+1 to avoid duplicates
        // Process (i, j) pairs
    }
}
```

**4. Test Constraint Impact**

```noir
#[test]
fn test_optimized_version_has_fewer_constraints() {
    // This test doesn't check functionality
    // It's documentation that optimization was considered
    
    // Original: ~1000 constraints
    // Optimized: ~750 constraints
    // Run `nargo info` to verify
    
    let result = optimized_function(test_input);
    assert(result == expected);
}
```

### Benchmarking in CI

Track constraint counts over time to catch regressions:

```bash
# In CI script
nargo info --json > constraints.json
# Compare with previous baseline
```

---

## 4. Edge Case Coverage

### Boundary Values

Test the limits of your inputs:

```noir
#[test]
fn test_card_boundaries() {
    // Minimum valid card
    assert_valid_card(0);
    
    // Maximum valid card
    assert_valid_card(51);
}

#[test(should_fail)]
fn test_card_out_of_range() {
    // Should fail: card value too high
    assert_valid_card(52);
}

#[test]
fn test_player_count_boundaries() {
    // Minimum players
    assert(validate_players(2) == true);
    
    // Maximum players
    assert(validate_players(6) == true);
}

#[test(should_fail)]
fn test_too_few_players() {
    validate_players(1);  // Should fail
}

#[test(should_fail)]
fn test_too_many_players() {
    validate_players(7);  // Should fail
}
```

### Duplicate Detection

Test uniqueness constraints:

```noir
#[test]
fn test_deck_no_duplicates() {
    let mut deck: [Field; 52] = [0; 52];
    for i in 0..52 {
        deck[i] = i as Field;
    }
    assert_all_unique(deck);  // Should pass
}

#[test(should_fail)]
fn test_deck_with_duplicate() {
    let mut deck: [Field; 52] = [0; 52];
    for i in 0..52 {
        deck[i] = i as Field;
    }
    deck[51] = 0;  // Duplicate!
    assert_all_unique(deck);  // Should fail
}
```


### Zero and Empty Cases

```noir
#[test]
fn test_commitment_with_zero_salt() {
    let card: Field = 25;
    let salt: Field = 0;
    let commit = commit_card(card, salt);
    
    // Should still produce valid commitment
    assert(commit != 0);
}

#[test]
fn test_zero_card_value() {
    // Card 0 is valid (2 of Clubs)
    assert_valid_card(0);
    assert(suit(0) == 0);
    assert(rank(0) == 0);
}
```

### Overflow and Wraparound

```noir
#[test]
fn test_large_field_values() {
    // Test with maximum Field value
    let max_safe: Field = (1 << 253) - 1;
    
    // Ensure operations don't overflow
    let result = safe_multiply(max_safe, 1);
    assert(result == max_safe);
}
```

### Permutation Edge Cases

```noir
#[test]
fn test_identity_permutation() {
    // No shuffle at all
    let mut perm: [u32; 52] = [0; 52];
    for i in 0..52 {
        perm[i] = i;
    }
    // Should still be valid
    let result = apply_permutation(deck, perm);
    assert_valid_deck(result);
}

#[test]
fn test_reverse_permutation() {
    // Complete reversal
    let mut perm: [u32; 52] = [0; 52];
    for i in 0..52 {
        perm[i] = 51 - i;
    }
    let result = apply_permutation(deck, perm);
    assert_valid_deck(result);
}

#[test]
fn test_single_swap_permutation() {
    // Minimal change: swap first and last
    let mut perm: [u32; 52] = [0; 52];
    for i in 0..52 {
        perm[i] = i;
    }
    perm[0] = 51;
    perm[51] = 0;
    let result = apply_permutation(deck, perm);
    assert_valid_deck(result);
}
```


### Hand Ranking Edge Cases

```noir
#[test]
fn test_wheel_straight() {
    // A-2-3-4-5 (Ace low straight)
    let hand = [
        12,  // Ace (rank 12)
        0,   // 2 (rank 0)
        1,   // 3 (rank 1)
        2,   // 4 (rank 2)
        3,   // 5 (rank 3)
        40,  // Random card
        41   // Random card
    ];
    let score = evaluate_hand_rank(hand);
    
    // Wheel should be recognized as straight
    assert(score > high_card_score);
}

#[test]
fn test_full_house_vs_flush() {
    let full_house = create_full_house_hand();
    let flush = create_flush_hand();
    
    // Full house beats flush
    let fh_score = evaluate_hand_rank(full_house);
    let fl_score = evaluate_hand_rank(flush);
    assert(fh_score > fl_score);
}
```


---

## 5. Integration with nargo test

### Running Tests

**Run all tests in a package:**
```bash
cd circuits/lib
nargo test
```

**Run specific test:**
```bash
nargo test test_card_encoding
```

**Run tests with verbose output:**
```bash
nargo test --show-output
```

**Run tests matching a pattern:**
```bash
nargo test test_hand_ranking
```

### Test Configuration

Configure test behavior in `Nargo.toml`:

```toml
[package]
name = "stellar_poker_lib"
type = "lib"
compiler_version = ">=0.36.0"

[dependencies]
# Your dependencies here
```

### Test Failures

When a test fails, Noir provides the assertion location:

```
[test_name] FAIL
   Constraint failed at cards.nr:42:5
```


### Expected Failures

Use `should_fail` for negative tests:

```noir
#[test(should_fail)]
fn test_invalid_card_should_fail() {
    assert_valid_card(52);  // Out of range
}

#[test(should_fail_with = "duplicate card")]
fn test_duplicate_detection() {
    let cards: [Field; 3] = [10, 20, 10];
    assert_all_unique(cards);
}
```

### Debugging Tests

Add temporary print statements for debugging (note: these only work in unconstrained functions):

```noir
unconstrained fn debug_print_deck(deck: [Field; 52]) {
    for i in 0..52 {
        std::println(deck[i]);
    }
}

#[test]
fn test_with_debugging() {
    let deck = generate_test_deck();
    // debug_print_deck(deck);  // Uncomment when debugging
    assert_valid_deck(deck);
}
```

---

## 6. CI Integration


### GitHub Actions Configuration

The project's CI pipeline (`.github/workflows/ci.yml`) includes Noir testing:

```yaml
circuits:
  name: Noir circuits
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4

    - name: Cache Nargo toolchain
      uses: actions/cache@v4
      with:
        path: |
          .tmp_tools
          .tmp_nargo_home
        key: ${{ runner.os }}-nargo-1.0.0-beta.17
        restore-keys: ${{ runner.os }}-nargo-

    - name: Compile all Noir circuits
      run: ./scripts/compile-circuits.sh

    - name: Install Nargo for tests
      run: |
        curl -L https://raw.githubusercontent.com/noir-lang/noirup/refs/heads/main/install | bash
        echo "$HOME/.nargo/bin" >> $GITHUB_PATH
        $HOME/.nargo/bin/noirup -v 1.0.0-beta.17

    - name: Run Noir library tests
      run: cd circuits/lib && nargo test
```


### Local CI Testing

Replicate CI environment locally:

```bash
# Install specific Nargo version
noirup -v 1.0.0-beta.17

# Run the same commands as CI
./scripts/compile-circuits.sh
cd circuits/lib && nargo test
```

### Test Coverage in CI

Add coverage reporting:

```bash
#!/bin/bash
# scripts/test-circuits.sh

set -e

echo "Testing Noir circuits..."

# Test library
cd circuits/lib
nargo test --show-output

# Test each circuit package
for circuit in deal_valid reveal_board_valid showdown_valid; do
    echo "Testing $circuit..."
    cd "../$circuit"
    
    # If tests exist, run them
    if grep -q "#\[test\]" src/*.nr; then
        nargo test
    fi
done

echo "All circuit tests passed!"
```


### Performance Benchmarking

Track constraint counts in CI:

```yaml
- name: Benchmark circuits
  run: |
    echo "Circuit Constraint Counts:" > benchmark.txt
    for circuit in deal_valid reveal_board_valid showdown_valid; do
      echo "=== $circuit ===" >> benchmark.txt
      cd circuits/$circuit
      nargo info >> ../../benchmark.txt
      cd ../..
    done
    cat benchmark.txt

- name: Upload benchmark results
  uses: actions/upload-artifact@v4
  with:
    name: circuit-benchmarks
    path: benchmark.txt
```

### Automated Regression Detection

Compare constraint counts against baseline:

```python
# scripts/check-constraint-regression.py
import json
import sys

BASELINE_FILE = "benchmarks/baseline-constraints.json"
CURRENT_FILE = "benchmarks/current-constraints.json"
THRESHOLD = 1.1  # 10% increase threshold

def check_regression():
    with open(BASELINE_FILE) as f:
        baseline = json.load(f)
    
    with open(CURRENT_FILE) as f:
        current = json.load(f)
    
    for circuit, baseline_count in baseline.items():
        current_count = current.get(circuit, 0)
        
        if current_count > baseline_count * THRESHOLD:
            print(f"❌ Regression in {circuit}:")
            print(f"   Baseline: {baseline_count} constraints")
            print(f"   Current:  {current_count} constraints")
            print(f"   Increase: {(current_count/baseline_count - 1)*100:.1f}%")
            sys.exit(1)
    
    print("✅ No constraint regressions detected")

if __name__ == "__main__":
    check_regression()
```

---

## 7. Best Practices

### 1. Test One Thing at a Time

```noir
// Bad: Testing multiple things
#[test]
fn test_everything() {
    assert_valid_card(0);
    assert(suit(0) == 0);
    assert(rank(0) == 0);
    let deck = generate_deck();
    assert_valid_deck(deck);
}

// Good: Separate focused tests
#[test]
fn test_card_validation() {
    assert_valid_card(0);
}

#[test]
fn test_suit_extraction() {
    assert(suit(0) == 0);
}

#[test]
fn test_rank_extraction() {
    assert(rank(0) == 0);
}
```

### 2. Use Descriptive Test Names

```noir
// Bad
#[test]
fn test1() { ... }

// Good
#[test]
fn test_card_commitment_different_salts_produce_different_commitments() { ... }
```

### 3. Document Complex Test Scenarios

```noir
#[test]
fn test_wheel_straight_recognized() {
    // The "wheel" is a special straight: A-2-3-4-5
    // where Ace acts as a low card (value 1)
    // This is the lowest possible straight
    let wheel_hand = [
        12,  // Ace (can be high or low)
        0,   // 2
        1,   // 3
        2,   // 4
        3,   // 5
        40,  // irrelevant
        41   // irrelevant
    ];
    
    let score = evaluate_hand_rank(wheel_hand);
    
    // Should be categorized as a straight (category 4)
    let category = score >> 20;
    assert(category == 4);
}
```

### 4. Test Both Success and Failure Paths

```noir
#[test]
fn test_valid_player_count_accepted() {
    assert(validate_player_count(4) == true);
}

#[test(should_fail)]
fn test_invalid_player_count_rejected() {
    validate_player_count(0);
}
```

### 5. Keep Tests Fast

```noir
// Avoid: Expensive operations in every test
#[test]
fn test_card_commit() {
    let full_deck = generate_expensive_deck();  // Don't do this
    let card = full_deck[0];
    let commit = commit_card(card, 123);
    assert(commit != 0);
}

// Prefer: Minimal test data
#[test]
fn test_card_commit() {
    let card: Field = 25;  // Just use a literal
    let commit = commit_card(card, 123);
    assert(commit != 0);
}
```

### 6. Maintain Test Independence

```noir
// Bad: Tests that depend on each other
let mut global_deck: [Field; 52] = [0; 52];

#[test]
fn test_initialize_deck() {
    global_deck = create_deck();  // Modifies global state
}

#[test]
fn test_shuffle_deck() {
    shuffle(global_deck);  // Depends on previous test
}

// Good: Independent tests
#[test]
fn test_initialize_deck() {
    let deck = create_deck();
    assert_valid_deck(deck);
}

#[test]
fn test_shuffle_deck() {
    let deck = create_deck();  // Fresh deck
    let shuffled = shuffle(deck);
    assert_valid_deck(shuffled);
}
```

### 7. Use Constants for Magic Numbers

```noir
// Bad
#[test]
fn test_deck_size() {
    assert(deck.len() == 52);
}

// Good
#[test]
fn test_deck_size() {
    assert(deck.len() == DECK_SIZE);
}
```

---

## 8. Common Patterns

### Pattern 1: Roundtrip Testing

Verify that encode/decode or commit/verify operations are consistent:

```noir
#[test]
fn test_card_commitment_roundtrip() {
    let card: Field = 42;
    let salt: Field = 123456789;
    
    // Commit
    let commitment = commit_card(card, salt);
    
    // Verify
    verify_card_commitment(card, salt, commitment);
    // Should not fail
}

#[test]
fn test_encoding_roundtrip() {
    for i in 0..52 {
        let card = i as Field;
        let s = suit(card);
        let r = rank(card);
        
        // Reconstruct card from suit and rank
        let reconstructed = (s * NUM_RANKS + r) as Field;
        assert(reconstructed == card);
    }
}
```


### Pattern 2: Property-Based Testing (Manual)

Test properties that should hold for all inputs:

```noir
#[test]
fn test_property_commitment_deterministic() {
    // Property: Same inputs always produce same output
    for i in 0..10 {
        let card = (i * 5) as Field;
        let salt = (i * 1000) as Field;
        
        let commit1 = commit_card(card, salt);
        let commit2 = commit_card(card, salt);
        
        assert(commit1 == commit2);
    }
}

#[test]
fn test_property_different_salts_different_commitments() {
    // Property: Different salts produce different commitments
    let card: Field = 25;
    
    for i in 0..10 {
        for j in (i + 1)..10 {
            let salt_i = (i * 1000) as Field;
            let salt_j = (j * 1000) as Field;
            
            let commit_i = commit_card(card, salt_i);
            let commit_j = commit_card(card, salt_j);
            
            assert(commit_i != commit_j);
        }
    }
}
```


### Pattern 3: Comparison Testing

Test relative ordering or comparisons:

```noir
#[test]
fn test_hand_rankings_order() {
    // Create hands of different ranks
    let high_card = create_high_card_hand();
    let pair = create_pair_hand();
    let two_pair = create_two_pair_hand();
    let three_kind = create_three_kind_hand();
    let straight = create_straight_hand();
    let flush = create_flush_hand();
    let full_house = create_full_house_hand();
    let four_kind = create_four_kind_hand();
    let straight_flush = create_straight_flush_hand();
    
    // Evaluate all
    let s1 = evaluate_hand_rank(high_card);
    let s2 = evaluate_hand_rank(pair);
    let s3 = evaluate_hand_rank(two_pair);
    let s4 = evaluate_hand_rank(three_kind);
    let s5 = evaluate_hand_rank(straight);
    let s6 = evaluate_hand_rank(flush);
    let s7 = evaluate_hand_rank(full_house);
    let s8 = evaluate_hand_rank(four_kind);
    let s9 = evaluate_hand_rank(straight_flush);
    
    // Verify order
    assert(s1 < s2);
    assert(s2 < s3);
    assert(s3 < s4);
    assert(s4 < s5);
    assert(s5 < s6);
    assert(s6 < s7);
    assert(s7 < s8);
    assert(s8 < s9);
}
```

### Pattern 4: Invariant Testing

Test invariants that must hold regardless of inputs:

```noir
#[test]
fn test_invariant_permutation_preserves_uniqueness() {
    // Invariant: Permuting a valid deck always produces a valid deck
    let mut original_deck: [Field; 52] = [0; 52];
    for i in 0..52 {
        original_deck[i] = i as Field;
    }
    
    // Try various permutations
    let perms = [
        create_identity_perm(),
        create_reverse_perm(),
        create_random_perm_1(),
        create_random_perm_2(),
    ];
    
    for perm in perms {
        let shuffled = apply_permutation(original_deck, perm);
        // Invariant: result is still a valid deck
        assert_valid_deck(shuffled);
    }
}
```


### Pattern 5: Merkle Proof Verification

Test cryptographic proofs:

```noir
#[test]
fn test_merkle_proof_valid_leaf() {
    // Build a tree
    let leaves: [Field; 64] = create_test_leaves();
    let root = merkle::compute_merkle_root(leaves);
    
    // Verify a specific leaf
    let leaf_index = 10;
    let leaf_value = leaves[leaf_index];
    let proof = merkle::generate_proof(leaves, leaf_index);
    
    // Verification should pass
    assert(merkle::verify_proof(leaf_value, leaf_index, proof, root));
}

#[test(should_fail)]
fn test_merkle_proof_wrong_leaf() {
    let leaves: [Field; 64] = create_test_leaves();
    let root = merkle::compute_merkle_root(leaves);
    
    let leaf_index = 10;
    let wrong_leaf = leaves[leaf_index] + 1;  // Tampered
    let proof = merkle::generate_proof(leaves, leaf_index);
    
    // Should fail
    assert(merkle::verify_proof(wrong_leaf, leaf_index, proof, root));
}
```


### Pattern 6: Regression Tests

Add tests for bugs that were found and fixed:

```noir
// Regression test for issue #123
// Bug: Wheel straight (A-2-3-4-5) was not being recognized
#[test]
fn test_regression_wheel_straight_detection() {
    let wheel = [12, 0, 1, 2, 3, 40, 41];
    let score = evaluate_hand_rank(wheel);
    
    // Should be categorized as straight (category 4), not high card
    let category = score >> 20;
    assert(category == 4, "Wheel should be recognized as straight");
}

// Regression test for issue #124  
// Bug: Duplicate detection missed duplicates at indices 0 and 51
#[test(should_fail)]
fn test_regression_duplicate_at_boundaries() {
    let mut deck: [Field; 52] = [0; 52];
    for i in 0..52 {
        deck[i] = i as Field;
    }
    deck[51] = 0;  // Duplicate of deck[0]
    
    assert_all_unique(deck);
}
```

---


## Quick Reference

### Test Commands

```bash
# Run all tests in current package
nargo test

# Run specific test
nargo test test_name

# Show output from tests
nargo test --show-output

# Get circuit info (constraint counts)
nargo info

# Compile without running
nargo compile
```

### Test Attributes

```noir
#[test]                              // Basic test
#[test(should_fail)]                 // Test expected to fail
#[test(should_fail_with = "msg")]    // Test expected to fail with specific message
```

### Common Assertions

```noir
assert(condition)                     // Basic assertion
assert(a == b)                        // Equality
assert(a != b)                        // Inequality
assert(a < b)                         // Less than
assert(a > b)                         // Greater than
assert(a <= b)                        // Less than or equal
assert(a >= b)                        // Greater than or equal
assert(condition, "error message")    // With custom message
```


### Test Checklist

When writing tests, ensure you cover:

- [ ] **Happy path**: Normal, expected inputs
- [ ] **Boundary values**: Min/max valid inputs
- [ ] **Invalid inputs**: Values that should be rejected
- [ ] **Zero/empty cases**: Zero values, empty arrays
- [ ] **Duplicates**: Uniqueness constraints
- [ ] **Ordering**: Relative comparisons
- [ ] **Invariants**: Properties that must always hold
- [ ] **Roundtrips**: Encode/decode consistency
- [ ] **Edge cases**: Special cases (wheel straight, etc.)
- [ ] **Performance**: Constraint count is reasonable

---

## Examples from Stellar Poker

### Example 1: Card Validation Tests

From `circuits/lib/src/cards.nr`:

```noir
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
    let pair_hand = evaluate_hand_rank([12, 25, 0, 1, 3, 5, 7]);
    let high_card = evaluate_hand_rank([11, 23, 0, 1, 3, 5, 7]);
    assert(pair_hand > high_card);
}

// Additional comprehensive edge case tests for showdown_valid circuit:

#[test]
fn test_flush_beats_straight() {
    // Flush (all hearts): 2♥, 3♥, 5♥, 7♥, 9♥, K♦, Q♠
    let flush = evaluate_hand_rank([13, 26, 32, 45, 48, 9, 22]);
    // Straight (K-Q-J-10-9): K♠, Q♠, J♠, 10♣, 9♣, 2♥, 3♥
    let straight = evaluate_hand_rank([11, 24, 37, 8, 21, 0, 1]);
    assert(flush > straight);
}

#[test]
fn test_identical_hands() {
    // Two players with the same exact cards should have equal score
    let hand1 = evaluate_hand_rank([12, 25, 38, 0, 1, 2, 3]);
    let hand2 = evaluate_hand_rank([12, 25, 38, 0, 1, 2, 3]);
    assert(hand1 == hand2);
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
```

### Example 2: Commitment Tests

From `circuits/lib/src/commitments.nr`:

```noir
#[test]
fn test_card_commitment_roundtrip() {
    let card: Field = 42;
    let salt: Field = 123456789;
    let commitment = commit_card(card, salt);

    let commitment2 = commit_card(card, salt);
    assert(commitment == commitment2);

    let commitment3 = commit_card(card, 987654321);
    assert(commitment != commitment3);
}

#[test]
fn test_hand_commitment() {
    let c1_commit = commit_card(10, 111);
    let c2_commit = commit_card(25, 222);
    let hand = commit_hand(c1_commit, c2_commit);

    let hand2 = commit_hand(c1_commit, c2_commit);
    assert(hand == hand2);
}
```

---

## Additional Resources

### Official Noir Documentation
- [Noir Language Documentation](https://noir-lang.org/)
- [Noir Testing Guide](https://noir-lang.org/docs/dev/testing/)
- [Nargo CLI Reference](https://noir-lang.org/docs/dev/cli/)

### Project-Specific Resources
- [Circuit Benchmarks](../circuits/BENCHMARKS.md) - Performance metrics for circuits
- [Compile Circuits Script](../scripts/compile-circuits.sh) - How circuits are built
- [CI Workflow](../.github/workflows/ci.yml) - How tests run in CI

### Community Resources
- [Noir Discord](https://discord.gg/noir-lang)
- [Noir GitHub Discussions](https://github.com/noir-lang/noir/discussions)

---

## Troubleshooting

### Common Issues

**1. Test fails with "Constraint failed"**
- Check the line number in the error message
- Verify your assertions are correct
- Add debug output (in unconstrained functions)

**2. Tests pass locally but fail in CI**
- Ensure you're using the same Noir version as CI
- Check for platform-specific behavior
- Verify dependencies are correctly specified

**3. Compiler version mismatch**
```bash
# Use exact version from CI
noirup -v 1.0.0-beta.17
```

**4. Out of memory during testing**
- Reduce test data size
- Split large tests into smaller ones
- Check for memory leaks in loops

**5. Tests are too slow**
- Minimize constraint-heavy operations
- Use simpler test data
- Profile with `nargo info`

---

## Contributing Tests

When contributing tests to Stellar Poker:

1. **Follow existing patterns** - Match the style of tests in the same module
2. **Test public APIs** - Focus on exported functions
3. **Document complex tests** - Add comments explaining non-obvious scenarios
4. **Run locally first** - Ensure `nargo test` passes before committing
5. **Update this guide** - Add new patterns or insights you discover

---


*Last updated: 2026-06-25*
*Noir version: 1.0.0-beta.17*
