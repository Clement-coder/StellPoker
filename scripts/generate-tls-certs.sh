#!/usr/bin/env bash
# Generate TLS certificates for coordinator and MPC nodes (Issue #93)
set -euo pipefail

CERT_DIR="${1:-./certs}"
mkdir -p "${CERT_DIR}"

# Generate CA
openssl req -x509 -newkey rsa:4096 -days 365 -nodes \
  -keyout "${CERT_DIR}/ca-key.pem" \
  -out "${CERT_DIR}/ca-cert.pem" \
  -subj "/CN=StellPoker-CA"

# Generate coordinator cert
openssl req -newkey rsa:4096 -nodes \
  -keyout "${CERT_DIR}/coordinator-key.pem" \
  -out "${CERT_DIR}/coordinator-req.pem" \
  -subj "/CN=coordinator"

openssl x509 -req -in "${CERT_DIR}/coordinator-req.pem" \
  -CA "${CERT_DIR}/ca-cert.pem" \
  -CAkey "${CERT_DIR}/ca-key.pem" \
  -CAcreateserial -days 365 \
  -out "${CERT_DIR}/coordinator-cert.pem"

# Generate node certs
for i in 0 1 2; do
  openssl req -newkey rsa:4096 -nodes \
    -keyout "${CERT_DIR}/node${i}-key.pem" \
    -out "${CERT_DIR}/node${i}-req.pem" \
    -subj "/CN=mpc-node-${i}"
  
  openssl x509 -req -in "${CERT_DIR}/node${i}-req.pem" \
    -CA "${CERT_DIR}/ca-cert.pem" \
    -CAkey "${CERT_DIR}/ca-key.pem" \
    -CAcreateserial -days 365 \
    -out "${CERT_DIR}/node${i}-cert.pem"
done

echo "TLS certificates generated in ${CERT_DIR}"
