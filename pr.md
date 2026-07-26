# Pull Request: Documentation — Issues #271, #272, #273, #276

## Summary

- **#273** Added `docs/circuit-optimization-cookbook.md`: a practical guide on reducing gate counts in Noir circuits, covering lookup tables, bit decomposition tricks, avoiding witness recomputation, circuit parallelization, and memory management. Includes a constraint cost reference table and a workflow checklist tied to the existing `constraint-budgets.json` and CI benchmark workflow.

- **#272** Created `docs/adr/` with five Architecture Decision Records documenting the major design choices made for Stellar Poker:
  - **ADR-001**: ZK + MPC (coSNARKs) over pure MPC or pure ZK — explains why neither approach alone is sufficient and how they complement each other.
  - **ADR-002**: UltraHonk (Barretenberg) as the proving system — covers why Groth16, PLONK, and STARKs were rejected and how Soroban's Protocol 25/26 host functions make UltraHonk verification economically viable.
  - **ADR-003**: Soroban over EVM — explains the role of Stellar Protocol 25 BN254 host functions in making UltraHonk on-chain verification tractable.
  - **ADR-004**: TACEO coNoir as the MPC framework — compares MP-SPDZ, SCALE-MAMBA, and a from-scratch REP3 implementation; documents the trust assumption around the coordinator's `split-input` step.
  - **ADR-005**: 3-node REP3 committee topology — covers why 2-of-2 and 3-of-5 were rejected and the security/liveness properties of the chosen configuration.

- **#276** Added `docs/local-committee-dev-guide.md`: a step-by-step developer guide for spinning up a full 3-node MPC committee locally. Covers prerequisite installation, CRS download, circuit compilation, node key generation, Stellar local network startup, contract deployment, committee registration, starting all three nodes and the coordinator, running a test hand end-to-end with `scripts/test-flow.py`, the Docker Compose alternative, and a troubleshooting section. Includes a port reference table.

- **#271** Added `docs/soroban-budget-profiling.md`: a guide on profiling Soroban CPU/memory budget usage. Covers the `simulateTransaction` RPC method (via Stellar CLI `--simulate` flag and raw JSON-RPC), interpreting the `cpuInsns`/`memBytes` output, reading `ResourceLimitExceeded` log messages from the coordinator's retry logic, contract-level optimization techniques (ledger read batching, VK caching, `instance` vs `persistent` storage), setting instruction leeway on programmatic transactions, and a CI regression guard shell script.

## Test plan

- [ ] Verify `docs/adr/README.md` index links resolve to the correct ADR files.
- [ ] Follow `docs/local-committee-dev-guide.md` on a clean machine and confirm each step produces the documented output.
- [ ] Run `stellar contract invoke --simulate` for each circuit type on a local Stellar node and confirm the format matches what `docs/soroban-budget-profiling.md` describes.
- [ ] Check that `circuits/constraint-budgets.json` thresholds referenced in `docs/circuit-optimization-cookbook.md` are still current against `circuits/BENCHMARKS.md`.
- [ ] Confirm no broken relative links in the new docs (e.g., `docs/soroban-budget-profiling.md` links to `docs/circuit-optimization-cookbook.md`).
