# ADR-004: coNoir (TACEO) as the MPC Framework

**Status**: Accepted

---

## Context

The MPC layer must enable three nodes to collaboratively generate Noir circuit witnesses and UltraHonk proofs without any single node seeing the full deck. The framework must:

1. Implement a secret-sharing scheme that is compatible with UltraHonk's BN254 scalar field.
2. Support collaborative proof generation (coSNARK), not just collaborative witness generation.
3. Have a working Rust/CLI integration that can be embedded into an Axum node service.
4. Be open-source and auditable.

---

## Decision

Use **TACEO coNoir** with REP3 (replicated 3-party) secret sharing.

---

## Options Considered

### Option A: MP-SPDZ

A well-established research MPC framework supporting many protocols.

**Pros:**
- Mature, extensively studied, large number of published protocols.
- Supports many secret-sharing schemes including SPDZ, Shamir, and replicated.

**Cons:**
- Written in C++ with a domain-specific language (MAMBA). Integration with Noir circuits would require transpiling ACIR to MAMBA, which does not exist.
- Not designed for ZK proof generation. There is no coSNARK implementation in MP-SPDZ.
- Python-heavy tooling; Rust integration is non-trivial.

### Option B: SCALE-MAMBA

Another research MPC framework, primarily academic.

**Pros:**
- Supports Shamir secret sharing and honest-majority protocols.

**Cons:**
- Same integration problems as MP-SPDZ: no Noir/ACIR/ZK support.
- No active production use cases; limited community support.

### Option C: Manual REP3 implementation

Implement replicated 3-party secret sharing from scratch over BN254 in Rust.

**Pros:**
- Full control over protocol details.
- No dependency on third-party MPC library.

**Cons:**
- Extremely high implementation complexity and risk. REP3 over a prime field with correct share distribution, reconstruction, and multiplication triple generation requires careful cryptographic engineering.
- No existing coSNARK implementation — collaborative proof generation would need to be written from scratch against Barretenberg's internals.
- Not feasible within a hackathon or small team timeline.

### Option D: TACEO coNoir — **chosen**

An open-source framework from TACEO GmbH specifically designed for collaborative Noir proof generation using REP3 secret sharing over BN254.

**Pros:**
- **Purpose-built for Noir + UltraHonk**: coNoir directly consumes Nargo-compiled ACIR bytecode and generates collaborative UltraHonk proofs. There is zero impedance mismatch between the Noir circuits and the MPC layer.
- **REP3 over BN254**: the secret-sharing scheme operates natively in the BN254 scalar field, matching the field used by Noir and UltraHonk. No field conversion or wrapping required.
- **CLI-based integration**: the `co-noir` binary can be invoked as a subprocess from the Rust node service, making integration straightforward without needing to link against MPC internals.
- **Well-defined input format**: `co-noir split-input` takes a TOML prover input file and produces three JSON share files — one per party. The split is the only step that requires plaintext access.
- **Active development**: TACEO maintains coNoir actively and has responded to issues in the `co-snarks` repository.

**Cons:**
- Pre-1.0 maturity: the API and configuration format have changed between releases. Pinning the version (installed via `cargo install --git`) is essential.
- Limited documentation: the coNoir README covers the basics; edge cases (large witness sizes, network timeouts) require reading source code.
- No formal security audit as of the project build date. Production deployments should await an audit.
- The `co-noir` binary must be present on each node's `$PATH`; it is not statically linked into the node service.

---

## Consequences

- Each MPC node runs two processes: the `mpc-node` Axum HTTP service and the `co-noir` CLI invoked as a subprocess.
- Collaborative proof generation requires all three nodes to be online and reachable on their MPC P2P ports (10000–10002). If any node is offline during a proving session, the session fails and must be retried.
- The proving workflow is: `split-input` (coordinator) → `merge-input` (each node) → `generate-witness` (collaborative, 3-party) → `prove` (collaborative, 3-party). Each step involves network round-trips between nodes.
- Upgrading coNoir (e.g., to a version that supports a new Nargo release) may require re-generating party configuration files. Monitor the TACEO `co-snarks` repository for breaking changes.
- Input sharing is done by the coordinator, which temporarily holds the combined plaintext for the `split-input` step. This is an acceptable trust assumption: the coordinator is operated by the game platform and cannot generate a valid proof alone (the MPC nodes must also participate), but it can learn the deck at split time. A fully trustless system would require the deck to be generated collaboratively without any entity ever holding the full plaintext — this is future work.
