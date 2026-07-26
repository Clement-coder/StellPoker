//! Idempotency key support for mutation endpoints (Issue #106).
//!
//! Clients may send an `Idempotency-Key` header on POST/PUT/PATCH/DELETE
//! requests. The first request seen for a given key + route is processed
//! normally and its response is cached; any retry with the same key (e.g.
//! after a dropped connection or client-side timeout) replays the cached
//! response instead of re-executing the mutation, preventing
//! double-submission (double table joins, duplicate deal/reveal/showdown
//! requests, etc).
//!
//! The store is in-memory with a bounded TTL, matching this coordinator's
//! other per-instance caches ([`crate::session_gc`], [`crate::stats`]).
//! Mutation endpoints only run on the elected leader instance (see
//! [`crate::leader_election`]), so at any moment there is a single writer
//! and a per-instance cache is sufficient.

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const DEFAULT_TTL_SECS: u64 = 86_400; // 24h
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024; // 10MB, generous for JSON API responses
const GC_INTERVAL_SECS: u64 = 300;
const HEADER_KEY: &str = "idempotency-key";

#[derive(Clone)]
struct CachedResponse {
    status: u16,
    content_type: Option<String>,
    body: Vec<u8>,
    inserted_at: Instant,
    ttl: Duration,
}

impl CachedResponse {
    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() >= self.ttl
    }
}

pub type IdempotencyStore = Arc<RwLock<HashMap<String, CachedResponse>>>;

pub fn new_store() -> IdempotencyStore {
    Arc::new(RwLock::new(HashMap::new()))
}

fn configured_ttl() -> Duration {
    let secs = std::env::var("IDEMPOTENCY_TTL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TTL_SECS);
    Duration::from_secs(secs)
}

/// Periodically evict expired idempotency records so the map doesn't grow
/// unbounded on a long-running instance.
pub fn spawn_gc_task(store: IdempotencyStore) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(GC_INTERVAL_SECS);
        loop {
            tokio::time::sleep(interval).await;
            let mut map = store.write().await;
            let before = map.len();
            map.retain(|_, v| !v.is_expired());
            let evicted = before - map.len();
            if evicted > 0 {
                tracing::debug!(evicted, remaining = map.len(), "idempotency cache swept");
            }
        }
    });
}

/// Axum middleware: replays the cached response for a repeated
/// `Idempotency-Key` on the same route + method, otherwise runs the request
/// and caches the outcome. Server errors (5xx) are not cached so the client
/// can safely retry with the same key once the transient failure clears.
pub async fn idempotency_middleware(
    State(store): State<IdempotencyStore>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    if !matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return next.run(req).await;
    }

    let key = req
        .headers()
        .get(HEADER_KEY)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());

    let Some(key) = key else {
        return next.run(req).await;
    };

    let scope_key = format!("{} {} {}", method, req.uri().path(), key);

    if let Some(cached) = {
        let map = store.read().await;
        map.get(&scope_key)
            .filter(|c| !c.is_expired())
            .cloned()
    } {
        let status =
            StatusCode::from_u16(cached.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut builder = Response::builder()
            .status(status)
            .header("Idempotent-Replayed", "true");
        if let Some(ct) = &cached.content_type {
            builder = builder.header("content-type", ct.as_str());
        }
        return builder
            .body(Body::from(cached.body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    let response = next.run(req).await;
    let status = response.status();
    let (parts, body) = response.into_parts();

    let bytes = match to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "idempotency: failed to buffer response body, skipping cache");
            return Response::from_parts(parts, Body::empty());
        }
    };

    if status.is_success() || status.is_client_error() {
        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let mut map = store.write().await;
        map.insert(
            scope_key,
            CachedResponse {
                status: status.as_u16(),
                content_type,
                body: bytes.to_vec(),
                inserted_at: Instant::now(),
                ttl: configured_ttl(),
            },
        );
    }

    Response::from_parts(parts, Body::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::Router;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    fn test_app(store: IdempotencyStore, counter: Arc<AtomicUsize>) -> Router {
        Router::new()
            .route(
                "/thing",
                post(move || {
                    let counter = counter.clone();
                    async move {
                        let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                        axum::Json(serde_json::json!({ "call_count": n }))
                    }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                store,
                idempotency_middleware,
            ))
    }

    #[tokio::test]
    async fn replays_cached_response_for_repeated_key() {
        let store = new_store();
        let counter = Arc::new(AtomicUsize::new(0));
        let app = test_app(store, counter.clone());

        let build_req = || {
            Request::builder()
                .method("POST")
                .uri("/thing")
                .header("Idempotency-Key", "abc-123")
                .body(Body::empty())
                .unwrap()
        };

        let res1 = app.clone().oneshot(build_req()).await.unwrap();
        assert_eq!(res1.status(), StatusCode::OK);
        let body1 = to_bytes(res1.into_body(), 1024).await.unwrap();

        let res2 = app.clone().oneshot(build_req()).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        assert_eq!(res2.headers().get("Idempotent-Replayed").unwrap(), "true");
        let body2 = to_bytes(res2.into_body(), 1024).await.unwrap();

        assert_eq!(body1, body2);
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "handler must run only once for a repeated idempotency key"
        );
    }

    #[tokio::test]
    async fn different_keys_both_execute_the_handler() {
        let store = new_store();
        let counter = Arc::new(AtomicUsize::new(0));
        let app = test_app(store, counter.clone());

        for key in ["key-a", "key-b"] {
            let req = Request::builder()
                .method("POST")
                .uri("/thing")
                .header("Idempotency-Key", key)
                .body(Body::empty())
                .unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn requests_without_a_key_are_never_cached() {
        let store = new_store();
        let counter = Arc::new(AtomicUsize::new(0));
        let app = test_app(store, counter.clone());

        for _ in 0..2 {
            let req = Request::builder()
                .method("POST")
                .uri("/thing")
                .body(Body::empty())
                .unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
