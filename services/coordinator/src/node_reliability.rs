//! Graceful degradation when MPC nodes are slow (Issue #110).
//!
//! Every coordinator → node round trip is timed. When a node's response
//! exceeds `MPC_NODE_EXPECTED_RESPONSE_MS` (default 2000ms):
//!   - the node is logged as slow at WARN level, and
//!   - its reliability score is reduced.
//!
//! During proof-generation polling ([`crate::mpc::generate_proof_from_share_sets`])
//! a slow-but-still-responding node extends the session's poll deadline
//! instead of the session failing outright, up to a bounded hard ceiling so
//! a genuinely dead node still eventually times out.
//!
//! The score is an intentionally simple bounded indicator in `[0, 100]`
//! (100 = fully reliable): each slow response subtracts a penalty, each
//! timely response recovers a smaller amount, so a node has to be
//! persistently slow — not just have one bad round trip — to end up flagged
//! as an offender. This is process-local, in-memory state; it resets on
//! restart and is not shared across coordinator instances.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::RwLock;

const DEFAULT_EXPECTED_RESPONSE_MS: u64 = 2_000;
const SCORE_MAX: f64 = 100.0;
const SCORE_MIN: f64 = 0.0;
const SCORE_PENALTY: f64 = 10.0;
const SCORE_RECOVERY: f64 = 2.0;
const INITIAL_SCORE: f64 = 100.0;
/// Below this score a node is logged as a persistent offender.
const OFFENDER_THRESHOLD: f64 = 30.0;

#[derive(Clone, Debug, Default)]
struct Entry {
    score: f64,
    slow_responses: u64,
    total_responses: u64,
}

type ReliabilityStore = Arc<RwLock<HashMap<String, Entry>>>;

static RELIABILITY: OnceLock<ReliabilityStore> = OnceLock::new();

fn store() -> &'static ReliabilityStore {
    RELIABILITY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// Expected coordinator → MPC node response time. Round trips slower than
/// this are treated as "slow" for both deadline extension and scoring.
pub fn expected_response_time() -> Duration {
    let ms = std::env::var("MPC_NODE_EXPECTED_RESPONSE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_EXPECTED_RESPONSE_MS);
    Duration::from_millis(ms)
}

/// Record a single node round-trip latency, updating its reliability score.
/// Returns `true` if this response was slower than expected.
pub async fn record(endpoint: &str, latency: Duration) -> bool {
    let expected = expected_response_time();
    let is_slow = latency > expected;

    let mut map = store().write().await;
    let entry = map.entry(endpoint.to_string()).or_insert(Entry {
        score: INITIAL_SCORE,
        slow_responses: 0,
        total_responses: 0,
    });
    entry.total_responses += 1;

    if is_slow {
        entry.slow_responses += 1;
        entry.score = (entry.score - SCORE_PENALTY).max(SCORE_MIN);
        tracing::warn!(
            node = %endpoint,
            latency_ms = latency.as_millis(),
            expected_ms = expected.as_millis(),
            score = entry.score,
            "MPC node responded slowly"
        );
        if entry.score < OFFENDER_THRESHOLD {
            tracing::error!(
                node = %endpoint,
                score = entry.score,
                slow_responses = entry.slow_responses,
                total_responses = entry.total_responses,
                "MPC node is a persistent slow-response offender"
            );
        }
    } else {
        entry.score = (entry.score + SCORE_RECOVERY).min(SCORE_MAX);
    }

    is_slow
}

/// Current reliability score for a node, or the default (fully reliable)
/// score if it has never been observed.
#[allow(dead_code)]
pub async fn score(endpoint: &str) -> f64 {
    store()
        .read()
        .await
        .get(endpoint)
        .map(|e| e.score)
        .unwrap_or(SCORE_MAX)
}

#[cfg(test)]
mod tests {
    //! Each test uses a unique endpoint string since the reliability store
    //! is a process-wide static shared across parallel test threads.
    use super::*;

    #[tokio::test]
    async fn unknown_node_defaults_to_full_score() {
        let score = score("http://unknown-node-reliability-test").await;
        assert_eq!(score, SCORE_MAX);
    }

    #[tokio::test]
    async fn fast_response_is_not_flagged_slow() {
        let endpoint = "http://fast-node-reliability-test";
        let was_slow = record(endpoint, Duration::from_millis(10)).await;
        assert!(!was_slow);
        assert!(score(endpoint).await >= INITIAL_SCORE);
    }

    #[tokio::test]
    async fn slow_response_reduces_score() {
        let endpoint = "http://slow-node-reliability-test";
        let expected = expected_response_time();
        let was_slow = record(endpoint, expected + Duration::from_secs(1)).await;
        assert!(was_slow);
        assert!(score(endpoint).await < INITIAL_SCORE);
    }

    #[tokio::test]
    async fn persistent_slowness_drives_score_toward_offender_threshold() {
        let endpoint = "http://persistent-offender-node-reliability-test";
        let expected = expected_response_time();
        for _ in 0..10 {
            record(endpoint, expected + Duration::from_secs(1)).await;
        }
        assert!(score(endpoint).await < OFFENDER_THRESHOLD);
    }
}
