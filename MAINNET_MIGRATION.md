# StellPoker — Testnet → Mainnet Migration Guide

This guide documents every step required to migrate a running StellPoker deployment
from Stellar testnet to mainnet. Follow the sections in order; each one must be
complete before the next begins.

---

## Table of Contents

1. [Pre-flight Checklist](#1-pre-flight-checklist)
2. [Mainnet Soroban RPC Configuration](#2-mainnet-soroban-rpc-configuration)
3. [Contract Deployment & Address Updates](#3-contract-deployment--address-updates)
4. [Committee Key Management](#4-committee-key-management)
5. [Frontend Deployment](#5-frontend-deployment)
6. [DNS Changes](#6-dns-changes)
7. [Monitoring Setup](#7-monitoring-setup)
8. [Post-Migration Smoke Tests](#8-post-migration-smoke-tests)
9. [Rollback Procedure](#9-rollback-procedure)

---

## 1. Pre-flight Checklist

Complete every item before touching any production environment variable or running
any deploy command.

- [ ] Deployer account funded with ≥ 100 XLM on mainnet
- [ ] BN254 Common Reference String (CRS) downloaded on every MPC node host
  (`./scripts/download-crs.sh`)
- [ ] Noir circuits compiled and verification keys generated
  (`./scripts/compile-circuits.sh`)
- [ ] MPC committee TLS key ceremony completed (see §4)
- [ ] All three MPC node hosts are reachable from the coordinator
- [ ] PostgreSQL database snapshot taken from testnet coordinator
- [ ] DNS TTLs lowered to 60 s at least 48 hours before the migration window
  (see §6)
- [ ] Staging/preview deployment validated against mainnet RPC in read-only mode

---

## 2. Mainnet Soroban RPC Configuration

### 2.1 Network values

| Parameter | Testnet | Mainnet |
|---|---|---|
| `SOROBAN_RPC` | `https://soroban-testnet.stellar.org` | `https://mainnet.sorobanrpc.com` |
| `NETWORK_PASSPHRASE` | `Test SDF Network ; September 2015` | `Public Global Stellar Network ; September 2015` |
| Friendbot | Available | **Not available** — remove `FRIENDBOT_URL` |

> **Important:** The network passphrase is embedded in every signed transaction.
> A mismatch causes all on-chain calls to fail with a signature error.
> Double-check there is no trailing whitespace in the value.

### 2.2 Coordinator environment file

Update `.env` (or your secrets manager) on every coordinator host:

```bash
# ── Soroban / Stellar ──
SOROBAN_RPC=https://mainnet.sorobanrpc.com
NETWORK_PASSPHRASE=Public Global Stellar Network ; September 2015

# Remove or comment out — Friendbot does not exist on mainnet
# FRIENDBOT_URL=
```

### 2.3 Verify connectivity before deploying contracts

```bash
curl -s -X POST \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
  https://mainnet.sorobanrpc.com \
  | jq .

# Expected: {"jsonrpc":"2.0","id":1,"result":{"status":"healthy",...}}
```

### 2.4 Alternative / fallback RPC providers

If `mainnet.sorobanrpc.com` is unavailable, the following public endpoints are
compatible with the `stellar-sdk` `rpc.Server` client used in `onchain.ts`:

- `https://rpc.mainnet.stellar.gateway.fm`
- `https://soroban-mainnet.stellar.org` (SDF backup)

Set `SOROBAN_RPC` to your preferred provider and restart the coordinator.

---

## 3. Contract Deployment & Address Updates

Three Soroban contracts must be deployed to mainnet: `zk-verifier`,
`committee-registry`, and `poker-table`. The existing deploy script handles all
three in sequence.

### 3.1 Run the deploy script

```bash
export NETWORK=mainnet
export DEPLOYER_SECRET=S...   # funded mainnet deployer secret key

./scripts/deploy.sh
```

The script will:

1. Build all three WASM contracts (`cargo build --release --target wasm32-unknown-unknown`)
2. Optimize the WASMs with `stellar contract optimize`
3. Compile Noir circuits
4. Deploy and initialize each contract on mainnet
5. Print the three contract IDs at the end

Sample output:

```
Contract Addresses:
  ZK_VERIFIER=C<hash-A>
  COMMITTEE_REGISTRY=C<hash-B>
  POKER_TABLE=C<hash-C>
```

### 3.2 Set verification keys on-chain

After deployment, upload the compiled verification key to the `zk-verifier`
contract:

```bash
stellar contract invoke \
  --id <ZK_VERIFIER_ID> \
  --source deployer \
  --network mainnet \
  -- set_verification_key \
     --vk "$(cat circuits/poker.vk | base64 -w0)"
```

### 3.3 Update coordinator environment

```bash
POKER_TABLE_CONTRACT=C<hash-C>
ONCHAIN_TABLE_ID=0
```

The coordinator reads `POKER_TABLE_CONTRACT` at startup via `SorobanConfig::from_env()`
(`services/coordinator/src/soroban/mod.rs`). Restart the coordinator after updating.

### 3.4 Remove testnet player identity mappings

Testnet `PLAYER{N}_ADDRESS` / `PLAYER{N}_IDENTITY` pairs reference funded test
accounts that do not exist on mainnet. Remove or replace every such pair in `.env`:

```bash
# Remove from .env:
# PLAYER1_ADDRESS=G...
# PLAYER1_IDENTITY=player1-local
# ... etc.
```

On mainnet, player identities are derived from wallet-signed transactions; no
pre-seeded mappings are needed.

### 3.5 Record addresses in version control

Commit the three mainnet contract IDs to a tracked file (e.g.
`contracts/mainnet-addresses.json`) so they can be referenced without reading live
environment variables:

```json
{
  "network": "mainnet",
  "deployed_at": "2026-06-25",
  "zk_verifier": "C<hash-A>",
  "committee_registry": "C<hash-B>",
  "poker_table": "C<hash-C>"
}
```

---

## 4. Committee Key Management

The coordinator holds a single Stellar secret key (`COMMITTEE_SECRET`) whose
corresponding G-address is registered on the `committee-registry` contract. The
three MPC nodes use TLS mutual authentication via DER-encoded certificates. Both
sets of keys must be generated and stored securely before mainnet launch.

### 4.1 MPC node TLS key ceremony

Each of the three MPC node operators must generate their own TLS private key and
self-signed certificate independently. **Never generate all three key pairs on the
same machine.**

On each node host (substitute `N` with 0, 1, or 2):

```bash
# Generate EC private key
openssl ecparam -name prime256v1 -genkey -noout -out keyN.pem

# Convert to DER (matches the format expected by services/node/src/main.rs)
openssl ec -in keyN.pem -outform DER -out services/node/data/keyN.der
chmod 600 services/node/data/keyN.der

# Generate self-signed certificate (365 days; rotate before expiry)
openssl req -new -x509 \
  -key keyN.pem \
  -sha256 -days 365 \
  -subj "/CN=partyN" \
  -addext "subjectAltName=DNS:mpc-node-N.stellpoker.com,IP:10.0.x.x" \
  -outform DER \
  -out services/node/data/certN.der
chmod 644 services/node/data/certN.der

# Delete the PEM key — it is no longer needed
shred -u keyN.pem
```

Each operator shares only their **certificate** (`certN.der`, public) with the
other two operators. Private keys (`keyN.der`) must never leave their host.

### 4.2 Production node TOML configuration

Update `services/node/config/party_N.toml` for each node with production hostnames
or private IPs. The MPC port (10000–10002) must be reachable between nodes but
**must not** be exposed to the public internet:

```toml
# services/node/config/party_0.toml  (production)
[party]
id = 0
bind = "0.0.0.0:10000"

[[peers]]
id = 1
address = "mpc-node-1.internal.stellpoker.com:10001"

[[peers]]
id = 2
address = "mpc-node-2.internal.stellpoker.com:10002"
```

Repeat for `party_1.toml` and `party_2.toml` with their respective IDs and peer
addresses.

### 4.3 Committee Stellar key — generation and storage

The `COMMITTEE_SECRET` is the Stellar secret key used by the coordinator to sign
on-chain transactions. Generate a dedicated mainnet key; **do not reuse the
deployer key or any testnet key.**

```bash
# Generate offline (air-gapped machine preferred)
stellar keys generate committee-mainnet --no-fund

# Record the address (G...) — needed for on-chain registration
stellar keys address committee-mainnet

# Export the secret key and store it in your secrets manager
stellar keys show committee-mainnet
# → S...
```

Storage requirements:

- Store `COMMITTEE_SECRET` in a secrets manager (AWS Secrets Manager, HashiCorp
  Vault, or equivalent). Do not store it in `.env` files that are committed to
  version control.
- Create a separate read-only backup of the key in cold storage (printed QR code
  or hardware security module).
- Fund the committee address with ≥ 10 XLM to cover transaction fees.

### 4.4 Register the committee on-chain

After the `committee-registry` contract is deployed (§3) and all three node
certificates are in place, run the mainnet DKG registration. Adapt
`scripts/setup-dkg.sh` with production values:

```bash
export COMMITTEE_REGISTRY_CONTRACT=C<hash-B>
export TOKEN_CONTRACT=<XLM or custom token contract>
export COMMITTEE_ADDRESS=$(stellar keys address committee-mainnet)
export SOROBAN_RPC=https://mainnet.sorobanrpc.com
export NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
export ENV_FILE=.env.mainnet

./scripts/setup-dkg.sh \
  --data-dir services/node/data \
  --config-dir services/node/config
```

Verify the epoch is active:

```bash
stellar contract invoke \
  --id <COMMITTEE_REGISTRY_CONTRACT> \
  --source committee-mainnet \
  --network mainnet \
  -- get_current_epoch
```

### 4.5 Key rotation schedule

| Key type | Rotation interval | Procedure |
|---|---|---|
| MPC node TLS cert | 12 months | Regenerate per §4.1, re-share cert, restart node |
| `COMMITTEE_SECRET` | 6 months or on suspected compromise | Generate new key, fund it, update `committee-registry` admin, update secrets manager, restart coordinator |
| Deployer key | After deployment only | Rotate or revoke; not needed for runtime |

### 4.6 Revoking a compromised key

If `COMMITTEE_SECRET` is compromised:

1. Generate a replacement key immediately (§4.3).
2. Use the **deployer** key (still valid) to call `set_admin` on
   `committee-registry` with the new committee address.
3. Update the secret in your secrets manager.
4. Restart all coordinator instances.
5. Notify the security team and audit all recent on-chain transactions.

---

## 5. Frontend Deployment

The Next.js frontend (`app/`) requires only two environment variable changes for
mainnet. All chain configuration (RPC URL, network passphrase, contract ID) is
fetched at runtime from the coordinator via `GET /api/chain-config`, so the
frontend itself does not hold any Stellar addresses.

### 5.1 Environment variables

```bash
# .env.production  (or injected via your CI/CD secrets)

# Point the frontend at the production coordinator
NEXT_PUBLIC_COORDINATOR_URL=https://api.stellpoker.com

# Disable insecure dev auth — must be absent or false on mainnet
NEXT_PUBLIC_ALLOW_INSECURE_DEV_AUTH=false
```

> **Never** set `NEXT_PUBLIC_ALLOW_INSECURE_DEV_AUTH=true` in a production build.
> This flag bypasses Stellar signature verification in the coordinator's auth
> middleware (`services/coordinator/src/api/auth.rs`).

### 5.2 Feature flags for mainnet launch

The coordinator reads feature flags from environment variables at startup
(`services/coordinator/src/feature_flags.rs`). For a conservative mainnet launch,
enable flags progressively:

```bash
# Coordinator .env — Phase 1 (launch day)
FEATURE_FLAG_SOLO_MODE=1          # Enable solo/bot tables for testing
FEATURE_FLAG_CHAT_ENABLED=0       # Disable chat until load-tested
FEATURE_FLAG_NEW_CIRCUITS=0       # Use stable circuits only
FEATURE_FLAG_CONTRACT_UPGRADE=0   # Gate new contract calls
FEATURE_FLAG_EXPERIMENTAL_UI=0    # Stable UI only
```

Enable additional flags after observing at least 24 hours of stable mainnet
metrics (see §7).

### 5.3 Build and deploy

```bash
cd app

# Install dependencies
npm ci

# Build for production
npm run build

# The output is in app/.next — deploy to Vercel, Netlify, or your CDN origin
```

If deploying to Vercel:

```bash
vercel --prod \
  -e NEXT_PUBLIC_COORDINATOR_URL=https://api.stellpoker.com \
  -e NEXT_PUBLIC_ALLOW_INSECURE_DEV_AUTH=false
```

### 5.4 Wallet adapter configuration

`onchain.ts` and `freighter.ts` derive network passphrase and contract addresses
from the coordinator's `GET /api/chain-config` response at runtime. No wallet
adapter source changes are required for mainnet — the coordinator provides the
correct values once `NETWORK_PASSPHRASE` and `POKER_TABLE_CONTRACT` are updated in
its environment.

Verify the chain-config endpoint returns mainnet values after coordinator restart:

```bash
curl -s https://api.stellpoker.com/api/chain-config | jq .
# Expected:
# {
#   "rpc_url": "https://mainnet.sorobanrpc.com",
#   "network_passphrase": "Public Global Stellar Network ; September 2015",
#   "poker_table_contract": "C<hash-C>"
# }
```

### 5.5 Docker build (if self-hosting the frontend)

```bash
docker build -f app/Dockerfile \
  --build-arg NEXT_PUBLIC_COORDINATOR_URL=https://api.stellpoker.com \
  -t stellpoker-frontend:mainnet \
  app/

docker push <your-registry>/stellpoker-frontend:mainnet
```

---

## 6. DNS Changes

### 6.1 Recommended subdomains

| Subdomain | Target | Purpose |
|---|---|---|
| `stellpoker.com` / `www` | CDN / Vercel | Frontend |
| `api.stellpoker.com` | ALB or reverse proxy | Coordinator HTTP + WebSocket |
| `mpc-node-{0,1,2}.internal.stellpoker.com` | Private IPs | MPC node peer traffic (internal only) |
| `metrics.stellpoker.com` | Grafana | Monitoring dashboard (restrict access) |

### 6.2 Pre-migration TTL reduction

Lower TTLs **48 hours before** the migration window so that DNS changes propagate
quickly if a rollback is needed:

```
stellpoker.com        A/CNAME   <testnet-origin>   TTL 60
api.stellpoker.com    A/CNAME   <testnet-api>      TTL 60
```

### 6.3 Migration cutover

During the maintenance window:

1. Update `api.stellpoker.com` to point to the production coordinator ALB or
   reverse proxy IP.
2. Update `stellpoker.com` and `www` to point to the mainnet frontend CDN origin
   (or Vercel production deployment URL).
3. Confirm propagation:

```bash
# Check from multiple regions
dig api.stellpoker.com +short
curl -s https://api.stellpoker.com/api/health | jq .
```

### 6.4 TLS certificates

Ensure TLS certificates cover all public subdomains before the cutover:

```bash
# Using certbot (Let's Encrypt)
certbot certonly --dns-cloudflare \
  -d stellpoker.com \
  -d www.stellpoker.com \
  -d api.stellpoker.com \
  -d metrics.stellpoker.com

# Or use your cloud provider's managed certificate service (ACM, GCP Certificate Manager)
```

### 6.5 CORS update

The coordinator uses a database-backed CORS allowlist (`services/coordinator/src/cors_db.rs`).
Add the mainnet frontend origin via the admin API after the DNS cutover:

```bash
curl -X POST https://api.stellpoker.com/admin/cors/origins \
  -H "x-admin-address: G<admin-key>" \
  -H "x-admin-signature: <stellar-signed-payload>" \
  -H "Content-Type: application/json" \
  -d '{"origin": "https://stellpoker.com"}'
```

Remove the testnet origin (`https://testnet.stellpoker.com` or `http://localhost:3000`)
after verifying the mainnet frontend is healthy.

### 6.6 Restore production TTLs

After confirming stability (≥ 2 hours with no errors), restore TTLs to normal:

```
stellpoker.com        A/CNAME   <mainnet-origin>   TTL 3600
api.stellpoker.com    A/CNAME   <mainnet-api>      TTL 3600
```

---

## 7. Monitoring Setup

The coordinator exposes a Prometheus metrics endpoint at `GET /metrics`
(port 8080). The load-testing stack in `load-testing/` provides a ready-made
Prometheus + Grafana configuration that should be promoted to production.

### 7.1 Prometheus configuration

Create `/etc/prometheus/prometheus.yml` on your monitoring host:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'coordinator'
    metrics_path: /metrics
    static_configs:
      - targets:
          - 'api.stellpoker.com:8080'   # or internal ALB DNS

  - job_name: 'mpc-nodes'
    static_configs:
      - targets:
          - 'mpc-node-0.internal.stellpoker.com:8101'
          - 'mpc-node-1.internal.stellpoker.com:8102'
          - 'mpc-node-2.internal.stellpoker.com:8103'

  - job_name: 'postgres'
    static_configs:
      - targets: ['localhost:9187']   # postgres_exporter
```

### 7.2 Key metrics to alert on

| Metric | Alert condition | Severity |
|---|---|---|
| `coordinator_requests_total{status="5xx"}` | Rate > 1/min over 5 min | Critical |
| `coordinator_proof_latency_ms` p99 > 30 000 ms | p99 exceeds 30 s | Warning |
| `coordinator_mpc_errors_total` | Any increment | Critical |
| `coordinator_active_sessions` | Drops to 0 unexpectedly | Warning |
| `process_resident_memory_bytes` (coordinator) | > 90% of container limit | Warning |
| `pg_up` | == 0 | Critical |

Add these as Alertmanager rules or CloudWatch alarms (see `infrastructure/terraform/variables.tf`,
`alarm_email` variable) depending on your infrastructure provider.

### 7.3 Grafana dashboard

The pre-built k6 coordinator dashboard is at
`load-testing/grafana/dashboards/k6-coordinator-dashboard.json`. Import it into
your production Grafana instance:

1. Navigate to **Dashboards → Import** in Grafana.
2. Upload `k6-coordinator-dashboard.json`.
3. Select your Prometheus data source.
4. Save.

### 7.4 Database backup monitoring

The coordinator database backup script (`scripts/backup-coordinator-db.sh`)
implements daily / weekly / monthly retention. Set it up as a cron job on the
database host:

```bash
# /etc/cron.d/stellpoker-backup
0 3 * * * coordinator \
  DB_HOST=<rds-endpoint> \
  DB_NAME=coordinator \
  DB_USER=coordinator \
  PGPASSWORD=<password> \
  BACKUP_DIR=/var/backups/stellpoker \
  /opt/stellpoker/scripts/backup-coordinator-db.sh >> /var/log/stellpoker-backup.log 2>&1
```

Verify the first backup completes successfully and test restoration with
`scripts/restore-coordinator-db.sh` before the migration window.

### 7.5 Soroban RPC health check

Add an external uptime check that probes the mainnet RPC every minute:

```bash
# Example: UptimeRobot or similar service hitting this endpoint
POST https://mainnet.sorobanrpc.com
Content-Type: application/json
{"jsonrpc":"2.0","id":1,"method":"getHealth"}

# Alert if response is not {"result":{"status":"healthy"}}
```

### 7.6 On-call runbook pointers

| Symptom | First check |
|---|---|
| Proof generation timing out | MPC node connectivity (`MPC_NODE_{0,1,2}` env vars); node logs for co-noir errors |
| All on-chain calls failing | `NETWORK_PASSPHRASE` mismatch; coordinator `COMMITTEE_SECRET` funding |
| Frontend shows wrong network | Coordinator `/api/chain-config` response; `NEXT_PUBLIC_COORDINATOR_URL` |
| Database connection errors | `DATABASE_URL`; RDS security group; coordinator migration logs on startup |

---

## 8. Post-Migration Smoke Tests

Run these checks immediately after the migration window closes.

```bash
# 1. Coordinator health
curl -s https://api.stellpoker.com/api/health | jq .
# Expected: {"status":"ok"}

# 2. Chain config returns mainnet values
curl -s https://api.stellpoker.com/api/chain-config | jq .rpc_url
# Expected: "https://mainnet.sorobanrpc.com"

# 3. Feature flags endpoint
curl -s https://api.stellpoker.com/api/flags | jq .
# Expected: JSON object with all flag keys

# 4. Soroban RPC reachable from coordinator host
curl -s -X POST \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
  https://mainnet.sorobanrpc.com | jq .result.status
# Expected: "healthy"

# 5. Committee epoch active
stellar contract invoke \
  --id <COMMITTEE_REGISTRY_CONTRACT> \
  --source committee-mainnet \
  --network mainnet \
  -- get_current_epoch
# Expected: non-null epoch object with threshold=2

# 6. Frontend loads and connects to correct network
# Open https://stellpoker.com in a browser with Freighter or LOBSTR wallet
# Connect wallet → confirm network passphrase displayed is "Public Global Stellar Network ; September 2015"
```

---

## 9. Rollback Procedure

If a critical issue is found after the migration window, roll back as follows:

1. **DNS:** Repoint `api.stellpoker.com` and `stellpoker.com` to testnet origins.
   Because TTLs were lowered to 60 s in §6.2, propagation takes < 2 minutes.

2. **Coordinator:** Restore `.env` to testnet values:
   ```bash
   SOROBAN_RPC=https://soroban-testnet.stellar.org
   NETWORK_PASSPHRASE=Test SDF Network ; September 2015
   POKER_TABLE_CONTRACT=<testnet-contract-id>
   ```
   Restart coordinator instances.

3. **Database:** If schema migrations ran, restore from the pre-migration snapshot
   taken in §1 using `scripts/restore-coordinator-db.sh`.

4. **Frontend:** Redeploy the last testnet-tagged frontend image or Vercel
   deployment.

5. **Contracts:** Mainnet contracts are immutable — no rollback required.
   The testnet contracts remain valid and unaffected.

6. Open a post-mortem issue before re-attempting the migration.
