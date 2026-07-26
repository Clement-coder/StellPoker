#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ENV_FILE="${ENV_FILE:-${PROJECT_DIR}/.env.staging}"

if [ ! -f "$ENV_FILE" ]; then
    echo "ERROR: staging env file not found at $ENV_FILE"
    echo "Copy .env.staging.example to .env.staging and fill in the staging values first."
    exit 1
fi

set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a

required_vars=(
    SOROBAN_RPC
    NETWORK_PASSPHRASE
    POKER_TABLE_CONTRACT
    COMMITTEE_IDENTITY
    PLAYER1_ADDRESS
    PLAYER1_IDENTITY
    PLAYER2_ADDRESS
    PLAYER2_IDENTITY
    NEXT_PUBLIC_COORDINATOR_URL
)

for var_name in "${required_vars[@]}"; do
    if [ -z "${!var_name:-}" ]; then
        echo "ERROR: $var_name must be set in $ENV_FILE"
        exit 1
    fi
done

COORDINATOR_HEALTH_URL="${STAGING_COORDINATOR_HEALTH_URL:-${NEXT_PUBLIC_COORDINATOR_URL%/}/api/health}"
FRONTEND_URL="${STAGING_FRONTEND_URL:-http://localhost:${STAGING_FRONTEND_PORT:-3000}}"
NODE0_HEALTH_URL="${STAGING_NODE0_HEALTH_URL:-http://localhost:${STAGING_NODE0_PORT:-8101}/health}"
NODE1_HEALTH_URL="${STAGING_NODE1_HEALTH_URL:-http://localhost:${STAGING_NODE1_PORT:-8102}/health}"
NODE2_HEALTH_URL="${STAGING_NODE2_HEALTH_URL:-http://localhost:${STAGING_NODE2_PORT:-8103}/health}"

wait_for_url() {
    local name="$1"
    local url="$2"
    local attempts="${3:-30}"

    for i in $(seq 1 "$attempts"); do
        if curl -sfL "$url" >/dev/null 2>&1; then
            echo "  $name ready: $url"
            return 0
        fi
        sleep 2
    done

    echo "ERROR: $name did not become ready at $url"
    return 1
}

echo "=== Verifying staging services ==="
wait_for_url "MPC Node 0" "$NODE0_HEALTH_URL"
wait_for_url "MPC Node 1" "$NODE1_HEALTH_URL"
wait_for_url "MPC Node 2" "$NODE2_HEALTH_URL"
wait_for_url "Coordinator" "$COORDINATOR_HEALTH_URL"
wait_for_url "Frontend" "$FRONTEND_URL"

echo ""
echo "=== Running staging integration flow ==="
TEST_ENV_FILE="$ENV_FILE" \
COORDINATOR_URL="${NEXT_PUBLIC_COORDINATOR_URL%/}" \
SOROBAN_RPC="$SOROBAN_RPC" \
NETWORK_PASSPHRASE="$NETWORK_PASSPHRASE" \
COMMITTEE_IDENTITY="$COMMITTEE_IDENTITY" \
PLAYER1_IDENTITY="$PLAYER1_IDENTITY" \
PLAYER2_IDENTITY="$PLAYER2_IDENTITY" \
python3 "${PROJECT_DIR}/scripts/test-flow.py"
