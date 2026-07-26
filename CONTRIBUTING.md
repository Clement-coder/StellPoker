# Contributing to Stellar Poker

Thank you for your interest in contributing. This document explains how to get started.

## Development Setup

```bash
./scripts/setup.sh
```

See the [README](README.md) for full prerequisites.

## Project Structure

| Directory | Description |
|---|---|
| `contracts/` | Soroban smart contracts (Rust) |
| `circuits/` | Noir ZK circuits |
| `stellar-zk-cards/` | Reusable card-game library crate |
| `services/coordinator/` | MPC session orchestrator (Axum) |
| `services/node/` | MPC node (TACEO coNoir participant) |
| `app/` | Next.js frontend |
| `scripts/` | Build, deploy, and test scripts |

## Workflow

1. Fork the repository and create a branch from `main`.
2. Make your changes. Run the relevant tests before opening a PR.
3. Open a pull request with a clear description of what changed and why.

## Running Tests

**Soroban contracts:**
```bash
cargo test -p poker-table
```

**Noir circuits:**
```bash
cd circuits/lib && nargo test
```

**Integration tests:**
```bash
./scripts/test-flow.py
```

**Frontend:**
```bash
cd app && npm test
```

## Code Style

- Rust: `cargo fmt` and `cargo clippy --all-targets`
- TypeScript: the project uses ESLint (`cd app && npm run lint`)
- Noir: follow the existing module structure in `circuits/lib/`

## Local Stack (docker-compose)

```bash
docker-compose up
```

Each service (`soroban`, `mpc-node-0/1/2`, `coordinator`) defines a `healthcheck`,
and the `coordinator` only starts once Soroban and all three MPC nodes report
healthy (`depends_on: condition: service_healthy`). Each node also has a
`start_period` grace window (60s for MPC nodes to cover co-noir key generation
and CRS load, 30s for Soroban) during which a failing check does **not** count
against the retry budget. Check status with:

```bash
docker-compose ps
```

### Diagnosing unhealthy services

`docker-compose ps` shows a `STATUS` of `healthy`, `health: starting`, or
`unhealthy` for each service. To investigate one that won't go healthy:

```bash
# See the most recent health-check probes (exit code + captured output).
docker inspect --format '{{json .State.Health}}' stellpoker-mpc-node-0-1 | jq

# Watch health transitions live across the whole stack.
docker events --filter event=health_status

# Tail a service's own logs for the underlying error (panic, bind conflict, …).
docker-compose logs -f mpc-node-0
```

Each health check is wrapped so that a failed probe writes a
`WARN: <service> health check failed` line to the health log (visible in the
`docker inspect` output above), making failures easy to grep. If a service is
stuck in `health: starting` past its `start_period`, the underlying process is
almost certainly still initializing or crash-looping — check its logs.

If you're running services directly instead of via docker-compose, use
`./scripts/start-local.sh` — it polls each node's `/health` endpoint (and the
coordinator's `/api/health`) and won't print the ready message until every
service actually responds:

```
=== Stack is ready — open http://localhost:3000 ===
```

### Common Startup Errors

**`co-noir not found`**
`./scripts/start-local.sh` requires the `co-noir` MPC binary on your `PATH`.
Install it with:
```bash
cargo install --git https://github.com/TaceoLabs/co-snarks --branch main co-noir
```

**Circuit not compiled**
If you see `ERROR: Circuit <name> not compiled`, run
`./scripts/compile-circuits.sh` (or let `start-local.sh` do it automatically —
set `SKIP_CIRCUIT_COMPILE=1` only if you've already compiled them).

**MPC node never becomes healthy / `start-local.sh` times out waiting**
This almost always means key generation hasn't finished or the CRS is
missing. Check:
- `./scripts/download-crs.sh` was run and `crs/` is populated.
- The node's stdout/stderr (printed inline by `start-local.sh`, or
  `docker-compose logs mpc-node-0`) for a panic or bind-address conflict.
- Ports `8101-8103` and `10000-10002` aren't already in use by a previous run
  (`pkill -f mpc-node` or `docker-compose down` to clean up stale processes).

**Coordinator fails health check, MPC nodes are healthy**
The coordinator depends on all three MPC nodes and Soroban being healthy
first (`depends_on: condition: service_healthy` in `docker-compose.yml`). If
it's still failing after the nodes are up, check `SOROBAN_RPC` is reachable
from inside the container — `http://soroban:8000` (not `localhost`) when using
docker-compose, and `${SOROBAN_RPC}` from `.env.local` when running locally.

**`No .env.local found — Soroban submission disabled`**
Run `./scripts/deploy-local.sh` first; it deploys the contracts and writes
the `.env.local` that `start-local.sh` and the coordinator read.

## MPC Node Discovery

The coordinator selects its MPC nodes via `select_mpc_nodes`, which picks a
source in this precedence order:

1. **On-chain committee registry** — used when `COMMITTEE_REGISTRY_CONTRACT` is
   configured.
2. **Static endpoints (backward compatible):** `MPC_NODE_0`, `MPC_NODE_1`,
   `MPC_NODE_2` (as `docker-compose.yml` sets). Used when the registry is not
   configured but these env vars are explicitly set.
3. **Dynamic discovery:** when neither of the above is configured, MPC nodes
   register themselves at runtime and sessions run on a healthy subset
   (minimum 3 healthy, else `503 Service Unavailable`).

The dynamic-discovery endpoints below are active only in case (3); when the
committee registry or static `MPC_NODE_*` endpoints are configured they return
`409 Conflict`:

```
POST   /api/node/register        {"id": "0", "endpoint": "http://node-0:8101"}
POST   /api/node/{id}/heartbeat  # keep registration alive (re-register also works)
DELETE /api/node/{id}            # graceful deregister on shutdown
```

A node is considered unhealthy once it has gone 30s without a heartbeat. Check
the active committee any time with `GET /api/committee/status`.

## Git Blame and Formatting Commits

This repository keeps a `.git-blame-ignore-revs` file that lists commits that
were purely formatting or refactoring changes (e.g. `cargo fmt`, `prettier`,
whitespace fixes). Configuring git to use it means `git blame` skips those
commits and points straight to the last meaningful change.

**One-time setup (per clone):**

```bash
git config blame.ignoreRevsFile .git-blame-ignore-revs
```

After that, `git blame` and editor integrations that rely on it (GitLens, etc.)
will automatically ignore formatting-only commits.

**Adding a formatting commit to the file:**

If your PR is a pure formatting/refactoring change, add its full SHA to
`.git-blame-ignore-revs` with a short comment explaining what it was:

```
# 2024-xx-xx — run cargo fmt across workspace
abcdef1234567890abcdef1234567890abcdef12
```

The SHA must be the full 40-character commit hash (available after merging).

## Reporting Issues

Open a GitHub issue with steps to reproduce, expected behaviour, and actual behaviour.

## License

By contributing you agree that your contributions will be licensed under the [MIT License](LICENSE).
