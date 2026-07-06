-- Migration for MPC node API key authentication (issue #310)
-- Stores hashed API keys with rotation and revocation capabilities

-- -------------------------------------------------------------------------
-- api_keys
-- API keys for MPC node authentication
-- -------------------------------------------------------------------------
CREATE TABLE api_keys (
    id                BIGSERIAL    PRIMARY KEY,
    key_id            TEXT         NOT NULL UNIQUE,
    key_hash          TEXT         NOT NULL,
    node_id           TEXT         NOT NULL,
    description       TEXT,
    is_active         BOOLEAN      NOT NULL DEFAULT true,
    created_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at        TIMESTAMPTZ,
    last_used_at      TIMESTAMPTZ,
    revoked_at        TIMESTAMPTZ,
    revoked_reason    TEXT
);

CREATE INDEX idx_api_keys_key_id ON api_keys (key_id);
CREATE INDEX idx_api_keys_node_id ON api_keys (node_id);
CREATE INDEX idx_api_keys_active ON api_keys (is_active);
CREATE INDEX idx_api_keys_expires_at ON api_keys (expires_at);

-- -------------------------------------------------------------------------
-- api_key_usage_log
-- Tracks API key usage for monitoring and forensics
-- -------------------------------------------------------------------------
CREATE TABLE api_key_usage_log (
    id                BIGSERIAL    PRIMARY KEY,
    key_id            TEXT         NOT NULL,
    node_id           TEXT         NOT NULL,
    endpoint          TEXT         NOT NULL,
    ip_address        TEXT,
    timestamp         TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    success           BOOLEAN      NOT NULL
);

CREATE INDEX idx_api_key_usage_log_key_id ON api_key_usage_log (key_id);
CREATE INDEX idx_api_key_usage_log_timestamp ON api_key_usage_log (timestamp DESC);
CREATE INDEX idx_api_key_usage_log_node_id ON api_key_usage_log (node_id);
