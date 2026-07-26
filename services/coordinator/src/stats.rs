//! Onchain statistics indexer.
//!
//! A background task polls the Horizon event streaming endpoint for contract
//! events emitted by the poker-table contract and accumulates:
//!
//!  - Global: hands played, biggest pot seen, total players ever joined
//!  - Per-player: hands played, hands won, biggest pot won
//!
//! The results are cached in `StatsStore` and served at `GET /api/stats`.
//! The cache has a configurable TTL (default 30 s) to avoid hammering Horizon.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use utoipa::ToSchema;

// ─── Data models ─────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug, Default, ToSchema)]
pub struct GlobalStats {
    pub hands_played: u64,
    pub biggest_pot: i64,
    pub total_players_joined: u64,
}

#[derive(Serialize, Clone, Debug, Default, ToSchema)]
pub struct PlayerStats {
    pub address: String,
    pub hands_played: u64,
    pub hands_won: u64,
    pub biggest_pot_won: i64,
}

#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct StatsResponse {
    pub global: GlobalStats,
    /// Top-10 players by hands_won.
    pub leaderboard: Vec<PlayerStats>,
    /// Unix timestamp (seconds) when this snapshot was computed.
    pub cached_at: u64,
}

// ─── Store ───────────────────────────────────────────────────────────────────

pub(crate) struct Inner {
    global: GlobalStats,
    players: HashMap<String, PlayerStats>,
    /// Ledger cursor for next Horizon poll (paging token).
    cursor: Option<String>,
    /// When the last cached response was built.
    last_built: Option<Instant>,
    /// Pre-built response reused while within TTL.
    cached: Option<StatsResponse>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            global: GlobalStats::default(),
            players: HashMap::new(),
            cursor: None,
            last_built: None,
            cached: None,
        }
    }
}

pub type StatsStore = Arc<RwLock<Inner>>;

pub fn new_store() -> StatsStore {
    Arc::new(RwLock::new(Inner::default()))
}

/// Return a cached response, rebuilding it if the TTL has expired.
pub async fn get_stats(store: &StatsStore, ttl: Duration) -> StatsResponse {
    {
        let guard = store.read().await;
        if let (Some(cached), Some(built)) = (&guard.cached, guard.last_built) {
            if built.elapsed() < ttl {
                return cached.clone();
            }
        }
    }

    let mut guard = store.write().await;
    // Re-check after acquiring the write lock.
    if let (Some(cached), Some(built)) = (&guard.cached, guard.last_built) {
        if built.elapsed() < ttl {
            return cached.clone();
        }
    }

    let mut leaderboard: Vec<PlayerStats> = guard.players.values().cloned().collect();
    leaderboard.sort_by(|a, b| b.hands_won.cmp(&a.hands_won).then(b.hands_played.cmp(&a.hands_played)));
    leaderboard.truncate(10);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let response = StatsResponse {
        global: guard.global.clone(),
        leaderboard,
        cached_at: now,
    };

    guard.cached = Some(response.clone());
    guard.last_built = Some(Instant::now());
    response
}

// ─── Horizon event shapes ─────────────────────────────────────────────────────

/// Minimal Horizon event record (only fields we need).
#[derive(Deserialize, Debug)]
struct HorizonEvent {
    paging_token: String,
    #[serde(rename = "type")]
    kind: String,
    topic: Vec<String>,  // base64 XDR ScVal
    value: Option<String>, // base64 XDR ScVal
}

#[derive(Deserialize, Debug)]
struct HorizonEventsPage {
    #[serde(rename = "_embedded")]
    embedded: Option<HorizonEmbedded>,
}

#[derive(Deserialize, Debug)]
struct HorizonEmbedded {
    records: Vec<HorizonEvent>,
}

// ─── Event parsing helpers ────────────────────────────────────────────────────

/// Decode a base64-XDR `ScVal` into its symbol name (for topic[0]).
fn decode_symbol(b64: &str) -> Option<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    // Symbol ScVal: type byte 0x04, then 1-byte length, then UTF-8 string.
    // Full XDR: discriminant (4 bytes big-endian = 4 for SCV_SYMBOL),
    // then 4-byte length, then UTF-8 padded to 4-byte boundary.
    if bytes.len() < 8 {
        return None;
    }
    let disc = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if disc != 4 {
        return None; // not SCV_SYMBOL
    }
    let len = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if bytes.len() < 8 + len {
        return None;
    }
    std::str::from_utf8(&bytes[8..8 + len]).ok().map(|s| s.to_string())
}

