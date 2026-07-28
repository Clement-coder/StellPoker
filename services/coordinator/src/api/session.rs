use axum::http::StatusCode;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid;

use crate::{soroban, AppState, TableSession};
use super::auth::is_valid_stellar_address;
use super::parsing::{map_onchain_phase_to_local, normalize_field_value, parse_u32_value};
use super::{MAX_PLAYERS, MIN_PLAYERS};

pub(crate) async fn ensure_session_exists(state: &AppState, table_id: u32) -> Result<(), StatusCode> {
    {
        let tables = state.tables.read().await;
        if tables.contains_key(&table_id) {
            return Ok(());
        }
    }

    if !state.soroban_config.is_configured() {
        return Err(StatusCode::NOT_FOUND);
    }

    let raw_state = soroban::get_table_state(&state.soroban_config, table_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                "failed to fetch on-chain table {} for session rehydrate: {}",
                table_id,
                e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let restored = build_session_from_onchain_state(table_id, &raw_state).map_err(|e| {
        tracing::warn!(
            "failed to rehydrate table {} from on-chain state: {}",
            table_id,
            e
        );
        StatusCode::NOT_FOUND
    })?;

    let mut tables = state.tables.write().await;
    tables.entry(table_id).or_insert(restored);
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct OnchainTableView {
    pub phase: String,
    pub max_players: u32,
    pub seats: Vec<(u32, String)>,
    /// Chip stacks aligned with `seats` order (Issue #53).
    pub stacks: Vec<i64>,
}

fn parse_i64_value(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    if let Some(n) = v.as_u64() {
        return Some(n as i64);
    }
    if let Some(s) = v.as_str() {
        return s.parse::<i64>().ok();
    }
    None
}

pub(crate) async fn fetch_onchain_table_view(
    soroban_config: &soroban::SorobanConfig,
    table_id: u32,
) -> Result<OnchainTableView, String> {
    let raw_state = soroban::get_table_state(soroban_config, table_id).await?;
    let value: Value =
        serde_json::from_str(&raw_state).map_err(|e| format!("invalid table json: {}", e))?;

    let phase = value
        .get("phase")
        .and_then(|v| v.as_str())
        .ok_or("missing phase")?
        .to_string();

    let mut seat_rows: Vec<(u32, String, i64)> = value
        .get("players")
        .and_then(|v| v.as_array())
        .ok_or("missing players")?
        .iter()
        .filter_map(|player| {
            let address = player.get("address")?.as_str()?.to_string();
            let seat = player
                .get("seat_index")
                .and_then(parse_u32_value)
                .unwrap_or(0);
            let stack = player
                .get("stack")
                .and_then(parse_i64_value)
                .unwrap_or(0);
            Some((seat, address, stack))
        })
        .collect();
    seat_rows.sort_by_key(|(seat, _, _)| *seat);

    let seats: Vec<(u32, String)> = seat_rows
        .iter()
        .map(|(seat, addr, _)| (*seat, addr.clone()))
        .collect();
    let stacks: Vec<i64> = seat_rows.iter().map(|(_, _, s)| *s).collect();

    let max_players = value
        .get("config")
        .and_then(|cfg| cfg.get("max_players"))
        .and_then(parse_u32_value)
        .unwrap_or_else(|| seats.len() as u32);

    Ok(OnchainTableView {
        phase,
        max_players,
        seats,
        stacks,
    })
}

pub(crate) async fn resolve_deal_players_from_lobby(
    state: &AppState,
    table_id: u32,
) -> Result<Vec<String>, StatusCode> {
    let view = fetch_onchain_table_view(&state.soroban_config, table_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let lobby = state.lobby_assignments.read().await;
    let table_lobby = lobby.get(&table_id);

    let mut ordered_players = Vec::new();
    for (_, chain_address) in &view.seats {
        let logical = table_lobby
            .and_then(|table| {
                table
                    .iter()
                    .find(|(_, mapped_chain)| *mapped_chain == chain_address)
                    .map(|(wallet, _)| wallet.clone())
            })
            .unwrap_or_else(|| chain_address.clone());
        ordered_players.push(logical);
    }

    if ordered_players.len() < MIN_PLAYERS {
        return Err(StatusCode::CONFLICT);
    }
    validate_players(&ordered_players)?;

    Ok(ordered_players)
}

fn build_session_from_onchain_state(
    table_id: u32,
    raw_state: &str,
) -> Result<TableSession, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw_state).map_err(|e| format!("invalid table json: {}", e))?;

    let phase_raw = value
        .get("phase")
        .and_then(|v| v.as_str())
        .ok_or("missing phase")?;
    let phase = map_onchain_phase_to_local(phase_raw)
        .ok_or_else(|| format!("unsupported on-chain phase '{}'", phase_raw))?;

    let mut seated: Vec<(u32, String)> = value
        .get("players")
        .and_then(|v| v.as_array())
        .ok_or("missing players")?
        .iter()
        .filter_map(|player| {
            let address = player.get("address")?.as_str()?.to_string();
            let seat = player
                .get("seat_index")
                .and_then(parse_u32_value)
                .unwrap_or(0);
            Some((seat, address))
        })
        .collect();
    seated.sort_by_key(|(seat, _)| *seat);
    let player_order: Vec<String> = seated.into_iter().map(|(_, address)| address).collect();

    if player_order.len() < MIN_PLAYERS {
        return Err(format!(
            "not enough seated players to restore session: {}",
            player_order.len()
        ));
    }

    let deck_root_raw = value
        .get("deck_root")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let deck_root = if deck_root_raw.is_empty() {
        String::new()
    } else {
        normalize_field_value(&deck_root_raw)?
    };

    if phase != "waiting" && phase != "dealing" && deck_root.is_empty() {
        return Err("missing deck_root for active hand".to_string());
    }

    let hand_commitments: Vec<String> = value
        .get("hand_commitments")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str())
                .map(normalize_field_value)
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let board_cards: Vec<u32> = value
        .get("board_cards")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_u32_value).collect())
        .unwrap_or_default();
    let board_count = board_cards.len();

    let mut hole_indices = Vec::with_capacity(player_order.len() * 2);
    let mut player_card_positions = Vec::with_capacity(player_order.len());
    for seat in 0..player_order.len() {
        let c1 = (seat * 2) as u32;
        let c2 = c1 + 1;
        player_card_positions.push((c1, c2));
        hole_indices.push(c1);
        hole_indices.push(c2);
    }

    let chain_dealt_indices: Vec<u32> = value
        .get("dealt_indices")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_u32_value).collect())
        .unwrap_or_default();

    let board_indices = if chain_dealt_indices.is_empty() {
        let start = (player_order.len() * 2) as u32;
        (0..board_count)
            .map(|i| start + i as u32)
            .collect::<Vec<u32>>()
    } else if chain_dealt_indices.len() >= hole_indices.len() + board_count {
        chain_dealt_indices[chain_dealt_indices.len() - board_count..].to_vec()
    } else {
        chain_dealt_indices.clone()
    };

    let dealt_indices = if chain_dealt_indices.is_empty() {
        let mut combined = hole_indices.clone();
        combined.extend(board_indices.iter().copied());
        combined
    } else if chain_dealt_indices.len() >= hole_indices.len() {
        chain_dealt_indices
    } else {
        let mut combined = hole_indices.clone();
        combined.extend(chain_dealt_indices.iter().copied());
        combined
    };

    let mut revealed_cards_by_phase = HashMap::new();
    if board_cards.len() >= 3 {
        revealed_cards_by_phase.insert("flop".to_string(), board_cards[0..3].to_vec());
    }
    if board_cards.len() >= 4 {
        revealed_cards_by_phase.insert("turn".to_string(), vec![board_cards[3]]);
    }
    if board_cards.len() >= 5 {
        revealed_cards_by_phase.insert("river".to_string(), vec![board_cards[4]]);
    }

    Ok(TableSession {
        table_id,
        deck_root,
        hand_commitments,
        player_order,
        dealt_indices,
        player_card_positions,
        board_indices,
        phase: phase.to_string(),
        deal_session_id: "rehydrated-from-chain".to_string(),
        deal_tx_hash: None,
        reveal_tx_hashes: HashMap::new(),
        reveal_session_ids: HashMap::new(),
        revealed_cards_by_phase,
        selected_node_endpoints: Vec::new(), // Will be populated on first MPC call if needed
        showdown_tx_hash: None,
        showdown_session_id: None,
        showdown_result: None,
        proof_nonce: 0,
        mpc_node_progress: Vec::new(),
        mpc_operation_started: None,
    })
}

