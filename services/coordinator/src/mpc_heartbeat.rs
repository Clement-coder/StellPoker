//! MPC node heartbeat monitoring and failure detection
//!
//! Issue #94: Add heartbeat mechanism where each MPC node sends liveness signal to coordinator.
//! Mark unhealthy after N missed heartbeats. Trigger committee failover.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// How often nodes should send heartbeats (seconds)
    pub heartbeat_interval: u64,
    /// Number of consecutive missed heartbeats before marking unhealthy
    pub max_missed_heartbeats: u32,
    /// Grace period before first heartbeat expected (seconds)
    pub startup_grace_period: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: 10,
            max_missed_heartbeats: 3,
            startup_grace_period: 30,
        }
    }
}

impl HeartbeatConfig {
    pub fn from_env() -> Self {
        Self {
            heartbeat_interval: std::env::var("MPC_HEARTBEAT_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            max_missed_heartbeats: std::env::var("MPC_MAX_MISSED_HEARTBEATS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
            startup_grace_period: std::env::var("MPC_STARTUP_GRACE_PERIOD_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeHeartbeatState {
    pub node_id: u32,
    pub endpoint: String,
    pub last_heartbeat: Option<SystemTime>,
    pub consecutive_failures: u32,
    pub is_healthy: bool,
    pub registered_at: SystemTime,
}

impl NodeHeartbeatState {
    pub fn new(node_id: u32, endpoint: String) -> Self {
        Self {
            node_id,
            endpoint,
            last_heartbeat: None,
            consecutive_failures: 0,
            is_healthy: true,
            registered_at: SystemTime::now(),
        }
    }

    pub fn record_success(&mut self) {
        self.last_heartbeat = Some(SystemTime::now());
        self.consecutive_failures = 0;
        self.is_healthy = true;
    }

    pub fn record_failure(&mut self, max_failures: u32) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= max_failures {
            self.is_healthy = false;
        }
    }

    pub fn is_in_grace_period(&self, grace_seconds: u64) -> bool {
        SystemTime::now()
            .duration_since(self.registered_at)
            .map(|d| d.as_secs() < grace_seconds)
            .unwrap_or(false)
    }
}

pub type HeartbeatStore = Arc<RwLock<HashMap<u32, NodeHeartbeatState>>>;

/// Initialize heartbeat monitoring for all MPC nodes
pub fn init_heartbeat_store(endpoints: &[(u32, String)]) -> HeartbeatStore {
    let mut store = HashMap::new();
    for (node_id, endpoint) in endpoints {
        store.insert(
            *node_id,
            NodeHeartbeatState::new(*node_id, endpoint.clone()),
        );
    }
    Arc::new(RwLock::new(store))
}

/// Spawn background task to monitor MPC node heartbeats
pub fn spawn_heartbeat_monitor(
    store: HeartbeatStore,
    config: HeartbeatConfig,
    on_node_failure: Arc<dyn Fn(u32, String) + Send + Sync>,
) {
    tokio::spawn(async move {
        let check_interval = Duration::from_secs(config.heartbeat_interval);

        loop {
            tokio::time::sleep(check_interval).await;

            let mut store_guard = store.write().await;
            let now = SystemTime::now();

            for (node_id, state) in store_guard.iter_mut() {
                // Skip nodes still in startup grace period
                if state.is_in_grace_period(config.startup_grace_period) {
                    continue;
                }

                // Check if heartbeat is overdue
                let is_overdue = match state.last_heartbeat {
                    Some(last) => now
                        .duration_since(last)
                        .map(|d| d.as_secs() > config.heartbeat_interval * 2)
                        .unwrap_or(true),
                    None => true,
                };

                if is_overdue {
                    let was_healthy = state.is_healthy;
                    state.record_failure(config.max_missed_heartbeats);

                    if was_healthy && !state.is_healthy {
                        tracing::error!(
                            "MPC Node {} ({}) marked unhealthy after {} consecutive failures",
                            node_id,
                            state.endpoint,
                            state.consecutive_failures
                        );

                        let endpoint = state.endpoint.clone();
                        let node_id = *node_id;
                        on_node_failure(node_id, endpoint);
                    }
                }
            }
        }
    });
}

/// Record a successful heartbeat from a node
pub async fn record_heartbeat(store: &HeartbeatStore, node_id: u32) -> Result<(), String> {
    let mut guard = store.write().await;
    let state = guard
        .get_mut(&node_id)
        .ok_or_else(|| format!("Unknown node_id: {}", node_id))?;

    let was_unhealthy = !state.is_healthy;
    state.record_success();

    if was_unhealthy {
        tracing::info!(
            "MPC Node {} ({}) recovered and marked healthy",
            node_id,
            state.endpoint
        );
    }

    Ok(())
}

/// Get health status of all nodes
pub async fn get_all_health_status(store: &HeartbeatStore) -> Vec<NodeHealthStatus> {
    let guard = store.read().await;
    guard
        .values()
        .map(|state| NodeHealthStatus {
            node_id: state.node_id,
            endpoint: state.endpoint.clone(),
            is_healthy: state.is_healthy,
            last_heartbeat: state.last_heartbeat,
            consecutive_failures: state.consecutive_failures,
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeHealthStatus {
    pub node_id: u32,
    pub endpoint: String,
    pub is_healthy: bool,
    pub last_heartbeat: Option<SystemTime>,
    pub consecutive_failures: u32,
}

/// Check if minimum number of healthy nodes are available for MPC operations
pub async fn check_committee_quorum(store: &HeartbeatStore, min_healthy: usize) -> bool {
    let guard = store.read().await;
    let healthy_count = guard.values().filter(|s| s.is_healthy).count();
    healthy_count >= min_healthy
}

/// Trigger committee failover procedure
pub async fn trigger_failover(store: &HeartbeatStore, failed_node_id: u32) -> Result<(), String> {
    tracing::warn!("Triggering committee failover for node {}", failed_node_id);

    let healthy_nodes = get_all_health_status(store)
        .await
        .into_iter()
        .filter(|s| s.is_healthy && s.node_id != failed_node_id)
        .collect::<Vec<_>>();

    if healthy_nodes.len() < 2 {
        return Err(format!(
            "Cannot failover: insufficient healthy nodes (need 2, have {})",
            healthy_nodes.len()
        ));
    }

    tracing::info!(
        "Failover initiated: {} healthy nodes available: {:?}",
        healthy_nodes.len(),
        healthy_nodes.iter().map(|n| n.node_id).collect::<Vec<_>>()
    );

    // In production, this would:
    // 1. Cancel ongoing MPC sessions using the failed node
    // 2. Redistribute shares to healthy nodes
    // 3. Update routing to exclude failed node
    // 4. Notify monitoring systems

    Ok(())
}
