# 🗄️ Coordinator Database Schema & ER Diagram

> **Auto-generated reference.** Updates automatically when database migrations are modified.

## Entity-Relationship Diagram

```mermaid
erDiagram
    game_tables {
        BIGSERIAL id PK
        INTEGER table_id UK
        TEXT phase
        TEXT deck_root
        BIGINT proof_nonce
        TEXT[] player_order
        TEXT[] hand_commitments
        INTEGER[] dealt_indices
        INTEGER[] board_indices
        TEXT deal_session_id
        TEXT deal_tx_hash
        TEXT showdown_tx_hash
        TEXT showdown_winner
        BIGINT showdown_winning_amount
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }
    mpc_sessions {
        UUID id PK
        INTEGER table_id FK
        TEXT session_type
        TEXT phase
        TEXT status
        TEXT tx_hash
        TIMESTAMPTZ created_at
        TIMESTAMPTZ completed_at
    }
    auth_nonces {
        TEXT address PK
        BIGINT last_nonce
        TIMESTAMPTZ updated_at
    }
    player_stats {
        TEXT address PK
        INTEGER hands_played
        INTEGER hands_won
        BIGINT total_winnings
        TIMESTAMPTZ updated_at
    }
    rate_limit_configs {
        BIGSERIAL id PK
        TEXT config_type
        TEXT endpoint
        TEXT wallet_address
        INTEGER max_requests
        INTEGER window_seconds
        BOOLEAN enabled
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }
    cors_configs {
        BIGSERIAL id PK
        TEXT origin UK
        BOOLEAN enabled
        TEXT description
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }
    audit_logs {
        BIGSERIAL id PK
        UUID request_id
        TIMESTAMPTZ timestamp
        TEXT requester_address
        TEXT action
        TEXT endpoint
        TEXT method
        TEXT ip_address
        INTEGER response_status
        TEXT error_message
        INTEGER table_id
        TEXT session_id
        TEXT previous_hash
        TEXT record_hash
    }
    session_migrations {
        BIGSERIAL id PK
        TEXT session_id
        INTEGER table_id
        TEXT from_instance_id
        TEXT to_instance_id
        TEXT migration_status
        JSONB state_snapshot
        JSONB mpc_connections
        JSONB pending_actions
        TIMESTAMPTZ initiated_at
        TIMESTAMPTZ completed_at
        TEXT error_message
    }
    api_keys {
        BIGSERIAL id PK
        TEXT key_id UK
        TEXT key_hash
        TEXT node_id
        TEXT description
        BOOLEAN is_active
        TIMESTAMPTZ created_at
        TIMESTAMPTZ expires_at
        TIMESTAMPTZ last_used_at
        TIMESTAMPTZ revoked_at
        TEXT revoked_reason
    }
    api_key_usage_log {
        BIGSERIAL id PK
        TEXT key_id
        TEXT node_id
        TEXT endpoint
        TEXT ip_address
        TIMESTAMPTZ timestamp
        BOOLEAN success
    }
    game_tables ||--o{ mpc_sessions : "table_id"
```

## Tables Detail

### `game_tables`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `BIGSERIAL` | PRIMARY KEY | - |
| `table_id` | `INTEGER` | NOT NULL, UNIQUE | - |
| `phase` | `TEXT` | NOT NULL | - |
| `deck_root` | `TEXT` | Nullable | - |
| `proof_nonce` | `BIGINT` | NOT NULL | - |
| `player_order` | `TEXT[]` | NOT NULL | - |
| `hand_commitments` | `TEXT[]` | NOT NULL | - |
| `dealt_indices` | `INTEGER[]` | NOT NULL | - |
| `board_indices` | `INTEGER[]` | NOT NULL | - |
| `deal_session_id` | `TEXT` | Nullable | - |
| `deal_tx_hash` | `TEXT` | Nullable | - |
| `showdown_tx_hash` | `TEXT` | Nullable | - |
| `showdown_winner` | `TEXT` | Nullable | - |
| `showdown_winning_amount` | `BIGINT` | Nullable | - |
| `created_at` | `TIMESTAMPTZ` | NOT NULL | - |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL | - |

### `mpc_sessions`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `UUID` | PRIMARY KEY | - |
| `table_id` | `INTEGER` | NOT NULL | `game_tables.$table_id` |
| `session_type` | `TEXT` | NOT NULL | - |
| `phase` | `TEXT` | Nullable | - |
| `status` | `TEXT` | NOT NULL | - |
| `tx_hash` | `TEXT` | Nullable | - |
| `created_at` | `TIMESTAMPTZ` | NOT NULL | - |
| `completed_at` | `TIMESTAMPTZ` | Nullable | - |

