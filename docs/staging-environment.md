# Staging Environment

Staging mirrors the production shape while continuing to use **Soroban testnet** as its chain backend:

- 1 staging coordinator
- 3 staging MPC nodes
- 1 staging frontend
- Shared staging contract set and seeded staging table on testnet

## Files

- `.env.staging.example` — template for staging contract IDs, identities, and public URLs
- `docker-compose.staging.yml` — production-like staging stack with coordinator, MPC nodes, and frontend
- `scripts/deploy-staging.sh` — build artifacts, start the staging stack, and run verification
- `scripts/run-staging-checks.sh` — smoke test the staging services and execute the full integration flow
- `infrastructure/terraform/environments/staging.aws.tfvars.example` — Terraform sizing example for AWS staging

## Bring Up Staging

1. Copy `.env.staging.example` to `.env.staging`.
2. Deploy or refresh the staging contracts on testnet when needed:

```bash
NETWORK=staging OUTPUT_ENV_FILE=.env.staging.deploy ./scripts/deploy.sh
```

3. Copy the generated contract IDs from `.env.staging.deploy` into `.env.staging`, then fill in the committee key, seeded player identities, and public coordinator URL.
4. Start the staging stack:

```bash
./scripts/deploy-staging.sh
```

This will:

- compile the Noir circuits
- ensure the CRS is present
- start the staging coordinator, MPC nodes, and frontend
- run the integration flow against staging before promotion

## Run Verification Only

Use this when the staging services are already running:

```bash
./scripts/run-staging-checks.sh
```

The verification script waits for:

- all 3 MPC node health endpoints
- the coordinator health endpoint
- the staging frontend

Then it runs `scripts/test-flow.py` against the staging coordinator using the identities and contracts from `.env.staging`.
