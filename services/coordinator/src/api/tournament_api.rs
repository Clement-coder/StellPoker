//! HTTP handlers for tournament management.
//!
//! Routes (all under /api/tournaments):
//!   POST   /                          Create a tournament
//!   GET    /                          List all tournaments (summaries)
//!   GET    /:id                       Get tournament detail
//!   POST   /:id/register              Register a player
//!   POST   /:id/start                 Start the tournament
//!   POST   /:id/hand-result           Record hand outcome + stack updates
//!   GET    /:id/balancing             Get table-balancing moves
//!   POST   /:id/cancel                Cancel a tournament in Registration

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::collections::HashMap;

use crate::tournament::{
    CreateTournamentRequest, HandResultRequest, RegisterPlayerRequest, Tournament,
    TournamentDetail, TournamentError, TournamentStatus, TournamentSummary,
};
use crate::AppState;

fn tournament_err_status(e: &TournamentError) -> StatusCode {
    match e {
        TournamentError::NotFound => StatusCode::NOT_FOUND,
        TournamentError::RegistrationClosed
        | TournamentError::TournamentFull
        | TournamentError::AlreadyRegistered
        | TournamentError::NotEnoughPlayers
        | TournamentError::InvalidState => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

/// POST /api/tournaments
pub async fn create_tournament(
    State(state): State<AppState>,
    Json(req): Json<CreateTournamentRequest>,
) -> Result<Json<TournamentDetail>, StatusCode> {
    let tournament = Tournament::new(req);
    let detail = TournamentDetail::from(&tournament);
    let mut store = state.tournaments.write().await;
    store.insert(tournament.id.clone(), tournament);
    Ok(Json(detail))
}

/// GET /api/tournaments
pub async fn list_tournaments(
    State(state): State<AppState>,
) -> Json<Vec<TournamentSummary>> {
    let store = state.tournaments.read().await;
    let mut list: Vec<TournamentSummary> = store.values().map(TournamentSummary::from).collect();
    // Most recently created first.
    list.sort_by(|a, b| b.id.cmp(&a.id));
    Json(list)
}

/// GET /api/tournaments/:id
pub async fn get_tournament(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TournamentDetail>, StatusCode> {
    let store = state.tournaments.read().await;
    store
        .get(&id)
        .map(TournamentDetail::from)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// POST /api/tournaments/:id/register
pub async fn register_player(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RegisterPlayerRequest>,
) -> Result<Json<TournamentDetail>, (StatusCode, String)> {
    let mut store = state.tournaments.write().await;
    let t = store
        .get_mut(&id)
        .ok_or((StatusCode::NOT_FOUND, "tournament not found".to_string()))?;
    t.register(req.address, req.table_contract)
        .map_err(|e| (tournament_err_status(&e), e.to_string()))?;
    Ok(Json(TournamentDetail::from(&*t)))
}

/// POST /api/tournaments/:id/start
pub async fn start_tournament(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TournamentDetail>, (StatusCode, String)> {
    let mut store = state.tournaments.write().await;
    let t = store
        .get_mut(&id)
        .ok_or((StatusCode::NOT_FOUND, "tournament not found".to_string()))?;
    t.start()
        .map_err(|e| (tournament_err_status(&e), e.to_string()))?;
    Ok(Json(TournamentDetail::from(&*t)))
}

/// POST /api/tournaments/:id/hand-result
///
/// Called by the coordinator after each hand settles on-chain.
/// Body: `{ "stacks": { "<address>": <new_stack_stroops>, ... } }`
/// Returns the updated tournament state plus any newly eliminated players.
pub async fn record_hand_result(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<HandResultRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut store = state.tournaments.write().await;
    let t = store
        .get_mut(&id)
        .ok_or((StatusCode::NOT_FOUND, "tournament not found".to_string()))?;

    if t.status != TournamentStatus::Running {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "tournament is not running".to_string(),
        ));
    }

    let eliminated = t.record_hand_result(req.stacks);
    let balancing = t.balancing_moves();
    let empty_tables = t.empty_tables();
    let detail = TournamentDetail::from(&*t);

    Ok(Json(serde_json::json!({
        "tournament": detail,
        "newly_eliminated": eliminated,
        "balancing_moves": balancing,
        "empty_tables": empty_tables,
    })))
}

/// GET /api/tournaments/:id/balancing
pub async fn get_balancing(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.tournaments.read().await;
    let t = store.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let moves = t.balancing_moves();
    let blind = t.current_blind_level();
    Ok(Json(serde_json::json!({
        "balancing_moves": moves,
        "current_small_blind": blind.small_blind,
        "current_big_blind": blind.big_blind,
        "blind_level": t.blind_state.level_index,
    })))
}

/// POST /api/tournaments/:id/cancel
pub async fn cancel_tournament(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TournamentDetail>, (StatusCode, String)> {
    let mut store = state.tournaments.write().await;
    let t = store
        .get_mut(&id)
        .ok_or((StatusCode::NOT_FOUND, "tournament not found".to_string()))?;
    if t.status != TournamentStatus::Registration {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "can only cancel during registration".to_string(),
        ));
    }
    t.status = TournamentStatus::Cancelled;
    Ok(Json(TournamentDetail::from(&*t)))
}
