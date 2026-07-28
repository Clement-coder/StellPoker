# MPC Committee Resilience

Covers two related pieces of committee hardening:

- **5-node committee, N=5/threshold=3** (issue #95)
- **Honest-majority fault tolerance in coNoir sessions** (issue #96)

## Why the REP3 party count doesn't change

The coordinator and MPC nodes run [TACEO's co-noir](https://github.com/TaceoLabs/co-snarks)
over the **REP3** protocol, a 3-party replicated secret-sharing scheme. A
session's witness/proof generation (`services/node/src/session.rs`,
`run_proof_generation`) always merges shares from and runs co-noir with
exactly 3 parties — that's fixed by the protocol, not a config knob. The
Helm chart (`infrastructure/helm/mpc-node/values.yaml`) says this
explicitly: *"do not change replicas without a corresponding party-config
update and re-key ceremony."*

So "N=5, threshold=3" does not mean 5 nodes running one 5-party protocol.
It means: **5 provisioned committee members, of which the 3 that actually
run REP3 sessions can be swapped without a re-key ceremony** — i.e.
redundancy for roles 0/1/2, not a new protocol.

## The model

- Roles 0, 1, 2 are the active REP3 parties, same as before — same
  `party_0.toml` / `party_1.toml` / `party_2.toml`, same TLS certs.
- Two extra nodes (`mpc-node-3`, `mpc-node-4` in `docker-compose.yml`) are
  pre-provisioned **standbys**, each holding an exact copy of one active
  role's identity: `mpc-node-3` standbys for role 0, `mpc-node-4` for role
  1. They run the *same* `PARTY_CONFIG` / `NODE_ID` as their primary. They
  are not started by default (compose `standby` profile).
- On-chain, `committee-registry` can register more members than are in the
  active epoch (`scripts/setup-dkg.sh --registered-nodes 5
  --epoch-threshold 3`): the active epoch's `members` list always has the 3
  roles currently running, `threshold` documents the minimum committee size
  the operator has committed to maintaining. If a standby is promoted to
  replace a role permanently, re-run `create_epoch` with its address
  swapped in for the one it replaced.

## Promoting a standby

When a primary (role 0 or 1) is down for good — not just a transient blip,
see [Honest-majority fault tolerance](#honest-majority-fault-tolerance-in-conoir-sessions)
below for that case — run:

```bash
./scripts/promote-mpc-standby.sh 0   # or 1
```

This stops the failed primary, starts its standby under the primary's
network alias (so `MPC_NODE_0=http://mpc-node-0:8101` on the coordinator
keeps working unchanged — no coordinator config edit needed, just a
restart to drop its stale connection), and restarts the coordinator.

Roles 0 and 1 have a standby; role 2 does not in the default compose file.
Add a third standby (`mpc-node-5`, standing by for role 2) the same way if
you need it.

## Honest-majority fault tolerance in coNoir sessions

Issue #96. Two distinct failure modes, handled differently:

1. **Transient node hiccup during a session** (slow response, brief
   disconnect, one-off subprocess error): already retried in place —
   `session.rs::run_proof_generation` retries `build-and-generate-proof` up
   to 3 times on transient resource errors, and `node_reliability.rs`
   extends the coordinator's poll deadline for a node that's slow but still
   responding (issue #110), instead of failing the session outright.
2. **A committee node is down for the rest of the session** (crashed,
   unreachable): the coordinator's health check
   (`mpc::check_node_health`) detects this before/while dispatching a
   phase. `mpc::generate_proof_from_share_sets` now distinguishes this from
   a generic error — see `MpcSessionError::NodeUnavailable` — so callers in
   `api/mod.rs` surface a `409 Conflict` ("fresh deal required") instead of
   a generic `500`, telling the caller unambiguously to restart the hand
   from a fresh shuffle rather than retry the same (now-impossible) proof
   session. This matches the reality that REP3 witness/proof generation
   needs all 3 original share-holders — once one is gone mid-session,
   the only "remaining honest nodes complete the session" path is a
   replacement that has the exact same party identity, which is exactly
   what `promote-mpc-standby.sh` provisions ahead of time.
