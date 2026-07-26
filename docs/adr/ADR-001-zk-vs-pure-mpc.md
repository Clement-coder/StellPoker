# ADR-001: ZK Proofs + MPC over Pure MPC

**Status**: Accepted

---

## Context

A private card game on a public blockchain requires two guarantees simultaneously:

1. **Privacy** — no single party (player, operator, or observer) learns another player's hole cards during play.
2. **Verifiability** — the on-chain settlement contract must be able to confirm that the deal was fair, reveals are correct, and the declared winner actually holds the best hand.

Two broad approaches exist:

- **Pure MPC**: the committee collectively holds and operates on secret-shared card values. The committee submits the final result (winner, pot amount) to the chain. The chain trusts the committee.
- **ZK + MPC (coSNARKs)**: the committee performs the same MPC computation but also generates a ZK proof that the computation was performed correctly. The chain verifies the proof trustlessly.

---

## Decision

Use **ZK proofs combined with MPC** (coSNARKs via TACEO coNoir).

---

## Options Considered

### Option A: Pure MPC, committee-signed result

The three nodes collectively compute the outcome and submit a multi-signed result. The contract verifies a threshold signature.

**Pros:**
- Simpler implementation — no ZK circuit development required.
- Faster proving time (no proof generation).
- Easier to upgrade game logic without recompiling circuits.

**Cons:**
- The on-chain contract must trust the committee majority. If 2 of 3 nodes collude, they can submit an arbitrary result and steal funds.
- No way for an observer (or auditor) to verify that a specific hand was dealt fairly without access to the MPC node logs.
- Weaker security model: trust-the-committee rather than verify-the-math.

### Option B: ZK proofs for all operations, no MPC

A single centralized prover knows the full deck and generates ZK proofs for each operation.

**Pros:**
- Simpler infrastructure — one prover, no MPC coordination.
- Faster proof generation (no network round-trips between MPC nodes).

**Cons:**
- The prover necessarily knows every player's hole cards at deal time, breaking privacy. A corrupt or compromised prover can share that information.
- ZK proves *correctness*, not *secrecy*. A single-prover system cannot guarantee that the prover did not leak private inputs.

### Option C: ZK + MPC (coSNARKs) — **chosen**

The three MPC nodes secret-share the deck using REP3. No single node ever holds the plaintext deck. The collaborative MPC computation also generates a distributed ZK proof (coSNARK). The on-chain contract verifies the proof without trusting any individual node.

**Pros:**
- Privacy holds as long as any 2 of 3 nodes are honest (standard REP3 guarantee).
- The contract verifies correctness cryptographically — no trust in the committee is required for honest-game guarantees.
- Auditable: every hand's proof is stored on-chain and can be independently verified.

**Cons:**
- Significantly more complex: circuits must be written in Noir, and MPC coordination adds latency (~10–30 seconds per proof for the showdown circuit).
- Circuit changes require recompiling and updating the on-chain verification key.
- coSNARK tooling (TACEO coNoir) is young and may change.

---

## Consequences

- Three Noir circuits (`deal_valid`, `reveal_board_valid`, `showdown_valid`) define the on-chain rules. Changing game logic requires updating circuits and redeploying the verifier contract.
- The `zk-verifier` Soroban contract stores verification keys and verifies UltraHonk proofs using Stellar's native BN254 host functions.
- Latency for the MPC+ZK round trip is a known UX trade-off. The frontend shows a "waiting for proof" state. Solo mode uses a single prover and is faster.
- The security model is: any 2-of-3 MPC nodes can collude on privacy (they learn the deck), but *no* set of nodes can forge a valid proof — the contract will reject it.
