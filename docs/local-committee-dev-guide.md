# Local Committee Development Guide

Step-by-step walkthrough for running a full 3-node MPC committee on your laptop for development and testing. By the end you will have:

- Three co-noir MPC nodes running locally
- A coordinator orchestrating them
- A Stellar local network with all contracts deployed
- A test hand running end-to-end

**Estimated time**: 20–30 minutes on first run (CRS download dominates); ~5 minutes on subsequent runs.

---

## Prerequisites

Install the following before starting:

```bash
# Rust (stable toolchain)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Nargo (Noir compiler) – pin to the version used by this project
noirup -v 1.0.0-beta.17

# Stellar CLI
cargo install stellar-cli --features opt

# TACEO coNoir CLI
cargo install --git https://github.com/TaceoLabs/co-snarks --branch main co-noir

# Docker and Docker Compose (for the Stellar local node)
# Install via your OS package manager; Docker Desktop works on Mac/Windows.

# OpenSSL (usually pre-installed on Linux/macOS)
openssl version
```

Verify the tools are available:

```bash
nargo --version     # should print 1.0.0-beta.17
stellar --version
co-noir --version
docker compose version
```

---

## Step 1: Clone and Build

```bash
git clone https://github.com/HitEmPoka/StellPoker.git
cd StellPoker

# Build all Rust crates (contracts, coordinator, node)
cargo build
```

---

## Step 2: Download the BN254 Common Reference String

The CRS is required for proof generation. It is ~1 GB and is not committed to the repository.

```bash
./scripts/download-crs.sh
```

This downloads the CRS into `./crs/`. The script is idempotent — re-running it skips the download if the file is already present.

---

## Step 3: Compile the Noir Circuits

```bash
./scripts/compile-circuits.sh
```

This runs `nargo compile` for each circuit under `circuits/` and places the compiled artifacts in `circuits/*/target/`. The artifacts are referenced at runtime by the coordinator and node services.

Verify:

```bash
ls circuits/deal_valid/target/
# deal_valid.json  deal_valid.gz
```

---

## Step 4: Generate Node Keys

Each MPC node needs a private TLS key and a self-signed certificate for mTLS peer authentication. The `setup-dkg.sh` script generates all three sets of credentials and writes the party configuration TOML files.

```bash
./scripts/setup-dkg.sh
```

**What this script does:**

1. Creates `services/node/data/` if it does not exist.
2. For each party (0, 1, 2):
   - Generates a P-256 ECDSA private key: `services/node/data/keyN.der`
   - Generates a self-signed certificate: `services/node/data/certN.der`
3. Writes `services/node/config/local/party_N.toml` for each node, referencing the generated key and certificate paths and listing all three peers.

Verify:

```bash
ls services/node/data/
# cert0.der  cert1.der  cert2.der  key0.der  key1.der  key2.der

cat services/node/config/local/party_0.toml
```

Expected `party_0.toml`:

```toml
[network]
my_id = 0
bind_addr = "0.0.0.0:10000"
key_path = "services/node/data/key0.der"
max_frame_length = 469762056

[[network.parties]]
id = 0
dns_name = "127.0.0.1:10000"
cert_path = "services/node/data/cert0.der"

[[network.parties]]
id = 1
dns_name = "127.0.0.1:10001"
cert_path = "services/node/data/cert1.der"

[[network.parties]]
id = 2
dns_name = "127.0.0.1:10002"
cert_path = "services/node/data/cert2.der"
```

---

## Step 5: Start the Stellar Local Network

```bash
docker compose up -d soroban
```

Wait for the node to be healthy:

```bash
docker compose ps
# soroban   running (healthy)
```

The Soroban RPC will be available at `http://localhost:8000/soroban/rpc`.

---

## Step 6: Deploy Contracts

```bash
./scripts/deploy-local.sh
```

This script:

1. Compiles all Soroban contracts (`cargo build --release` for each contract crate).
2. Deploys `poker-table`, `zk-verifier`, `committee-registry`, and `game-hub` to the local Stellar node.
3. Uploads verification keys for all three circuits to the `zk-verifier` contract.
4. Writes contract addresses to `.env.local`.

Verify:

```bash
cat .env.local
# POKER_TABLE_CONTRACT=C...
# ZK_VERIFIER_CONTRACT=C...
# COMMITTEE_REGISTRY_CONTRACT=C...
```

Source the environment:

```bash
source .env.local
```

---

## Step 7: Register the Committee

The `setup-dkg.sh` script from Step 4 also handles on-chain registration when `COMMITTEE_REGISTRY_CONTRACT` is set in the environment. If you sourced `.env.local` before running it, registration happened automatically. Otherwise, run it again:

```bash
source .env.local
./scripts/setup-dkg.sh
```

To verify registration:

```bash
stellar contract invoke \
  --id "$COMMITTEE_REGISTRY_CONTRACT" \
  --source committee-local \
  --network local \
  -- get_current_epoch
```

Expected output:

```json
{
  "epoch_id": 1,
  "members": ["GB...", "GD...", "GC..."],
  "threshold": 2,
  "start_ledger": 125,
  "end_ledger": 0
}
```

---

## Step 8: Start the Three MPC Nodes

Open three terminal tabs or use `tmux`. Each node binds to a different port pair:

