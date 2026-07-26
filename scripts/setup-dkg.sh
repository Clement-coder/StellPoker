#!/usr/bin/env bash
# Stellar Poker - MPC Committee Setup / Local DKG Setup
#
# This script automates:
# 1. Generating local TLS private keys and self-signed certificates for the 3 MPC nodes.
# 2. Writing party configuration TOML files with peer routing.
# 3. Generating Stellar node accounts (node0-local, node1-local, node2-local) via stellar CLI.
# 4. Funding these node accounts from the local network's Friendbot.
# 5. Initializing the on-chain CommitteeRegistry contract.
# 6. Registering the 3 nodes as committee members with stakes.
# 7. Creating the active committee epoch.
#
# Prerequisites:
#   - Local Stellar quickstart network running (or run ./scripts/deploy-local.sh first)
#   - stellar CLI installed
#   - openssl and curl installed
#
# Usage:
#   ./scripts/setup-dkg.sh [OPTIONS]

set -euo pipefail

# Default values
DATA_DIR="services/node/data"
CONFIG_DIR="services/node/config/local"
MIN_STAKE=1000000000 # 100 XLM in stroops
ENV_FILE=".env.local"

# Show usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --data-dir PATH     Path to store generated keys/certs (default: $DATA_DIR)"
    echo "  --config-dir PATH   Path to store TOML config files (default: $CONFIG_DIR)"
    echo "  --min-stake STROOPS Minimum stake in stroops (default: $MIN_STAKE)"
    echo "  --help              Display this message"
    exit 0
}

# Parse options
while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-dir)
            DATA_DIR="$2"
            shift 2
            ;;
        --config-dir)
            CONFIG_DIR="$2"
            shift 2
            ;;
        --min-stake)
            MIN_STAKE="$2"
            shift 2
            ;;
        --help|-h)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

echo "=== Stellar Poker MPC Committee Setup ==="
echo ""

# 1. Verify Host Prerequisites
echo "Checking prerequisites..."
command -v openssl >/dev/null 2>&1 || { echo "ERROR: openssl is required but not installed."; exit 1; }
command -v curl >/dev/null 2>&1 || { echo "ERROR: curl is required but not installed."; exit 1; }
command -v stellar >/dev/null 2>&1 || { echo "ERROR: stellar CLI is required but not installed. Install it with: cargo install stellar-cli --features opt"; exit 1; }
echo "  All host binaries found."

# 2. Source Deployed Contracts Config
if [ ! -f "$ENV_FILE" ]; then
    echo "ERROR: Deployed contract configurations not found at $ENV_FILE."
    echo "       Please run './scripts/deploy-local.sh' first to deploy the contracts."
    exit 1
fi

# shellcheck disable=SC1090
source "$ENV_FILE"

# Verify environment variables exist
for var in COMMITTEE_REGISTRY_CONTRACT TOKEN_CONTRACT COMMITTEE_ADDRESS SOROBAN_RPC NETWORK_PASSPHRASE; do
    if [ -z "${!var:-}" ]; then
        echo "ERROR: Required environment variable $var is not set in $ENV_FILE."
        exit 1
    fi
done
echo "  Loaded deployed contract configurations from $ENV_FILE."

# 3. Verify RPC Health
echo "Verifying local Stellar network connectivity..."
if ! curl -sf -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' "$SOROBAN_RPC" >/dev/null 2>&1; then
    echo "ERROR: Local Stellar RPC is not running or not reachable at $SOROBAN_RPC."
    echo "       Please ensure your Stellar docker container is running (e.g. docker-compose up soroban)."
    exit 1
fi
echo "  Connected to local network successfully."

# 4. Generate TLS Credentials
mkdir -p "$DATA_DIR"
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

echo "Generating TLS credentials..."
for i in 0 1 2; do
    cn="party${i}"
    key_out="${DATA_DIR}/key${i}.der"
    cert_out="${DATA_DIR}/cert${i}.der"

    # Generate private key (SEC1 EC private key in PEM format)
    openssl ecparam -name prime256v1 -genkey -noout -out "${TEMP_DIR}/key_${i}.pem" 2>/dev/null
    
    # Convert private key to DER format
    openssl ec -in "${TEMP_DIR}/key_${i}.pem" -outform DER -out "${key_out}" 2>/dev/null
    
    # Configure Subject Alternative Names
    cat > "${TEMP_DIR}/cert_${i}.conf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = ${cn}

