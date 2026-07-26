//! Handlers for the feature-flag admin API.
//!
//! `GET  /api/flags`       — returns a JSON snapshot of all flag values.
//! `POST /api/flags/:key`  — sets a specific flag key (body: `{"enabled": bool}`).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::AppState;

/// GET /api/flags
///
/// Returns every flag key currently held in the store, including any
/// per-table / per-player scoped overrides that were loaded from env vars
/// or set via the admin endpoint at runtime.
#[utoipa::path(
    get,
    path = "/api/flags",
    tag = "Flags",
    responses(
        (status = 200, description = "Feature flag snapshot", body = HashMap<String, bool>)
    )
)]
pub async fn list_flags(
    State(state): State<AppState>,
) -> Json<HashMap<String, bool>> {
    let snap = state.feature_flags.snapshot().await;
    Json(snap)
}

/// POST /api/flags/:key
///
/// Set or override a flag value at runtime.
///
/// # Path parameters
/// - `key` — the full flag key, e.g. `solo_mode`, `chat_enabled.table.3`
///
/// # Request body
/// ```json
/// { "enabled": true }
/// ```
///
/// Returns `200 OK` on success, `400 Bad Request` if the key is empty.
#[utoipa::path(
    post,
    path = "/api/flags/{key}",
    tag = "Flags",
    params(
        ("key" = String, Path, description = "Full flag key, e.g. `solo_mode`, `chat_enabled.table.3`")
    ),
    request_body = SetFlagBody,
    responses(
        (status = 200, description = "Flag updated"),
        (status = 400, description = "Empty key")
    )
)]
pub async fn set_flag(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<super::types::SetFlagBody>,
) -> StatusCode {
    let key = key.trim().to_string();
    if key.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    state.feature_flags.set_flag(&key, body.enabled).await;
    StatusCode::OK
}

/// Request body for `POST /api/flags/:key`.
#[derive(ToSchema)]
pub struct SetFlagBody {
    pub enabled: bool,
}
