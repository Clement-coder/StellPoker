# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for Stellar Poker.

An ADR captures a significant architectural decision: the context that led to it, the options that were considered, and the rationale for the choice made. ADRs are immutable once accepted — if a decision is reversed, a new ADR supersedes the old one rather than replacing it.

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [ADR-001](ADR-001-zk-vs-pure-mpc.md) | ZK Proofs + MPC over Pure MPC | Accepted |
| [ADR-002](ADR-002-ultrahonk-proving-system.md) | UltraHonk as the Proving System | Accepted |
| [ADR-003](ADR-003-soroban-over-evm.md) | Soroban (Stellar) over EVM | Accepted |
| [ADR-004](ADR-004-conoir-framework.md) | coNoir (TACEO) as the MPC Framework | Accepted |
| [ADR-005](ADR-005-committee-node-topology.md) | 3-Node REP3 Committee Topology | Accepted |

## Format

Each ADR follows this structure:

```
# ADR-NNN: Title

**Status**: Proposed | Accepted | Deprecated | Superseded by ADR-NNN

## Context
What is the problem or decision that needs to be made?

## Decision
What did we decide?

## Options Considered
What alternatives were evaluated?

## Consequences
What are the trade-offs?
```
