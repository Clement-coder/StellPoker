# Stellar Poker

[![CI](https://github.com/HitEmPoka/StellPoker/actions/workflows/ci.yml/badge.svg)](https://github.com/HitEmPoka/StellPoker/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Onchain Texas Hold'em poker on Stellar with cryptographically private cards using ZK-MPC (coSNARKs).

No single party ever sees the full deck. A 3-node MPC committee (TACEO coNoir) shuffles and deals cards using REP3 secret sharing. UltraHonk ZK proofs verify every deal, reveal, and showdown onchain via Soroban's native BN254 host functions.

**[Live Demo](https://stellar-poker-six.vercel.app)** · [Demo Video](#) · [Slide Deck](https://www.canva.com/design/DAHB5JrdEAk/XThK1QgbEATHwZ0rX-W2aA/view?utm_content=DAHB5JrdEAk&utm_campaign=designshare&utm_medium=link2&utm_source=uniquelinks&utlId=hb4aca74548)

![Gameplay](assets/game.png)

## Stellar Protocol 25 & 26

Stellar Poker is built directly on top of the cryptographic primitives introduced in Stellar's recent protocol upgrades:

- **Protocol 25 (X-Ray)** introduced native BN254 elliptic-curve operations and Poseidon2 hashing as Soroban host functions. The `zk-verifier` contract uses these to verify UltraHonk proofs onchain at a fraction of the cost of a pure-Rust implementation.
- **Protocol 26 (Yardstick)** added multi-scalar multiplication, scalar-field arithmetic, and curve-membership checks. These are used inside the verifier's Shplemini opening scheme, which is the most compute-intensive part of UltraHonk verification.

Without these host functions, onchain UltraHonk verification would exceed Soroban's instruction budget. Protocol 25 and 26 are what make this project possible.

## Why ZK alone is not enough

ZK proofs can verify that a computation was done correctly, but they cannot keep inputs secret from the prover. In a card game, the entity generating the proof would necessarily know all cards. REP3 secret sharing across multiple MPC nodes ensures no single party — including the coordinator — ever holds the full deck. The slide deck above covers this in detail.

## Architecture

```
Player A          Player B
   |                  |
   +------+  +--------+
          |  |
       [Web App]              Next.js frontend
          |
       [Coordinator]          Orchestrates MPC sessions (Axum)
       /    |    \
   [Node0] [Node1] [Node2]    TACEO coNoir MPC nodes (REP3)
          |
       [Soroban]              Onchain settlement
    /      |        \
[PokerTable] [ZKVerifier] [CommitteeRegistry]
```

Supports up to 6 players. Includes a solo mode against a deterministic AI opponent.

## Key Properties

- **Private cards** — Cards exist only as REP3 secret shares across 3 MPC nodes. Privacy holds as long as at least 2 nodes are honest.
- **ZK-verified** — Deal, reveal, and showdown proofs are UltraHonk proofs verified onchain via Soroban's native BN254 host functions (Protocol 25).
- **Trustless settlement** — All bets, pot calculation, and payouts are handled entirely in Soroban smart contracts.
- **Reusable library** — `stellar-zk-cards` is a standalone Rust crate for card encoding and hand evaluation that any Soroban app can use.
- **On-chain hand history** — the last 16 settled hands per table are archived in a circular buffer, readable by anyone. See [docs/poker-table-hand-history.md](docs/poker-table-hand-history.md).
- **Partial buy-ins** — players top their stack up between hands for any amount inside the table's buy-in band, with a configurable per-session rebuy limit. See [docs/poker-table-rebuys.md](docs/poker-table-rebuys.md).
- **Multi-table play** — a wallet may sit at any number of tables at once; submissions are sequenced per account so parallel actions never collide. See [docs/multi-table-play.md](docs/multi-table-play.md).
- **Node fee sharing** — rake is split among the active MPC nodes in proportion to stake. See [docs/committee-fee-distribution.md](docs/committee-fee-distribution.md).

## Contracts on Testnet

| Contract | Address |
|---|---|---|
| Poker Table | [CA3RAB66WJ3OONO4OFYEATISRZA3ND65MN3DET5IE2XXSZZMPKH3CHAV](https://stellar.expert/explorer/testnet/contract/CA3RAB66WJ3OONO4OFYEATISRZA3ND65MN3DET5IE2XXSZZMPKH3CHAV) |
| Committee Registry | [GBTYELEQ2YZH2W6SXLHT4AX6TYBHHU7LNNPKJV7J37VS3S5GPA75KRDU](https://stellar.expert/explorer/testnet/account/GBTYELEQ2YZH2W6SXLHT4AX6TYBHHU7LNNPKJV7J37VS3S5GPA75KRDU) |

Works alongside the Stellar Game Studio deployed at [CB4VZAT2U3UC6XFK3N23SKRF2NDCMP3QHJYMCHHFMZO7MRQO6DQ2EMYG](https://stellar.expert/explorer/testnet/contract/CB4VZAT2U3UC6XFK3N23SKRF2NDCMP3QHJYMCHHFMZO7MRQO6DQ2EMYG).

## Repository Structure

```
stellar-poker/
  contracts/
    poker-table/        Main game contract (betting, state machine, settlement)
    zk-verifier/        UltraHonk proof verification (BN254 native ops)
    committee-registry/ MPC committee management and slashing
    game-hub/           Mock Game Hub contract (Stellar Game Studio interface)
  circuits/
    lib/                Shared Noir library (cards, commitments, Merkle)
    deal_valid/         Proves deck shuffle + deal consistency
    reveal_board_valid/ Proves community card reveals match committed deck
    showdown_valid/     Proves winner has the best hand
  stellar-zk-cards/     Reusable card-game library crate (encoding, hand eval)
  services/
    coordinator/        Axum HTTP server orchestrating MPC sessions
    node/               MPC node (TACEO coNoir participant)
  app/                  Next.js web frontend
  tests/                Integration tests
  vendor/               Vendored UltraHonk verifier
  scripts/              Build, deploy, and test scripts
  docker-compose.yml    Full-stack local development
```

## Tech Stack

| Component | Technology |
|---|---|
| Smart contracts | Soroban (Rust, soroban-sdk 22.0.0) |
| ZK proofs | Noir circuits + UltraHonk (Barretenberg) |
| MPC | TACEO coNoir (REP3, 3-party) |
| Hash function | Poseidon2 |
| Frontend | Next.js 15, Freighter wallet |

## Prerequisites

- Rust (stable)
- Nargo 1.0.0-beta.17 — `noirup -v 1.0.0-beta.17`
- Node.js 18+
- Docker
- Stellar CLI — `cargo install stellar-cli --features opt`
- co-noir (for CRS download) — `cargo install --git https://github.com/TaceoLabs/co-snarks co-noir`

## Quick Start

```bash
# Install dependencies and verify build
./scripts/setup.sh

# Download BN254 common reference string
./scripts/download-crs.sh

# Start full stack
docker-compose up
```

Then open `http://localhost:3000`.

## Development

```bash
# Check all Rust crates
cargo check

# Run contract tests
cargo test -p poker-table

# Compile and test Noir circuits
./scripts/compile-circuits.sh
cd circuits/lib && nargo test

# Run the frontend
cd app && npm run dev

# Run integration tests (requires docker-compose up)
python3 scripts/test-flow.py
```

## Circuit Benchmarking

The Noir circuits are benchmarked for constraint count, proof size, and proof generation time.

### Constraint Metrics

View circuit constraint counts and proof sizes:

```bash
# Compile all circuits
./scripts/compile-circuits.sh

# Display human-readable metrics
nargo info --program-dir circuits/deal_valid
nargo info --program-dir circuits/reveal_board_valid
nargo info --program-dir circuits/showdown_valid
nargo info --program-dir circuits/muck_valid

# Extract JSON metrics (machine-parseable)
nargo info --json --program-dir circuits/deal_valid
```

Backend gate counts (UltraHonk circuit size):

```bash
bb gates --scheme ultra_honk --bytecode_path circuits/deal_valid/target/deal_valid.json
```

### Proof Generation Time

Proof generation time is measured for each circuit at varying player counts (2-6 players):

| Circuit          | Players | Proof Time (ms) | Backend Opcodes |
|------------------|---------|-----------------|-----------------|
| `deal_valid`     | 2-6     | ~50-60          | 25 117          |
| `reveal_board_valid` | 2-6 | ~45-55          | 32 792          |
| `showdown_valid` | 2-6     | ~150-180        | 237 018         |
| `muck_valid`     | 2-6     | ~40-50          | ~15 000         |

**Hardware Specifications (Benchmark Reference):**
- CPU: Intel Xeon Platinum 8375C @ 2.90 GHz
- RAM: 16 GB
- OS: Ubuntu 22.04.5 LTS (Linux 6.8.0-1014-azure)
- Noir Version: 1.0.0-beta.17
- Backend: Barretenberg UltraHonk (BN254 scalar field)

**Note:** Proof generation time varies with hardware, node count, and MPC communication latency. Measurements should be taken on consistent hardware for regression detection. The CI workflow (`.github/workflows/circuit-benchmarks.yml`) automatically validates constraint budgets on every circuit change.

## Deploy to Testnet

```bash
NETWORK=testnet ./scripts/deploy.sh
```

## Staging Environment

Staging mirrors production with a dedicated coordinator, three MPC nodes, and a staging frontend, while still settling against **Soroban testnet**.

```bash
cp .env.staging.example .env.staging
# fill in staging contracts, keys, identities, and public URLs
NETWORK=staging OUTPUT_ENV_FILE=.env.staging.deploy ./scripts/deploy.sh
# copy the generated contract IDs from .env.staging.deploy into .env.staging
./scripts/deploy-staging.sh
```

That flow starts the staging services and runs the integration suite against staging before production promotion. The staging runbook lives in [docs/staging-environment.md](docs/staging-environment.md).

## Deploy to Mainnet

### Pre-flight checklist

Before deploying to mainnet, complete every item below:

1. **Fund deployer account** — The deployer account needs at least ~100 XLM to cover contract deployment fees and minimum reserve. Export the secret key:
   ```bash
   export DEPLOYER_SECRET=S...   # secret key of your funded Stellar account
   ```
2. **Download the CRS** — The BN254 Common Reference String must be present on every machine that generates proofs (coordinator and all MPC nodes):
   ```bash
   ./scripts/download-crs.sh
   ```
3. **Complete the MPC committee key ceremony** — Each of the three node operators must generate their REP3 key share independently. Key shares must never reside on the same machine. Coordinate this out-of-band before registering the committee onchain.
4. **Compile circuits** — Verification keys embedded in the `zk-verifier` contract must match the compiled circuit artifacts:
   ```bash
   ./scripts/compile-circuits.sh
   ```
5. **Provision MPC node infrastructure** — All three nodes must be running and reachable at their public endpoints before `register_member` calls are made.
6. **Back up the deployer key** — Contract admin operations (upgrades, key rotation) require the deployer secret. Store it securely offline.

### Deploy

```bash
export DEPLOYER_SECRET=S...      # funded mainnet account
NETWORK=mainnet ./scripts/deploy.sh
```

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `NETWORK` | `testnet` | `testnet` or `mainnet` |
| `SOROBAN_RPC` | auto | Soroban RPC URL (auto-selected per network) |
| `SOROBAN_NETWORK_PASSPHRASE` | auto | Stellar network passphrase (auto-selected per network) |
| `DEPLOYER_SECRET` | — | **Required for mainnet.** Secret key (`S...`) of the funded deployer account. |

For production infrastructure, including a self-hosted Soroban RPC stack with Stellar Core, Horizon, tuning, and monitoring guidance, see [docs/soroban-rpc-node.md](docs/soroban-rpc-node.md).

### Mainnet contract addresses

> Mainnet deployment has not been performed yet. This table will be updated once contracts are deployed to the Stellar public network.

| Contract | Address |
|---|---|
| Poker Table | — |
| ZK Verifier | — |
| Committee Registry | — |

## Game Flow

1. **Create table** — Admin creates a `PokerTable` contract with config (blinds, buy-in range, timeout).
2. **Join** — Players join with a buy-in (tokens escrowed in contract).
3. **Start hand** — Triggers the MPC committee to shuffle and deal.
4. **Deal** — Committee generates a `deal_valid` ZK proof, commits deck Merkle root + hand commitments onchain, privately delivers hole cards to each player.
5. **Betting** — Players submit actions (fold / check / call / bet / raise / all-in) to the contract.
6. **Reveal** — After each betting round, committee reveals community cards with a `reveal_board_valid` proof.
7. **Showdown** — Committee reveals remaining hands, generates a `showdown_valid` proof, contract settles the pot.

## Circuits

### `deal_valid`
- **Private**: `deck[52]`, `salts[52]` (secret-shared in MPC)
- **Public**: `deck_root`, `hand_commitments[6]`, `dealt_indices`
- **Proves**: Valid 52-card deck, Merkle root matches commitments, hand commitments match dealt cards.

### `reveal_board_valid`
- **Private**: `deck[52]`, `salts[52]`
- **Public**: `deck_root`, `revealed_cards`, `revealed_indices`, `previously_used_indices`
- **Proves**: Revealed cards match committed deck, no indices reused.

### `showdown_valid`
- **Private**: `hole_cards`, `board_cards`, `salts`
- **Public**: `hand_commitments`, `board_commitments`, `declared_winner`
- **Proves**: Cards match commitments, hand evaluation is correct, winner has best hand.

## What is live vs mocked in the demo

The live demo at [stellar-poker-six.vercel.app](https://stellar-poker-six.vercel.app) runs against Stellar testnet. Here is exactly what is live and what is simulated:

| Component | Status | Notes |
|---|---|---|
| Soroban contracts | ✅ Live on testnet | Poker table, ZK verifier, committee registry all deployed |
| ZK proof verification | ✅ Live onchain | `zk-verifier` contract verifies real UltraHonk proofs via BN254 host functions |
| Frontend | ✅ Live | Hosted on Vercel, connects to testnet via Freighter wallet |
| Solo mode (vs AI) | ✅ Fully functional | Deals, betting, showdown, and settlement all work end-to-end |
| MPC nodes (multiplayer) | ⚠️ Demo infrastructure | 3 TACEO coNoir nodes run for demo purposes; in production these would be independently operated |
| Multiplayer (2–6 players) | ✅ Functional | Requires all players to have Freighter and testnet XLM |

The ZK proofs in solo mode are generated and verified onchain in every hand — this is not mocked.

## Beyond poker: generalising the pattern

The technical patterns in this project are directly applicable to real-world use cases:

- **MPC committee + ZK verification** — the same architecture works for any multi-party secret: sealed-bid auctions, private voting, threshold key custody, blind RFQ matching in DeFi.
- **`stellar-zk-cards`** — the card encoding, commitment, and hand evaluation library is a standalone Rust crate any Soroban app can use for any card or tile-based game.
- **Onchain UltraHonk verifier** — the `zk-verifier` contract is a general-purpose Noir proof verifier. Any Noir circuit can be verified through it by swapping the verification key.
- **REP3 secret sharing via coNoir** — the node and coordinator services are structured to be reused for any coSNARK application, not just poker.

## License

[MIT](LICENSE)

## Feature Flags

StellPoker includes a built-in feature-flag system for gradual rollouts. No
third-party service is required — flags are stored in memory and controlled
through environment variables and a runtime admin API.

### Available flags

| Flag key            | Env variable                      | Purpose                                  |
|---------------------|-----------------------------------|------------------------------------------|
| `new_circuits`      | `FEATURE_FLAG_NEW_CIRCUITS`       | Enable experimental ZK circuit versions  |
| `contract_upgrade`  | `FEATURE_FLAG_CONTRACT_UPGRADE`   | Gate new Soroban contract function calls |
| `experimental_ui`   | `FEATURE_FLAG_EXPERIMENTAL_UI`    | Signal the frontend to use new UI        |
| `chat_enabled`      | `FEATURE_FLAG_CHAT_ENABLED`       | Enable in-table WebSocket chat           |
| `solo_mode`         | `FEATURE_FLAG_SOLO_MODE`          | Allow solo / bot-opponent table creation |

### Setting flags at startup

Set any flag via an environment variable before starting the coordinator:

```bash
FEATURE_FLAG_SOLO_MODE=1 FEATURE_FLAG_CHAT_ENABLED=1 cargo run -p coordinator
```

Accepted truthy values: `1`, `true`, `yes`, `on`. Everything else is `false`.

### Per-table and per-player overrides

Append `_TABLE_<id>` or `_PLAYER_<stellar-address>` to scope an override:

```bash
# Enable chat only on table 3
FEATURE_FLAG_CHAT_ENABLED_TABLE_3=1

# Give one specific player access to experimental UI
FEATURE_FLAG_EXPERIMENTAL_UI_PLAYER_GABC...=1
```

Resolution order: **per-player → per-table → global** (most-specific wins).

### Runtime API

The coordinator exposes two endpoints for live flag management without a restart:

| Method | Path                  | Description                                |
|--------|-----------------------|--------------------------------------------|
| `GET`  | `/api/flags`          | List all current flag values               |
| `POST` | `/api/flags/:key`     | Set a flag (`{"enabled": true\|false}`) |

```bash
# Check current flags
curl http://localhost:8080/api/flags

# Enable solo_mode at runtime
curl -X POST http://localhost:8080/api/flags/solo_mode \
     -H 'Content-Type: application/json' \
     -d '{"enabled": true}'

# Enable chat only for table 5 (scoped key)
curl -X POST http://localhost:8080/api/flags/chat_enabled.table.5 \
     -H 'Content-Type: application/json' \
     -d '{"enabled": true}'
```

### Frontend usage

```tsx
import { useFeatureFlags, isFlagEnabled } from "@/lib/use-feature-flags";

export function CreateTableButton() {
  const { flags, loading } = useFeatureFlags();

  if (loading || !isFlagEnabled(flags, "solo_mode")) return null;

  return <button>Create Solo Table</button>;
}
```

### Documentation & Guides

- [Developer Tutorial: Building Card Games with `stellar-zk-cards`](docs/tutorial-card-game.md)
- [Noir Testing Guide](docs/NOIR_TESTING_GUIDE.md)
- [Local Committee Development Guide](docs/local-committee-dev-guide.md)
- [Soroban Storage Optimization](docs/soroban-storage-optimization.md)

