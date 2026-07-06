//! REST API handlers for the coordinator service.

pub mod admin;
mod admin_extended;
pub mod api_key_admin;
mod auth;
pub mod flags;
mod parsing;
pub mod plugins;
mod session;
pub mod types;

pub use admin_extended::*;
pub use types::*;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{feature_flags, mpc, session_gc, soroban, AppState, TableSession};
use auth::{allow_insecure_dev_auth, enforce_rate_limit, validate_signed_request};
use parsing::{
    parse_deal_outputs, parse_requested_buy_in, parse_reveal_outputs, parse_showdown_outputs,
};
use session::{
    dynamic_discovery_active, ensure_session_exists, fetch_onchain_table_view,
    is_identity_missing_error, next_proof_session_id, resolve_deal_players_from_lobby,
    select_mpc_nodes, validate_players, validate_reveal_phase, validate_table_id,
};

const MAX_PLAYERS: usize = 6;
const MIN_PLAYERS: usize = 2;

pub struct SessionGuard {
    counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl SessionGuard {
    pub fn new(counter: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Self {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self { counter }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.counter
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// GET /api/chain-config
///
/// Public chain parameters used by the frontend for wallet-signed
/// on-chain transactions.
pub async fn get_chain_config(
    State(state): State<AppState>,
) -> Result<Json<ChainConfigResponse>, StatusCode> {
    if !state.soroban_config.is_configured() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    Ok(Json(ChainConfigResponse {
        rpc_url: state.soroban_config.rpc_url.clone(),
        network_passphrase: state.soroban_config.network_passphrase.clone(),
        poker_table_contract: state.soroban_config.poker_table_contract.clone(),
    }))
}

/// POST /api/tables/create
///
/// Creates a new empty on-chain table by copying config from the reference
/// table. Players then join directly on-chain with their own wallet auth.
pub async fn create_table(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateTableRequest>,
) -> Result<Json<CreateTableResponse>, StatusCode> {
    if !state.soroban_config.is_configured() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    enforce_rate_limit(&state, &headers, 0, "create_table").await?;
    let auth = validate_signed_request(&state, &headers, 0, "create_table", None).await?;

    let solo_mode = req.solo.unwrap_or(false);
    if solo_mode
        && !state
            .feature_flags
            .is_enabled(
                feature_flags::keys::SOLO_MODE,
                &feature_flags::FlagScope::Global,
            )
            .await
    {
        tracing::warn!("create_table: solo_mode requested but feature flag is disabled");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let max_players = if solo_mode {
        2
    } else {
        req.max_players.unwrap_or(2)
    };
    if !(2..=MAX_PLAYERS as u32).contains(&max_players) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let requested_buy_in = req
        .buy_in
        .as_deref()
        .map(parse_requested_buy_in)
        .transpose()
        .map_err(|e| {
            tracing::warn!("create_table invalid buy_in: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    let reference_table_id = state.soroban_config.onchain_table_id.unwrap_or(0);
    let table_id = soroban::create_seeded_table(
        &state.soroban_config,
        reference_table_id,
        max_players,
        requested_buy_in,
    )
    .await
    .map_err(|e| {
        tracing::error!("create_table failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    if solo_mode {
        let default_buy_in = std::env::var("LOBBY_BUY_IN")
            .ok()
            .and_then(|v| v.parse::<i128>().ok())
            .unwrap_or(1_000_000_000i128);
        let buy_in = requested_buy_in.unwrap_or(default_buy_in);
        let creator_seat =
            soroban::join_next_available_local_player(&state.soroban_config, table_id, buy_in)
                .await
                .map_err(|e| {
                    tracing::error!("create_table solo creator-seat join failed: {}", e);
                    StatusCode::BAD_GATEWAY
                })?;
        let _bot_seat = soroban::join_single_bot_player(&state.soroban_config, table_id, buy_in)
            .await
            .map_err(|e| {
                tracing::error!("create_table solo bot join failed: {}", e);
                StatusCode::BAD_GATEWAY
            })?;

        let mut lobby = state.lobby_assignments.write().await;
        lobby
            .entry(table_id)
            .or_default()
            .insert(auth.address, creator_seat);
    }

    let selected_node_endpoints = select_mpc_nodes(&state, req.region).await?;

    let table_view = fetch_onchain_table_view(&state.soroban_config, table_id)
        .await
        .map_err(|e| {
            tracing::error!("create_table fetch failed: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    let session = TableSession {
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
        reveal_tx_hashes: HashMap::new(),
        reveal_session_ids: HashMap::new(),
        revealed_cards_by_phase: HashMap::new(),
        selected_node_endpoints,
        showdown_tx_hash: None,
        showdown_session_id: None,
        showdown_result: None,
        proof_nonce: 0,
    };
    state.tables.write().await.insert(table_id, session);

    Ok(Json(CreateTableResponse {
        table_id,
        max_players: table_view.max_players,
        joined_wallets: table_view.seats.len(),
    }))
}

/// GET /api/tables/open
///
/// List open tables (waiting phase) that still have unclaimed wallet slots.
pub async fn list_open_tables(
    State(state): State<AppState>,
) -> Result<Json<OpenTablesResponse>, StatusCode> {
    if !state.soroban_config.is_configured() {
        return Ok(Json(OpenTablesResponse { tables: Vec::new() }));
    }

    let scan_max = std::env::var("OPEN_TABLE_SCAN_MAX")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(32);
    let mut tables = Vec::new();
    for table_id in 0..scan_max {
        let Ok(view) = fetch_onchain_table_view(&state.soroban_config, table_id).await else {
            continue;
        };

        if view.phase != "Waiting" {
            continue;
        }

        let joined_wallets = view.seats.len();
        let open_wallet_slots = view.max_players.saturating_sub(joined_wallets as u32) as usize;
        if open_wallet_slots == 0 {
            continue;
        }

        tables.push(OpenTableInfo {
            table_id,
            phase: view.phase.clone(),
            max_players: view.max_players,
            joined_wallets,
            open_wallet_slots,
        });
    }

    Ok(Json(OpenTablesResponse { tables }))
}

/// POST /api/table/{table_id}/join
///
/// Register wallet-to-seat mapping for a wallet that already joined on-chain.
pub async fn join_table(
    State(state): State<AppState>,
    Path(table_id): Path<u32>,
    headers: HeaderMap,
) -> Result<Json<JoinTableResponse>, StatusCode> {
    validate_table_id(table_id)?;
    enforce_rate_limit(&state, &headers, table_id, "join_table").await?;
    let auth = validate_signed_request(&state, &headers, table_id, "join_table", None).await?;

    let view = fetch_onchain_table_view(&state.soroban_config, table_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if view.phase != "Waiting" {
        return Err(StatusCode::CONFLICT);
    }

    let (seat_index, seat_address) = view
        .seats
        .iter()
        .find_map(|(idx, chain)| {
            if chain == &auth.address {
                Some((*idx, chain.clone()))
            } else {
                None
            }
        })
        .ok_or(StatusCode::CONFLICT)?;

    let mut lobby = state.lobby_assignments.write().await;
    let table_lobby = lobby.entry(table_id).or_default();
    table_lobby.insert(auth.address, seat_address.clone());

    Ok(Json(JoinTableResponse {
        table_id,
        seat_index,
        seat_address,
        joined_wallets: view.seats.len(),
        max_players: view.max_players,
    }))
}

/// GET /api/table/{table_id}/lobby
pub async fn get_table_lobby(
    State(state): State<AppState>,
    Path(table_id): Path<u32>,
) -> Result<Json<TableLobbyResponse>, StatusCode> {
    validate_table_id(table_id)?;
    let view = fetch_onchain_table_view(&state.soroban_config, table_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let lobby = state.lobby_assignments.read().await;
    let table_lobby = lobby.get(&table_id);

    let seats = view
        .seats
        .iter()
        .map(|(seat_index, chain_address)| {
            let wallet_address = table_lobby
                .and_then(|map| {
                    map.iter().find_map(|(wallet, chain)| {
                        if chain == chain_address {
                            Some(wallet.clone())
                        } else {
                            None
                        }
                    })
                })
                .or_else(|| Some(chain_address.clone()));
            LobbySeat {
                seat_index: *seat_index,
                chain_address: chain_address.clone(),
                wallet_address,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(TableLobbyResponse {
        table_id,
        phase: view.phase,
        max_players: view.max_players,
        joined_wallets: view.seats.len(),
        seats,
    }))
}

/// POST /api/table/{table_id}/request-deal
///
/// All MPC nodes prepare private deal contributions and exchange share fragments.
/// Coordinator triggers proof generation and parses public outputs from the proof.
pub async fn request_deal(
    State(state): State<AppState>,
    Path(table_id): Path<u32>,
    headers: HeaderMap,
    Json(req): Json<DealRequest>,
) -> Result<Json<DealResponse>, StatusCode> {
    validate_table_id(table_id)?;
    enforce_rate_limit(&state, &headers, table_id, "request_deal").await?;

    let players = if req.players.is_empty() {
        resolve_deal_players_from_lobby(&state, table_id).await?
    } else {
        validate_players(&req.players)?;
        req.players
    };

    {
        let tables = state.tables.read().await;
        if let Some(existing) = tables.get(&table_id) {
            if existing.phase != "waiting" && existing.phase != "settlement" {
                return Err(StatusCode::CONFLICT);
            }
        }
    }

    let mut tables = state.tables.write().await;
    let session = if let Some(existing) = tables.get_mut(&table_id) {
        if existing.phase != "waiting" && existing.phase != "settlement" {
            return Err(StatusCode::CONFLICT);
        }
        existing
    } else {
        // This case shouldn't happen if create_table was called, but for robustness:
        let selected_node_endpoints = select_mpc_nodes(&state, None).await?;
        let new_session = TableSession {
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
            reveal_tx_hashes: HashMap::new(),
            reveal_session_ids: HashMap::new(),
            revealed_cards_by_phase: HashMap::new(),
            selected_node_endpoints,
            showdown_tx_hash: None,
            showdown_session_id: None,
            showdown_result: None,
            proof_nonce: 0,
        };
        tables.insert(table_id, new_session);
        tables.get_mut(&table_id).unwrap()
    };

    let node_endpoints = session.selected_node_endpoints.clone();
    if node_endpoints.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let prepared_deal = mpc::prepare_deal_from_nodes(
        &node_endpoints,
        &state.mpc_config.circuit_dir,
        table_id,
        &players,
    )
    .await
    .map_err(|e| {
        tracing::error!("Deal preparation failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let proof_session_id = format!("table-{}-deal-{}", table_id, Uuid::new_v4());
    let _guard = SessionGuard::new(state.metrics.active_mpc_sessions.clone());
    let deal_proof = mpc::generate_proof_from_share_sets(
        table_id,
        &prepared_deal.share_set_ids,
        &proof_session_id,
        "deal_valid",
        &state.mpc_config.circuit_dir,
        &node_endpoints,
    )
    .await
    .map_err(|e| {
        tracing::error!("Deal proof generation failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let parsed_deal =
        parse_deal_outputs(&deal_proof.public_inputs, players.len()).map_err(|e| {
            tracing::error!("Deal public input parsing failed: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    let tx_hash = match soroban::submit_deal_proof(
        &state.soroban_config,
        table_id,
        &deal_proof.proof,
        &deal_proof.public_inputs,
        &parsed_deal.deck_root,
        &parsed_deal.hand_commitments,
    )
    .await
    {
        Ok(h) if !h.is_empty() => Some(h),
        Ok(_) => None,
        Err(e) => {
            if state.soroban_config.is_configured() {
                tracing::error!("Soroban deal proof submission failed: {}", e);
                return Err(StatusCode::BAD_GATEWAY);
            }
            tracing::warn!("Soroban deal proof submission skipped/failed: {}", e);
            None
        }
    };

    let player_card_positions: Vec<(u32, u32)> = (0..players.len())
        .map(|p| {
            (
                parsed_deal.dealt_indices[p * 2],
                parsed_deal.dealt_indices[p * 2 + 1],
            )
        })
        .collect();

    session.deck_root = parsed_deal.deck_root.clone();
    session.hand_commitments = parsed_deal.hand_commitments.clone();
    session.player_order = players;
    session.dealt_indices = parsed_deal.dealt_indices;
    session.player_card_positions = player_card_positions;
    session.board_indices = Vec::new();
    session.phase = "preflop".to_string();
    session.deal_session_id = deal_proof.session_id.clone();
    session.deal_tx_hash = tx_hash.clone();
    session.reveal_tx_hashes = HashMap::new();
    session.reveal_session_ids = HashMap::new();
    session.revealed_cards_by_phase = HashMap::new();
    session.showdown_tx_hash = None;
    session.showdown_session_id = None;
    session.showdown_result = None;
    session.proof_nonce = 0;

    Ok(Json(DealResponse {
        status: "dealt".to_string(),
        deck_root: parsed_deal.deck_root,
        hand_commitments: parsed_deal.hand_commitments,
        proof_size: deal_proof.proof.len(),
        session_id: deal_proof.session_id,
        tx_hash,
    }))
}

/// POST /api/table/{table_id}/request-reveal/{phase}
pub async fn request_reveal(
    State(state): State<AppState>,
    Path((table_id, phase)): Path<(u32, String)>,
    headers: HeaderMap,
) -> Result<Json<RevealResponse>, StatusCode> {
    validate_table_id(table_id)?;
    validate_reveal_phase(&phase)?;

    let action = format!("request_reveal:{}", phase);
    enforce_rate_limit(&state, &headers, table_id, &action).await?;

    ensure_session_exists(&state, table_id).await?;

    let mut tables = state.tables.write().await;
    let session = tables.get_mut(&table_id).ok_or(StatusCode::NOT_FOUND)?;

    if session.selected_node_endpoints.is_empty() {
        session.selected_node_endpoints = select_mpc_nodes(&state, None).await?;
    }
    let node_endpoints = session.selected_node_endpoints.clone();

    // Any caller may trigger reveal progression.
    // Private card data remains protected by get_player_cards auth checks.

    let expected_next_phase = match session.phase.as_str() {
        "preflop" => "flop",
        "flop" => "turn",
        "turn" => "river",
        _ => return Err(StatusCode::CONFLICT),
    };
    if phase != expected_next_phase {
        return Err(StatusCode::CONFLICT);
    }

    if let Some(existing_hash) = session.reveal_tx_hashes.get(&phase) {
        let cards = session
            .revealed_cards_by_phase
            .get(&phase)
            .cloned()
            .unwrap_or_default();
        let session_id = session
            .reveal_session_ids
            .get(&phase)
            .cloned()
            .unwrap_or_default();
        return Ok(Json(RevealResponse {
            status: "revealed".to_string(),
            cards,
            proof_size: 0,
            session_id,
            tx_hash: Some(existing_hash.clone()),
        }));
    }

    if state.soroban_config.is_configured() {
        if let Err(e) =
            soroban::maybe_auto_advance_betting_for_reveal(&state.soroban_config, table_id, &phase)
                .await
        {
            if is_identity_missing_error(&e) {
                tracing::warn!(
                    "Skipping local auto-advance before reveal (phase={}): {}",
                    phase,
                    e
                );
            } else {
                tracing::error!(
                    "Failed to auto-advance betting before reveal (phase={}): {}",
                    phase,
                    e
                );
                return Err(StatusCode::BAD_GATEWAY);
            }
        }
    }

    let prepared_reveal = mpc::prepare_reveal_from_nodes(
        &node_endpoints,
        &state.mpc_config.circuit_dir,
        table_id,
        &phase,
        &session.dealt_indices,
        &session.deck_root,
    )
    .await
    .map_err(|e| {
        tracing::error!("Reveal preparation failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let proof_session_id = next_proof_session_id(session, &format!("reveal-{}", phase));
    let _guard = SessionGuard::new(state.metrics.active_mpc_sessions.clone());
    let reveal_proof = mpc::generate_proof_from_share_sets(
        table_id,
        &prepared_reveal.share_set_ids,
        &proof_session_id,
        "reveal_board_valid",
        &state.mpc_config.circuit_dir,
        &node_endpoints,
    )
    .await
    .map_err(|e| {
        tracing::error!("Reveal proof generation failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let num_revealed = match phase.as_str() {
        "flop" => 3usize,
        "turn" => 1usize,
        "river" => 1usize,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let parsed_reveal =
        parse_reveal_outputs(&reveal_proof.public_inputs, num_revealed).map_err(|e| {
            tracing::error!("Reveal public input parsing failed: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    let tx_hash = match soroban::submit_reveal_proof(
        &state.soroban_config,
        table_id,
        &reveal_proof.proof,
        &reveal_proof.public_inputs,
        &parsed_reveal.cards,
        &parsed_reveal.indices,
    )
    .await
    {
        Ok(h) if !h.is_empty() => Some(h),
        Ok(_) => None,
        Err(e) => {
            if state.soroban_config.is_configured() {
                tracing::error!("Soroban reveal proof submission failed: {}", e);
                return Err(StatusCode::BAD_GATEWAY);
            }
            tracing::warn!("Soroban reveal proof submission skipped/failed: {}", e);
            None
        }
    };

    session
        .dealt_indices
        .extend(parsed_reveal.indices.iter().copied());
    session
        .board_indices
        .extend(parsed_reveal.indices.iter().copied());
    session.phase = phase.clone();
    if let Some(hash) = tx_hash.clone() {
        session.reveal_tx_hashes.insert(phase.clone(), hash);
    }
    session
        .reveal_session_ids
        .insert(phase.clone(), reveal_proof.session_id.clone());
    session
        .revealed_cards_by_phase
        .insert(phase.clone(), parsed_reveal.cards.clone());

    Ok(Json(RevealResponse {
        status: "revealed".to_string(),
        cards: parsed_reveal.cards,
        proof_size: reveal_proof.proof.len(),
        session_id: reveal_proof.session_id,
        tx_hash,
    }))
}

/// POST /api/table/{table_id}/request-showdown
pub async fn request_showdown(
    State(state): State<AppState>,
    Path(table_id): Path<u32>,
    headers: HeaderMap,
) -> Result<Json<ShowdownResponse>, StatusCode> {
    validate_table_id(table_id)?;

    enforce_rate_limit(&state, &headers, table_id, "request_showdown").await?;

    ensure_session_exists(&state, table_id).await?;

    let mut tables = state.tables.write().await;
    let session = tables.get_mut(&table_id).ok_or(StatusCode::NOT_FOUND)?;

    if session.selected_node_endpoints.is_empty() {
        session.selected_node_endpoints = select_mpc_nodes(&state, None).await?;
    }
    let node_endpoints = session.selected_node_endpoints.clone();

    // Any caller may trigger showdown progression.

    if session.phase == "settlement" {
        let (status, winner, winner_index) =
            if let Some((winner, winner_index)) = &session.showdown_result {
                (
                    "showdown_complete".to_string(),
                    winner.clone(),
                    *winner_index,
                )
            } else {
                ("settled_timeout".to_string(), String::new(), 0)
            };

        return Ok(Json(ShowdownResponse {
            status,
            winner,
            winner_index,
            proof_size: 0,
            session_id: session.showdown_session_id.clone().unwrap_or_default(),
            tx_hash: session.showdown_tx_hash.clone(),
        }));
    }

    if session.phase != "river" && session.phase != "showdown" {
        return Err(StatusCode::CONFLICT);
    }

    if state.soroban_config.is_configured() && session.phase == "river" {
        if let Err(e) =
            soroban::maybe_auto_advance_betting_for_showdown(&state.soroban_config, table_id).await
        {
            if is_identity_missing_error(&e) {
                tracing::warn!("Skipping local auto-advance before showdown: {}", e);
            } else {
                tracing::error!("Failed to auto-advance betting before showdown: {}", e);
                return Err(StatusCode::BAD_GATEWAY);
            }
        }
    }

    let prepared_showdown = mpc::prepare_showdown_from_nodes(
        &node_endpoints,
        &state.mpc_config.circuit_dir,
        table_id,
        &session.board_indices,
        session.player_order.len() as u32,
        &session.hand_commitments,
        &session.deck_root,
    )
    .await
    .map_err(|e| {
        tracing::error!("Showdown preparation failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let proof_session_id = next_proof_session_id(session, "showdown");
    let _guard = SessionGuard::new(state.metrics.active_mpc_sessions.clone());
    let showdown_proof = mpc::generate_proof_from_share_sets(
        table_id,
        &prepared_showdown.share_set_ids,
        &proof_session_id,
        "showdown_valid",
        &state.mpc_config.circuit_dir,
        &node_endpoints,
    )
    .await
    .map_err(|e| {
        tracing::error!("Showdown proof generation failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let parsed_showdown =
        parse_showdown_outputs(&showdown_proof.public_inputs, session.player_order.len()).map_err(
            |e| {
                tracing::error!("Showdown public input parsing failed: {}", e);
                StatusCode::BAD_GATEWAY
            },
        )?;

    if parsed_showdown.winner_index as usize >= session.player_order.len() {
        tracing::error!(
            "Showdown winner index out of range: {} >= {}",
            parsed_showdown.winner_index,
            session.player_order.len()
        );
        return Err(StatusCode::BAD_GATEWAY);
    }
    if parsed_showdown.tie_mask & (1u32 << parsed_showdown.winner_index) == 0 {
        tracing::error!(
            "Showdown tie mask {} does not include winner index {}",
            parsed_showdown.tie_mask,
            parsed_showdown.winner_index
        );
        return Err(StatusCode::BAD_GATEWAY);
    }
    let winner = session.player_order[parsed_showdown.winner_index as usize].clone();

    let (tx_hash, settled_by_timeout) = match soroban::submit_showdown_proof(
        &state.soroban_config,
        table_id,
        &showdown_proof.proof,
        &showdown_proof.public_inputs,
        &parsed_showdown.hole_cards,
    )
    .await
    {
        Ok(h) if !h.is_empty() => (Some(h), false),
        Ok(_) => (None, false),
        Err(e) => {
            if state.soroban_config.is_configured() {
                tracing::error!("Soroban showdown proof submission failed: {}", e);
                match soroban::claim_timeout(&state.soroban_config, table_id).await {
                    Ok(h) if !h.is_empty() => {
                        tracing::warn!(
                            "Showdown proof rejected on-chain; settled table {} via timeout fallback",
                            table_id
                        );
                        (Some(h), true)
                    }
                    Ok(_) => {
                        tracing::warn!(
                            "Showdown proof rejected on-chain; timeout fallback returned empty hash for table {}",
                            table_id
                        );
                        (None, true)
                    }
                    Err(timeout_err) => {
                        tracing::error!(
                            "Showdown proof rejected and timeout fallback failed for table {}: {}",
                            table_id,
                            timeout_err
                        );
                        return Err(StatusCode::BAD_GATEWAY);
                    }
                }
            } else {
                tracing::warn!("Soroban showdown proof submission skipped/failed: {}", e);
                (None, false)
            }
        }
    };

    session.phase = "settlement".to_string();
    session.showdown_tx_hash = tx_hash.clone();
    session.showdown_session_id = Some(showdown_proof.session_id.clone());
    session.showdown_result = if settled_by_timeout {
        None
    } else {
        Some((winner.clone(), parsed_showdown.winner_index))
    };

    let (status, winner, winner_index) = if settled_by_timeout {
        ("settled_timeout".to_string(), String::new(), 0)
    } else {
        (
            "showdown_complete".to_string(),
            winner,
            parsed_showdown.winner_index,
        )
    };

    Ok(Json(ShowdownResponse {
        status,
        winner,
        winner_index,
        proof_size: showdown_proof.proof.len(),
        session_id: showdown_proof.session_id,
        tx_hash,
    }))
}

/// POST /api/table/{table_id}/player-action
///
/// Submit a player betting action to the on-chain poker-table contract.
/// In lobby mode, authenticated wallet addresses are translated to their
/// mapped on-chain seat address.
pub async fn player_action(
    State(state): State<AppState>,
    Path(table_id): Path<u32>,
    headers: HeaderMap,
    Json(req): Json<PlayerActionRequest>,
) -> Result<Json<PlayerActionResponse>, StatusCode> {
    validate_table_id(table_id)?;

    let normalized = req.action.trim().to_ascii_lowercase();
    let amount = match normalized.as_str() {
        "fold" | "check" | "call" | "allin" | "all_in" => None,
        "bet" | "raise" => {
            let amount = req.amount.ok_or(StatusCode::BAD_REQUEST)?;
            if amount <= 0 {
                return Err(StatusCode::BAD_REQUEST);
            }
            Some(amount)
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let action_key = format!("player_action:{}", normalized);
    enforce_rate_limit(&state, &headers, table_id, &action_key).await?;
    let auth = validate_signed_request(&state, &headers, table_id, &action_key, None).await?;

    if !state.soroban_config.is_configured() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let mapped_player = {
        let lobby = state.lobby_assignments.read().await;
        lobby
            .get(&table_id)
            .and_then(|table_lobby| table_lobby.get(&auth.address))
            .cloned()
    };

    let caller_is_seated = fetch_onchain_table_view(&state.soroban_config, table_id)
        .await
        .map(|view| view.seats.iter().any(|(_, chain)| chain == &auth.address))
        .unwrap_or(false);

    let player_address = if let Some(mapped) = mapped_player {
        mapped
    } else if caller_is_seated {
        auth.address.clone()
    } else if state.soroban_config.has_identity_for_player(&auth.address) {
        auth.address.clone()
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let tx_hash = soroban::submit_player_action(
        &state.soroban_config,
        table_id,
        &player_address,
        &normalized,
        amount,
    )
    .await
    .map_err(|e| {
        tracing::error!(
            "player_action failed: table={}, caller={}, player={}, action={}, amount={:?}, err={}",
            table_id,
            auth.address,
            player_address,
            normalized,
            amount,
            e
        );
        if e.contains("Error(Contract,") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_GATEWAY
        }
    })?;

    let tx_hash = if tx_hash.is_empty() {
        None
    } else {
        Some(tx_hash)
    };
    Ok(Json(PlayerActionResponse {
        status: "applied".to_string(),
        action: normalized,
        amount,
        player: player_address,
        tx_hash,
    }))
}

/// GET /api/table/{table_id}/player/{address}/cards
///
/// Resolve and return a player's hole cards by chaining permutation lookups
/// across MPC nodes.
pub async fn get_player_cards(
    State(state): State<AppState>,
    Path((table_id, address)): Path<(u32, String)>,
    headers: HeaderMap,
) -> Result<Json<PlayerCardsResponse>, StatusCode> {
    validate_table_id(table_id)?;
    let auth = validate_signed_request(
        &state,
        &headers,
        table_id,
        "get_player_cards",
        Some(&address),
    )
    .await?;

    ensure_session_exists(&state, table_id).await?;

    let tables = state.tables.read().await;
    let session = tables.get(&table_id).ok_or(StatusCode::NOT_FOUND)?;

    let insecure_auth = allow_insecure_dev_auth();
    if !insecure_auth && !session.player_order.iter().any(|p| p == &auth.address) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let player_index = session
        .player_order
        .iter()
        .position(|p| p == &address)
        .or_else(|| if insecure_auth { Some(0) } else { None })
        .ok_or(StatusCode::NOT_FOUND)?;

    let (pos1, pos2) = session
        .player_card_positions
        .get(player_index)
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut node_endpoints = session.selected_node_endpoints.clone();
    if node_endpoints.is_empty() {
        node_endpoints = select_mpc_nodes(&state, None).await?;
    }
    let positions = vec![*pos1, *pos2];
    drop(tables); // release read lock before async call

    let (cards, salts) = mpc::resolve_hole_cards(&node_endpoints, table_id, &positions)
        .await
        .map_err(|e| {
            tracing::error!("Failed to resolve hole cards: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    if cards.len() < 2 || salts.len() < 2 {
        return Err(StatusCode::BAD_GATEWAY);
    }

    Ok(Json(PlayerCardsResponse {
        card1: cards[0],
        card2: cards[1],
        salt1: salts[0].clone(),
        salt2: salts[1].clone(),
    }))
}

/// GET /api/table/{table_id}/state
pub async fn get_table_state(
    State(state): State<AppState>,
    Path(table_id): Path<u32>,
) -> Result<Json<TableStateResponse>, StatusCode> {
    let result = soroban::get_table_state(&state.soroban_config, table_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to read table state: {}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    Ok(Json(TableStateResponse { state: result }))
}

/// GET /api/committee/status
pub async fn committee_status(State(state): State<AppState>) -> Json<CommitteeStatusResponse> {
    let healths = state.metrics.node_healths.lock().await;
    let nodes = healths.len();
    let healthy = healths.iter().map(|h| h.connected).collect();

    Json(CommitteeStatusResponse {
        nodes,
        healthy,
        status: "active".to_string(),
    })
}

/// POST /api/node/register
///
/// Self-registration (and implicit heartbeat) for an MPC node. Only available
/// when dynamic discovery is active (no committee registry and no static
/// `MPC_NODE_*` endpoints); otherwise returns `409 Conflict`.
pub async fn register_node(
    State(state): State<AppState>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<Json<NodeRegistryResponse>, StatusCode> {
    if !dynamic_discovery_active(&state) {
        return Err(StatusCode::CONFLICT);
    }
    let id = req.id.trim();
    let endpoint = req.endpoint.trim();
    if id.is_empty() || !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut registry = state.node_registry.write().await;
    let created = registry.register(id.to_string(), endpoint.to_string());
    tracing::info!(
        "MPC node '{}' {} ({})",
        id,
        if created { "registered" } else { "refreshed" },
        endpoint
    );
    Ok(Json(NodeRegistryResponse {
        id: id.to_string(),
        registered: registry.len(),
        healthy: registry.healthy_endpoints().len(),
    }))
}

/// POST /api/node/{id}/heartbeat
///
/// Keep an existing registration alive. Returns `404` if the node is unknown
/// (the node should re-register), or `409` when dynamic discovery is inactive.
pub async fn node_heartbeat(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NodeRegistryResponse>, StatusCode> {
    if !dynamic_discovery_active(&state) {
        return Err(StatusCode::CONFLICT);
    }
    let mut registry = state.node_registry.write().await;
    if !registry.heartbeat(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(NodeRegistryResponse {
        id,
        registered: registry.len(),
        healthy: registry.healthy_endpoints().len(),
    }))
}

/// DELETE /api/node/{id}
///
/// Graceful deregistration (e.g. on node shutdown). Returns `404` if unknown,
/// or `409` when dynamic discovery is inactive.
pub async fn deregister_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NodeRegistryResponse>, StatusCode> {
    if !dynamic_discovery_active(&state) {
        return Err(StatusCode::CONFLICT);
    }
    let mut registry = state.node_registry.write().await;
    if !registry.deregister(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    tracing::info!("MPC node '{}' deregistered", id);
    Ok(Json(NodeRegistryResponse {
        id,
        registered: registry.len(),
        healthy: registry.healthy_endpoints().len(),
    }))
}

/// POST /api/session/:session_id/cancel
///
/// Deprecated admin endpoint for manual MPC session cancellation.
/// Use POST /api/admin/sessions/:session_id/cancel instead.
/// Kills associated processes, removes temp files, and marks the session freed.
pub async fn cancel_mpc_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let auth =
        admin::validate_admin_request(&state, &headers, "cancel_session", &state.admin_state)
            .await?;
    admin::require_role(&auth, admin::AdminRole::Operator)?;

    let cancelled = session_gc::cancel_session(
        &state.mpc_sessions,
        &session_id,
        &format!(
            "manual admin cancel by {} ({})",
            auth.address,
            auth.role.as_str()
        ),
        false,
    )
    .await;

    if cancelled {
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// ─── Admin Endpoints ────────────────────────────────────────────────────────

/// GET /api/admin/health
///
/// Detailed health information. Requires read-only or higher.
pub async fn admin_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth =
        admin::validate_admin_request(&state, &headers, "admin_health", &state.admin_state).await?;
    admin::require_role(&auth, admin::AdminRole::ReadOnly)?;

    let uptime_seconds = state.metrics.boot_time.elapsed().as_secs();
    let mpc_nodes = state.metrics.node_healths.lock().await.clone();
    let active_mpc_sessions = state
        .metrics
        .active_mpc_sessions
        .load(std::sync::atomic::Ordering::SeqCst);
    let request_metrics = state.metrics.route_metrics.lock().await.clone();

    let session_count = state.mpc_sessions.read().await.len();

    Ok(Json(serde_json::json!({
        "service": "coordinator",
        "uptime_seconds": uptime_seconds,
        "mpc_nodes": mpc_nodes,
        "active_mpc_sessions": active_mpc_sessions,
        "total_session_records": session_count,
        "request_metrics": request_metrics,
        "admin": {
            "address": auth.address,
            "role": auth.role.as_str(),
        }
    })))
}

/// GET /api/admin/sessions
///
/// List all tracked MPC sessions with status and metadata.
/// Requires operator or higher.
pub async fn admin_list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth =
        admin::validate_admin_request(&state, &headers, "admin_list_sessions", &state.admin_state)
            .await?;
    admin::require_role(&auth, admin::AdminRole::Operator)?;

    let sessions = state.mpc_sessions.read().await;
    let mut list = Vec::new();
    for (id, session) in sessions.iter() {
        list.push(serde_json::json!({
            "session_id": id,
            "table_id": session.table_id,
            "status": session.status.to_string(),
            "cancel_reason": session.cancel_reason,
            "elapsed_secs": session.started_at.elapsed().as_secs(),
        }));
    }

    Ok(Json(serde_json::json!({
        "count": list.len(),
        "sessions": list,
    })))
}

/// POST /api/admin/sessions/:session_id/cancel
///
/// Cancel a specific MPC session by ID. Requires operator or higher.
pub async fn admin_cancel_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth =
        admin::validate_admin_request(&state, &headers, "admin_cancel_session", &state.admin_state)
            .await?;
    admin::require_role(&auth, admin::AdminRole::Operator)?;

    let reason = format!("admin cancel by {} ({})", auth.address, auth.role.as_str());
    let cancelled =
        session_gc::cancel_session(&state.mpc_sessions, &session_id, &reason, false).await;

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "cancelled": cancelled,
        "cancelled_by": auth.address,
        "role": auth.role.as_str(),
    })))
}

/// POST /api/admin/sessions/cleanup
///
/// Force-cleanup all expired/stale MPC sessions. Requires operator or higher.
pub async fn admin_cleanup_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = admin::validate_admin_request(
        &state,
        &headers,
        "admin_cleanup_sessions",
        &state.admin_state,
    )
    .await?;
    admin::require_role(&auth, admin::AdminRole::Operator)?;

    let mut sessions = state.mpc_sessions.write().await;
    let now = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(3600); // 1 hour default
    let mut removed = 0u32;

    sessions.retain(|id, s| {
        if now.duration_since(s.started_at) > timeout {
            tracing::info!(
                "Admin cleanup: removing stale session {} (table={}, status={})",
                id,
                s.table_id,
                s.status
            );
            removed += 1;
            false
        } else {
            true
        }
    });

    Ok(Json(serde_json::json!({
        "removed": removed,
        "remaining": sessions.len(),
        "action_by": auth.address,
        "role": auth.role.as_str(),
    })))
}

/// GET /api/admin/stats
///
/// Detailed admin stats including per-route metrics and system health.
/// Requires read-only or higher.
pub async fn admin_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth =
        admin::validate_admin_request(&state, &headers, "admin_stats", &state.admin_state).await?;
    admin::require_role(&auth, admin::AdminRole::ReadOnly)?;

    let uptime = state.metrics.boot_time.elapsed().as_secs();
    let active_sessions = state
        .metrics
        .active_mpc_sessions
        .load(std::sync::atomic::Ordering::SeqCst);
    let route_metrics = state.metrics.route_metrics.lock().await.clone();
    let node_healths = state.metrics.node_healths.lock().await.clone();

    // Aggregate some stats
    let total_requests: u64 = route_metrics.values().map(|m| m.count).sum();
    let total_errors: u64 = route_metrics.values().map(|m| m.errors).sum();
    let healthy_nodes = node_healths.iter().filter(|n| n.connected).count();

    Ok(Json(serde_json::json!({
        "uptime_seconds": uptime,
        "active_mpc_sessions": active_sessions,
        "total_requests": total_requests,
        "total_errors": total_errors,
        "healthy_nodes": healthy_nodes,
        "total_nodes": node_healths.len(),
        "route_metrics": route_metrics,
        "admin": {
            "address": auth.address,
            "role": auth.role.as_str(),
        }
    })))
}

/// POST /api/admin/config/reload
///
/// Reload admin configuration (ADMIN_KEYS) from environment without restart.
/// Requires super-admin.
pub async fn admin_reload_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth =
        admin::validate_admin_request(&state, &headers, "admin_reload_config", &state.admin_state)
            .await?;
    admin::require_role(&auth, admin::AdminRole::SuperAdmin)?;

    let new_config = admin::AdminConfig::from_env();
    tracing::info!(
        "Admin config reloaded by {} — {} admin key(s) loaded",
        auth.address,
        new_config.entries.len()
    );

    *state.admin_config.write().await = new_config;

    Ok(Json(serde_json::json!({
        "status": "reloaded",
        "action_by": auth.address,
        "role": auth.role.as_str(),
    })))
}

/// GET /api/session/:session_id/status
///
/// Returns the current status of an MPC session so clients can detect timeouts.
pub async fn get_mpc_session_status(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sessions = state.mpc_sessions.read().await;
    let Some(session) = sessions.get(&session_id) else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(Json(serde_json::json!({
        "session_id": session.session_id,
        "table_id": session.table_id,
        "status": session.status.to_string(),
        "cancel_reason": session.cancel_reason,
        "elapsed_secs": session.started_at.elapsed().as_secs(),
    })))
}

/// GET /api/admin/archives
///
/// Query archived sessions by ID, table ID, or time range.
/// Requires operator or higher.
pub async fn admin_list_archives(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ArchiveQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = admin::validate_admin_request(
        &state,
        &headers,
        "admin_list_archives",
        &state.admin_state,
    )
    .await?;
    admin::require_role(&auth, admin::AdminRole::Operator)?;

    let from_ts = query
        .from_timestamp
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let to_ts = query
        .to_timestamp
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    let archives = crate::archiver::query_archives(
        &state.archive_store,
        query.session_id.as_deref(),
        query.table_id,
        from_ts,
        to_ts,
        limit,
        offset,
    )
    .await;

    Ok(Json(serde_json::json!({
        "archives": archives,
        "count": archives.len(),
        "limit": limit,
        "offset": offset,
    })))
}

/// GET /api/admin/archives/:archive_id
///
/// Retrieve a single archived session by archive ID.
/// Requires operator or higher.
pub async fn admin_get_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(archive_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = admin::validate_admin_request(
        &state,
        &headers,
        "admin_get_archive",
        &state.admin_state,
    )
    .await?;
    admin::require_role(&auth, admin::AdminRole::Operator)?;

    match crate::archiver::restore_archived_session(&state.archive_store, &archive_id).await {
        Some(archived) => Ok(Json(serde_json::json!(archived))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /api/admin/archives/purge
///
/// Manually trigger purge of archives that exceed the purge TTL.
/// Requires operator or higher.
pub async fn admin_purge_archives(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = admin::validate_admin_request(
        &state,
        &headers,
        "admin_purge_archives",
        &state.admin_state,
    )
    .await?;
    admin::require_role(&auth, admin::AdminRole::Operator)?;

    let purged = crate::archiver::purge_old_archives(&state.archive_store, &state.archive_config).await;

    Ok(Json(serde_json::json!({
        "purged": purged,
        "action_by": auth.address,
        "role": auth.role.as_str(),
    })))
}

#[derive(serde::Deserialize)]
pub struct ArchiveQuery {
    pub session_id: Option<String>,
    pub table_id: Option<u32>,
    pub from_timestamp: Option<String>,
    pub to_timestamp: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

