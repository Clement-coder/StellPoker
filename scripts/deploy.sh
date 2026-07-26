#!/usr/bin/env bash
set -euo pipefail

# Stellar Poker - Deploy Script
# Deploys contracts to Soroban testnet or mainnet and starts services
#
# Usage:
#   NETWORK=testnet ./scripts/deploy.sh   # deploy to testnet (default)
#   NETWORK=staging ./scripts/deploy.sh   # deploy staging contracts on testnet
#   NETWORK=mainnet ./scripts/deploy.sh   # deploy to mainnet
#
# Environment variables:
#   NETWORK                    Target network: "testnet" (default), "staging", or "mainnet"
#   SOROBAN_RPC                Soroban RPC URL (auto-set per network if not provided)
#   SOROBAN_NETWORK_PASSPHRASE Stellar network passphrase (auto-set per network if not provided)
#   DEPLOYER_SECRET            (mainnet) Secret key of the funded deployer account (S...)
#                              If unset, a new key is generated (testnet only; not suitable for mainnet)
#   OUTPUT_ENV_FILE            Optional file to write deployed contract IDs into

NETWORK="${NETWORK:-testnet}"
STELLAR_NETWORK="$NETWORK"
OUTPUT_ENV_FILE="${OUTPUT_ENV_FILE:-}"

# ── Network defaults ───────────────────────────────────────────────────────────
if [[ "$NETWORK" == "staging" ]]; then
  STELLAR_NETWORK="testnet"
  SOROBAN_RPC="${SOROBAN_RPC:-https://soroban-testnet.stellar.org}"
  SOROBAN_NETWORK_PASSPHRASE="${SOROBAN_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
elif [[ "$NETWORK" == "mainnet" ]]; then
  SOROBAN_RPC="${SOROBAN_RPC:-https://mainnet.sorobanrpc.com}"
  SOROBAN_NETWORK_PASSPHRASE="${SOROBAN_NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
else
  SOROBAN_RPC="${SOROBAN_RPC:-https://soroban-testnet.stellar.org}"
  SOROBAN_NETWORK_PASSPHRASE="${SOROBAN_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
fi

echo "=== Stellar Poker Deploy ==="
echo "Network: $NETWORK"
echo "Stellar CLI network: $STELLAR_NETWORK"
echo "RPC: $SOROBAN_RPC"
echo ""

# ── Pre-flight checklist (mainnet only) ───────────────────────────────────────
if [[ "$NETWORK" == "mainnet" ]]; then
  echo "=== Mainnet Pre-flight Checklist ==="
  echo ""
  echo "Before deploying to mainnet, confirm ALL of the following:"
  echo ""
  echo "  [1] Deployer account is funded with enough XLM to cover contract deployment"
  echo "      fees and reserve (minimum ~100 XLM recommended)."
  echo "      Set DEPLOYER_SECRET to the S... secret key of that account."
  echo ""
  echo "  [2] BN254 Common Reference String (CRS) is downloaded and available."
  echo "      Run: ./scripts/download-crs.sh"
  echo "      The same CRS must be present on every MPC node."
  echo ""
  echo "  [3] MPC committee key ceremony has been completed."
  echo "      Each node operator must have generated their REP3 key share."
  echo "      Key shares must NOT be on the same machine."
  echo ""
  echo "  [4] All three MPC nodes are reachable and configured with production"
  echo "      endpoints before registering the committee onchain."
  echo ""
  echo "  [5] Noir circuits have been compiled and verification keys generated."
  echo "      Run: ./scripts/compile-circuits.sh"
  echo ""
  echo "  [6] This is a production deployment — contract upgrades require admin"
  echo "      key access. Back up the deployer secret key securely."
  echo ""

  if [[ -z "${DEPLOYER_SECRET:-}" ]]; then
    echo "ERROR: DEPLOYER_SECRET is required for mainnet deployment."
    echo "       Export the secret key of your funded deployer account:"
    echo "       export DEPLOYER_SECRET=S..."
    exit 1
  fi

  read -rp "Have you completed all checklist items above? [yes/N] " CONFIRM
  if [[ "$CONFIRM" != "yes" ]]; then
    echo "Aborted. Re-run when the checklist is complete."
    exit 1
  fi
  echo ""
fi

# Check dependencies
command -v stellar >/dev/null 2>&1 || { echo "stellar CLI not found. Install: cargo install stellar-cli"; exit 1; }

