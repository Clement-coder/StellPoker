# Noir Circuit Constraint Optimization Cookbook

Reference guide for reducing gate counts in Stellar Poker's Noir circuits, targeting the UltraHonk (Barretenberg) backend on BN254.

---

## 1. Measure Before You Optimize

Always baseline before touching anything:

```bash
# ACIR opcode count (fast, no backend needed)
nargo info --program-dir circuits/deal_valid

# Backend gate count (requires Barretenberg)
bb gates --scheme ultra_honk \
  --bytecode_path circuits/deal_valid/target/deal_valid.json

# Regression thresholds are in circuits/constraint-budgets.json
```

The `constraint-budgets.json` file enforces per-circuit ceilings in CI. When a planned increase is accepted, update the budget there; otherwise CI will catch regressions automatically.

---

## 2. Lookup Tables

UltraHonk natively supports custom lookup tables (Plookup-style). Use them for operations whose arithmetic encoding is expensive but whose input domain is small.

### When to use

| Operation | Arithmetic cost | Lookup cost | Use lookup? |
|-----------|----------------|-------------|-------------|
| Range check 0–15 (4-bit) | ~4 gates | 1 gate | Yes |
| Card validity (0–51) | ~6 gates | 1 gate | Yes |
| Suit extraction (card / 13) | ~8 gates | 1 gate | Yes |
| Poseidon2 round constants | N/A (built-in) | built-in | Use Poseidon2 |
| Arbitrary multiplication | 1 gate | N/A | No |

### Noir example: range-checked card index

```noir
// Without lookup: manually range-checked
fn assert_valid_card_naive(card: u32) {
    assert(card < 52);
}

// With lookup: declare a table and constrain via it
// (Noir ≥1.0 syntax; Barretenberg compiles u8 casts to ROM lookups)
fn assert_valid_card(card: u32) {
    let _: u8 = card as u8; // implicitly range-checks 0..255
    assert(card < 52);       // further narrows to card domain
}
```

For hand-rank lookups (the showdown circuit), consider packing the 5-card combination index into a lookup table instead of re-evaluating the poker rank arithmetic:

```noir
// Expensive: full arithmetic evaluation per combination
let rank = evaluate_hand_arithmetic(c0, c1, c2, c3, c4);

// Cheaper: pack cards into a canonical key and look up pre-computed rank
let key = pack_hand_key(c0, c1, c2, c3, c4); // sort + pack into ~26 bits
let rank = lookup_hand_rank(key);             // single Plookup gate
```

The `showdown_valid` circuit accounts for ~90% of the project's constraint budget (237 018 backend gates) primarily because of 7-choose-5 hand evaluation. A lookup table for the rank function would be the single highest-impact optimization available.

---

## 3. Bit Decomposition Tricks

Arithmetic over the BN254 scalar field is cheap, but operations that require knowing individual bits (comparisons, conditional selection, shuffled-array proofs) are expensive unless decomposed carefully.

### Prefer field arithmetic over bit arrays

```noir
// Slow: decompose x into bits and check
let bits = x.to_le_bits(32);
let is_zero = bits.all(|b| b == 0);

// Fast: single constraint
let is_zero = x == 0;
```

### Conditional select without branching

```noir
// Expensive: Noir `if` over witnesses generates ~3 extra constraints
let result = if condition { a } else { b };

// Efficient: single multiplication
// result = condition * a + (1 - condition) * b
fn sel(condition: Field, a: Field, b: Field) -> Field {
    condition * a + (Field::from(1) - condition) * b
}
```

### Safe integer ranges

Declaring a value as `u32` instead of `Field` instructs Barretenberg to range-check it to 32 bits. Use the narrowest integer type that covers your domain:

```noir
// 52 cards fit in u6 (0–63); using u32 wastes range-check width
// Noir doesn't have u6, but you can assert manually
let card: u8 = raw_card as u8; // 8-bit range check (smallest available)
assert(card < 52);
```

### Decompose once, reuse everywhere

If a value needs bit decomposition in multiple places, compute it once and pass the bits array as a parameter rather than re-decomposing:

```noir
fn check_hand(card: u32) -> (u32, u32) {
    // Decompose once
    let rank = card % 13;
    let suit = card / 13;
    (rank, suit)
}
```

---

## 4. Avoiding Witness Recomputation

In MPC mode (coNoir REP3), witness generation runs collaboratively across all three nodes. Any computation that can be moved to the prover/witness phase rather than the constraint phase reduces the per-party communication cost.

### Move derivations outside the circuit

The deck derivation in `deal_valid` uses three `party_permutation` and three `party_salts` arrays. The XOR/addition to combine them is done inside the circuit. If the combination step were done off-circuit and only the combined result were used as a private input, it would reduce ACIR opcodes but break the security property (no single party would hold the combined deck). Do not move that step out.

What *can* be moved out: deterministic computations on public inputs.

```noir
// Inside circuit: wastes constraints on public-only arithmetic
let double_root = deck_root * 2; // this is just for illustration

// Better: compute outside and pass as additional public input,
// or accept that it is cheap enough not to matter
```

### Cache intermediates with `let`

Noir's compiler performs some CSE (common subexpression elimination), but explicit `let` bindings help:

