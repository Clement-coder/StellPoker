//! Stellar Poker MPC Coordinator Service
//!
//! This service orchestrates the MPC committee for:
//! 1. Distributed share preparation across all MPC nodes (coNoir split-input)
//! 2. Proof generation (deal, reveal, showdown proofs via coNoir)
//! 3. Submitting proofs to Soroban
//!
//! Architecture:
//! - The coordinator receives requests from the web app
//! - It orchestrates 3 MPC nodes running coNoir
//! - Each node prepares only its own private witness contribution
//! - Coordinator never sees plaintext deck/salts/hole cards
//! - Proofs are generated collaboratively and are identical to standard
//!   Barretenberg/UltraHonk proofs

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware,
    middleware::Next,
    response::Response,
    routing::{delete, get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use prometheus::{
    Encoder, Gauge, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use sysinfo::{get_current_pid, ProcessesToUpdate, System};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::CorsLayer;

mod api;
mod archiver;
mod audit_log;
mod cors_db;
mod db;
mod discovery;
mod feature_flags;
mod hot_reload;
mod mpc;
mod mpc_auth_middleware;
mod plugin;
mod rate_limit_db;
#[path = "middleware.rs"]
mod request_log;
mod session_gc;
mod session_migration;
mod soroban;
mod stats;

use api::admin::{AdminConfig, AdminState};

#[derive(Serialize, Clone, Debug)]
pub struct LatencyHistogram {
    pub under_50ms: u64,
    pub under_250ms: u64,
    pub under_1000ms: u64,
    pub under_5000ms: u64,
    pub over_5000ms: u64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self {
            under_50ms: 0,
            under_250ms: 0,
            under_1000ms: 0,
            under_5000ms: 0,
            over_5000ms: 0,
        }
    }
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct RouteMetric {
    pub count: u64,
    pub errors: u64,
    pub latency_histogram: LatencyHistogram,
}

#[derive(Serialize, Clone, Debug)]
pub struct MpcNodeHealth {
    pub endpoint: String,
    pub connected: bool,
    pub last_heartbeat: Option<SystemTime>,
}

#[derive(Clone)]
pub struct PrometheusMetrics {
    pub registry: Arc<Registry>,
    pub request_counter: IntCounterVec,
    pub request_errors: IntCounterVec,
    pub request_latency: HistogramVec,
    pub process_cpu_percent: Gauge,
    pub process_memory_bytes: Gauge,
}

#[derive(Clone)]
pub struct MetricsState {
    pub boot_time: Instant,
    pub active_mpc_sessions: Arc<AtomicUsize>,
    pub route_metrics: Arc<Mutex<HashMap<String, RouteMetric>>>,
    pub node_healths: Arc<Mutex<Vec<MpcNodeHealth>>>,
    pub prometheus: PrometheusMetrics,
}

#[derive(Clone)]
struct AppState {
    tables: Arc<RwLock<HashMap<u32, TableSession>>>,
    lobby_assignments: Arc<RwLock<HashMap<u32, HashMap<String, String>>>>,
    mpc_config: MpcConfig,
    soroban_config: soroban::SorobanConfig,
    auth_state: Arc<RwLock<AuthState>>,
    admin_config: Arc<RwLock<api::admin::AdminConfig>>,
    admin_state: api::admin::AdminState,
    rate_limit_state: Arc<RwLock<RateLimitState>>,
    metrics: MetricsState,
    chat_channels: Arc<Mutex<HashMap<u32, tokio::sync::broadcast::Sender<String>>>>,
    mpc_sessions: session_gc::SessionStore,
    stats: stats::StatsStore,
    feature_flags: feature_flags::FeatureFlagStore,
    db_pool: Option<Arc<sqlx::PgPool>>,
    instance_id: String,
    pub plugin_loader: Arc<tokio::sync::RwLock<plugin::PluginLoader>>,
    pub archive_store: archiver::ArchiveStore,
    pub archive_config: archiver::ArchiveConfig,
}

#[derive(Clone)]
#[allow(dead_code)]
struct MpcConfig {
    /// Endpoints of the 3 MPC nodes
    node_endpoints: Vec<String>,
    /// Whether `node_endpoints` came from explicitly-set `MPC_NODE_*` env vars
    /// (vs. built-in localhost defaults). When true, dynamic node discovery is
    /// disabled and these static endpoints are used (backward compatibility).
    static_endpoints_configured: bool,
    /// Path to compiled Noir circuits (ACIR)
    circuit_dir: String,
    /// Soroban RPC endpoint
    soroban_rpc: String,
    /// Committee signing key (for submitting txns)
    committee_secret: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct TableSession {
    table_id: u32,
    /// Deck Merkle root (public, posted on-chain)
    deck_root: String,
    /// Per-player hand commitments in seat order.
    hand_commitments: Vec<String>,
    /// Players in deterministic seat order.
    player_order: Vec<String>,
    /// Cards already dealt/revealed (indices).
    dealt_indices: Vec<u32>,
    /// Per-player dealt card positions: (card1_deck_pos, card2_deck_pos).
    player_card_positions: Vec<(u32, u32)>,
    /// Revealed board indices.
    board_indices: Vec<u32>,
    /// Current game phase.
    phase: String,
    /// Last deal proof session ID.
    deal_session_id: String,
    /// Latest deal tx hash, if submitted.
    deal_tx_hash: Option<String>,
    /// Reveal tx hashes by phase.
    reveal_tx_hashes: HashMap<String, String>,
    /// Reveal proof session IDs by phase.
    reveal_session_ids: HashMap<String, String>,
    /// Revealed cards by phase.
    revealed_cards_by_phase: HashMap<String, Vec<u32>>,
    /// Selected MPC node endpoints for this table.
    selected_node_endpoints: Vec<String>,
    /// Latest showdown tx hash, if submitted.
    showdown_tx_hash: Option<String>,
    /// Last showdown proof session ID, if submitted.
    showdown_session_id: Option<String>,
    /// Cached showdown result for idempotent retries.
    showdown_result: Option<(String, u32)>,
    /// Monotonic nonce for unique proof session IDs.
    proof_nonce: u64,
}

#[derive(Clone, Debug, Default)]
struct AuthState {
    last_nonce_by_address: HashMap<String, u64>,
}

#[derive(Clone, Debug, Default)]
struct RateLimitState {
    requests_by_bucket: HashMap<String, Vec<u64>>,
}

#[tokio::main]
async fn main() {
    // Structured logging: REQUEST_LOG_FORMAT=json uses JSON output; default is human-readable.
    let log_format = std::env::var("REQUEST_LOG_FORMAT").unwrap_or_default();
    if log_format.eq_ignore_ascii_case("json") {
        tracing_subscriber::fmt().json().init();
    } else {
        tracing_subscriber::fmt().init();
    }

    // If any MPC_NODE_* env var is explicitly set, treat the endpoints as
    // statically configured and disable dynamic discovery (backward compat).
    let static_endpoints_configured = (0..3)
        .any(|i| std::env::var(format!("MPC_NODE_{}", i)).is_ok());
    let mpc_config = MpcConfig {
        node_endpoints: vec![
            std::env::var("MPC_NODE_0").unwrap_or_else(|_| "http://localhost:8101".to_string()),
            std::env::var("MPC_NODE_1").unwrap_or_else(|_| "http://localhost:8102".to_string()),
            std::env::var("MPC_NODE_2").unwrap_or_else(|_| "http://localhost:8103".to_string()),
        ],
        static_endpoints_configured,
        circuit_dir: std::env::var("CIRCUIT_DIR").unwrap_or_else(|_| "./circuits".to_string()),
        soroban_rpc: std::env::var("SOROBAN_RPC")
            .unwrap_or_else(|_| "http://localhost:8000/soroban/rpc".to_string()),
        committee_secret: std::env::var("COMMITTEE_SECRET")
            .unwrap_or_else(|_| "test_secret".to_string()),
    };

    let soroban_config = soroban::SorobanConfig::from_env();
    if soroban_config.is_configured() {
        tracing::info!(
            "Soroban configured: contract={}",
            soroban_config.poker_table_contract
        );
    } else {
        tracing::warn!("Soroban not configured — on-chain submission disabled");
    }

    let initial_node_healths = mpc_config
        .node_endpoints
        .iter()
        .map(|ep| MpcNodeHealth {
            endpoint: ep.clone(),
            connected: false,
            last_heartbeat: None,
        })
        .collect::<Vec<_>>();

    let prometheus_registry = Arc::new(Registry::new());
    let request_counter = IntCounterVec::new(
        Opts::new("coordinator_requests_total", "Total coordinator requests."),
        &["method", "route"],
    )
    .unwrap();
    let request_errors = IntCounterVec::new(
        Opts::new(
            "coordinator_request_errors_total",
            "Total coordinator request errors.",
        ),
        &["method", "route"],
    )
    .unwrap();
    let request_latency = HistogramVec::new(
        HistogramOpts::new(
            "coordinator_request_latency_seconds",
            "Request latency histogram in seconds.",
        )
        .buckets(vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]),
        &["method", "route"],
    )
    .unwrap();
    let process_cpu_percent = Gauge::with_opts(Opts::new(
        "coordinator_process_cpu_percent",
        "Coordinator process CPU usage percentage.",
    ))
    .unwrap();
    let process_memory_bytes = Gauge::with_opts(Opts::new(
        "coordinator_process_memory_bytes",
        "Coordinator process memory usage in bytes.",
    ))
    .unwrap();

    prometheus_registry
        .register(Box::new(request_counter.clone()))
        .unwrap();
    prometheus_registry
        .register(Box::new(request_errors.clone()))
        .unwrap();
    prometheus_registry
        .register(Box::new(request_latency.clone()))
        .unwrap();
    prometheus_registry
        .register(Box::new(process_cpu_percent.clone()))
        .unwrap();
    prometheus_registry
        .register(Box::new(process_memory_bytes.clone()))
        .unwrap();

    let metrics = MetricsState {
        boot_time: Instant::now(),
        active_mpc_sessions: Arc::new(AtomicUsize::new(0)),
        route_metrics: Arc::new(Mutex::new(HashMap::new())),
        node_healths: Arc::new(Mutex::new(initial_node_healths)),
        prometheus: PrometheusMetrics {
            registry: prometheus_registry,
            request_counter,
            request_errors,
            request_latency,
            process_cpu_percent: process_cpu_percent.clone(),
            process_memory_bytes: process_memory_bytes.clone(),
        },
    };

    let system_state = Arc::new(Mutex::new(System::new_all()));
    let mpc_sessions: session_gc::SessionStore =
        Arc::new(RwLock::new(std::collections::HashMap::new()));

    let metrics_clone = metrics.clone();
    let system_state_clone = Arc::clone(&system_state);
    tokio::spawn(async move {
        let pid = get_current_pid().unwrap();
        loop {
            {
                let mut system = system_state_clone.lock().await;
                system.refresh_processes(ProcessesToUpdate::All, true);
                if let Some(process) = system.process(pid) {
                    metrics_clone
                        .prometheus
                        .process_cpu_percent
                        .set(process.cpu_usage() as f64);
                    metrics_clone
                        .prometheus
                        .process_memory_bytes
                        .set(process.memory() as f64 * 1024.0);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    });

    session_gc::spawn_gc_task(Arc::clone(&mpc_sessions));

    let stats_store = stats::new_store();

    let admin_config = AdminConfig::from_env();
    let admin_state = AdminState::new();

    // Spawn the Horizon event indexer if Soroban is configured.
    if soroban_config.is_configured() && !soroban_config.poker_table_contract.is_empty() {
        let horizon_url = std::env::var("HORIZON_URL")
            .unwrap_or_else(|_| "https://horizon-testnet.stellar.org".to_string());
        let contract_id = soroban_config.poker_table_contract.clone();
        let poll_secs: u64 = std::env::var("STATS_POLL_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15);
        stats::spawn_indexer(
            Arc::clone(&stats_store),
            horizon_url,
            contract_id,
            std::time::Duration::from_secs(poll_secs),
        );
        tracing::info!("Stats indexer started (poll={}s)", poll_secs);
    }

    let feature_flag_store = feature_flags::FeatureFlagStore::from_env();
    tracing::info!("Feature flags initialised");

    // Connect to database if DATABASE_URL is provided
    let db_pool = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        match db::connect(&database_url).await {
            Ok(pool) => {
                tracing::info!("Connected to PostgreSQL database");

                // Run migrations
                if let Err(e) = db::run_migrations(&pool).await {
                    tracing::error!("Failed to run database migrations: {}", e);
                } else {
                    tracing::info!("Database migrations applied successfully");
                }

                Some(Arc::new(pool))
            }
            Err(e) => {
                tracing::error!("Failed to connect to database: {}", e);
                None
            }
        }
    } else {
        tracing::warn!("DATABASE_URL not set - running without database persistence");
        None
    };

    let instance_id = session_migration::generate_instance_id();
    tracing::info!("Coordinator instance ID: {}", instance_id);

    let mut wasm_config = wasmtime::Config::new();
    wasm_config
        .consume_fuel(true)
        .wasm_multi_value(true)
        .wasm_memory64(false);
    let plugin_engine =
        wasmtime::Engine::new(&wasm_config).expect("failed to create wasmtime engine");
    let plugin_loader = plugin::PluginLoader::new(plugin_engine);
    let plugin_loader = Arc::new(tokio::sync::RwLock::new(plugin_loader));
    {
        let loader = plugin_loader.read().await;
        let loaded = loader.scan_plugin_directory(None).await;
        if loaded.is_empty() {
            tracing::info!("No Wasm plugins found in ./plugins directory");
        } else {
            tracing::info!("Auto-loaded {} Wasm plugin(s): {:?}", loaded.len(), loaded);
        }
    }

    let hot_reload_snapshot = hot_reload::snapshot_path_from_env();
    let restored_snapshot = hot_reload_snapshot
        .as_ref()
        .and_then(|path| hot_reload::load_snapshot(path));
    let tables = Arc::new(RwLock::new(
        restored_snapshot
            .as_ref()
            .map(|snapshot| snapshot.tables.clone())
            .unwrap_or_default(),
    ));
    let lobby_assignments = Arc::new(RwLock::new(
        restored_snapshot
            .as_ref()
            .map(|snapshot| snapshot.lobby_assignments.clone())
            .unwrap_or_default(),
    ));
    if let Some(snapshot) = &restored_snapshot {
        tracing::info!(
            "Restored hot-reload snapshot with {} table session(s)",
            snapshot.tables.len()
        );
    }

    let archive_config = archiver::ArchiveConfig::from_env();
    let archive_store = archiver::new_store();
    archiver::load_existing_archives(&archive_store, &archive_config).await;

    let state = AppState {
        tables: Arc::clone(&tables),
        lobby_assignments: Arc::clone(&lobby_assignments),
        mpc_config,
        soroban_config,
        auth_state: Arc::new(RwLock::new(AuthState::default())),
        admin_config: Arc::new(RwLock::new(admin_config)),
        admin_state,
        rate_limit_state: Arc::new(RwLock::new(RateLimitState::default())),
        metrics: metrics.clone(),
        chat_channels: Arc::new(Mutex::new(HashMap::new())),
        mpc_sessions,
        stats: stats_store,
        feature_flags: feature_flag_store,
        db_pool,
        instance_id,
        plugin_loader,
        archive_store: archive_store.clone(),
        archive_config: archive_config.clone(),
    };

    if let Some(path) = hot_reload_snapshot {
        hot_reload::spawn_snapshot_task(path, tables, lobby_assignments);
    }

    archiver::spawn_archive_task(
        state.mpc_sessions.clone(),
        Arc::clone(&state.tables),
        archive_store.clone(),
        archive_config,
    );

    // Spawn background node health check task
    let node_healths = state.metrics.node_healths.clone();
    let soroban_config = state.soroban_config.clone();
    let default_endpoints = state.mpc_config.node_endpoints.clone();
    tokio::spawn(async move {
        loop {
            let endpoints = if soroban_config.committee_registry_contract.is_empty() {
                default_endpoints.clone()
            } else {
                match soroban::fetch_active_nodes_from_registry(&soroban_config).await {
                    Ok(members) => members.into_iter().map(|m| m.endpoint).collect(),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to fetch nodes from registry for health check: {}",
                            e
                        );
                        default_endpoints.clone()
                    }
                }
            };

            for endpoint in endpoints {
                let url = format!("{}/health", endpoint);
                let is_healthy = reqwest::get(&url)
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false);

                let mut guard = node_healths.lock().await;
                if let Some(node) = guard.iter_mut().find(|n| n.endpoint == endpoint) {
                    let prev_connected = node.connected;
                    if is_healthy {
                        node.connected = true;
                        node.last_heartbeat = Some(SystemTime::now());
                    } else {
                        if prev_connected {
                            tracing::warn!("MPC Node ({}) went offline", endpoint);
                        }
                        node.connected = false;
                    }
                } else {
                    // New node discovered
                    guard.push(MpcNodeHealth {
                        endpoint,
                        connected: is_healthy,
                        last_heartbeat: if is_healthy {
                            Some(SystemTime::now())
                        } else {
                            None
                        },
                    });
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
    });

    let app = Router::new()
        .route("/metrics", get(metrics_endpoint))
        .route("/api/health", get(health))
        .route("/api/stats", get(get_stats))
        // Dynamic MPC node discovery (active only when neither the committee
        // registry nor static MPC_NODE_* endpoints are configured).
        .route("/api/node/register", post(api::register_node))
        .route("/api/node/:id/heartbeat", post(api::node_heartbeat))
        .route("/api/node/:id", delete(api::deregister_node))
        .route("/api/flags", get(api::flags::list_flags))
        .route("/api/flags/:key", post(api::flags::set_flag))
        // Plugin management endpoints
        .route("/api/plugins", get(api::plugins::list_plugins))
        .route("/api/plugins/health", get(api::plugins::plugin_health))
        .route("/api/plugins/load", post(api::plugins::load_plugin))
        .route("/api/plugins/rescan", post(api::plugins::rescan_plugins))
        .route("/api/plugins/:name", get(api::plugins::get_plugin))
        .route(
            "/api/plugins/:name/unload",
            post(api::plugins::unload_plugin),
        )
        .route(
            "/api/plugins/:name/call",
            post(api::plugins::call_plugin_function),
        )
        .route("/api/tables/create", post(api::create_table))
        .route("/api/tables/open", get(api::list_open_tables))
        .route("/api/chain-config", get(api::get_chain_config))
        .route("/api/table/:table_id/join", post(api::join_table))
        .route("/api/table/:table_id/lobby", get(api::get_table_lobby))
        .route("/api/table/:table_id/request-deal", post(api::request_deal))
        .route(
            "/api/table/:table_id/request-reveal/:phase",
            post(api::request_reveal),
        )
        .route(
            "/api/table/:table_id/request-showdown",
            post(api::request_showdown),
        )
        .route(
            "/api/table/:table_id/player-action",
            post(api::player_action),
        )
        .route(
            "/api/table/:table_id/player/:address/cards",
            get(api::get_player_cards),
        )
        .route("/api/table/:table_id/state", get(api::get_table_state))
        .route("/api/committee/status", get(api::committee_status))
        .route("/api/table/:table_id/chat/ws", get(chat_ws_handler))
        .route(
            "/api/session/:session_id/cancel",
            post(api::cancel_mpc_session),
        )
        .route(
            "/api/session/:session_id/status",
            get(api::get_mpc_session_status),
        )
        // Admin endpoints (RBAC-protected)
        .route("/api/admin/health", get(api::admin_health))
        .route("/api/admin/sessions", get(api::admin_list_sessions))
        .route(
            "/api/admin/sessions/:session_id/cancel",
            post(api::admin_cancel_session),
        )
        .route(
            "/api/admin/sessions/cleanup",
            post(api::admin_cleanup_sessions),
        )
        .route("/api/admin/stats", get(api::admin_stats))
        .route("/api/admin/config/reload", post(api::admin_reload_config))
        // New admin endpoints for issues #267, #261, #264, #265
        .route("/api/admin/rate-limits", get(api::admin_list_rate_limits))
        .route("/api/admin/rate-limits", post(api::admin_upsert_rate_limit))
        .route(
            "/api/admin/rate-limits/:id",
            axum::routing::delete(api::admin_delete_rate_limit),
        )
        .route("/api/admin/cors", get(api::admin_list_cors))
        .route("/api/admin/cors", post(api::admin_upsert_cors))
        .route(
            "/api/admin/cors/:id",
            axum::routing::delete(api::admin_delete_cors),
        )
        .route("/api/admin/audit-logs", get(api::admin_query_audit_logs))
        .route(
            "/api/admin/audit-logs/verify",
            post(api::admin_verify_audit_chain),
        )
        .route("/api/admin/migrations", get(api::admin_list_migrations))
        .route(
            "/api/admin/migrations/initiate",
            post(api::admin_initiate_migration),
        )
        .route(
            "/api/admin/migrations/:id/complete",
            post(api::admin_complete_migration),
        )
        .route(
            "/api/admin/migrations/:id/cancel",
            post(api::admin_cancel_migration),
        )
        // Session archiving endpoints (Issue #259)
        .route(
            "/api/admin/archives",
            get(api::admin_list_archives),
        )
        .route(
            "/api/admin/archives/:archive_id",
            get(api::admin_get_archive),
        )
        .route(
            "/api/admin/archives/purge",
            post(api::admin_purge_archives),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            metrics_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            mpc_auth_middleware::authenticate_mpc_request,
        ))
        .layer(middleware::from_fn(request_log::log_request))
        .layer(build_cors_layer(state.db_pool.as_deref()).await)
        .with_state(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    tracing::info!("Coordinator listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn build_cors_layer(db_pool: Option<&sqlx::PgPool>) -> CorsLayer {
    let origins = cors_db::get_effective_cors_origins(db_pool).await;

    if origins.contains(&"*".to_string()) {
        tracing::warn!("CORS configured in permissive mode - all origins allowed");
        return CorsLayer::permissive();
    }

    tracing::info!("CORS configured with {} allowed origin(s)", origins.len());
    for origin in &origins {
        tracing::debug!("  CORS allowed origin: {}", origin);
    }

    use axum::http::Method;
    use tower_http::cors::AllowOrigin;

    let allow_origins: Vec<axum::http::HeaderValue> =
        origins.iter().filter_map(|o| o.parse().ok()).collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allow_origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any)
        .allow_credentials(true)
}

#[derive(Serialize)]
struct HealthResponse {
    uptime_seconds: u64,
    mpc_nodes: Vec<MpcNodeHealth>,
    soroban_rpc: SorobanHealth,
    active_mpc_sessions: usize,
    request_metrics: HashMap<String, RouteMetric>,
}

#[derive(Serialize)]
struct SorobanHealth {
    endpoint: String,
    status: String,
}

async fn check_soroban_connectivity(rpc_url: &str) -> bool {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestLedger"
    });

    let resp = client.post(rpc_url).json(&body).send().await;
    match resp {
        Ok(r) => r.status().is_success() || r.status() == 200,
        Err(_) => false,
    }
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime_seconds = state.metrics.boot_time.elapsed().as_secs();
    let mpc_nodes = state.metrics.node_healths.lock().await.clone();

    // Check Soroban RPC connectivity
    let soroban_status = if check_soroban_connectivity(&state.soroban_config.rpc_url).await {
        "connected".to_string()
    } else {
        tracing::warn!(
            "Soroban RPC connectivity check failed for {}",
            state.soroban_config.rpc_url
        );
        "disconnected".to_string()
    };

    // Log health check failures at WARN level
    for node in &mpc_nodes {
        if !node.connected {
            tracing::warn!(
                "Health check warning: MPC Node {} is disconnected",
                node.endpoint
            );
        }
    }
    if soroban_status == "disconnected" {
        tracing::warn!("Health check warning: Soroban RPC is disconnected");
    }

    let active_mpc_sessions = state.metrics.active_mpc_sessions.load(Ordering::SeqCst);
    let request_metrics = state.metrics.route_metrics.lock().await.clone();

    Json(HealthResponse {
        uptime_seconds,
        mpc_nodes,
        soroban_rpc: SorobanHealth {
            endpoint: state.soroban_config.rpc_url.clone(),
            status: soroban_status,
        },
        active_mpc_sessions,
        request_metrics,
    })
}

async fn metrics_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let method = req.method().to_string();
    let route = format!("{} {}", method, path);

    if path == "/api/health" || path.ends_with("/chat/ws") {
        return next.run(req).await;
    }

    let start = Instant::now();
    let response = next.run(req).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let status = response.status();
    let is_error = status.is_server_error() || status.is_client_error();

    let labels = [method.as_str(), route.as_str()];
    state
        .metrics
        .prometheus
        .request_counter
        .with_label_values(&labels)
        .inc();
    if is_error {
        state
            .metrics
            .prometheus
            .request_errors
            .with_label_values(&labels)
            .inc();
    }
    state
        .metrics
        .prometheus
        .request_latency
        .with_label_values(&labels)
        .observe(duration_ms as f64 / 1000.0);

    let mut route_metrics = state.metrics.route_metrics.lock().await;
    let entry = route_metrics.entry(route).or_default();
    entry.count += 1;
    if is_error {
        entry.errors += 1;
    }
    if duration_ms < 50 {
        entry.latency_histogram.under_50ms += 1;
    } else if duration_ms < 250 {
        entry.latency_histogram.under_250ms += 1;
    } else if duration_ms < 1000 {
        entry.latency_histogram.under_1000ms += 1;
    } else if duration_ms < 5000 {
        entry.latency_histogram.under_5000ms += 1;
    } else {
        entry.latency_histogram.over_5000ms += 1;
    }

    response
}

async fn metrics_endpoint(State(state): State<AppState>) -> Response {
    let metric_families = state.metrics.prometheus.registry.gather();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    Response::builder()
        .status(200)
        .header("Content-Type", encoder.format_type())
        .body(Body::from(buffer))
        .unwrap()
}

fn sanitize_chat_message(input: &str) -> String {
    let trimmed = input.trim();
    let limited = if trimmed.len() > 128 {
        &trimmed[..128]
    } else {
        trimmed
    };
    limited.replace('<', "&lt;").replace('>', "&gt;")
}

fn sanitize_alias(input: &str) -> String {
    let trimmed = input.trim();
    let limited = if trimmed.len() > 24 {
        &trimmed[..24]
    } else {
        trimmed
    };
    limited.replace('<', "&lt;").replace('>', "&gt;")
}

async fn chat_ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::Path(table_id): axum::extract::Path<u32>,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_chat_socket(socket, table_id, state))
}