# --- Step 1: Build Soroban contracts ---
echo "=== Building Soroban contracts ==="
cargo build --release --target wasm32-unknown-unknown \
  -p poker-table \
  -p zk-verifier \
  -p committee-registry

echo "Optimizing WASM..."
for contract in poker_table zk_verifier committee_registry; do
  stellar contract optimize \
    --wasm "target/wasm32-unknown-unknown/release/${contract}.wasm" 2>/dev/null || true
done

# --- Step 2: Compile Noir circuits ---
echo ""
echo "=== Compiling Noir circuits ==="
./scripts/compile-circuits.sh

# --- Step 3: Set up deployer identity ---
echo ""
echo "=== Setting up deployer identity ==="
if [[ "$NETWORK" == "mainnet" ]]; then
  # Mainnet: import the pre-funded key provided via DEPLOYER_SECRET
  stellar keys add deployer --secret-key "$DEPLOYER_SECRET" 2>/dev/null || true
else
  # Testnet: generate and auto-fund a fresh key if one doesn't exist
  if ! stellar keys show deployer >/dev/null 2>&1; then
    stellar keys generate deployer --network "$STELLAR_NETWORK"
    echo "Funding deployer account..."
    stellar keys fund deployer --network "$STELLAR_NETWORK" || true
  fi
fi

DEPLOYER=$(stellar keys address deployer)
echo "Deployer: $DEPLOYER"

# --- Step 4: Deploy contracts ---
echo ""
echo "=== Deploying contracts ==="

echo "Deploying zk-verifier..."
ZK_VERIFIER_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/zk_verifier.wasm \
  --source deployer \
  --network "$STELLAR_NETWORK" 2>/dev/null)
echo "  ZK Verifier: $ZK_VERIFIER_ID"

echo "Deploying committee-registry..."
COMMITTEE_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/committee_registry.wasm \
  --source deployer \
  --network "$STELLAR_NETWORK" 2>/dev/null)
echo "  Committee Registry: $COMMITTEE_ID"

echo "Deploying poker-table..."
POKER_TABLE_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/poker_table.wasm \
  --source deployer \
  --network "$STELLAR_NETWORK" 2>/dev/null)
echo "  Poker Table: $POKER_TABLE_ID"

# --- Step 5: Initialize contracts ---
echo ""
echo "=== Initializing contracts ==="

stellar contract invoke \
  --id "$ZK_VERIFIER_ID" \
  --source deployer \
  --network "$STELLAR_NETWORK" \
  -- initialize --admin "$DEPLOYER" 2>/dev/null

stellar contract invoke \
  --id "$COMMITTEE_ID" \
  --source deployer \
  --network "$STELLAR_NETWORK" \
  -- initialize --admin "$DEPLOYER" 2>/dev/null

if [[ -n "$OUTPUT_ENV_FILE" ]]; then
  cat > "$OUTPUT_ENV_FILE" <<EOF
# Generated by scripts/deploy.sh on $(date -u +"%Y-%m-%dT%H:%M:%SZ")
SOROBAN_RPC=$SOROBAN_RPC
NETWORK_PASSPHRASE="$SOROBAN_NETWORK_PASSPHRASE"
POKER_TABLE_CONTRACT=$POKER_TABLE_ID
ZK_VERIFIER_CONTRACT=$ZK_VERIFIER_ID
COMMITTEE_REGISTRY_CONTRACT=$COMMITTEE_ID
EOF
  echo ""
  echo "Wrote deploy output to: $OUTPUT_ENV_FILE"
fi

echo ""
echo "=== Deploy Complete ==="
echo ""
echo "Contract Addresses:"
echo "  ZK_VERIFIER=$ZK_VERIFIER_ID"
echo "  COMMITTEE_REGISTRY=$COMMITTEE_ID"
echo "  POKER_TABLE=$POKER_TABLE_ID"
echo ""
echo "Next steps:"
echo "  1. Set verification keys: stellar contract invoke --id $ZK_VERIFIER_ID -- set_verification_key ..."
echo "  2. Register committee members: stellar contract invoke --id $COMMITTEE_ID -- register_member ..."
echo "  3. Start MPC nodes: docker-compose up mpc-node-0 mpc-node-1 mpc-node-2"
echo "  4. Start coordinator: docker-compose up coordinator"
echo "  5. Start web app: cd app && npm run dev"
