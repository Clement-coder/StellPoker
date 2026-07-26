//! PostgreSQL advisory-lock-based leader election for coordinator HA deployments
//! (issue #266).
//!
//! ## How it works
//!
//! When multiple coordinator replicas run behind a load balancer (e.g. AWS ALB),
//! only one of them should be the **leader** — the one that accepts new game
//! sessions, orchestrates MPC proofs, and writes to Soroban. Followers serve
//! read-only endpoints (health, stats, table state) and return `503 Not Leader`
//! for write operations.
//!
//! Leader election uses a PostgreSQL *session-level advisory lock*:
//!
//! - `pg_try_advisory_lock(LOCK_KEY)` — non-blocking; returns `true` if the
//!   calling session acquired the lock, `false` if another session already holds it.
//! - The lock is tied to the *physical database session* (TCP connection). When
//!   the leader process exits or loses its database connection, the lock is
//!   released automatically and a follower can acquire it within `POLL_INTERVAL`.
//!
//! ## Advisory lock vs. `pg_advisory_lock`
//!
//! `pg_advisory_lock` (blocking) would stall the startup of new replicas.
//! `pg_try_advisory_lock` (non-blocking) lets followers start immediately and
//! serve read-only traffic while polling for leadership.
//!
//! ## etcd alternative
//!
//! If the deployment does not include PostgreSQL, etcd can be used instead
//! (see [`LeaderBackend`] trait). The `pg_try_advisory_lock` approach is the
//! default because the coordinator already depends on Postgres.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use sqlx::PgPool;
use tracing::{debug, info, warn};

/// Fixed 64-bit key for the coordinator leader advisory lock.
/// Chosen to be unique within the application's use of advisory locks.
const LEADER_LOCK_KEY: i64 = 0x5354_454C_4C50_4B52; // "STELLPKR" in hex

/// How often a follower retries acquiring the lock.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// How often the leader sends a keepalive query to prevent its pooled
/// connection from timing out and inadvertently releasing the lock.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// Shared boolean that the rest of the application reads to decide whether
/// this instance is the current leader.
#[derive(Clone)]
pub struct LeaderState {
    is_leader: Arc<AtomicBool>,
}

impl LeaderState {
    pub fn new() -> Self {
        Self {
            is_leader: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns `true` if this instance currently holds the leader lock.
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::SeqCst)
    }

    /// Force-sets the leader flag. Used in single-node (no-database) mode
    /// where there is no contention and this instance is always the leader.
    pub fn force_leader(&self) {
        self.is_leader.store(true, Ordering::SeqCst);
    }

    /// Returns a copy of the inner `Arc<AtomicBool>` for passing into the
    /// background task without cloning the full [`LeaderState`].
    fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_leader)
    }
}

impl Default for LeaderState {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns the leader-election background task.
///
/// The task acquires a dedicated connection from the pool and attempts to
/// hold the PostgreSQL session-level advisory lock. While it holds the lock
/// the `LeaderState` is set to `true`; it is set to `false` on loss or error.
///
/// This function returns immediately; the task runs indefinitely.
pub fn spawn(pool: Arc<PgPool>, state: LeaderState) {
    let flag = state.flag();
    tokio::spawn(async move {
        loop {
            run_election_loop(&pool, Arc::clone(&flag)).await;
            // If we reach here, either the lock was lost or the DB connection
            // failed. Wait a moment before retrying so we don't spin tight.
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

/// Inner loop: acquires a connection, tries the lock, holds it, or polls.
async fn run_election_loop(pool: &PgPool, flag: Arc<AtomicBool>) {
    let mut conn = match pool.acquire().await {
        Ok(c) => c,
        Err(e) => {
            warn!("leader-election: could not acquire DB connection: {}", e);
            flag.store(false, Ordering::SeqCst);
            return;
        }
    };

    // Non-blocking attempt to get the advisory lock.
    let acquired: bool = match sqlx::query_scalar(
        "SELECT pg_try_advisory_lock($1)",
    )
    .bind(LEADER_LOCK_KEY)
    .fetch_one(&mut *conn)
    .await
    {
        Ok(b) => b,
        Err(e) => {
            warn!("leader-election: advisory lock query failed: {}", e);
            flag.store(false, Ordering::SeqCst);
            return;
        }
    };

    if !acquired {
        // Another instance holds the lock; this one is a follower.
        debug!("leader-election: follower (lock held by another instance)");
        flag.store(false, Ordering::SeqCst);
        // Wait before retrying so we become leader quickly when the current
        // leader exits.
        tokio::time::sleep(POLL_INTERVAL).await;
        return;
    }

    // We acquired the lock — this instance is the leader.
    info!(
        "leader-election: acquired advisory lock (key=0x{:X}); this instance is the leader",
        LEADER_LOCK_KEY
    );
    flag.store(true, Ordering::SeqCst);

    // Hold the connection alive with periodic keepalives. The advisory lock
    // is released automatically when this connection closes.
    loop {
        tokio::time::sleep(KEEPALIVE_INTERVAL).await;
        match sqlx::query("SELECT 1").execute(&mut *conn).await {
            Ok(_) => {
                debug!("leader-election: keepalive OK");
            }
            Err(e) => {
                warn!("leader-election: keepalive failed, releasing leader role: {}", e);
                flag.store(false, Ordering::SeqCst);
                // Connection is broken; drop it so the advisory lock is released.
                return;
            }
        }
    }
}

/// Returns an HTTP status code and JSON body for requests that require
/// leadership but this instance is a follower.
///
/// The caller should return a `503 Service Unavailable` response with this body
/// so that clients can detect the condition and optionally retry against a
/// different replica.
pub fn not_leader_response() -> serde_json::Value {
    serde_json::json!({
        "error": "not_leader",
        "message": "This coordinator instance is a follower. Retry the request against the leader replica.",
        "hint": "Use the /api/leader endpoint to identify the current leader."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leader_state_default_is_follower() {
        let state = LeaderState::new();
        assert!(!state.is_leader(), "new instance must start as follower");
    }

    #[test]
    fn leader_state_can_be_set() {
        let state = LeaderState::new();
        state.is_leader.store(true, Ordering::SeqCst);
        assert!(state.is_leader());
        state.is_leader.store(false, Ordering::SeqCst);
        assert!(!state.is_leader());
    }

    #[test]
    fn leader_state_clone_shares_flag() {
        let s1 = LeaderState::new();
        let s2 = s1.clone();
        s1.is_leader.store(true, Ordering::SeqCst);
        assert!(s2.is_leader(), "cloned state must share the same atomic flag");
    }
}
