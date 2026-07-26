//! Database-backed rate limiting with dynamic configuration
//!
//! Issue #267: Allow dynamic rate limit configuration via admin API.
//! Update limits per endpoint, per wallet, or globally without restart.
//! Persist configuration to database.

use sqlx::PgPool;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RateLimitConfig {
    pub id: i64,
    pub config_type: String,
    pub endpoint: Option<String>,
    pub wallet_address: Option<String>,
    pub max_requests: i32,
    pub window_seconds: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct RateLimitBucket {
    pub timestamps: Vec<Instant>,
    pub max_requests: usize,
    pub window: Duration,
}

impl RateLimitBucket {
    pub fn new(max_requests: usize, window_seconds: u64) -> Self {
        Self {
            timestamps: Vec::new(),
            max_requests,
            window: Duration::from_secs(window_seconds),
        }
    }

    pub fn check_and_add(&mut self) -> bool {
        let now = Instant::now();
        self.timestamps.retain(|&ts| now.duration_since(ts) < self.window);
        
        if self.timestamps.len() < self.max_requests {
            self.timestamps.push(now);
            true
        } else {
            false
        }
    }
}

/// Load all rate limit configurations from the database
pub async fn load_rate_limit_configs(pool: &PgPool) -> Result<Vec<RateLimitConfig>, sqlx::Error> {
    sqlx::query_as::<_, RateLimitConfig>(
        "SELECT id, config_type, endpoint, wallet_address, max_requests, window_seconds, enabled 
         FROM rate_limit_configs 
         WHERE enabled = true 
         ORDER BY config_type DESC"
    )
    .fetch_all(pool)
    .await
}

/// Get the most specific rate limit for a given endpoint and wallet
pub async fn get_rate_limit_for_request(
    pool: &PgPool,
    endpoint: &str,
    wallet: Option<&str>,
) -> Result<Option<RateLimitConfig>, sqlx::Error> {
    // Priority: endpoint_wallet > wallet > endpoint > global
    
    if let Some(wallet_addr) = wallet {
        // Check endpoint + wallet specific
        if let Some(config) = sqlx::query_as::<_, RateLimitConfig>(
            "SELECT id, config_type, endpoint, wallet_address, max_requests, window_seconds, enabled 
             FROM rate_limit_configs 
             WHERE config_type = 'endpoint_wallet' 
             AND endpoint = $1 
             AND wallet_address = $2 
             AND enabled = true 
             LIMIT 1"
        )
        .bind(endpoint)
        .bind(wallet_addr)
        .fetch_optional(pool)
        .await? 
        {
            return Ok(Some(config));
        }
        
        // Check wallet-only
        if let Some(config) = sqlx::query_as::<_, RateLimitConfig>(
            "SELECT id, config_type, endpoint, wallet_address, max_requests, window_seconds, enabled 
             FROM rate_limit_configs 
             WHERE config_type = 'wallet' 
             AND wallet_address = $1 
             AND enabled = true 
             LIMIT 1"
        )
        .bind(wallet_addr)
        .fetch_optional(pool)
        .await? 
        {
            return Ok(Some(config));
        }
    }
    
    // Check endpoint-only
    if let Some(config) = sqlx::query_as::<_, RateLimitConfig>(
        "SELECT id, config_type, endpoint, wallet_address, max_requests, window_seconds, enabled 
         FROM rate_limit_configs 
         WHERE config_type = 'endpoint' 
         AND endpoint = $1 
         AND enabled = true 
         LIMIT 1"
    )
    .bind(endpoint)
    .fetch_optional(pool)
    .await? 
    {
        return Ok(Some(config));
    }
    
    // Fall back to global
    sqlx::query_as::<_, RateLimitConfig>(
        "SELECT id, config_type, endpoint, wallet_address, max_requests, window_seconds, enabled 
         FROM rate_limit_configs 
         WHERE config_type = 'global' 
         AND enabled = true 
         LIMIT 1"
    )
    .fetch_optional(pool)
    .await
}

/// Create or update a rate limit configuration
pub async fn upsert_rate_limit_config(
    pool: &PgPool,
    config_type: &str,
    endpoint: Option<&str>,
    wallet_address: Option<&str>,
    max_requests: i32,
    window_seconds: i32,
    enabled: bool,
) -> Result<RateLimitConfig, sqlx::Error> {
    sqlx::query_as::<_, RateLimitConfig>(
        "INSERT INTO rate_limit_configs 
         (config_type, endpoint, wallet_address, max_requests, window_seconds, enabled) 
         VALUES ($1, $2, $3, $4, $5, $6) 
         ON CONFLICT (config_type, endpoint, wallet_address) 
         DO UPDATE SET 
             max_requests = EXCLUDED.max_requests, 
             window_seconds = EXCLUDED.window_seconds, 
             enabled = EXCLUDED.enabled,
             updated_at = NOW()
         RETURNING id, config_type, endpoint, wallet_address, max_requests, window_seconds, enabled"
    )
    .bind(config_type)
    .bind(endpoint)
    .bind(wallet_address)
    .bind(max_requests)
    .bind(window_seconds)
    .bind(enabled)
    .fetch_one(pool)
    .await
}

/// Delete a rate limit configuration
pub async fn delete_rate_limit_config(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM rate_limit_configs WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    
    Ok(result.rows_affected() > 0)
}

/// List all rate limit configurations
pub async fn list_rate_limit_configs(pool: &PgPool) -> Result<Vec<RateLimitConfig>, sqlx::Error> {
    sqlx::query_as::<_, RateLimitConfig>(
        "SELECT id, config_type, endpoint, wallet_address, max_requests, window_seconds, enabled 
         FROM rate_limit_configs 
         ORDER BY config_type, endpoint, wallet_address"
    )
    .fetch_all(pool)
    .await
}