async fn handle_chat_socket(socket: WebSocket, table_id: u32, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    let tx = {
        let mut channels = state.chat_channels.lock().await;
        channels
            .entry(table_id)
            .or_insert_with(|| {
                let (tx, _) = tokio::sync::broadcast::channel(100);
                tx
            })
            .clone()
    };

    let mut rx = tx.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg_str) = rx.recv().await {
            if ws_sender.send(Message::Text(msg_str.into())).await.is_err() {
                break;
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Ok(text) = msg.to_text() {
                if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(text_val) = json_val.get_mut("text") {
                        if let Some(s) = text_val.as_str() {
                            let sanitized = sanitize_chat_message(s);
                            *text_val = serde_json::Value::String(sanitized);
                        }
                    }
                    if let Some(alias_val) = json_val.get_mut("alias") {
                        if let Some(s) = alias_val.as_str() {
                            let sanitized = sanitize_alias(s);
                            *alias_val = serde_json::Value::String(sanitized);
                        }
                    }

                    if let Ok(broadcast_msg) = serde_json::to_string(&json_val) {
                        let _ = tx.send(broadcast_msg);
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

/// GET /api/stats
///
/// Returns global statistics and a top-10 leaderboard, served from an
/// in-memory cache with a 30-second TTL.
async fn get_stats(State(state): State<AppState>) -> Json<stats::StatsResponse> {
    let ttl = std::time::Duration::from_secs(30);
    Json(stats::get_stats(&state.stats, ttl).await)
}
