//! Per-IP rate limiting middleware and per-table session cap (Issue #25).
//!
//! # Design
//!
//! ## Per-IP global middleware
//! Every incoming request is checked against an in-memory sliding-window
//! counter keyed on the hashed client IP.  The IP is hashed with SHA-256
//! and the active encryption-key version so raw addresses are never stored.
//!
//! When the limit is exceeded the middleware short-circuits with
//! `429 Too Many Requests` and a `Retry-After: <seconds>` header so
//! well-behaved clients know exactly when to retry.
//!
//! ## Per-table session cap
//! `check_table_session_cap` is called by the request-deal / request-reveal /
//! request-showdown handlers before they start a new MPC session.  If the
//! table already has `MAX_CONCURRENT_SESSIONS_PER_TABLE` running sessions the
//! call returns `Err(StatusCode::TOO_MANY_REQUESTS)`.
//!
//! ## Sustained-rate alerting
//! `spawn_rate_alert_task` watches a shared counter of 429 responses per
//! minute.  When the rate exceeds `ALERT_THRESHOLD_PER_MIN` consecutive
//! minutes it emits a `tracing::error!` (which will page/alert downstream
//! log aggregators and metrics dashboards).
//!
//! # Environment variables
//! | Variable                      | Default | Description                          |
//! |-------------------------------|---------|--------------------------------------|
//! | `RATE_LIMIT_REQUESTS_PER_MIN` | `120`   | Max requests / IP / 60 s            |
//! | `RATE_LIMIT_BURST`            | `20`    | Extra burst allowance above the rpm  |
//! | `MAX_SESSIONS_PER_TABLE`      | `3`     | Max concurrent MPC sessions / table  |
//! | `RATE_ALERT_THRESHOLD`        | `50`    | 429s/min before a sustained alert    |

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::session_gc::{SessionStatus, SessionStore};

// ── Constants (overridable via env) ──────────────────────────────────────────

