-- Rollback migration for API key authentication

DROP TABLE IF EXISTS api_key_usage_log;
DROP TABLE IF EXISTS api_keys;
