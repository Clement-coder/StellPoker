//! Authentication middleware for MPC node requests.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, Method},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::{api_keys, AppState};

/// Middleware to authenticate MPC node requests using API keys
pub async fn authenticate_mpc_request(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = request.headers();
    let method = request.method();
    let path = request.uri().path();

    // Only authenticate MPC node endpoints (internal API calls)
    if !is_mpc_endpoint(path) {
        return Ok(next.run(request).await);
    }

    // Skip authentication for health checks
    if method == Method::GET && path.ends_with("/health") {
        return Ok(next.run(request).await);
    }

    let ip_address = extract_ip_address(headers);
    
    let pool = match state.db_pool.as_ref() {
        Some(pool) => pool,
        None => return Ok(next.run(request).await), // No database, skip auth for dev mode
    };
    
    match api_keys::authenticate_mpc_node(pool, headers, path, ip_address.as_deref()).await {
        Ok(node_id) => {
            tracing::debug!("Authenticated MPC node: {} for endpoint: {}", node_id, path);
            Ok(next.run(request).await)
        }
        Err(status) => {
            tracing::warn!(
                "MPC authentication failed for endpoint: {}, IP: {:?}, Status: {}",
                path,
                ip_address,
                status.as_u16()
            );

            // Log failed authentication attempt
            if let Some(api_key) = extract_api_key(headers) {
                let key_id = format!("failed_{}", &api_key[..std::cmp::min(8, api_key.len())]);
                let _ = api_keys::log_api_key_usage(
                    pool,
                    &key_id,
                    "unknown",
                    path,
                    ip_address.as_deref(),
                    false,
                ).await;
            }

            Err(status)
        }
    }
}

/// Check if the path is an MPC node endpoint that requires authentication
fn is_mpc_endpoint(path: &str) -> bool {
    // These are internal endpoints used by MPC nodes
    path.contains("/mpc/") || 
    path.contains("/prepare/") ||
    path.contains("/prove/") ||
    path.contains("/shares/") ||
    path.starts_with("/internal/")
}

/// Extract IP address from request headers
fn extract_ip_address(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
}

/// Extract API key from Authorization header
fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}
