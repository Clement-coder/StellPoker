//! Extended admin API handlers for issues #267, #261, #264, #265

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};

use super::admin::{require_role, validate_admin_request, AdminRole};
use crate::{audit_log, cors_db, rate_limit_db, session_migration, AppState};

// ============================================================================
// Issue #267: Rate Limit Configuration API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct UpsertRateLimitRequest {
    pub config_type: String,
    pub endpoint: Option<String>,
    pub wallet_address: Option<String>,
    pub max_requests: i32,
    pub window_seconds: i32,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct RateLimitConfigResponse {
    pub id: i64,
    pub config_type: String,
    pub endpoint: Option<String>,
    pub wallet_address: Option<String>,
    pub max_requests: i32,
    pub window_seconds: i32,
    pub enabled: bool,
}

pub async fn admin_list_rate_limits(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_list_rate_limits",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::ReadOnly)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let configs = rate_limit_db::list_rate_limit_configs(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list rate limit configs: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let response_configs: Vec<RateLimitConfigResponse> = configs
        .into_iter()
        .map(|c| RateLimitConfigResponse {
            id: c.id,
            config_type: c.config_type,
            endpoint: c.endpoint,
            wallet_address: c.wallet_address,
            max_requests: c.max_requests,
            window_seconds: c.window_seconds,
            enabled: c.enabled,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "configs": response_configs,
        "count": response_configs.len()
    })))
}

pub async fn admin_upsert_rate_limit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpsertRateLimitRequest>,
) -> Result<Json<RateLimitConfigResponse>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_upsert_rate_limit",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let config = rate_limit_db::upsert_rate_limit_config(
        pool,
        &req.config_type,
        req.endpoint.as_deref(),
        req.wallet_address.as_deref(),
        req.max_requests,
        req.window_seconds,
        req.enabled,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to upsert rate limit config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!(
        "Rate limit config upserted by {}: type={}, endpoint={:?}, max_requests={}",
        auth.address,
        config.config_type,
        config.endpoint,
        config.max_requests
    );

    Ok(Json(RateLimitConfigResponse {
        id: config.id,
        config_type: config.config_type,
        endpoint: config.endpoint,
        wallet_address: config.wallet_address,
        max_requests: config.max_requests,
        window_seconds: config.window_seconds,
        enabled: config.enabled,
    }))
}

pub async fn admin_delete_rate_limit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_delete_rate_limit",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let deleted = rate_limit_db::delete_rate_limit_config(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete rate limit config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    tracing::info!("Rate limit config {} deleted by {}", id, auth.address);

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id
    })))
}

// ============================================================================
// Issue #261: CORS Configuration API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct UpsertCorsRequest {
    pub origin: String,
    pub enabled: bool,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CorsConfigResponse {
    pub id: i64,
    pub origin: String,
    pub enabled: bool,
    pub description: Option<String>,
}