[v3_req]
basicConstraints = critical, CA:TRUE
keyUsage = keyEncipherment, dataEncipherment
extendedKeyUsage = serverAuth, clientAuth
subjectAltName = @alt_names

[alt_names]
IP.1 = 127.0.0.1
DNS.1 = localhost
EOF

    # Generate self-signed certificate in DER format
    openssl req -new -x509 -key "${TEMP_DIR}/key_${i}.pem" -sha256 -days 365 -config "${TEMP_DIR}/cert_${i}.conf" -outform DER -out "${cert_out}" 2>/dev/null
    
    chmod 600 "${key_out}"
    chmod 644 "${cert_out}"
    echo "  Node $i: TLS credentials generated in $DATA_DIR"
done

# 4b. Generate Coordinator TLS client certificate (used for mTLS pinning)
echo "Generating coordinator TLS client certificate..."
COORDINATOR_KEY_OUT="${DATA_DIR}/coordinator_key.der"
COORDINATOR_CERT_OUT="${DATA_DIR}/coordinator_cert.der"

openssl ecparam -name prime256v1 -genkey -noout -out "${TEMP_DIR}/coordinator_key.pem" 2>/dev/null
openssl ec -in "${TEMP_DIR}/coordinator_key.pem" -outform DER -out "${COORDINATOR_KEY_OUT}" 2>/dev/null

cat > "${TEMP_DIR}/coordinator_cert.conf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = stellpoker-coordinator

[v3_req]
basicConstraints = critical, CA:FALSE
keyUsage = digitalSignature
extendedKeyUsage = clientAuth
EOF

openssl req -new -x509 \
    -key "${TEMP_DIR}/coordinator_key.pem" \
    -sha256 -days 365 \
    -config "${TEMP_DIR}/coordinator_cert.conf" \
    -outform DER -out "${COORDINATOR_CERT_OUT}" 2>/dev/null

chmod 600 "${COORDINATOR_KEY_OUT}"
chmod 644 "${COORDINATOR_CERT_OUT}"

# Compute SPKI hash (SHA-256 of SubjectPublicKeyInfo DER) for coordinator cert
# openssl x509 -noout -pubkey outputs the SPKI in PEM; we DER-encode and hash it.
COORDINATOR_SPKI_HASH=$(openssl x509 \
    -inform DER \
    -in "${COORDINATOR_CERT_OUT}" \
    -noout -pubkey 2>/dev/null | \
    openssl pkey -pubin -outform DER 2>/dev/null | \
    openssl dgst -sha256 -hex 2>/dev/null | \
    awk '{print $2}')

echo "  Coordinator client cert: ${COORDINATOR_CERT_OUT}"
echo "  Coordinator private key: ${COORDINATOR_KEY_OUT}"
echo "  Coordinator SPKI SHA-256: ${COORDINATOR_SPKI_HASH}"


# 5. Generate Node Config TOMLs
mkdir -p "$CONFIG_DIR"
echo "Generating node config TOML files..."
for i in 0 1 2; do
    port=$((10000 + i))
    config_path="${CONFIG_DIR}/party_${i}.toml"
    cat > "$config_path" <<EOF
# co-noir REP3 party configuration for Node ${i} (local development)

[network]
my_id = ${i}
bind_addr = "0.0.0.0:${port}"
key_path = "services/node/data/key${i}.der"
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
EOF
    echo "  Node $i: TOML configuration written to $config_path"
done

# 6. Generate Stellar Node Identities and Fund
echo "Generating node Stellar keys..."
FRIENDBOT_URL="http://localhost:8000/friendbot"
for i in 0 1 2; do
    ident="node${i}-local"
    stellar keys generate "$ident" --overwrite 2>/dev/null || true
    addr=$(stellar keys address "$ident")
    echo "  Node $i address: $addr"
    
    # Fund via friendbot
    echo "  Funding node${i}-local via Friendbot..."
    curl -sf "${FRIENDBOT_URL}?addr=${addr}" >/dev/null || {
        echo "  WARNING: Friendbot funding failed for node${i}-local"
    }
done