```noir
// Potentially re-computed internally
fn bad(deck: [Field; 52], salts: [Field; 52]) {
    for i in 0..52 {
        leaves[i] = commit_card(deck[i], salts[i]);
        other[i]  = commit_card(deck[i], salts[i]); // re-computes
    }
}

// Explicit binding – single computation
fn good(deck: [Field; 52], salts: [Field; 52]) {
    for i in 0..52 {
        let c = commit_card(deck[i], salts[i]);
        leaves[i] = c;
        other[i]  = c;
    }
}
```

### Avoid redundant Merkle re-hashes

`deal_valid`, `reveal_board_valid`, and `showdown_valid` all recompute the full deck Merkle root from `party_salts` and `party_permutation`. If the deck root is already verified in one circuit, subsequent circuits do not need to re-derive it from scratch — they can accept it as a public input and verify only that the revealed cards hash to leaf positions in that root. The current circuit design intentionally re-derives to keep each proof self-contained; this is the right security trade-off, but the cost is visible in the gate counts.

---

## 5. Circuit Parallelization

UltraHonk proof generation is CPU-bound and single-threaded per circuit instance. Parallelism in this project operates at a different level.

### Multiple independent sub-circuits

If a circuit contains logically independent sub-computations (e.g., evaluating each player's 7-card hand independently), split them into separate circuits and prove each in parallel:

```
showdown_valid (monolithic, 237 018 gates)
         ↓  (hypothetical split)
hand_eval_p0 + hand_eval_p1 + ... + hand_eval_p5   (parallel)
         ↓
winner_select (cheap aggregation, ~5 000 gates)
```

Each sub-proof can be generated on a separate CPU core or node. The aggregation circuit accepts the sub-proof public outputs as private inputs and checks consistency. This is future work for the showdown circuit, which accounts for most of the proving time.

### Parallel witness generation in coNoir

coNoir REP3 parallelizes witness generation across the three MPC nodes. Each node independently computes its share of the witness; network round-trips synchronize only the secret-shared values. To reduce round-trip count:

1. **Batch permutation shares**: send all 52 permutation shares in a single message rather than one at a time.
2. **Overlap Merkle hash and card assignment**: these are independent computations and can proceed concurrently.
3. **Avoid sequential dependencies**: restructure code so that loop iterations in the circuit do not have data dependencies on previous iterations, allowing the MPC nodes to partially parallelize their per-share computations.

### Hardware concurrency

When running nodes locally via Docker Compose, allocate at least 2 CPUs to each `mpc-node-*` container to allow the Barretenberg prover to use its internal thread pool:

```yaml
# docker-compose.yml (development override)
mpc-node-0:
  deploy:
    resources:
      limits:
        cpus: '2'
```

---

## 6. Memory Management in Noir

Noir programs run inside a Barretenberg WASM or native binary. Large arrays are the main source of memory pressure.

### Fixed-size arrays vs dynamic allocation

Noir only supports fixed-size arrays. This means all arrays must be declared at their maximum size, and unused elements must be handled with sentinel values or masks:

```noir
global MAX_PLAYERS: u32 = 6;

// Always 6 elements; fill unused seats with 0
let mut hand_commitments: [Field; MAX_PLAYERS] = [0; MAX_PLAYERS];
for p in 0..MAX_PLAYERS {
    if p < num_players {
        hand_commitments[p] = compute_commitment(p);
    }
    // unused slots remain 0
}
```

This means `MAX_PLAYERS` has a direct linear effect on circuit size. Reducing it from 6 to 4 would cut the `deal_valid` and `reveal_board_valid` constraint counts proportionally.

### Avoid large intermediate arrays

Temporary arrays inside functions are allocated for the full circuit lifetime. Prefer computing intermediates in-place:

```noir
// Memory-heavy: two 52-element intermediate arrays
let combined_perms = combine_perms(p0, p1, p2);   // 52 Fields
let combined_salts = combine_salts(s0, s1, s2);   // 52 Fields
let deck = apply_permutation(combined_perms);

// Lighter: compute combined values inline without materialization
// (depends on whether the compiler can inline; check gate count)
```

### Merkle tree sizing

The Merkle tree in `deal_valid` uses a padded leaf array of size 64 (next power of 2 above 52). Leaves 52–63 are zeroed. This is necessary for the tree to have a consistent height. Do not reduce this without also updating the tree depth constant and the on-chain verifier's public input expectations.

---

## 7. Quick Reference: Constraint Cost by Operation

Approximate UltraHonk gate costs for common Noir operations (BN254 scalar field):

| Operation | Approx. gates |
|-----------|:---:|
| Field addition / subtraction | 1 |
| Field multiplication | 1 |
| Range check (u8) | 1–2 |
| Range check (u32) | 3–4 |
| Equality assertion | 1 |
| Poseidon2 hash (2 inputs) | ~30 |
| Pedersen commitment | ~100 |
| Array index (dynamic) | log2(n) |
| 32-bit integer division | ~50 |
| Bit decomposition (32 bits) | ~32 |

> These are rough estimates. Always measure with `bb gates` after any significant change.

---

## 8. Workflow Summary

1. Run `nargo info` and `bb gates` to baseline the current gate counts.
2. Identify the most expensive function with profiling (not yet automated — add `println!` or split the circuit into sub-functions and measure individually).
3. Apply the relevant technique from sections 2–6.
4. Re-run `bb gates` and compare against `circuits/constraint-budgets.json`.
5. If the change increases thresholds intentionally, update the budget JSON and document the reason in the commit message.
6. CI (`circuit-benchmarks.yml`) will catch regressions on every push.