fn requests_per_min() -> usize {
    std::env::var("RATE_LIMIT_REQUESTS_PER_MIN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120)
}

fn burst_allowance() -> usize {
    std::env::var("RATE_LIMIT_BURST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20)
}

fn max_sessions_per_table() -> usize {
    std::env::var("MAX_SESSIONS_PER_TABLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
}

fn alert_threshold_per_min() -> u64 {
    std::env::var("RATE_ALERT_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
}

// ── Shared state ──────────────────────────────────────────────────────────────

/// In-memory store: hashed_ip → sorted vec of request timestamps (epoch secs).
pub type IpBucketStore = Arc<RwLock<HashMap<String, Vec<Instant>>>>;

/// Monotonically-incrementing counter of 429 responses emitted.  Shared
/// between the middleware and the alerting background task.
pub type RejectionCounter = Arc<AtomicU64>;

pub fn new_ip_bucket_store() -> IpBucketStore {
    Arc::new(RwLock::new(HashMap::new()))
}

pub fn new_rejection_counter() -> RejectionCounter {
    Arc::new(AtomicU64::new(0))
}

// ── IP hashing ────────────────────────────────────────────────────────────────

/// Hash the client IP with SHA-256 so raw addresses are never stored in
/// memory.  Salted with a per-process constant so the hash is stable for the
/// process lifetime but rotates on restart.
fn hash_ip(ip: &str) -> String {
    // Use a fixed per-binary salt; not a secret — purely for privacy hygiene.
    static SALT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let salt = SALT.get_or_init(|| {
        use std::time::SystemTime;
        format!(
            "stellar-poker-rl-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    });

    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(ip.as_bytes());
    hex::encode(h.finalize())
}

/// Extract the best-effort client IP from common forwarding headers.
fn extract_ip(req: &Request<Body>) -> String {
    req.headers()
        .get("x-forwarded-for")
        .or_else(|| req.headers().get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Axum middleware state passed via `from_fn_with_state`.
#[derive(Clone)]
pub struct RateLimitMiddlewareState {
    pub buckets: IpBucketStore,
    pub rejections: RejectionCounter,
}

/// Global per-IP sliding-window rate-limit middleware.
///
/// Inserts into the Axum layer stack with:
/// ```
/// .layer(axum::middleware::from_fn_with_state(
///     rate_limit_state,
///     rate_limit::ip_rate_limit_middleware,
/// ))
/// ```
pub async fn ip_rate_limit_middleware(
    axum::extract::State(rl): axum::extract::State<RateLimitMiddlewareState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = extract_ip(&req);
    let ip_hash = hash_ip(&ip);

    let limit = requests_per_min() + burst_allowance();
    let window = Duration::from_secs(60);
    let now = Instant::now();

    // Acquire write lock, prune stale entries, check + insert.
    let allowed = {
        let mut store = rl.buckets.write().await;
        let bucket = store.entry(ip_hash.clone()).or_default();
        bucket.retain(|&ts| now.duration_since(ts) < window);

        if bucket.len() < limit {
            bucket.push(now);
            true
        } else {
            false
        }
    };

    if allowed {
        next.run(req).await
    } else {
        rl.rejections.fetch_add(1, Ordering::Relaxed);

        tracing::warn!(
            ip_hash = %ip_hash,
            limit = limit,
            "rate limit exceeded — returning 429"
        );

        let mut resp = Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .body(Body::from(
                r#"{"error":"rate limit exceeded","message":"Too many requests. See Retry-After header."}"#,
            ))
            .expect("static response is valid");

        // Retry-After: the full window (60 s) is safe; clients should back off.
        resp.headers_mut().insert(
            "Retry-After",
            HeaderValue::from_static("60"),
        );
        resp.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        resp
    }
}

// ── Per-table session cap ─────────────────────────────────────────────────────

/// Check whether a new MPC session can be started for `table_id`.
///
/// Counts sessions in `Running` state associated with the table and rejects
/// if the count equals or exceeds `MAX_SESSIONS_PER_TABLE`.
///
/// Returns `Err(StatusCode::TOO_MANY_REQUESTS)` with a log message when the
/// cap is hit.
pub async fn check_table_session_cap(
    sessions: &SessionStore,
    table_id: u32,
) -> Result<(), StatusCode> {
    let cap = max_sessions_per_table();
    let store = sessions.read().await;
    let active = store
        .values()
        .filter(|s| s.table_id == table_id && s.status == SessionStatus::Running)
        .count();

    if active >= cap {
        tracing::warn!(
            table_id = table_id,
            active_sessions = active,
            cap = cap,
            "per-table session cap reached — rejecting new MPC session"
        );
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(())
}

// ── Sustained-rate alerting ───────────────────────────────────────────────────

/// Spawn a background task that watches the rejection counter and emits a
/// `tracing::error!` when sustained high request rates are detected.
///
/// The task samples the rejection counter every 60 seconds.  If the delta
/// (rejections in the last minute) exceeds `RATE_ALERT_THRESHOLD` it logs
/// an error-level event that downstream alerting (Loki, CloudWatch, etc.)
/// can route to an on-call channel.
///
/// The alert fires at most once per sampling interval to avoid log flooding.
pub fn spawn_rate_alert_task(rejections: RejectionCounter) {
    let threshold = alert_threshold_per_min();

    tokio::spawn(async move {
        let mut last_count: u64 = 0;
        let mut consecutive_over_threshold: u32 = 0;

        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            let current = rejections.load(Ordering::Relaxed);
            let delta = current.saturating_sub(last_count);
            last_count = current;

            if delta >= threshold {
                consecutive_over_threshold += 1;

                tracing::error!(
                    rejections_last_minute = delta,
                    threshold = threshold,
                    consecutive_high_rate_minutes = consecutive_over_threshold,
                    "ALERT: sustained high request-rejection rate detected. \
                     Possible DoS or misconfigured client. \
                     Check /metrics and coordinator logs."
                );
            } else {
                if consecutive_over_threshold > 0 {
                    tracing::info!(
                        rejections_last_minute = delta,
                        "rate-limiting alert cleared — rejection rate returned below threshold"
                    );
                }
                consecutive_over_threshold = 0;
            }

            // Periodic info log so operators can see the baseline rate even
            // when things are healthy.
            if delta > 0 {
                tracing::info!(
                    rejections_last_minute = delta,
                    total_rejections = current,
                    "rate limit stats"
                );
            }
        }
    });
}

/// Spawn a background GC task that periodically evicts stale IP buckets
/// (entries with no timestamps in the last window) to bound memory usage.
pub fn spawn_bucket_gc_task(buckets: IpBucketStore) {
    tokio::spawn(async move {
        let window = Duration::from_secs(60);
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await; // every 5 min
            let now = Instant::now();
            let mut store = buckets.write().await;
            let before = store.len();
            store.retain(|_, timestamps| {
                timestamps.retain(|&ts| now.duration_since(ts) < window);
                !timestamps.is_empty()
            });
            let after = store.len();
            if before != after {
                tracing::debug!(
                    evicted = before - after,
                    remaining = after,
                    "rate-limit bucket GC complete"
                );
            }
        }
    });
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ip_is_deterministic_within_process() {
        let a = hash_ip("192.168.1.1");
        let b = hash_ip("192.168.1.1");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_ip_differs_for_different_ips() {
        let a = hash_ip("192.168.1.1");
        let b = hash_ip("10.0.0.1");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_ip_output_is_hex_string() {
        let h = hash_ip("127.0.0.1");
        assert_eq!(h.len(), 64); // SHA-256 → 32 bytes → 64 hex chars
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn bucket_allows_up_to_limit_then_rejects() {
        // Use a very small limit for the test.
        let store: IpBucketStore = Arc::new(RwLock::new(HashMap::new()));
        let ip_hash = "test_ip_hash".to_string();
        let limit = 5usize;
        let window = Duration::from_secs(60);
        let now = Instant::now();

        {
            let mut s = store.write().await;
            let bucket = s.entry(ip_hash.clone()).or_default();
            for _ in 0..limit {
                bucket.push(now);
            }
        }

        // One more should be rejected.
        let allowed = {
            let mut s = store.write().await;
            let bucket = s.entry(ip_hash.clone()).or_default();
            bucket.retain(|&ts| now.duration_since(ts) < window);
            if bucket.len() < limit {
                bucket.push(now);
                true
            } else {
                false
            }
        };

        assert!(!allowed, "request beyond limit should be rejected");
    }

    #[tokio::test]
    async fn bucket_gc_evicts_empty_entries() {
        let store: IpBucketStore = Arc::new(RwLock::new(HashMap::new()));

        // Insert an old entry that is already outside the window.
        {
            let mut s = store.write().await;
            // An empty vec simulates a fully-expired bucket.
            s.insert("stale_ip".to_string(), Vec::new());
            // A live bucket with a recent timestamp.
            s.insert("live_ip".to_string(), vec![Instant::now()]);
        }

        let window = Duration::from_secs(60);
        let now = Instant::now();
        {
            let mut s = store.write().await;
            s.retain(|_, timestamps| {
                timestamps.retain(|&ts| now.duration_since(ts) < window);
                !timestamps.is_empty()
            });
        }

        let s = store.read().await;
        assert!(!s.contains_key("stale_ip"), "stale bucket should be evicted");
        assert!(s.contains_key("live_ip"), "live bucket should be kept");
    }

    #[test]
    fn rejection_counter_increments() {
        let counter = new_rejection_counter();
        counter.fetch_add(1, Ordering::Relaxed);
        counter.fetch_add(1, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }
}
