#!/usr/bin/env bash
# Download CRS for circuits with caching (Issue #92)
set -euo pipefail

CRS_DIR="${CRS_DIR:-./.crs}"
CRS_URL="https://aztec-ignition.s3.amazonaws.com/MAIN%20IGNITION/sealed/transcript00.dat"
CRS_FILE="${CRS_DIR}/bn254_g1.dat"
CRS_EXPECTED_HASH="c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"

mkdir -p "${CRS_DIR}"

if [ -f "${CRS_FILE}" ]; then
  existing_hash=$(sha256sum "${CRS_FILE}" | cut -d' ' -f1)
  if [ "${existing_hash}" = "${CRS_EXPECTED_HASH}" ]; then
    echo "CRS already downloaded and verified: ${CRS_FILE}"
    exit 0
  fi
  echo "CRS hash mismatch, re-downloading..."
fi

echo "Downloading CRS from ${CRS_URL}..."
curl -fSL --progress-bar -o "${CRS_FILE}.tmp" "${CRS_URL}"

downloaded_hash=$(sha256sum "${CRS_FILE}.tmp" | cut -d' ' -f1)
if [ "${downloaded_hash}" != "${CRS_EXPECTED_HASH}" ]; then
  echo "ERROR: Downloaded CRS hash mismatch"
  echo "Expected: ${CRS_EXPECTED_HASH}"
  echo "Got:      ${downloaded_hash}"
  rm -f "${CRS_FILE}.tmp"
  exit 1
fi

mv "${CRS_FILE}.tmp" "${CRS_FILE}"
echo "CRS downloaded and verified: ${CRS_FILE}"
