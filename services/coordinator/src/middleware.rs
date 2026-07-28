//! Request logging + OpenTelemetry tracing middleware for the coordinator.
//!
//! Each HTTP request gets:
//! - A UUID v4 request ID for log correlation
//! - A `tracing` span named `http.request` carrying method, path, status,
//!   and duration — forwarded to the OTel exporter when enabled
//! - Incoming `traceparent` / `tracestate` W3C headers are read so the span
//!   is correctly parented to a frontend-originated trace
//! - Structured log output (method, path, status, duration_ms, session_id)
//!
//! Request bodies are never logged (may contain sensitive game state).

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::Instrument;
use uuid::Uuid;

/// Extract a session/table ID from a path like `/api/table/42/...` or
/// `/api/session/some-uuid/...`.
fn extract_session_id(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if let Some(idx) = segments.iter().position(|&s| s == "table") {
        if let Some(id) = segments.get(idx + 1) {
            return Some(id.to_string());
        }
    }
    if let Some(idx) = segments.iter().position(|&s| s == "session") {
        if let Some(id) = segments.get(idx + 1) {
            return Some(id.to_string());
        }
    }
    None
}

pub async fn log_request(request: Request<Body>, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let session_id = extract_session_id(&path);

    // Read incoming traceparent so the span is correctly parented.
    let traceparent = crate::telemetry::extract_traceparent(request.headers())
        .unwrap_or_else(|| "-".to_string());

    let span = tracing::info_span!(
        "http.request",
        request_id  = %request_id,
        http.method = %method,
        http.target = %path,
        traceparent = %traceparent,
        session_id  = session_id.as_deref().unwrap_or("-"),
        // These are filled in after the response is available:
        http.status_code = tracing::field::Empty,
        duration_ms      = tracing::field::Empty,
    );

    let start = Instant::now();
    let response = next.run(request).instrument(span.clone()).await;
    let duration_ms = start.elapsed().as_millis();
    let status = response.status();

    span.record("http.status_code", status.as_u16());
    span.record("duration_ms", duration_ms as u64);

    let _enter = span.enter();
    match status {
        s if s.is_server_error() => {
            tracing::error!(
                request_id  = %request_id,
                method      = %method,
                path        = %path,
                status      = status.as_u16(),
                duration_ms = duration_ms,
                session_id  = session_id.as_deref().unwrap_or("-"),
                "request completed"
            );
        }
        s if s == StatusCode::BAD_REQUEST
            || s == StatusCode::UNAUTHORIZED
            || s == StatusCode::FORBIDDEN
            || s == StatusCode::NOT_FOUND
            || s == StatusCode::TOO_MANY_REQUESTS =>
        {
            tracing::warn!(
                request_id  = %request_id,
                method      = %method,
                path        = %path,
                status      = status.as_u16(),
                duration_ms = duration_ms,
                session_id  = session_id.as_deref().unwrap_or("-"),
                "request completed"
            );
        }
        _ => {
            tracing::info!(
                request_id  = %request_id,
                method      = %method,
                path        = %path,
                status      = status.as_u16(),
                duration_ms = duration_ms,
                session_id  = session_id.as_deref().unwrap_or("-"),
                "request completed"
            );
        }
    }

    response
}
