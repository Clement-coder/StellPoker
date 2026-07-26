mod crypto;

//! Stellar Poker MPC Node
//!
//! Each node is a participant in the REP3 MPC protocol via TACEO's co-noir.
//! It holds secret shares and participates in collaborative proof generation.
//!
//! Lifecycle:
//! 1. Coordinator asks each node to prepare its own share bundle (/table/:id/prepare-*)
//! 2. Coordinator asks each node to dispatch its bundle to peers (/session/:id/shares)
//! 3. Coordinator triggers proof gen via POST /session/:id/generate
//! 4. Node merges all source fragments, then runs co-noir witness/proof subprocesses
//! 5. Coordinator polls GET /session/:id/status and retrieves proof via GET /session/:id/proof
//!
//! co-noir handles peer-to-peer MPC communication internally via TCP (ports 10000-10002).
//!
//! ## TLS and coordinator certificate pinning
//!
//! When TLS environment variables are set (`TLS_SERVER_CERT_PATH` / `TLS_SERVER_KEY_PATH`
//! or the `_B64` variants), the node serves HTTPS instead of plain HTTP.
//!
//! Optionally, the coordinator's identity can be pinned via:
//! - `COORDINATOR_TLS_PIN_PUBKEY_HASH` – SHA-256 hex of the coordinator's SPKI
//! - `COORDINATOR_TLS_PIN_CERT_PATH` / `COORDINATOR_TLS_PIN_CERT_B64` – full cert pin
//!
//! When a pin is set, the node demands mutual TLS (mTLS) and rejects any
//! connection whose client certificate does not match the pin.

use axum::{
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

mod api;
mod limits;
mod private_table;
mod session;
mod tls;
mod heartbeat;

use limits::ResourceLimits;
use private_table::PrivateTableState;
use session::MpcSessionState;

#[derive(Clone)]
pub struct NodeState {
    pub node_id: u32,
    pub sessions: Arc<RwLock<HashMap<String, Arc<RwLock<MpcSessionState>>>>>,
    pub tables: Arc<RwLock<HashMap<u32, PrivateTableState>>>,
    pub party_config_path: String,
    pub peer_http_endpoints: Vec<String>,
    /// Per-node resource ceilings guarding against exhaustion / session flooding.
    pub limits: ResourceLimits,
}

#[tokio::main]
async fn main() {
    let log_format = std::env::var("REQUEST_LOG_FORMAT").unwrap_or_default();
    if log_format.eq_ignore_ascii_case("json") {
        tracing_subscriber::fmt().json().init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let node_id: u32 = std::env::var("NODE_ID")
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .unwrap();
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| format!("{}", 8101 + node_id))
        .parse()
        .unwrap();
    let party_config_path = std::env::var("PARTY_CONFIG")
        .unwrap_or_else(|_| format!("./config/party_{}.toml", node_id));
    let peer_http_endpoints = std::env::var("NODE_HTTP_ENDPOINTS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            vec![
                "http://localhost:8101".to_string(),
                "http://localhost:8102".to_string(),
                "http://localhost:8103".to_string(),
            ]
        });

    let limits = ResourceLimits::from_env();

    tracing::info!("MPC Node {} starting on port {}", node_id, port);
    tracing::info!("Party config: {}", party_config_path);
    tracing::info!("Peer HTTP endpoints: {:?}", peer_http_endpoints);
    tracing::info!(
        "Resource limits: max_concurrent_sessions={}, max_session_memory_bytes={}, max_session_cpu_seconds={}, max_session_wall_seconds={}",
        limits.max_concurrent_sessions,
        limits.max_session_memory_bytes,
        limits.max_session_cpu_seconds,
        limits.max_session_wall_seconds,
    );

    // ── TLS configuration ────────────────────────────────────────────────────
    let tls_cfg = match tls::load_from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("TLS configuration error: {}", e);
            std::process::exit(1);
        }
    };

    let state = NodeState {
        node_id,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        tables: Arc::new(RwLock::new(HashMap::new())),
        party_config_path,
        peer_http_endpoints,
        limits,
    };

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/table/:table_id/prepare-deal",
            post(api::post_prepare_deal),
        )
        .route(
            "/table/:table_id/prepare-reveal/:phase",
            post(api::post_prepare_reveal),
        )
        .route(
            "/table/:table_id/prepare-showdown",
            post(api::post_prepare_showdown),
        )
        .route(
            "/table/:table_id/dispatch-shares",
            post(api::post_dispatch_shares),
        )
        .route("/table/:table_id/perm-lookup", post(api::post_perm_lookup))
        .route("/session/:id/shares", post(api::post_shares))
        .route("/session/:id/generate", post(api::post_generate))
        .route("/session/:id/status", get(api::get_status))
        .route("/session/:id/proof", get(api::get_proof))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);

    match tls_cfg {
        // ── Plain HTTP (no TLS env vars set) ─────────────────────────────────
        None => {
            tracing::info!("Listening on {} (plain HTTP)", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        }

        // ── TLS / mTLS ────────────────────────────────────────────────────────
        Some(cfg) => {
            let pin_mode = if cfg.pinned_spki_hash.is_some() {
                "mTLS with SPKI hash pin"
            } else if cfg.pinned_cert_der.is_some() {
                "mTLS with full cert pin"
            } else {
                "TLS (no coordinator pin)"
            };
            tracing::info!("Listening on {} ({}) ", addr, pin_mode);

            let server_config = match tls::build_server_config(cfg) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to build TLS server config: {}", e);
                    std::process::exit(1);
                }
            };
            let acceptor = tokio_rustls::TlsAcceptor::from(server_config);

            let tcp_listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            serve_tls(tcp_listener, acceptor, app).await;
        }
    }
}

/// Accept TLS connections in a loop and hand them to hyper-util for HTTP serving.
///
/// Each accepted TLS stream is served in its own `tokio::spawn`'d task so that
/// a slow or stalled client does not block other connections.
async fn serve_tls(
    listener: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
    app: Router,
) {
    use hyper_util::{
        rt::{TokioExecutor, TokioIo},
        server::conn::auto::Builder,
    };
    use tower::Service as _;

    loop {
        let (tcp_stream, remote_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!("TCP accept error: {}", e);
                continue;
            }
        };

        let acceptor = acceptor.clone();
        // Clone the Axum Router (it is cheap — backed by Arc).
        let mut svc = app.clone();

        tokio::spawn(async move {
            let tls_stream = match acceptor.accept(tcp_stream).await {
                Ok(s) => s,
                Err(e) => {
                    // TLS handshake failures are expected when misconfigured
                    // clients connect; log at debug level to avoid noise.
                    tracing::debug!(
                        "TLS handshake failed from {}: {}",
                        remote_addr,
                        e
                    );
                    return;
                }
            };
            tracing::debug!("TLS connection accepted from {}", remote_addr);
            let io = TokioIo::new(tls_stream);

            // Poll the service so it is ready before handing it to hyper.
            if let Err(e) = std::future::poll_fn(|cx| svc.poll_ready(cx)).await {
                tracing::warn!(
                    "Axum service not ready for {}: {}",
                    remote_addr,
                    e
                );
                return;
            }

            if let Err(e) = Builder::new(TokioExecutor::new())
                .serve_connection(io, svc)
                .await
            {
                tracing::debug!(
                    "HTTP/TLS connection error from {}: {}",
                    remote_addr,
                    e
                );
            }
        });
    }
}