pub async fn admin_list_cors(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth =
        validate_admin_request(&state, &headers, "admin_list_cors", &state.admin_state).await?;
    require_role(&auth, AdminRole::ReadOnly)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let configs = cors_db::list_cors_configs(pool).await.map_err(|e| {
        tracing::error!("Failed to list CORS configs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let response_configs: Vec<CorsConfigResponse> = configs
        .into_iter()
        .map(|c| CorsConfigResponse {
            id: c.id,
            origin: c.origin,
            enabled: c.enabled,
            description: c.description,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "configs": response_configs,
        "count": response_configs.len()
    })))
}

pub async fn admin_upsert_cors(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpsertCorsRequest>,
) -> Result<Json<CorsConfigResponse>, StatusCode> {
    let auth =
        validate_admin_request(&state, &headers, "admin_upsert_cors", &state.admin_state).await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let config =
        cors_db::upsert_cors_origin(pool, &req.origin, req.enabled, req.description.as_deref())
            .await
            .map_err(|e| {
                tracing::error!("Failed to upsert CORS config: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    tracing::info!(
        "CORS config upserted by {}: origin={}, enabled={}",
        auth.address,
        config.origin,
        config.enabled
    );

    Ok(Json(CorsConfigResponse {
        id: config.id,
        origin: config.origin,
        enabled: config.enabled,
        description: config.description,
    }))
}

pub async fn admin_delete_cors(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth =
        validate_admin_request(&state, &headers, "admin_delete_cors", &state.admin_state).await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let deleted = cors_db::delete_cors_origin(pool, id).await.map_err(|e| {
        tracing::error!("Failed to delete CORS config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    };

    tracing::info!("CORS config {} deleted by {}", id, auth.address);

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id
    })))
}

// ============================================================================
// Issue #265: Audit Log Query API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub requester_address: Option<String>,
    pub action: Option<String>,
    pub table_id: Option<i32>,
    pub from_timestamp: Option<String>,
    pub to_timestamp: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub id: i64,
    pub request_id: String,
    pub timestamp: String,
    pub requester_address: Option<String>,
    pub action: String,
    pub endpoint: String,
    pub method: String,
    pub ip_address: Option<String>,
    pub response_status: Option<i32>,
    pub error_message: Option<String>,
    pub table_id: Option<i32>,
    pub session_id: Option<String>,
    pub record_hash: String,
}

pub async fn admin_query_audit_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_query_audit_logs",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::ReadOnly)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let from_timestamp = query
        .from_timestamp
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let to_timestamp = query
        .to_timestamp
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    let logs = audit_log::query_audit_logs(
        pool,
        query.requester_address.as_deref(),
        query.action.as_deref(),
        query.table_id,
        from_timestamp,
        to_timestamp,
        limit,
        offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to query audit logs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let response_logs: Vec<AuditLogResponse> = logs
        .into_iter()
        .map(|log| AuditLogResponse {
            id: log.id,
            request_id: log.request_id.to_string(),
            timestamp: log.timestamp.to_rfc3339(),
            requester_address: log.requester_address,
            action: log.action,
            endpoint: log.endpoint,
            method: log.method,
            ip_address: log.ip_address,
            response_status: log.response_status,
            error_message: log.error_message,
            table_id: log.table_id,
            session_id: log.session_id,
            record_hash: log.record_hash,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "logs": response_logs,
        "count": response_logs.len(),
        "limit": limit,
        "offset": offset
    })))
}

