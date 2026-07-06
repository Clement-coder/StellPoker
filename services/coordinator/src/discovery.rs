//! Dynamic MPC node discovery.
//!
//! By default the coordinator talks to a fixed set of MPC node endpoints taken
//! from the `MPC_NODE_0/1/2` environment variables (the historical, fully
//! backward-compatible behaviour). When those env vars are *not* set, nodes
//! register themselves at runtime via `POST /api/node/register` and keep their
//! registration alive with periodic heartbeats (`POST /api/node/{id}/heartbeat`
//! or by re-`register`ing). A node is considered healthy only while its most
//! recent heartbeat is within [`HEARTBEAT_TIMEOUT`].
//!
//! The registry is intentionally in-memory: discovery state is ephemeral and
//! rebuilt from node heartbeats, so it does not need to survive a restart.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A node is considered unhealthy once this much time has elapsed without a
/// heartbeat.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum number of healthy nodes required to orchestrate a session.
pub const MIN_HEALTHY_NODES: usize = 3;

/// How long a silent node is retained before being pruned from the registry.
/// Generous relative to [`HEARTBEAT_TIMEOUT`] so a node that briefly goes
/// unhealthy can recover without losing its slot, while churned/replaced nodes
/// are eventually reclaimed.
const STALE_TTL: Duration = Duration::from_secs(600);

/// A single registered MPC node.
#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub id: String,
    pub endpoint: String,
    pub last_heartbeat: Instant,
}

/// In-memory registry of dynamically discovered MPC nodes.
#[derive(Default)]
pub struct NodeRegistry {
    nodes: HashMap<String, NodeInfo>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a node, or refresh an existing one's endpoint + heartbeat.
    /// Registration doubles as a heartbeat. Returns `true` when this created a
    /// brand-new registration (vs. refreshing an existing node).
    pub fn register(&mut self, id: String, endpoint: String) -> bool {
        self.register_at(id, endpoint, Instant::now())
    }

    /// [`register`](Self::register) with an explicit timestamp (for testing).
    pub fn register_at(&mut self, id: String, endpoint: String, now: Instant) -> bool {
        self.prune_at(now);
        let is_new = !self.nodes.contains_key(&id);
        self.nodes.insert(
            id.clone(),
            NodeInfo {
                id,
                endpoint,
                last_heartbeat: now,
            },
        );
        is_new
    }

    /// Refresh the heartbeat of an already-registered node. Returns `false` if
    /// the node is unknown (the node should re-`register` in that case).
    pub fn heartbeat(&mut self, id: &str) -> bool {
        self.heartbeat_at(id, Instant::now())
    }

