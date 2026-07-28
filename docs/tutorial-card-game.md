# Developer Tutorial: Building a Simple Card Game with `stellar-zk-cards`

This tutorial provides a complete walkthrough for building a zero-knowledge card game on the Stellar network using the `stellar-zk-cards` library and Soroban smart contracts.

---

## 1. Overview & Architecture

`stellar-zk-cards` allows developers to build mental poker and card games with provable randomness and zero-knowledge card dealing.

Key components:
- **Card Encoding & Shuffling**: Mental poker masking via ECC / Poseidon commitments.
- **Hand Evaluation**: Fast, deterministic rank computation in Rust / Soroban environment.
- **Commitments & Verification**: Zero-knowledge proof verification for hidden card values.

---

## 2. Adding `stellar-zk-cards` to Your Crate

Add `stellar-zk-cards` to your `Cargo.toml`:

```toml
[dependencies]
stellar-zk-cards = { version = "0.1.0", path = "../stellar-zk-cards" }
soroban-sdk = "20.0.0"
```

---

## 3. Encoding a Deck and Shuffling

```rust
use stellar_zk_cards::{Card, Deck, PoseidonCommitment};

pub fn initialize_deck() -> Deck {
    // Generate standard 52-card deck encoded as 0..52
    let deck = Deck::standard_52();
    
    // Each card is represented as a 64-bit integer or field element
    println!("Deck created with {} cards", deck.len());
    deck
}
```

---

## 4. Evaluating Poker Hands

Evaluate hole cards and community cards to determine hand rankings:

```rust
use stellar_zk_cards::evaluator::{evaluate_hand, HandRank};

pub fn check_winning_hand(hole_cards: &[Card; 2], community: &[Card; 5]) -> HandRank {
    let mut all_cards = Vec::new();
    all_cards.extend_from_slice(hole_cards);
    all_cards.extend_from_slice(community);
    
    let rank = evaluate_hand(&all_cards);
    println!("Evaluated hand rank: {:?}", rank);
    rank
}
```

---

## 5. Verifying Poseidon Commitments

To deal cards privately without revealing values until showdown:

```rust
use stellar_zk_cards::crypto::verify_poseidon_commitment;

pub fn verify_card_reveal(
    card_value: u8,
    blinding_factor: [u8; 32],
    expected_commitment: [u8; 32],
) -> bool {
    let valid = verify_poseidon_commitment(card_value, &blinding_factor, &expected_commitment);
    assert!(valid, "Invalid card reveal commitment!");
    valid
}
```

---

## 6. Minimal Soroban Smart Contract Example

Here is a minimal Soroban smart contract demonstrating on-chain table initialization and hand validation:

```rust
#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Env, Symbol, Vec};
use stellar_zk_cards::soroban::verify_card_proof;

const TABLE_STATE: Symbol = symbol_short!("STATE");

#[contract]
pub struct ZkCardGameContract;

#[contractimpl]
impl ZkCardGameContract {
    /// Initialize a new card game table
    pub fn init_game(env: Env, player_count: u32) {
        env.storage().instance().set(&TABLE_STATE, &player_count);
    }

    /// Verify private card deal proof submitted by a player
    pub fn verify_deal(env: Env, card_id: u32, proof_bytes: Vec<u8>) -> bool {
        let is_valid = verify_card_proof(&env, card_id, &proof_bytes);
        is_valid
    }
}
```

---

## Summary & Next Steps

With `stellar-zk-cards`, you can construct secure, decentralized card games on Stellar without relying on a trusted central dealer.

For further reference, check:
- `stellar-zk-cards` crate documentation
- [Soroban Smart Contract Documentation](https://soroban.stellar.org)
