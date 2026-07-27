#!/usr/bin/env bash
# Stellar Poker - Promote an MPC standby node (Issue #95, #96)
#
# co-noir's REP3 protocol runs with exactly 3 active parties (roles 0, 1, 2);
# that doesn't change. This script implements the "N=5, threshold=3" model:
# roles 0 and 1 each have a pre-provisioned standby (mpc-node-3, mpc-node-4 in
# docker-compose.yml) holding an identical party config/identity. When a
# primary is down for good (not just a transient blip — see
# node_reliability.rs / mpc.rs's in-session retry for that case), this script
# promotes its standby to take over:
#   1. Stop the failed primary container.
#   2. Start its standby under the primary's network alias, so peers
#      addressing e.g. "mpc-node-0:10000" transparently reach the standby.
#   3. Restart the coordinator so it picks up a healthy connection.
#
# Usage:
#   ./scripts/promote-mpc-standby.sh <role>
#
# <role> is 0 or 1 (the only roles with a provisioned standby).

set -euo pipefail

ROLE="${1:-}"
if [[ "$ROLE" != "0" && "$ROLE" != "1" ]]; then
    echo "Usage: $0 <role>" >&2
    echo "  <role> must be 0 or 1 (the roles with a provisioned standby)" >&2
    exit 1
fi

PRIMARY="mpc-node-${ROLE}"
STANDBY_INDEX=$((ROLE + 3)) # role 0 -> mpc-node-3, role 1 -> mpc-node-4
STANDBY="mpc-node-${STANDBY_INDEX}"

echo "Promoting ${STANDBY} to take over role ${ROLE} (replacing ${PRIMARY})..."

echo "  Stopping ${PRIMARY}..."
docker compose stop "${PRIMARY}"

echo "  Starting ${STANDBY} (profile: standby) under ${PRIMARY}'s network alias..."
docker compose --profile standby up -d \
    --scale "${STANDBY}=1" \
    --no-deps \
    "${STANDBY}"
docker network connect \
    --alias "${PRIMARY}" \
    "$(basename "$(pwd)")_default" \
    "$(docker compose ps -q "${STANDBY}")" 2>/dev/null || {
    echo "  NOTE: could not add network alias automatically (already connected, or" >&2
    echo "  a non-default compose network name is in use). If the coordinator can't" >&2
    echo "  reach '${PRIMARY}', point MPC_NODE_${ROLE} at the standby's own service" >&2
    echo "  name/port directly and restart the coordinator instead." >&2
}

echo "  Restarting coordinator to pick up the new connection..."
docker compose restart coordinator

echo "Done. ${STANDBY} is now serving role ${ROLE}."
echo "Once ${PRIMARY} is repaired, reverse this (stop the standby, start the primary,"
echo "restart the coordinator) to fail back."