# 7. On-chain Initialization & Node Registration
echo "Initializing CommitteeRegistry..."
# Catch error if already initialized
stellar contract invoke \
    --id "$COMMITTEE_REGISTRY_CONTRACT" \
    --source committee-local \
    --network local \
    -- initialize \
    --admin "$COMMITTEE_ADDRESS" \
    --stake_token "$TOKEN_CONTRACT" \
    --min_stake "$MIN_STAKE" 2>&1 | grep -i "already initialized" || true

echo "Registering committee members..."
for i in 0 1 2; do
    ident="node${i}-local"
    addr=$(stellar keys address "$ident")
    port=$((8101 + i))
    endpoint="http://localhost:${port}"
    
    region="us-east-1"
    echo "  Registering Node $i ($addr) at $endpoint (region: $region)..."
    # Invoke as the member node itself to satisfy require_auth
    stellar contract invoke \
        --id "$COMMITTEE_REGISTRY_CONTRACT" \
        --source "$ident" \
        --network local \
        --instruction-leeway 500000000 \
        -- register_member \
        --member "$addr" \
        --stake "$MIN_STAKE" \
        --endpoint "$endpoint" \
        --region "$region" >/dev/null
done

# 8. Create Epoch
echo "Creating active epoch..."
node0_addr=$(stellar keys address node0-local)
node1_addr=$(stellar keys address node1-local)
node2_addr=$(stellar keys address node2-local)

# Invoke as admin to activate the epoch with threshold=2
stellar contract invoke \
    --id "$COMMITTEE_REGISTRY_CONTRACT" \
    --source committee-local \
    --network local \
    --instruction-leeway 500000000 \
    -- create_epoch \
    --admin "$COMMITTEE_ADDRESS" \
    --members "[\"$node0_addr\",\"$node1_addr\",\"$node2_addr\"]" \
    --threshold 2 >/dev/null

# 9. Verify Setup
echo "Verifying active committee epoch..."
stellar contract invoke \
    --id "$COMMITTEE_REGISTRY_CONTRACT" \
    --source committee-local \
    --network local \
    -- get_current_epoch

echo ""
echo "=== Setup Complete ==="
echo "You can now run: ./scripts/start-local.sh"
echo ""
echo "--- mTLS Certificate Pinning (optional but recommended) ---"
echo ""
echo "To enable coordinator TLS certificate pinning on the MPC nodes, add these"
echo "environment variables to your node container / docker-compose service:"
echo ""
echo "  # Node server TLS (nodes serve HTTPS):"
echo "  TLS_SERVER_CERT_PATH=services/node/data/cert0.der   # (cert1/cert2 for nodes 1/2)"
echo "  TLS_SERVER_KEY_PATH=services/node/data/key0.der     # (key1/key2  for nodes 1/2)"
echo ""
echo "  # Coordinator certificate pin (nodes accept only THIS coordinator):"
echo "  COORDINATOR_TLS_PIN_PUBKEY_HASH=${COORDINATOR_SPKI_HASH}"
echo ""
echo "  # Optional: also set on the coordinator so it presents its cert (mTLS):"
echo "  COORDINATOR_CLIENT_CERT_PATH=services/node/data/coordinator_cert.der"
echo "  COORDINATOR_CLIENT_KEY_PATH=services/node/data/coordinator_key.der"
echo ""
echo "  # Coordinator must also trust the node TLS certificates:"
echo "  MPC_NODE_CERT_PATHS=services/node/data/cert0.der,services/node/data/cert1.der,services/node/data/cert2.der"
echo ""
echo "COORDINATOR_SPKI_HASH=${COORDINATOR_SPKI_HASH}" >> "${ENV_FILE}"
echo "COORDINATOR_CLIENT_CERT_PATH=$(pwd)/${COORDINATOR_CERT_OUT}" >> "${ENV_FILE}"
echo "COORDINATOR_CLIENT_KEY_PATH=$(pwd)/${COORDINATOR_KEY_OUT}" >> "${ENV_FILE}"
echo "MPC_NODE_CERT_PATHS=$(pwd)/${DATA_DIR}/cert0.der,$(pwd)/${DATA_DIR}/cert1.der,$(pwd)/${DATA_DIR}/cert2.der" >> "${ENV_FILE}"
echo "  (also appended to ${ENV_FILE})"
echo ""
