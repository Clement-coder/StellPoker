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

echo "=== Preparing staging artifacts ==="
"${PROJECT_DIR}/scripts/compile-circuits.sh"
"${PROJECT_DIR}/scripts/download-crs.sh"

echo ""
echo "=== Starting staging stack ==="
docker compose --env-file "$ENV_FILE" -f "${PROJECT_DIR}/docker-compose.staging.yml" up -d --build

echo ""
echo "=== Running staging verification ==="
ENV_FILE="$ENV_FILE" "${PROJECT_DIR}/scripts/run-staging-checks.sh"
