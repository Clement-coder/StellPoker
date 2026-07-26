# Coordinator API Versioning Strategy

Issue #107. This document defines how the coordinator's HTTP API is
versioned, how breaking changes are rolled out, and how coordinator releases
line up with the on-chain `contracts/` and the Noir `circuits/` they depend
on.

This is distinct from `docs/api/` (which publishes a snapshot of the OpenAPI
*spec document* per git tag for browsing in Swagger UI). This document
covers the *runtime* contract: how a client picks which API version it
talks to, and what changes are allowed within a version.

## Scheme: URL prefix

The coordinator versions its HTTP API with a URL path prefix:

```
/api/v1/tables/create
/api/v1/table/42/request-deal
/api/v1/health
```

`v1` is the only version today. The unversioned form (`/api/tables/create`,
etc.) keeps working and is treated as an alias for `v1` — the coordinator
rewrites `/api/v1/<rest>` to `/api/<rest>` internally
(`services/coordinator/src/api_version.rs`) before routing, so both forms
are served by the same handlers. There is no behavioral difference between
`/api/v1/...` and `/api/...` today; the unversioned form exists only for
backward compatibility with existing clients (the frontend, MPC nodes)
written before this scheme was introduced.

Every response to an `/api/...` request carries:

- `X-API-Version: v1` — the version that served the request.

Every response to the **unversioned** form additionally carries:

- `Deprecation: true`
- `Link: </api/v1>; rel="successor-version"`

New clients should call `/api/v1/...` directly. Existing clients should
migrate to the versioned prefix at their own pace; see the deprecation
policy below for when the unversioned alias is actually removed.

### Why URL prefix over `Accept` header

Both are legitimate. URL-prefix was chosen because:

- It is trivially cacheable, loggable, and debuggable from a URL alone
  (matches this coordinator's existing per-route Prometheus metrics and
  request logging, which are keyed by path).
- It requires no content negotiation logic in the MPC nodes or frontend,
  which are simple HTTP clients.
- It composes cleanly with the existing reverse-proxy / load-balancer setup
  described in `docs/production-config.md` (path-based routing rules are
  simpler to operate than header-based ones).

## Breaking vs. non-breaking changes

Non-breaking (allowed within `v1`, no version bump required):
- Adding a new endpoint.
- Adding a new optional request field.
- Adding a new response field.
- Adding a new enum variant to a field that is documented as open-ended.

Breaking (requires a new version prefix, e.g. `v2`):
- Removing or renaming an endpoint, request field, or response field.
- Changing a field's type or semantics.
- Changing a success/error status code for an existing scenario.
- Tightening request validation in a way that rejects previously-accepted
  requests.

When a breaking change is needed, the new behavior is introduced under
`/api/v2/...` while `/api/v1/...` keeps its existing behavior until the
deprecation window (below) elapses.

## Deprecation policy

1. A version (or the unversioned alias) is marked deprecated by adding the
   `Deprecation: true` response header (already in place for the
   unversioned alias) and a `CHANGELOG.md` entry.
2. Deprecated versions are supported for **at least 2 minor coordinator
   releases or 90 days, whichever is longer**, before removal.
3. Removal is itself a breaking change: it ships in a release noted in
   `CHANGELOG.md` under "Removed", and the coordinator's `Cargo.toml`
   version is bumped accordingly (see `docs/api/README.md` for how that
   feeds the published OpenAPI docs).
4. MPC nodes and the frontend (`app/`) are updated to the new prefix in the
   same PR that introduces it wherever feasible, so the reference clients
   never depend on a deprecated version.

## Compatibility matrix: coordinator ↔ contracts ↔ circuits

The coordinator's API version is independent from the wire compatibility
between the coordinator, the Soroban contracts, and the Noir circuits — the
three must still agree on the proof format either way. A proof is only
accepted if:
- the circuit ACIR in `circuits/<circuit>/` matches what the MPC nodes were
  built against (`CIRCUIT_DIR` / `circuit_dir` in `MpcConfig`), and
- the on-chain `zk-verifier` / `poker-table` contracts implement the
  matching UltraHonk verification format that
  `services/coordinator/src/soroban/proofs.rs::convert_keccak_proof_to_soroban`
  produces.

| Coordinator API | Coordinator crate | `contracts/*` | `circuits/*` | Compatible |
|---|---|---|---|---|
| `v1` | 0.1.0 | 0.1.0 | (Protocol 25/26 UltraHonk format, see `circuits/BENCHMARKS.md`) | Yes — current release train |

Update this table whenever any of the four columns changes independently
(e.g. a contract-only fix that doesn't touch the coordinator still gets a
row if it changes the accepted proof format). Until a `v2` API or a
breaking proof-format change ships, this is the only supported
combination — mixing a newer coordinator with older deployed contracts (or
vice versa) is not tested and not supported.
