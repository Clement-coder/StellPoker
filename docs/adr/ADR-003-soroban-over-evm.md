# ADR-003: Soroban (Stellar) over EVM

**Status**: Accepted

---

## Context

Stellar Poker requires an on-chain settlement layer that can:

1. **Verify UltraHonk ZK proofs** natively and cheaply — BN254 pairing and elliptic-curve operations must be available as precompiles or host functions.
2. **Escrow player funds** trustlessly and execute deterministic payout logic.
3. **Manage a committee registry** — registration, staking, slashing, epoch transitions.
4. **Run within practical gas/resource limits** — UltraHonk verification on EVM costs millions of gas with pure-Solidity implementations.

---

## Decision

Deploy all smart contracts on **Soroban (Stellar)**.

---

## Options Considered

### Option A: EVM (Ethereum or L2)

The most widely deployed smart contract platform.

**Pros:**
- Largest existing developer ecosystem and tooling.
- EIP-196/197 provides BN254 `ecAdd`, `ecMul`, and pairing precompiles. These are sufficient for Groth16 verification.
- Large existing liquidity and user base.

**Cons:**
- EIP-196/197 precompiles cover BN254 operations needed for Groth16, but UltraHonk (Shplemini opening scheme) requires multi-scalar multiplication and Poseidon2, which are not in the precompile set. A pure-Solidity UltraHonk verifier for a circuit with 237 000 gates costs on the order of 20–100 million gas — far exceeding block gas limits for an inline call, and expensive even as an off-chain simulation.
- Ethereum L1 transaction fees during high demand would make per-hand proof submission prohibitively expensive for a poker game.
- L2 solutions (Optimism, Arbitrum) reduce gas costs but introduce additional trust assumptions (sequencer centralization) and complicate the bridge for player funds.

### Option B: Solana (with native BN254 precompiles)

Solana has introduced `syscalls` for BN254 curve operations in recent versions.

**Pros:**
- Low transaction fees.
- High throughput.
- BN254 syscalls would enable efficient UltraHonk verification.

**Cons:**
- BN254 syscalls are relatively new and less battle-tested.
- Solana's account model and program architecture require significant rework compared to Soroban's contract model.
- The team has no Solana expertise, and the project was started in response to a Stellar-specific grant / hackathon requirement.

### Option C: Soroban (Stellar) — **chosen**

Stellar's Soroban smart contract platform, introduced with Protocol 22 and significantly enhanced in Protocols 25 and 26.

**Pros:**
- **Protocol 25 (X-Ray)** added native Soroban host functions for BN254 operations and Poseidon2 hashing. These are hardware-accelerated precompiles, not Rust implementations running in WASM. UltraHonk verification cost drops from intractable to within budget.
- **Protocol 26 (Yardstick)** added multi-scalar multiplication and scalar-field arithmetic — exactly the operations required by UltraHonk's Shplemini opening scheme.
- Soroban transactions have a flat, predictable resource budget (CPU instructions, memory, ledger entries) rather than variable gas pricing. This makes fee estimation for proof submission reliable.
- Stellar's 5-second ledger close time is well-matched to a poker game's turn cadence.
- Stellar's built-in asset primitives make player buy-in escrow and pot payout straightforward without needing ERC-20 mechanics.
- The project was built for and submitted to the Stellar hackathon; Soroban was the target platform.

**Cons:**
- Smaller developer community and fewer audited primitives than EVM.
- Soroban is newer and the host function API may change between protocol versions (Protocol 25 → 26 already added new operations required by Shplemini).
- Less DeFi liquidity; players need Stellar-native assets or bridge solutions for large stakes.
- Tooling (Stellar CLI, soroban-sdk) is maturing but less polished than Foundry/Hardhat.

---

## Consequences

- All contracts (`poker-table`, `zk-verifier`, `committee-registry`, `game-hub`) are written in Rust using `soroban-sdk 22.0.0`.
- The `zk-verifier` contract uses `ultrahonk-soroban-verifier` (vendored in `vendor/`), which calls Soroban host functions directly.
- Protocol 25 and 26 are minimum requirements. Deploying on an older protocol version will fail at verification time.
- Contract upgrades require the deployer account to call `upgrade` with a new WASM hash. The deployer key must be kept secure; losing it means no future upgrades.
- Transaction budget profiling (see `docs/soroban-budget-profiling.md`) is critical before mainnet deployment to ensure verification fits within ledger limits under all circuit sizes.
