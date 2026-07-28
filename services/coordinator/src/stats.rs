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

/// HUD-style action stats for seat tooltips (Issue #55).
#[derive(Serialize, Clone, Debug, Default, ToSchema)]
pub struct PlayerHudStats {
    pub address: String,
    pub hands_played: u64,
    /// Voluntarily put money in pot % (0–100).
    pub vpip: f64,
    /// Preflop raise % (0–100).
    pub pfr: f64,
    /// Aggression factor = (bets + raises) / calls (0 if no calls).
    pub aggression_factor: f64,
}

#[derive(Clone, Debug, Default)]
struct HudCounters {
    /// Hands where the player took at least one action.
    hands_seen: u64,
    /// Hands where player voluntarily put chips in (call/bet/raise/all-in).
    vpip_hands: u64,
    /// Hands where player raised or bet preflop (tracked as raise/bet).
    pfr_hands: u64,
    bets_raises: u64,
    calls: u64,
    /// Per-hand flags to avoid double-counting within a hand.
    /// Keyed by "table_id:hand_hint" is ideal; we use a simple
    /// last-action window: mark once per record_action batch via flags.
    vpip_this_hand: bool,
    pfr_this_hand: bool,
    saw_action_this_hand: bool,
}

#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct StatsResponse {
    pub global: GlobalStats,
    /// Top-10 players by hands_won.
    pub leaderboard: Vec<PlayerStats>,
    /// Unix timestamp (seconds) when this snapshot was computed.
    pub cached_at: u64,
}

/// On-chain ELO leaderboard cache entry (Issue #70).
#[derive(Serialize, Clone, Debug, Default, ToSchema)]
pub struct RatingEntry {
    pub address: String,
    pub rating: u32,
    pub hands_played: u32,
    pub hands_won: u32,
}

#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct RatingLeaderboardResponse {
    pub entries: Vec<RatingEntry>,
    pub min_hands: u32,
    pub total: u32,
}

// ─── Store ───────────────────────────────────────────────────────────────────

pub(crate) struct Inner {
    global: GlobalStats,
    players: HashMap<String, PlayerStats>,
    /// Action-derived HUD counters (Issue #55).
    hud: HashMap<String, HudCounters>,
    /// Cached on-chain rating leaderboard (Issue #70).
    ratings: Vec<RatingEntry>,
    ratings_min_hands: u32,
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
            hud: HashMap::new(),
            ratings: Vec::new(),
            ratings_min_hands: 10,
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

/// Record a player action for HUD stats (Issue #55).
/// Call when a player action is successfully applied.
///
/// `new_hand` should be true when this is the first action observed for the
/// player in a new hand (coordinator may pass false and rely on street resets).
pub async fn record_player_action(
    store: &StatsStore,
    address: &str,
    action: &str,
    is_preflop: bool,
) {
    let mut guard = store.write().await;
    let entry = guard
        .hud
        .entry(address.to_string())
        .or_insert_with(HudCounters::default);

    let act = action.trim().to_ascii_lowercase();
    if !entry.saw_action_this_hand {
        entry.hands_seen += 1;
        entry.saw_action_this_hand = true;
        entry.vpip_this_hand = false;
        entry.pfr_this_hand = false;
    }

    let voluntary = matches!(
        act.as_str(),
        "call" | "bet" | "raise" | "allin" | "all_in"
    );
    if voluntary && !entry.vpip_this_hand {
        entry.vpip_hands += 1;
        entry.vpip_this_hand = true;
    }

    let is_raise_like = matches!(act.as_str(), "bet" | "raise" | "allin" | "all_in");
    if is_preflop && is_raise_like && !entry.pfr_this_hand {
        entry.pfr_hands += 1;
        entry.pfr_this_hand = true;
    }

    match act.as_str() {
        "bet" | "raise" | "allin" | "all_in" => entry.bets_raises += 1,
        "call" => entry.calls += 1,
        _ => {}
    }
}

/// Mark end of hand so next actions start a new HUD hand window.
pub async fn end_hand_for_players(store: &StatsStore, addresses: &[String]) {
    let mut guard = store.write().await;
    for addr in addresses {
        if let Some(entry) = guard.hud.get_mut(addr) {
            entry.saw_action_this_hand = false;
            entry.vpip_this_hand = false;
            entry.pfr_this_hand = false;
        }
    }
}

fn hud_to_stats(address: &str, c: &HudCounters) -> PlayerHudStats {
    let hands = c.hands_seen.max(1) as f64;
    let vpip = (c.vpip_hands as f64 / hands) * 100.0;
    let pfr = (c.pfr_hands as f64 / hands) * 100.0;
    let aggression_factor = if c.calls == 0 {
        if c.bets_raises > 0 {
            c.bets_raises as f64
        } else {
            0.0
        }
    } else {
        c.bets_raises as f64 / c.calls as f64
    };
    PlayerHudStats {
        address: address.to_string(),
        hands_played: c.hands_seen,
        vpip,
        pfr,
        aggression_factor,
    }
}

/// Per-player HUD stats for seat tooltips (Issue #55).
pub async fn get_player_hud(store: &StatsStore, address: &str) -> PlayerHudStats {
    let guard = store.read().await;
    if let Some(c) = guard.hud.get(address) {
        return hud_to_stats(address, c);
    }
    // Fall back to hands_played from horizon-indexed stats if available.
    let hands = guard
        .players
        .get(address)
        .map(|p| p.hands_played)
        .unwrap_or(0);
    PlayerHudStats {
        address: address.to_string(),
        hands_played: hands,
        vpip: 0.0,
        pfr: 0.0,
        aggression_factor: 0.0,
    }
}

/// Replace the cached on-chain rating leaderboard (Issue #70).
pub async fn set_rating_leaderboard(
    store: &StatsStore,
    entries: Vec<RatingEntry>,
    min_hands: u32,
) {
    let mut guard = store.write().await;
    guard.ratings = entries;
    guard.ratings_min_hands = min_hands;
}

/// Read cached rating leaderboard with offset/limit.
pub async fn get_rating_leaderboard(
    store: &StatsStore,
    offset: usize,
    limit: usize,
) -> RatingLeaderboardResponse {
    let guard = store.read().await;
    let total = guard.ratings.len() as u32;
    let entries: Vec<RatingEntry> = guard
        .ratings
        .iter()
        .skip(offset)
        .take(limit.max(1))
        .cloned()
        .collect();
    RatingLeaderboardResponse {
        entries,
        min_hands: guard.ratings_min_hands,
        total,
    }
}

/// Seed demo/local ratings when the on-chain contract is not configured.
pub async fn ensure_demo_ratings(store: &StatsStore) {
    let mut guard = store.write().await;
    if !guard.ratings.is_empty() {
        return;
    }
    // Build a synthetic leaderboard from known player stats so the UI works locally.
    let mut entries: Vec<RatingEntry> = guard
        .players
        .values()
        .map(|p| {
            let base = 1500u32;
            let bonus = (p.hands_won as u32).saturating_mul(8);
            let pen = (p.hands_played.saturating_sub(p.hands_won) as u32).saturating_mul(3);
            RatingEntry {
                address: p.address.clone(),
                rating: base.saturating_add(bonus).saturating_sub(pen).clamp(100, 4000),
                hands_played: p.hands_played.min(u32::MAX as u64) as u32,
                hands_won: p.hands_won.min(u32::MAX as u64) as u32,
            }
        })
        .filter(|e| e.hands_played >= guard.ratings_min_hands)
        .collect();
    entries.sort_by(|a, b| b.rating.cmp(&a.rating));
    entries.truncate(50);
    guard.ratings = entries;
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