pub async fn admin_verify_audit_chain(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_verify_audit_chain",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::ReadOnly)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let valid = audit_log::verify_audit_chain(pool).await.map_err(|e| {
        tracing::error!("Failed to verify audit chain: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "valid": valid,
        "verified_by": auth.address,
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

// ============================================================================
// Issue #264: Session Migration API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct InitiateMigrationRequest {
    pub session_id: String,
    pub table_id: u32,
    pub to_instance_id: String,
}

#[derive(Debug, Serialize)]
pub struct MigrationResponse {
    pub id: i64,
    pub session_id: String,
    pub table_id: i32,
    pub from_instance_id: String,
    pub to_instance_id: String,
    pub migration_status: String,
    pub initiated_at: String,
}

pub async fn admin_list_migrations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_list_migrations",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let migrations = session_migration::list_pending_migrations(pool, &state.instance_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list migrations: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let response_migrations: Vec<MigrationResponse> = migrations
        .into_iter()
        .map(|m| MigrationResponse {
            id: m.id,
            session_id: m.session_id,
            table_id: m.table_id,
            from_instance_id: m.from_instance_id,
            to_instance_id: m.to_instance_id,
            migration_status: m.migration_status,
            initiated_at: m.initiated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(serde_json::json!({
        "migrations": response_migrations,
        "count": response_migrations.len(),
        "instance_id": state.instance_id
    })))
}

pub async fn admin_initiate_migration(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<InitiateMigrationRequest>,
) -> Result<Json<MigrationResponse>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_initiate_migration",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    // Get session state
    let tables = state.tables.read().await;
    let table_session = tables.get(&req.table_id).ok_or(StatusCode::NOT_FOUND)?;

    let session_state = session_migration::SessionState {
        table_id: table_session.table_id,
        phase: table_session.phase.clone(),
        deck_root: table_session.deck_root.clone(),
        hand_commitments: table_session.hand_commitments.clone(),
        player_order: table_session.player_order.clone(),
        dealt_indices: table_session.dealt_indices.clone(),
        board_indices: table_session.board_indices.clone(),
        reveal_tx_hashes: table_session.reveal_tx_hashes.clone(),
        proof_nonce: table_session.proof_nonce,
    };

    let mpc_connections = session_migration::MpcConnections {
        node_endpoints: state.mpc_config.node_endpoints.clone(),
        active_share_sets: Vec::new(),
    };

    let pending_actions = session_migration::PendingActions {
        actions: Vec::new(),
    };

    drop(tables);

    let migration = session_migration::initiate_migration(
        pool,
        &req.session_id,
        req.table_id,
        &state.instance_id,
        &req.to_instance_id,
        &session_state,
        &mpc_connections,
        &pending_actions,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to initiate migration: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!(
        "Session migration initiated by {}: session={}, from={}, to={}",
        auth.address,
        req.session_id,
        state.instance_id,
        req.to_instance_id
    );

    Ok(Json(MigrationResponse {
        id: migration.id,
        session_id: migration.session_id,
        table_id: migration.table_id,
        from_instance_id: migration.from_instance_id,
        to_instance_id: migration.to_instance_id,
        migration_status: migration.migration_status,
        initiated_at: migration.initiated_at.to_rfc3339(),
    }))
}

pub async fn admin_complete_migration(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_complete_migration",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let (session_state, _mpc_connections, _pending_actions) =
        session_migration::complete_migration(pool, id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to complete migration: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    tracing::info!(
        "Session migration {} completed by {} for table {}",
        id,
        auth.address,
        session_state.table_id
    );

    Ok(Json(serde_json::json!({
        "migration_id": id,
        "status": "complete",
        "table_id": session_state.table_id
    })))
}

pub async fn admin_cancel_migration(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_cancel_migration",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::Operator)?;

    let Some(pool) = state.db_pool.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let reason = format!("Cancelled by admin {}", auth.address);
    session_migration::cancel_migration(pool, id, &reason)
        .await
        .map_err(|e| {
            tracing::error!("Failed to cancel migration: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tracing::info!("Session migration {} cancelled by {}", id, auth.address);

    Ok(Json(serde_json::json!({
        "migration_id": id,
        "status": "cancelled",
        "cancelled_by": auth.address
    })))
}

// ============================================================================
// Issue #102: Committee Key Rotation Status
// ============================================================================

#[derive(Debug, Serialize)]
pub struct CommitteeKeyRotationStatus {
    pub active_address: String,
    pub active_issued_at_unix: u64,
    pub retiring_address: Option<String>,
    pub rotation_interval_secs: u64,
    pub overlap_secs: u64,
}

/// Read-only view into the committee identity rotation state: which address
/// is currently active, which (if any) is mid-overlap and about to be
/// deregistered, and the configured rotation cadence.
pub async fn admin_committee_key_rotation_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CommitteeKeyRotationStatus>, StatusCode> {
    let auth = validate_admin_request(
        &state,
        &headers,
        "admin_committee_key_rotation_status",
        &state.admin_state,
    )
    .await?;
    require_role(&auth, AdminRole::ReadOnly)?;

    let rotation = state.committee_key_rotation.read().await;
    Ok(Json(CommitteeKeyRotationStatus {
        active_address: rotation.active.address.clone(),
        active_issued_at_unix: rotation.active.issued_at_unix,
        retiring_address: rotation.retiring.as_ref().map(|k| k.address.clone()),
        rotation_interval_secs: rotation.config.rotation_interval.as_secs(),
        overlap_secs: rotation.config.overlap.as_secs(),
    }))
}
