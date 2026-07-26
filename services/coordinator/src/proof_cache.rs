//! Coordinator-side caching of verified proofs (Issue #108).
//!
//! The coordinator does not verify UltraHonk proofs locally — verification
//! happens on-chain inside the Soroban `zk-verifier`/`poker-table` contract
//! when a proof is submitted via `commit_deal` / `reveal_board` /
//! `submit_showdown` (see `soroban::proofs`). That on-chain verification is
//! the expensive step this cache protects: if a client (or the coordinator's
//! own retry logic) re-submits byte-identical proof data, we skip the
//! redundant on-chain invocation entirely and replay the previously
//! observed transaction hash.
//!
//! The cache is content-addressable: the key is a SHA-256 hash of the proof
//! bytes plus its public inputs, so it is independent of which table/session
//! produced the proof. Entries expire after a TTL proportional to the
//! Stellar ledger close time — once several ledgers have closed, the
//! original submission is final and a resubmission would no longer be a
//! same-round retry, so the entry is left to expire and a fresh submission
//! is allowed.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

struct CachedProof {
    tx_hash: String,
    inserted_at: Instant,
    ttl: Duration,
}

type ProofCacheStore = Arc<RwLock<HashMap<String, CachedProof>>>;

static PROOF_CACHE: OnceLock<ProofCacheStore> = OnceLock::new();

fn store() -> &'static ProofCacheStore {
    PROOF_CACHE.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// Content-address a proof by its bytes and public inputs.
pub fn proof_key(proof: &[u8], public_inputs: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(proof);
    for pi in public_inputs {
        hasher.update(pi.as_bytes());
        hasher.update(b"|");
    }
    hex::encode(hasher.finalize())
}

/// TTL proportional to the Stellar ledger close time: cache a verified
/// proof's result for `PROOF_CACHE_TTL_LEDGERS` ledger-close intervals.
fn ttl() -> Duration {
    let ledger_close_secs: u64 = std::env::var("SOROBAN_LEDGER_CLOSE_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let ttl_ledgers: u64 = std::env::var("PROOF_CACHE_TTL_LEDGERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12);
    Duration::from_secs(ledger_close_secs.saturating_mul(ttl_ledgers).max(1))
}

/// Look up a cached on-chain result for this exact proof, if still fresh.
pub async fn get(key: &str) -> Option<String> {
    let cache = store().read().await;
    cache.get(key).and_then(|entry| {
        if entry.inserted_at.elapsed() < entry.ttl {
            Some(entry.tx_hash.clone())
        } else {
            None
        }
    })
}

/// Record a verified proof's on-chain submission result, content-addressed
/// by `key` (see [`proof_key`]). Also opportunistically evicts expired
/// entries so the cache doesn't grow unbounded.
pub async fn insert(key: String, tx_hash: String) {
    let mut cache = store().write().await;
    cache.insert(
        key,
        CachedProof {
            tx_hash,
            inserted_at: Instant::now(),
            ttl: ttl(),
        },
    );
    cache.retain(|_, v| v.inserted_at.elapsed() < v.ttl);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_proof_and_inputs_hash_to_the_same_key() {
        let proof = vec![1u8, 2, 3, 4];
        let inputs = vec!["1".to_string(), "2".to_string()];
        assert_eq!(proof_key(&proof, &inputs), proof_key(&proof, &inputs));
    }

    #[test]
    fn different_public_inputs_change_the_key() {
        let proof = vec![1u8, 2, 3, 4];
        let a = proof_key(&proof, &["1".to_string()]);
        let b = proof_key(&proof, &["2".to_string()]);
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn cache_miss_then_hit_after_insert() {
        let key = proof_key(b"unique-test-proof-bytes", &["42".to_string()]);
        assert_eq!(get(&key).await, None);

        insert(key.clone(), "tx-hash-abc".to_string()).await;
        assert_eq!(get(&key).await, Some("tx-hash-abc".to_string()));
    }
}