/// Generate a cryptographically random, single-use proof session ID.
///
/// The ID embeds the table ID and a label for traceability, but the UUID
/// component ensures the full ID is unpredictable and non-replayable.
/// Session IDs must be checked against the `used_session_ids` set in
/// `AppState` before being forwarded to MPC nodes.
pub(crate) fn next_proof_session_id(session: &mut TableSession, label: &str) -> String {
    session.proof_nonce = session.proof_nonce.saturating_add(1);
    format!(
        "table-{}-{}-{}",
        session.table_id,
        label,
        uuid::Uuid::new_v4(),
    )
}

pub(crate) fn validate_table_id(_table_id: u32) -> Result<(), StatusCode> {
    Ok(())
}

pub(crate) fn validate_players(players: &[String]) -> Result<(), StatusCode> {
    if players.len() < MIN_PLAYERS || players.len() > MAX_PLAYERS {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut seen = HashSet::new();
    for address in players {
        if !is_valid_stellar_address(address) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if !seen.insert(address) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    Ok(())
}

pub(crate) fn validate_reveal_phase(phase: &str) -> Result<(), StatusCode> {
    match phase {
        "flop" | "turn" | "river" => Ok(()),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Whether dynamic MPC node discovery is active: it is used only when neither
/// the on-chain committee registry nor static `MPC_NODE_*` endpoints are
/// configured. This preserves backward compatibility — explicit static config
/// always takes precedence over runtime self-registration.
pub(crate) fn dynamic_discovery_active(state: &AppState) -> bool {
    state.soroban_config.committee_registry_contract.is_empty()
        && !state.mpc_config.static_endpoints_configured
}

/// Select 3 healthy MPC nodes, prioritizing those in the requested region.
pub(crate) async fn select_mpc_nodes(
    state: &AppState,
    requested_region: Option<String>,
) -> Result<Vec<String>, StatusCode> {
    // Dynamic discovery: choose from nodes that self-registered at runtime and
    // are still heartbeating. The registry enforces the 3-healthy-node minimum
    // (returning None -> 503) just like the registry/static paths below.
    if dynamic_discovery_active(state) {
        return state
            .node_registry
            .read()
            .await
            .select_session_nodes()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE);
    }

    let all_nodes = if state.soroban_config.committee_registry_contract.is_empty() {
        // Fallback to static config if registry not configured
        state
            .mpc_config
            .node_endpoints
            .iter()
            .map(|ep| soroban::CommitteeMember {
                address: String::new(),
                stake: 0,
                endpoint: ep.clone(),
                region: "unknown".to_string(),
                active: true,
                slash_count: 0,
            })
            .collect()
    } else {
        soroban::fetch_active_nodes_from_registry(&state.soroban_config)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch nodes from registry: {}", e);
                StatusCode::SERVICE_UNAVAILABLE
            })?
    };

    let healths = state.metrics.node_healths.lock().await;
    let mut healthy_members: Vec<soroban::CommitteeMember> = all_nodes
        .into_iter()
        .filter(|m| {
            healths
                .iter()
                .find(|h| h.endpoint == m.endpoint)
                .map(|h| h.connected)
                .unwrap_or(false)
        })
        .collect();

    if healthy_members.len() < 3 {
        tracing::warn!(
            "Not enough healthy MPC nodes: found {}",
            healthy_members.len()
        );
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Sort by region match
    if let Some(ref region) = requested_region {
        healthy_members.sort_by_key(|m| m.region != *region);
    }

    let selected = healthy_members
        .into_iter()
        .take(3)
        .map(|m| m.endpoint)
        .collect();
    Ok(selected)
}

pub(crate) fn is_identity_missing_error(error: &str) -> bool {
    error
        .to_ascii_lowercase()
        .contains("no local identity configured")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TableSession;
    use std::collections::HashSet;

    fn make_session(table_id: u32) -> TableSession {
        TableSession {
            table_id,
            deck_root: String::new(),
            hand_commitments: Vec::new(),
            player_order: Vec::new(),
            dealt_indices: Vec::new(),
            player_card_positions: Vec::new(),
            board_indices: Vec::new(),
            phase: "waiting".to_string(),
            deal_session_id: String::new(),
            deal_tx_hash: None,
            reveal_tx_hashes: std::collections::HashMap::new(),
            reveal_session_ids: std::collections::HashMap::new(),
            revealed_cards_by_phase: std::collections::HashMap::new(),
            selected_node_endpoints: Vec::new(),
            showdown_tx_hash: None,
            showdown_session_id: None,
            showdown_result: None,
            proof_nonce: 0,
            mpc_node_progress: Vec::new(),
            mpc_operation_started: None,
        }
    }

    /// Session IDs must be unpredictable: two calls with the same label must
    /// yield different values (UUID component makes collisions astronomically
    /// unlikely — treat a collision as a test failure).
    #[test]
    fn session_ids_are_unique() {
        let mut session = make_session(42);
        let id1 = next_proof_session_id(&mut session, "deal");
        let id2 = next_proof_session_id(&mut session, "deal");
        assert_ne!(id1, id2, "session IDs must not repeat");
    }

    /// Session IDs must embed the table ID so they are scoped to a specific
    /// game and cannot be replayed against a different table.
    #[test]
    fn session_id_binds_to_table_id() {
        let mut s1 = make_session(1);
        let mut s2 = make_session(2);
        let id1 = next_proof_session_id(&mut s1, "deal");
        let id2 = next_proof_session_id(&mut s2, "deal");
        assert!(id1.contains("table-1-"), "ID should contain table-1-");
        assert!(id2.contains("table-2-"), "ID should contain table-2-");
        assert_ne!(id1, id2);
    }

    /// Simulate the single-use enforcement: inserting the same ID twice must
    /// be detected (HashSet::insert returns false on duplicate).
    #[test]
    fn used_session_ids_detects_replay() {
        let mut session = make_session(7);
        let id = next_proof_session_id(&mut session, "reveal-flop");

        let mut used: HashSet<String> = HashSet::new();
        assert!(used.insert(id.clone()), "first registration must succeed");
        assert!(!used.insert(id.clone()), "replay must be detected");
    }

    /// Verify 100 consecutive IDs are all distinct (no sequential pattern).
    #[test]
    fn session_ids_are_non_sequential() {
        let mut session = make_session(99);
        let ids: HashSet<String> = (0..100)
            .map(|_| next_proof_session_id(&mut session, "deal"))
            .collect();
        assert_eq!(ids.len(), 100, "all 100 IDs must be unique");
    }
}
