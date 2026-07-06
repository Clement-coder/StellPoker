//! API key management endpoints for admin users.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api_keys::{self, ApiKey};
use crate::api::admin::{validate_admin_request, require_role, AdminRole};
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub node_id: String,
    pub description: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct CreateApiKeyResponse {
    pub api_key: String,
    pub key_info: ApiKeyInfo,
}

#[derive(Serialize)]
pub struct ApiKeyInfo {
    pub key_id: String,
    pub node_id: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
}

impl From<ApiKey> for ApiKeyInfo {
    fn from(key: ApiKey) -> Self {
        Self {
            key_id: key.key_id,
            node_id: key.node_id,
            description: key.description,
            is_active: key.is_active,
            created_at: key.created_at,
            expires_at: key.expires_at,
            last_used_at: key.last_used_at,
            revoked_at: key.revoked_at,
            revoked_reason: key.revoked_reason,
        }
    }
}

#[derive(Deserialize)]
pub struct RevokeApiKeyRequest {
    pub reason: Option<String>,
}

/// Create a new API key for an MPC node
pub async fn create_api_key(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, StatusCode> {
    let auth = validate_admin_request(&state, &headers, "create_api_key", &state.admin_state).await?;
    require_role(&auth, AdminRole::Operator)?;

    let pool = state.db_pool.as_ref().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let (api_key, key_record) = api_keys::create_api_key(
        pool,
        &req.node_id,
        req.description.as_deref(),
        req.expires_at,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(CreateApiKeyResponse {
        api_key,
        key_info: key_record.into(),
    }))
}

/// List API keys for a node
pub async fn list_api_keys(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(node_id): Path<String>,
) -> Result<Json<Vec<ApiKeyInfo>>, StatusCode> {
    let auth = validate_admin_request(&state, &headers, "list_api_keys", &state.admin_state).await?;
    require_role(&auth, AdminRole::ReadOnly)?;

    let pool = state.db_pool.as_ref().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let keys = api_keys::list_api_keys(pool, &node_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let key_infos = keys.into_iter().map(ApiKeyInfo::from).collect();
    Ok(Json(key_infos))
}

/// Revoke an API key
pub async fn revoke_api_key(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(key_id): Path<String>,
    Json(req): Json<RevokeApiKeyRequest>,
) -> Result<StatusCode, StatusCode> {
    let auth = validate_admin_request(&state, &headers, "revoke_api_key", &state.admin_state).await?;
    require_role(&auth, AdminRole::Operator)?;

    let pool = state.db_pool.as_ref().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let success = api_keys::revoke_api_key(
        pool,
        &key_id,
        req.reason.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if success {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}
