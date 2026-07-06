# MPC Node API Key Authentication

This document describes the API key authentication system for MPC node-to-coordinator communication.

## Overview

The coordinator requires API key authentication for all MPC node requests to ensure only authorized nodes can participate in MPC sessions. This system provides:

- **Secure authentication** - SHA-256 hashed keys stored in the database
- **Key rotation** - Periodic key updates without downtime
- **Immediate revocation** - Instant deactivation of compromised keys
- **Usage monitoring** - Complete audit trail of key usage

## Architecture

```
MPC Node                    Coordinator
   |                           |
   | POST /mpc/prepare         |
   | Authorization: Bearer sk_... |
   |-------------------------->|
   |                           | 1. Extract API key
   |                           | 2. Hash and lookup in DB
   |                           | 3. Validate (active, not expired, not revoked)
   |                           | 4. Log usage
   |                           | 5. Process request
   |<--------------------------|
   |         Response          |
```

## API Key Format

API keys follow the format: `sk_<base64-encoded-32-bytes>`

Example: `sk_Kx5k8YQrB9XnH2vL9mPwR4sT6uI8oP3qA7cE5fG1hJ0N`

## Database Schema

### `api_keys` table
- `key_id` - Unique identifier for the key
- `key_hash` - SHA-256 hash of the API key
- `node_id` - MPC node identifier
- `is_active` - Whether the key is active
- `expires_at` - Optional expiration timestamp
- `revoked_at` - Revocation timestamp
- `last_used_at` - Last successful authentication

### `api_key_usage_log` table
- `key_id` - Reference to the API key
- `node_id` - MPC node identifier  
- `endpoint` - API endpoint accessed
- `ip_address` - Source IP address
- `timestamp` - Access timestamp
- `success` - Whether authentication succeeded

## Management Operations

### Create API Key

```bash
# Create a key for node0
python3 scripts/manage_api_keys.py create --node-id node0 --description "Node 0 production key"

# Create a key that expires in 90 days
python3 scripts/manage_api_keys.py create --node-id node1 --expires-days 90
```

### List API Keys

```bash
# List all keys for a node
python3 scripts/manage_api_keys.py list --node-id node0
```

### Revoke API Key

```bash
# Revoke a compromised key
python3 scripts/manage_api_keys.py revoke --key-id key_abc123 --reason "Suspected compromise"
```

### Rotate API Keys

```bash
# Create new key and show old keys to revoke
python3 scripts/manage_api_keys.py rotate --node-id node0
```

## MPC Node Configuration

MPC nodes must include the API key in the `Authorization` header:

```bash
curl -X POST http://coordinator:8080/mpc/prepare/deal \
  -H "Authorization: Bearer sk_Kx5k8YQrB9XnH2vL9mPwR4sT6uI8oP3qA7cE5fG1hJ0N" \
  -H "Content-Type: application/json" \
  -d '{"table_id": 1, "players": ["GA...", "GB..."]}'
```

## Security Considerations

### Key Storage
- API keys are hashed using SHA-256 before storage
- Raw keys are never stored in the database
- Keys should be stored securely on MPC nodes (environment variables, key management systems)

### Key Rotation
- Rotate keys periodically (recommended: 90 days)
- Zero-downtime rotation: create new key → update nodes → revoke old key
- Monitor usage logs during rotation to ensure smooth transition

### Monitoring
- All authentication attempts are logged
- Failed attempts include partial key information for forensics
- Set up alerts on unusual access patterns

### Revocation
- Immediate revocation for compromised keys
- Automatic expiration cleanup via background task
- Audit trail maintained for compliance

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `COORDINATOR_URL` | Coordinator base URL for CLI | `http://localhost:8080` |
| `ADMIN_SECRET` | Admin authentication secret | Required |

## API Endpoints

### Admin Endpoints (Authenticated)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/admin/api-keys` | Create new API key |
| `GET` | `/api/admin/api-keys/:node_id` | List keys for node |
| `POST` | `/api/admin/api-keys/:key_id/revoke` | Revoke API key |

### MPC Endpoints (API Key Required)

All endpoints matching these patterns require API key authentication:
- `/mpc/*` - MPC operations
- `/prepare/*` - Share preparation
- `/prove/*` - Proof generation
- `/shares/*` - Share exchange
- `/internal/*` - Internal node communication

## Migration Guide

### Initial Setup

1. Run the migration to create API key tables:
```bash
cd services/coordinator
sqlx migrate run
```

2. Create initial API keys for each MPC node:
```bash
python3 scripts/manage_api_keys.py create --node-id node0 --description "Initial key"
python3 scripts/manage_api_keys.py create --node-id node1 --description "Initial key"  
python3 scripts/manage_api_keys.py create --node-id node2 --description "Initial key"
```

3. Update MPC node configurations with the generated keys.

4. Restart the coordinator with the authentication middleware enabled.

### Existing Deployment

For existing deployments, API key authentication can be enabled gradually:

1. Deploy the coordinator with authentication disabled (set `ALLOW_INSECURE_DEV_AUTH=1`)
2. Create and distribute API keys to MPC nodes
3. Update MPC node configurations
4. Remove `ALLOW_INSECURE_DEV_AUTH` to enable authentication

## Troubleshooting

### Authentication Failures

Check the coordinator logs for authentication errors:
```bash
grep "MPC authentication failed" coordinator.log
```

Common issues:
- **Invalid API key format** - Key doesn't start with `sk_` or invalid base64
- **Key not found** - Key doesn't exist in database
- **Key revoked** - Key was explicitly revoked
- **Key expired** - Key past expiration date
- **Missing Authorization header** - MPC node not sending API key

### Key Management

View key usage:
```bash
# Check recent usage in database
SELECT * FROM api_key_usage_log WHERE timestamp > NOW() - INTERVAL '1 hour';

# Check key status
SELECT key_id, node_id, is_active, expires_at, last_used_at FROM api_keys;
```

### Performance

API key validation adds minimal overhead:
- SHA-256 hash computation: ~1μs
- Database lookup: ~1ms (with proper indexing)
- Usage logging: async, non-blocking

Monitor authentication performance:
```bash
grep "Authenticated MPC node" coordinator.log | tail -100
```