    /// [`heartbeat`](Self::heartbeat) with an explicit timestamp (for testing).
    pub fn heartbeat_at(&mut self, id: &str, now: Instant) -> bool {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.last_heartbeat = now;
                true
            }
            None => false,
        }
    }

    /// Remove a node (e.g. on graceful shutdown). Returns `true` if it existed.
    pub fn deregister(&mut self, id: &str) -> bool {
        self.nodes.remove(id).is_some()
    }

    /// Total number of registered nodes (healthy or not).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether no nodes are registered.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Endpoints of every currently-healthy node, sorted by node id so the
    /// ordering is deterministic (node `"0"`, `"1"`, `"2"` map to MPC parties
    /// in a stable order).
    pub fn healthy_endpoints(&self) -> Vec<String> {
        self.healthy_endpoints_at(Instant::now())
    }

    /// [`healthy_endpoints`](Self::healthy_endpoints) at an explicit time.
    pub fn healthy_endpoints_at(&self, now: Instant) -> Vec<String> {
        let mut healthy: Vec<&NodeInfo> = self
            .nodes
            .values()
            .filter(|n| now.saturating_duration_since(n.last_heartbeat) <= HEARTBEAT_TIMEOUT)
            .collect();
        healthy.sort_by(|a, b| a.id.cmp(&b.id));
        healthy.into_iter().map(|n| n.endpoint.clone()).collect()
    }

    /// Select [`MIN_HEALTHY_NODES`] healthy endpoints for a session, or `None`
    /// when fewer than that are currently healthy.
    pub fn select_session_nodes(&self) -> Option<Vec<String>> {
        self.select_session_nodes_at(Instant::now())
    }

    /// [`select_session_nodes`](Self::select_session_nodes) at an explicit time.
    pub fn select_session_nodes_at(&self, now: Instant) -> Option<Vec<String>> {
        let healthy = self.healthy_endpoints_at(now);
        if healthy.len() < MIN_HEALTHY_NODES {
            return None;
        }
        Some(healthy.into_iter().take(MIN_HEALTHY_NODES).collect())
    }

    /// Drop nodes that have been silent for longer than [`STALE_TTL`].
    fn prune_at(&mut self, now: Instant) {
        self.nodes
            .retain(|_, n| now.saturating_duration_since(n.last_heartbeat) <= STALE_TTL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_is_new_then_refresh_updates_endpoint() {
        let mut r = NodeRegistry::new();
        let t0 = Instant::now();

        assert!(r.register_at("0".into(), "http://a:1".into(), t0));
        assert!(r.register_at("1".into(), "http://b:1".into(), t0));
        assert!(r.register_at("2".into(), "http://c:1".into(), t0));
        // Re-registering an existing id refreshes rather than adds.
        assert!(!r.register_at("2".into(), "http://c:2".into(), t0));
        assert_eq!(r.len(), 3);

        let selected = r.select_session_nodes_at(t0).expect("three healthy nodes");
        assert_eq!(selected, vec!["http://a:1", "http://b:1", "http://c:2"]);
    }

    #[test]
    fn heartbeat_expiry_marks_node_unhealthy() {
        let mut r = NodeRegistry::new();
        let t0 = Instant::now();
        r.register_at("0".into(), "http://a".into(), t0);
        r.register_at("1".into(), "http://b".into(), t0);
        r.register_at("2".into(), "http://c".into(), t0);

        // Just inside the timeout window: all three are healthy.
        assert_eq!(r.healthy_endpoints_at(t0 + HEARTBEAT_TIMEOUT).len(), 3);

        // Past the timeout with no heartbeats: none are healthy.
        let expired = t0 + HEARTBEAT_TIMEOUT + Duration::from_secs(1);
        assert!(r.healthy_endpoints_at(expired).is_empty());
        assert!(r.select_session_nodes_at(expired).is_none());

        // A single heartbeat brings one node back, but two is still too few.
        assert!(r.heartbeat_at("0", expired));
        assert_eq!(r.healthy_endpoints_at(expired).len(), 1);
        assert!(r.select_session_nodes_at(expired).is_none());
    }

    #[test]
    fn node_replacement_during_session() {
        let mut r = NodeRegistry::new();
        let t0 = Instant::now();
        for (id, ep) in [("0", "http://a"), ("1", "http://b"), ("2", "http://c")] {
            r.register_at(id.into(), ep.into(), t0);
        }

        // Node 1 goes silent; a replacement node "3" registers, and the
        // surviving nodes keep heartbeating.
        let now = t0 + HEARTBEAT_TIMEOUT + Duration::from_secs(1);
        r.heartbeat_at("0", now);
        r.heartbeat_at("2", now);
        r.register_at("3".into(), "http://d".into(), now);

        let selected = r
            .select_session_nodes_at(now)
            .expect("replacement keeps three healthy nodes");
        assert_eq!(selected.len(), MIN_HEALTHY_NODES);
        assert!(!selected.contains(&"http://b".to_string())); // dead node excluded
        assert!(selected.contains(&"http://d".to_string())); // replacement included

        // Graceful deregister of the dead node.
        assert!(r.deregister("1"));
        assert!(!r.deregister("1"));
    }

    #[test]
    fn deregister_unknown_node_is_false() {
        let mut r = NodeRegistry::new();
        assert!(!r.deregister("ghost"));
    }

    #[test]
    fn silent_nodes_are_pruned_after_ttl() {
        let mut r = NodeRegistry::new();
        let t0 = Instant::now();
        r.register_at("0".into(), "http://a".into(), t0);

        // A later registration triggers pruning of the long-silent node.
        let much_later = t0 + STALE_TTL + Duration::from_secs(1);
        r.register_at("1".into(), "http://b".into(), much_later);
        assert_eq!(r.len(), 1);
        assert!(!r.deregister("0"));
    }
}