| Node | HTTP port | MPC P2P port | Config |
|------|-----------|--------------|--------|
| 0 | 8101 | 10000 | `party_0.toml` |
| 1 | 8102 | 10001 | `party_1.toml` |
| 2 | 8103 | 10002 | `party_2.toml` |

```bash
# Terminal 1 — Node 0
NODE_ID=0 PORT=8101 MPC_PORT=10000 \
  PARTY_CONFIG="services/node/config/local/party_0.toml" \
  CIRCUIT_DIR="./circuits" CRS_DIR="./crs" \
  cargo run -p mpc-node

# Terminal 2 — Node 1
NODE_ID=1 PORT=8102 MPC_PORT=10001 \
  PARTY_CONFIG="services/node/config/local/party_1.toml" \
  CIRCUIT_DIR="./circuits" CRS_DIR="./crs" \
  cargo run -p mpc-node

# Terminal 3 — Node 2
NODE_ID=2 PORT=8103 MPC_PORT=10002 \
  PARTY_CONFIG="services/node/config/local/party_2.toml" \
  CIRCUIT_DIR="./circuits" CRS_DIR="./crs" \
  cargo run -p mpc-node
```

Wait for each node to print something like:

```
Listening on 0.0.0.0:8101
CRS loaded from ./crs
```

Health check:

```bash
curl -s http://localhost:8101/health
curl -s http://localhost:8102/health
curl -s http://localhost:8103/health
# {"status":"ok"} × 3
```

---

## Step 9: Start the Coordinator

The coordinator orchestrates MPC sessions and exposes the API consumed by the frontend.

```bash
source .env.local

MPC_NODE_0="http://localhost:8101" \
MPC_NODE_1="http://localhost:8102" \
MPC_NODE_2="http://localhost:8103" \
SOROBAN_RPC="http://localhost:8000/soroban/rpc" \
CIRCUIT_DIR="./circuits" \
CRS_DIR="./crs" \
BIND_ADDR="0.0.0.0:8080" \
cargo run -p coordinator
```

Verify the committee is visible to the coordinator:

```bash
curl -s http://localhost:8080/api/committee/status
# {"nodes":3,"healthy":[true,true,true],"status":"active"}
```

---

## Step 10: Run a Test Hand

Use the integration test script to trigger a full hand end-to-end (deal → betting → reveal → showdown) and verify on-chain settlement:

```bash
# Requires: Stellar local node, 3 MPC nodes, and coordinator all running.
python3 scripts/test-flow.py
```

The script:

1. Creates a poker table on-chain.
2. Joins two test players.
3. Triggers `start_hand` → the coordinator deals via MPC, generates a `deal_valid` proof, and submits it to Soroban.
4. Simulates betting actions.
5. Reveals community cards with a `reveal_board_valid` proof.
6. Resolves the showdown with a `showdown_valid` proof.
7. Asserts that pot settlement matches the expected winner.

A successful run prints:

```
[PASS] deal proof verified on-chain
[PASS] reveal proof verified on-chain
[PASS] showdown proof verified on-chain
[PASS] pot settled to winner
```

---

## Step 11 (Optional): Docker Compose All-in-One

If you prefer not to manage separate terminals, Docker Compose starts the full stack automatically:

```bash
docker compose up
```

This builds and starts all three MPC nodes, the coordinator, and the Soroban local node in dependency order. The coordinator waits for all node health checks before starting.

Note: first run includes a Docker build step which takes several minutes.

---

## Troubleshooting

### Node fails to start: "key file not found"

Run `./scripts/setup-dkg.sh` to regenerate TLS credentials. Ensure the `key_path` in `party_N.toml` matches the actual location of `keyN.der`.

### Coordinator reports nodes unhealthy

Check that all three node processes are running and their health endpoints return `{"status":"ok"}`. If a node exited, check its output for TLS handshake errors (see `docs/committee-setup.md` for certificate troubleshooting).

### `deploy-local.sh` fails: "account not found"

Stellar quickstart's Friendbot must be reachable. Wait for the soroban container to be fully healthy (`docker compose ps` shows `healthy`) before running the deploy script.

### Proof generation times out

The `showdown_valid` circuit has ~237 000 backend gates and can take 30–60 seconds on a laptop. The coordinator has a default 120-second timeout for MPC sessions. If your machine is slow, set:

```bash
MPC_PROVE_TIMEOUT_SECS=300 cargo run -p coordinator
```

### `ResourceLimitExceeded` on contract invocation

The local Stellar node applies default resource limits. The coordinator automatically retries with increasing `--instruction-leeway` values (0, 50M, 200M, 500M). If all retries fail, the local network may need a restart: `docker compose restart soroban`.

---

## Port Reference

| Service | Port | Protocol |
|---------|------|----------|
| Stellar local node | 8000 | HTTP (Soroban RPC + Horizon) |
| MPC Node 0 (HTTP) | 8101 | HTTP |
| MPC Node 1 (HTTP) | 8102 | HTTP |
| MPC Node 2 (HTTP) | 8103 | HTTP |
| MPC Node 0 (P2P) | 10000 | mTLS TCP |
| MPC Node 1 (P2P) | 10001 | mTLS TCP |
| MPC Node 2 (P2P) | 10002 | mTLS TCP |
| Coordinator | 8080 | HTTP |
| Frontend (dev) | 3000 | HTTP |
