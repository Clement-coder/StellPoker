# ADR-005: 3-Node REP3 Committee Topology

**Status**: Accepted

---

## Context

The MPC committee structure determines the security, liveness, and operational complexity of the system. Key trade-offs:

- **More nodes** → stronger privacy and fault tolerance, but more latency, more coordination complexity, and higher operational cost.
- **Fewer nodes** → faster proving, simpler operations, but weaker security guarantees.
- **Threshold** → how many nodes must collude to break privacy or how many must be online to maintain liveness.

The committee is also registered on-chain via `CommitteeRegistry`. The topology affects the on-chain slashing model and epoch management.

---

## Decision

Use a **3-node replicated secret-sharing (REP3) committee** with a quorum threshold of 2.

---

## Options Considered

### Option A: 2-of-2 (two nodes, both required)

The minimum meaningful MPC configuration.

**Pros:**
- Minimum coordination — only 2 network round-trips per proving round.
- Smallest infrastructure footprint.

**Cons:**
- No fault tolerance: if either node goes offline, the system halts.
- Privacy breaks if either node is compromised (2-of-2 means both must be honest for privacy).
- REP3 is not defined for n=2 in the standard construction.

### Option B: 3-of-5 (five nodes, 3 required for liveness)

A common threshold configuration for higher security.

**Pros:**
- Stronger privacy: 3 nodes must collude to break privacy.
- Higher fault tolerance: system survives 2 node failures simultaneously.

**Cons:**
- 5 nodes means 5x the infrastructure cost and operational complexity.
- Each proving round requires synchronization with 5 nodes, adding latency.
- REP3 is specifically defined for 3 parties. A 5-node scheme would require a different protocol (e.g., Shamir with honest-majority), which coNoir does not currently support.

### Option C: 2-of-3 REP3 (three nodes, any 2 for liveness) — **chosen**

The standard configuration for REP3 (replicated secret sharing among 3 parties).

**Pros:**
- **REP3 is natively supported by coNoir**: the entire coNoir framework is built around the 3-party REP3 construction. Using any other party count would require a different MPC protocol.
- **Privacy guarantee**: any 2 nodes must collude to reconstruct the shared secret. A single compromised or curious node learns nothing about the deck.
- **Liveness guarantee**: the system continues as long as any 2 of 3 nodes are online and honest. One node can fail or be under maintenance without halting the game.
- **Manageable operational complexity**: 3 nodes can be independently operated by 3 separate parties (e.g., the game platform, a casino partner, and a community-elected operator) to distribute trust.
- **Small on-chain footprint**: `CommitteeRegistry` stores 3 member records per epoch. This is within comfortable Soroban ledger entry limits.

**Cons:**
- Privacy is weaker than a 5-node scheme: 2 colluding nodes break privacy.
- Losing 2 nodes simultaneously halts the game (no 1-of-3 liveness).
- If REP3's honest-majority assumption is violated (2 corrupt), the system degrades to a trusted-operator model (security falls back to correctness proofs only, not privacy).

### Option D: Single prover (1 node)

No MPC — a single trusted prover generates proofs.

**Pros:**
- Fastest proving time.
- Simplest infrastructure.

**Cons:**
- The prover sees the full deck; privacy requires trusting the operator.
- This is the "solo mode" implementation, not the multiplayer security model.

---

## Node Roles and Connectivity

```
                    [ Coordinator ]
                   /      |       \
           HTTP   /   HTTP|    HTTP \
                 /        |         \
          [Node 0]   [Node 1]   [Node 2]
              \          |          /
               \  mTLS P2P links   /
                \        |        /
                 +--------+-------+
                          |
                    co-noir proving
                    (REP3 protocol)
```

Each node connects to the coordinator via HTTP (the coordinator orchestrates sessions) and to the other two nodes via mTLS TCP (the co-noir P2P protocol for share exchange).

---

## On-Chain Registry Model

The `CommitteeRegistry` contract maintains:

- A list of registered members (address, stake, endpoint, region).
- Active **epochs**: a snapshot of which members form the current committee and what the quorum threshold is.
- A **slashing mechanism**: if a node fails to submit a proof within `timeout_ledgers`, any affected player can report the timeout and receive a portion of the node's stake as compensation.

The epoch model allows rotating committee members without redeploying contracts. A new epoch can omit a faulty node and add a replacement after an out-of-band key ceremony.

---

## Consequences

- Each committee node must maintain persistent mTLS connectivity to the other two nodes. Firewall rules must permit TCP on the P2P ports (10000–10002) between all three nodes.
- The coordinator must know the HTTP endpoints of all three nodes. These are configured via `MPC_NODE_0`, `MPC_NODE_1`, `MPC_NODE_2` environment variables.
- Node operators must independently generate and safeguard their TLS private keys. Lost keys require committee epoch rotation.
- The minimum viable committee for production is 3 nodes operated by 3 independent parties. Running all 3 on the same server or under the same administrative control defeats the security model.
- During the demo phase, all 3 nodes are operated by the project team. The `README` clearly documents this limitation.