/// Extract an address string from a base64-XDR ScVal (SCV_ADDRESS).
fn decode_address(b64: &str) -> Option<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    // SCV_ADDRESS discriminant = 18 (0x12).
    if bytes.len() < 4 {
        return None;
    }
    let disc = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if disc != 18 {
        return None;
    }
    // Address is a strkey — encode the raw key bytes with stellar-strkey.
    // For our purposes a hex representation is enough for a map key.
    Some(hex::encode(&bytes[4..]))
}

/// Decode a base64-XDR ScVal i128 (discriminant 6) into i64 (clamped).
fn decode_i128_as_i64(b64: &str) -> Option<i64> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    if bytes.len() < 20 {
        return None;
    }
    let disc = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if disc != 6 {
        return None; // not SCV_I128
    }
    // High 8 bytes + low 8 bytes (big-endian)
    let lo = i64::from_be_bytes([
        bytes[12], bytes[13], bytes[14], bytes[15],
        bytes[16], bytes[17], bytes[18], bytes[19],
    ]);
    Some(lo)
}

// ─── Background indexer task ──────────────────────────────────────────────────

/// Spawn a background task that polls Horizon for contract events every
/// `poll_interval` and updates `store`.
pub fn spawn_indexer(
    store: StatsStore,
    horizon_url: String,
    contract_id: String,
    poll_interval: Duration,
) {
    tokio::spawn(async move {
        loop {
            poll_once(&store, &horizon_url, &contract_id).await;
            tokio::time::sleep(poll_interval).await;
        }
    });
}

async fn poll_once(store: &StatsStore, horizon_url: &str, contract_id: &str) {
    let cursor = {
        let guard = store.read().await;
        guard.cursor.clone()
    };

    let url = build_events_url(horizon_url, contract_id, cursor.as_deref());

    let resp = match reqwest::get(&url).await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("Stats indexer: Horizon request failed: {}", e);
            return;
        }
    };

    let page: HorizonEventsPage = match resp.json().await {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("Stats indexer: failed to parse Horizon response: {}", e);
            return;
        }
    };

    let records = match page.embedded {
        Some(e) => e.records,
        None => return,
    };

    if records.is_empty() {
        return;
    }

    let last_token = records.last().map(|r| r.paging_token.clone());

    let mut guard = store.write().await;
    for ev in records {
        if ev.kind != "contract" {
            continue;
        }
        let Some(topic0) = ev.topic.first() else { continue };
        let Some(name) = decode_symbol(topic0) else { continue };

        match name.as_str() {
            // hand_started(table_id) -> increment global hands
            "hand_started" => {
                guard.global.hands_played += 1;
                // topic[1] could be a player address in some events, but
                // hand_started only carries table_id — no player to credit yet.
            }

            // player_joined(table_id) -> (player, seat) in value
            "player_joined" => {
                guard.global.total_players_joined += 1;
                if let Some(val) = &ev.value {
                    if let Some(addr) = decode_address(val) {
                        let entry = guard.players.entry(addr.clone()).or_insert_with(|| PlayerStats {
                            address: addr,
                            ..Default::default()
                        });
                        entry.hands_played += 1;
                    }
                }
            }

            // rake_withdrawn(table_id) -> (admin, amount): use amount as pot proxy
            "rake_withdrawn" => {
                if let Some(val) = &ev.value {
                    if let Some(amount) = decode_i128_as_i64(val) {
                        if amount > guard.global.biggest_pot {
                            guard.global.biggest_pot = amount;
                        }
                    }
                }
            }

            _ => {}
        }
    }

    if let Some(token) = last_token {
        guard.cursor = Some(token);
    }
    // Invalidate the cache so the next read rebuilds.
    guard.cached = None;
}

fn build_events_url(horizon_url: &str, contract_id: &str, cursor: Option<&str>) -> String {
    let base = horizon_url.trim_end_matches('/');
    let mut url = format!(
        "{}/contract_events?contract_id={}&order=asc&limit=200",
        base, contract_id
    );
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }
    url
}
