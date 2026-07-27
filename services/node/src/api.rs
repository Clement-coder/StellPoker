//! HTTP API handlers for the MPC node.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::private_table::{self, DealPreparation, RevealPreparation, ShowdownPreparation};
use crate::session::{self, MpcSessionState, SessionStatus};
use crate::NodeState;

#[derive(Deserialize)]
pub struct PrepareDealRequest {
    pub players: Vec<String>,
    pub circuit_dir: String,
}

#[derive(Deserialize)]
pub struct PrepareRevealRequest {
    pub circuit_dir: String,
    pub previously_used_indices: Vec<u32>,
    pub deck_root: String,
}

#[derive(Deserialize)]
pub struct PrepareShowdownRequest {
    pub circuit_dir: String,
    pub board_indices: Vec<u32>,
    pub num_active_players: u32,
    pub hand_commitments: Vec<String>,
    pub deck_root: String,
}

#[derive(Deserialize)]
pub struct DispatchSharesRequest {
    pub share_set_id: String,
    pub proof_session_id: String,
    pub circuit_name: String,
}

#[derive(Deserialize)]
pub struct SharesRequest {
    pub circuit_name: String,
    pub share_data: String, // base64-encoded share file
    pub source_party_id: u32,
    pub total_parties: u32,
}

#[derive(Deserialize)]
pub struct PermLookupRequest {
    pub indices: Vec<u32>,
}

#[derive(Serialize)]
pub struct PermLookupResponse {
    pub mapped_indices: Vec<u32>,
    pub salts: Vec<String>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub session_id: String,
    pub status: String,
}

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub circuit_dir: String,
    pub crs_path: String,
}

/// POST /table/:id/prepare-deal
///
/// Node prepares its own private contribution and returns a share-set handle.
pub async fn post_prepare_deal(
    State(state): State<NodeState>,
    Path(table_id): Path<u32>,
    Json(req): Json<PrepareDealRequest>,
) -> Result<Json<DealPreparation>, (StatusCode, String)> {
    let mut seen = HashSet::new();
    for player in &req.players {
        if player.trim().is_empty() {
            return Err((StatusCode::BAD_REQUEST, "empty player address".to_string()));
        }
        if !seen.insert(player) {
            return Err((
                StatusCode::BAD_REQUEST,
                "duplicate player address".to_string(),
            ));
        }
    }

    let mut tables = state.tables.write().await;
    let prepared = private_table::prepare_deal(
        table_id,
        state.node_id,
        &req.players,
        &req.circuit_dir,
        &mut tables,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(prepared))
}

/// POST /table/:id/prepare-reveal/:phase
///
/// Node prepares reveal contribution shares and returns a share-set handle.
pub async fn post_prepare_reveal(
    State(state): State<NodeState>,
    Path((table_id, phase)): Path<(u32, String)>,
    Json(req): Json<PrepareRevealRequest>,
) -> Result<Json<RevealPreparation>, (StatusCode, String)> {
    let mut tables = state.tables.write().await;
    let prepared = private_table::prepare_reveal(
        table_id,
        state.node_id,
        &phase,
        &req.previously_used_indices,
        &req.deck_root,
        &req.circuit_dir,
        &mut tables,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(prepared))
}

/// POST /table/:id/prepare-showdown
///
/// Node prepares showdown contribution shares and returns a share-set handle.
pub async fn post_prepare_showdown(
    State(state): State<NodeState>,
    Path(table_id): Path<u32>,
    Json(req): Json<PrepareShowdownRequest>,
) -> Result<Json<ShowdownPreparation>, (StatusCode, String)> {
    let mut tables = state.tables.write().await;
    let prepared = private_table::prepare_showdown(
        table_id,
        state.node_id,
        &req.board_indices,
        req.num_active_players,
        &req.hand_commitments,
        &req.deck_root,
        &req.circuit_dir,
        &mut tables,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(prepared))
}

