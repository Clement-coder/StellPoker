//! API versioning middleware (Issue #107).
//!
//! Scheme: URL-prefix versioning. `/api/v1/...` is the canonical, versioned
//! form of every existing `/api/...` route. Rather than duplicating the
//! entire route table, this middleware rewrites `/api/v1/<rest>` to
//! `/api/<rest>` in place *before* the request reaches the router, so a
//! single set of handlers serves both forms.
//!
//! Legacy unversioned `/api/...` requests keep working (treated as `v1`) but
//! are flagged as deprecated via response headers, per the deprecation
//! policy documented in `docs/API_VERSIONING.md`.
//!
//! See `docs/API_VERSIONING.md` for the full versioning scheme, deprecation
//! policy, and the coordinator/contracts/circuits compatibility matrix.

use axum::{
    body::Body,
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

pub const CURRENT_API_VERSION: &str = "v1";
const VERSION_PREFIX: &str = "/api/v1";

/// Rewrites `/api/v1/<rest>` to `/api/<rest>` before routing, and tags every
/// `/api/...` response with `X-API-Version`. Unversioned `/api/...` requests
/// are additionally marked `Deprecation: true` with a `Link` pointing at the
/// versioned successor path.
pub async fn rewrite_and_tag_version(mut req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path();
    let mut is_api_request = path.starts_with("/api/");
    let mut is_deprecated_unversioned = is_api_request;

    if let Some(rest) = path.strip_prefix(VERSION_PREFIX) {
        if rest.is_empty() || rest.starts_with('/') {
            is_api_request = true;
            is_deprecated_unversioned = false;
            let new_path = format!("/api{}", rest);
            let rewritten = match req.uri().query() {
                Some(q) => format!("{}?{}", new_path, q),
                None => new_path,
            };
            if let Ok(new_uri) = rewritten.parse() {
                *req.uri_mut() = new_uri;
            }
        }
    }

    let mut response = next.run(req).await;
    if is_api_request {
        let headers = response.headers_mut();
        headers.insert(
            "X-API-Version",
            HeaderValue::from_static(CURRENT_API_VERSION),
        );
        if is_deprecated_unversioned {
            headers.insert("Deprecation", HeaderValue::from_static("true"));
            headers.insert(
                "Link",
                HeaderValue::from_static("</api/v1>; rel=\"successor-version\""),
            );
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_app() -> Router {
        Router::new()
            .route("/api/health", get(|| async { "ok" }))
            .route("/other", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(rewrite_and_tag_version))
    }

    #[tokio::test]
    async fn versioned_prefix_is_rewritten_and_served() {
        let req = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        let res = test_app().oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
        assert_eq!(
            res.headers().get("X-API-Version").unwrap(),
            CURRENT_API_VERSION
        );
        assert!(res.headers().get("Deprecation").is_none());
    }

    #[tokio::test]
    async fn unversioned_legacy_path_still_works_but_is_flagged_deprecated() {
        let req = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let res = test_app().oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
        assert_eq!(res.headers().get("Deprecation").unwrap(), "true");
        assert_eq!(
            res.headers().get("X-API-Version").unwrap(),
            CURRENT_API_VERSION
        );
    }

    #[tokio::test]
    async fn non_api_paths_are_untouched() {
        let req = Request::builder()
            .uri("/other")
            .body(Body::empty())
            .unwrap();
        let res = test_app().oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
        assert!(res.headers().get("X-API-Version").is_none());
    }
}