### `auth_nonces`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `address` | `TEXT` | PRIMARY KEY | - |
| `last_nonce` | `BIGINT` | NOT NULL | - |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL | - |

### `player_stats`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `address` | `TEXT` | PRIMARY KEY | - |
| `hands_played` | `INTEGER` | NOT NULL | - |
| `hands_won` | `INTEGER` | NOT NULL | - |
| `total_winnings` | `BIGINT` | NOT NULL | - |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL | - |

### `rate_limit_configs`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `BIGSERIAL` | PRIMARY KEY | - |
| `config_type` | `TEXT` | NOT NULL | - |
| `endpoint` | `TEXT` | Nullable | - |
| `wallet_address` | `TEXT` | Nullable | - |
| `max_requests` | `INTEGER` | NOT NULL | - |
| `window_seconds` | `INTEGER` | NOT NULL | - |
| `enabled` | `BOOLEAN` | NOT NULL | - |
| `created_at` | `TIMESTAMPTZ` | NOT NULL | - |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL | - |

### `cors_configs`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `BIGSERIAL` | PRIMARY KEY | - |
| `origin` | `TEXT` | NOT NULL, UNIQUE | - |
| `enabled` | `BOOLEAN` | NOT NULL | - |
| `description` | `TEXT` | Nullable | - |
| `created_at` | `TIMESTAMPTZ` | NOT NULL | - |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL | - |

### `audit_logs`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `BIGSERIAL` | PRIMARY KEY | - |
| `request_id` | `UUID` | NOT NULL | - |
| `timestamp` | `TIMESTAMPTZ` | NOT NULL | - |
| `requester_address` | `TEXT` | Nullable | - |
| `action` | `TEXT` | NOT NULL | - |
| `endpoint` | `TEXT` | NOT NULL | - |
| `method` | `TEXT` | NOT NULL | - |
| `ip_address` | `TEXT` | Nullable | - |
| `response_status` | `INTEGER` | Nullable | - |
| `error_message` | `TEXT` | Nullable | - |
| `table_id` | `INTEGER` | Nullable | - |
| `session_id` | `TEXT` | Nullable | - |
| `previous_hash` | `TEXT` | Nullable | - |
| `record_hash` | `TEXT` | NOT NULL | - |

### `session_migrations`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `BIGSERIAL` | PRIMARY KEY | - |
| `session_id` | `TEXT` | NOT NULL | - |
| `table_id` | `INTEGER` | NOT NULL | - |
| `from_instance_id` | `TEXT` | NOT NULL | - |
| `to_instance_id` | `TEXT` | NOT NULL | - |
| `migration_status` | `TEXT` | NOT NULL | - |
| `state_snapshot` | `JSONB` | Nullable | - |
| `mpc_connections` | `JSONB` | Nullable | - |
| `pending_actions` | `JSONB` | Nullable | - |
| `initiated_at` | `TIMESTAMPTZ` | NOT NULL | - |
| `completed_at` | `TIMESTAMPTZ` | Nullable | - |
| `error_message` | `TEXT` | Nullable | - |

### `api_keys`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `BIGSERIAL` | PRIMARY KEY | - |
| `key_id` | `TEXT` | NOT NULL, UNIQUE | - |
| `key_hash` | `TEXT` | NOT NULL | - |
| `node_id` | `TEXT` | NOT NULL | - |
| `description` | `TEXT` | Nullable | - |
| `is_active` | `BOOLEAN` | NOT NULL | - |
| `created_at` | `TIMESTAMPTZ` | NOT NULL | - |
| `expires_at` | `TIMESTAMPTZ` | Nullable | - |
| `last_used_at` | `TIMESTAMPTZ` | Nullable | - |
| `revoked_at` | `TIMESTAMPTZ` | Nullable | - |
| `revoked_reason` | `TEXT` | Nullable | - |

### `api_key_usage_log`

| Column | Type | Constraints | Reference |
| --- | --- | --- | --- |
| `id` | `BIGSERIAL` | PRIMARY KEY | - |
| `key_id` | `TEXT` | NOT NULL | - |
| `node_id` | `TEXT` | NOT NULL | - |
| `endpoint` | `TEXT` | NOT NULL | - |
| `ip_address` | `TEXT` | Nullable | - |
| `timestamp` | `TIMESTAMPTZ` | NOT NULL | - |
| `success` | `BOOLEAN` | NOT NULL | - |