/// POST /table/:id/dispatch-shares
///
/// Node sends this source party's per-recipient shares directly to MPC peers.
pub async fn post_dispatch_shares(
    State(state): State<NodeState>,
    Path(table_id): Path<u32>,
    Json(req): Json<DispatchSharesRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if req.share_set_id.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "missing share_set_id".to_string()));
    }
    if req.proof_session_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "missing proof_session_id".to_string(),
        ));
    }
    if req.circuit_name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "missing circuit_name".to_string()));
    }

    let share_data_by_party = {
        let tables = state.tables.read().await;
        private_table::clone_share_set(table_id, &req.share_set_id, &tables)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?
    };

    private_table::dispatch_share_payloads(
        &req.proof_session_id,
        &req.circuit_name,
        &state.peer_http_endpoints,
        state.node_id,
        &share_data_by_party,
    )
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;

    {
        let mut tables = state.tables.write().await;
        private_table::remove_share_set(table_id, &req.share_set_id, &mut tables)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    }

    Ok(StatusCode::OK)
}

/// POST /table/:table_id/perm-lookup
///
/// Look up permutation mappings and salts for given deck positions.
/// Used by the coordinator to resolve hole cards after a deal.
pub async fn post_perm_lookup(
    State(state): State<NodeState>,
    Path(table_id): Path<u32>,
    Json(req): Json<PermLookupRequest>,
) -> Result<Json<PermLookupResponse>, (StatusCode, String)> {
    if req.indices.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "indices must not be empty".to_string(),
        ));
    }

    let tables = state.tables.read().await;
    let mapped_indices = private_table::perm_lookup(table_id, &req.indices, &tables)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let salts = private_table::salt_lookup(table_id, &req.indices, &tables)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(PermLookupResponse {
        mapped_indices,
        salts,
    }))
}

/// POST /session/:id/shares
///
/// Receive one source party's secret-share fragment for a proof session.
pub async fn post_shares(
    State(state): State<NodeState>,
    Path(session_id): Path<String>,
    Json(req): Json<SharesRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if req.total_parties == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "total_parties must be > 0".to_string(),
        ));
    }

    let session_lock = {
        let mut sessions = state.sessions.write().await;
        if let Some(existing) = sessions.get(&session_id) {
            existing.clone()
        } else {
            // Reject new sessions once the node is saturated, to prevent DoS via
            // session flooding (issue #315). Existing sessions are never rejected.
            let max = state.limits.max_concurrent_sessions;
            if max > 0 {
                let active = crate::limits::count_active_sessions(&sessions).await;
                if active >= max {
                    tracing::warn!(
                        "rejecting session {}: node at capacity ({} active, limit {})",
                        session_id,
                        active,
                        max
                    );
                    return Err((
                        StatusCode::TOO_MANY_REQUESTS,
                        format!(
                            "node at capacity: {} concurrent sessions (limit {})",
                            active, max
                        ),
                    ));
                }
            }
            let work_dir = tempfile::tempdir()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("tmpdir: {}", e)))?;
            let work_path = work_dir.keep();
            let session =
                MpcSessionState::new(session_id.clone(), req.circuit_name.clone(), work_path);
            let lock = Arc::new(RwLock::new(session));
            sessions.insert(session_id.clone(), lock.clone());
            lock
        }
    };

    let mut session = session_lock.write().await;
    if session.circuit_name != req.circuit_name {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "session circuit mismatch: existing={}, got={}",
                session.circuit_name, req.circuit_name
            ),
        ));
    }

    session::receive_share_fragment(
        &mut session,
        &req.share_data,
        req.source_party_id,
        req.total_parties,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(StatusCode::OK)
}

/// POST /session/:id/generate
///
/// Trigger MPC proof generation in the background.
pub async fn post_generate(
    State(state): State<NodeState>,
    Path(session_id): Path<String>,
    Json(req): Json<GenerateRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sessions = state.sessions.read().await;
    let session_lock = sessions
        .get(&session_id)
        .ok_or((StatusCode::NOT_FOUND, "session not found".to_string()))?
        .clone();

    let mut session = session_lock.write().await;
    let expected_total_parties = session.expected_total_parties.ok_or((
        StatusCode::BAD_REQUEST,
        "no share fragments received".to_string(),
    ))?;
    let partial_share_paths = session
        .partial_share_paths
        .iter()
        .map(|(source, path)| (*source, path.clone()))
        .collect::<Vec<_>>();
    session.status = SessionStatus::WitnessGenerating;

    let sid = session_id.clone();
    let circuit_dir = req.circuit_dir.clone();
    let circuit_name = session.circuit_name.clone();
    let work_dir = session.work_dir.clone();
    let node_id = state.node_id;
    let party_config = state.party_config_path.clone();
    let crs_path = req.crs_path.clone();
    let limits = state.limits.clone();
    let wall_seconds = limits.max_session_wall_seconds;

    let session_lock_bg = session_lock.clone();
    drop(session); // release write lock before spawning

    let metrics = state.metrics.clone();
    metrics.active_sessions.inc();
    let circuit_label = circuit_name.clone();

    tokio::spawn(async move {
        let proof_future = session::run_proof_generation(
            sid.clone(),
            circuit_dir,
            circuit_name,
            work_dir.clone(),
            node_id,
            partial_share_paths,
            expected_total_parties,
            party_config,
            crs_path,
            limits,
        );

        // Enforce a per-session wall-clock budget so a hung proof generation can't
        // pin node resources indefinitely (issue #315). On timeout the subprocess
        // future is dropped, which kills the co-noir child (kill_on_drop).
        let result = if wall_seconds > 0 {
            match tokio::time::timeout(std::time::Duration::from_secs(wall_seconds), proof_future)
                .await
            {
                Ok(r) => r,
                Err(_) => Err(format!(
                    "proof generation exceeded wall-clock limit of {}s",
                    wall_seconds
                )),
            }
        } else {
            proof_future.await
        };

        metrics.active_sessions.dec();

        let mut session = session_lock_bg.write().await;
        match result {
            Ok((proof_bytes, public_inputs)) => {
                let proof_path = work_dir.join("proof.bin");
                if let Err(e) = std::fs::write(&proof_path, &proof_bytes) {
                    metrics
                        .session_errors
                        .with_label_values(&["write_proof"])
                        .inc();
                    session.status = SessionStatus::Failed(format!("write proof: {}", e));
                    tracing::error!(
                        session_id = %sid,
                        phase = "write_proof",
                        node_id,
                        "failed to write proof to disk: {}",
                        e
                    );
                    return;
                }
                session.proof_path = Some(proof_path);
                session.public_inputs = Some(public_inputs);
                session.status = SessionStatus::Complete;
                metrics
                    .proofs_generated
                    .with_label_values(&[&circuit_label])
                    .inc();
                tracing::info!(
                    session_id = %sid,
                    phase = "complete",
                    node_id,
                    circuit = %circuit_label,
                    "proof generation complete"
                );
            }
            Err(e) => {
                metrics
                    .session_errors
                    .with_label_values(&["proof_generation"])
                    .inc();
                session.status = SessionStatus::Failed(e.clone());
                tracing::error!(
                    session_id = %sid,
                    phase = "proof_generation",
                    node_id,
                    error = %e,
                    "proof generation failed"
                );
            }
        }
    });

    Ok(StatusCode::ACCEPTED)
}

/// GET /session/:id/status
pub async fn get_status(
    State(state): State<NodeState>,
    Path(session_id): Path<String>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let sessions = state.sessions.read().await;
    let session_lock = sessions.get(&session_id).ok_or(StatusCode::NOT_FOUND)?;
    let session = session_lock.read().await;

    let status_str = match &session.status {
        SessionStatus::SharesReceived => "shares_received".to_string(),
        SessionStatus::WitnessGenerating => "witness_generating".to_string(),
        SessionStatus::ProofGenerating => "proof_generating".to_string(),
        SessionStatus::Complete => "complete".to_string(),
        SessionStatus::Failed(e) => format!("failed: {}", e),
    };

    Ok(Json(StatusResponse {
        session_id: session.session_id.clone(),
        status: status_str,
    }))
}

/// GET /session/:id/proof
pub async fn get_proof(
    State(state): State<NodeState>,
    Path(session_id): Path<String>,
) -> Result<Json<ProofResponse>, (StatusCode, String)> {
    let sessions = state.sessions.read().await;
    let session_lock = sessions
        .get(&session_id)
        .ok_or((StatusCode::NOT_FOUND, "session not found".to_string()))?;
    let session = session_lock.read().await;

    if session.status != SessionStatus::Complete {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("proof not ready, status: {:?}", session.status),
        ));
    }

    let proof_bytes =
        session::get_proof(&session).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    use base64::Engine;
    let proof_b64 = base64::engine::general_purpose::STANDARD.encode(&proof_bytes);

    Ok(Json(ProofResponse {
        session_id: session.session_id.clone(),
        proof: proof_b64,
        public_inputs: session.public_inputs.clone().unwrap_or_default(),
    }))
}

#[derive(Serialize)]
pub struct ProofResponse {
    pub session_id: String,
    pub proof: String, // base64-encoded proof bytes
    pub public_inputs: Vec<String>,
}
