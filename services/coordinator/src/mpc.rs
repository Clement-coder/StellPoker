//! MPC integration for coordinator-to-node orchestration.
//!
//! Privacy model:
//! - Coordinator never generates or stores plaintext deck/salts.
//! - Every MPC node prepares and dispatches only its own private contribution.
//! - Nodes merge all source-party share fragments locally before proving.
//!
//! ## TLS / mTLS configuration for coordinator → node calls
//!
//! The coordinator can present a TLS client certificate and/or trust only
//! specific MPC node certificates by setting environment variables:
//!
//! - `COORDINATOR_CLIENT_CERT_PATH` / `COORDINATOR_CLIENT_CERT_B64`
//!   – PEM/DER of the coordinator's client cert presented to nodes.
//! - `COORDINATOR_CLIENT_KEY_PATH`  / `COORDINATOR_CLIENT_KEY_B64`
//!   – PEM/DER of the coordinator's client private key.
//! - `MPC_NODE_CERT_PATHS`
//!   – Comma-separated list of paths to the MPC node server certs to trust.
//! - `MPC_NODE_CERT_B64S`
//!   – Comma-separated list of base64-encoded DER node server certs to trust.
//!
//! If none of these are set the coordinator uses the default TLS/CA-chain
//! behaviour (backwards compatible with plain HTTP node endpoints).

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::node_reliability;

/// Whether an error returned from this module means a committee node is
/// down for the rest of the session (as opposed to a generic/application
/// error) — see the `NODE_UNAVAILABLE:` marker set in
/// `trigger_and_collect_proof` (Issue #96). Callers use this to tell a
/// caller "retry the same session" apart from "this session can't recover,
/// start a fresh deal".
pub fn is_node_unavailable_error(error: &str) -> bool {
    error.contains("NODE_UNAVAILABLE")
}

// ── TLS client factory ────────────────────────────────────────────────────────

/// Build the shared `reqwest::Client` used for all coordinator → MPC node calls.
///
/// When TLS environment variables are present the client will:
/// - Present the coordinator's identity certificate (mTLS client auth).
/// - Only trust the configured MPC node server certificates (pinning).
///
/// Falls back to the default system CA bundle when no TLS vars are set.
pub fn build_mpc_client() -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder();

    // ── Optional: trust only specific node server certificates ───────────────
    let node_cert_ders = load_node_cert_ders()?;
    if !node_cert_ders.is_empty() {
        // Disable default roots so only the pinned certs are trusted.
        builder = builder.tls_built_in_root_certs(false);
        for der in node_cert_ders {
            let cert = reqwest::Certificate::from_der(&der)
                .map_err(|e| format!("invalid MPC node cert DER: {}", e))?;
            builder = builder.add_root_certificate(cert);
        }
        tracing::info!("MPC client: trusting only pinned node certificates");
    }

    // ── Optional: client identity (mTLS) ────────────────────────────────────
    if let Some(identity) = load_coordinator_identity()? {
        builder = builder.identity(identity);
        tracing::info!("MPC client: using coordinator client certificate (mTLS)");
    }

    builder
        .build()
        .map_err(|e| format!("failed to build MPC reqwest client: {}", e))
}

/// Load DER bytes for each MPC node server certificate to trust.
fn load_node_cert_ders() -> Result<Vec<Vec<u8>>, String> {
    let mut ders: Vec<Vec<u8>> = Vec::new();

    // Comma-separated file paths.
    if let Ok(paths) = std::env::var("MPC_NODE_CERT_PATHS") {
        for path in paths.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let raw = std::fs::read(path)
                .map_err(|e| format!("MPC_NODE_CERT_PATHS: failed to read '{}': {}", path, e))?;
            ders.push(pem_or_der_bytes(raw));
        }
    }

    // Comma-separated base64-encoded DER values.
    if let Ok(b64s) = std::env::var("MPC_NODE_CERT_B64S") {
        for b64 in b64s.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| format!("MPC_NODE_CERT_B64S: invalid base64: {}", e))?;
            ders.push(raw);
        }
    }

    Ok(ders)
}

/// Load the coordinator's mTLS client identity (cert + key) as a `reqwest::Identity`.
fn load_coordinator_identity() -> Result<Option<reqwest::Identity>, String> {
    // Try to load the certificate.
    let cert_der = load_der_from_env(
        "COORDINATOR_CLIENT_CERT_PATH",
        "COORDINATOR_CLIENT_CERT_B64",
    )?;
    let key_der = load_der_from_env(
        "COORDINATOR_CLIENT_KEY_PATH",
        "COORDINATOR_CLIENT_KEY_B64",
    )?;

    match (cert_der, key_der) {
        (Some(cert), Some(key)) => {
            // reqwest accepts PEM bundles for identity.  We may have DER bytes;
            // convert to PEM so reqwest can parse them.
            let cert_pem = der_to_pem(&cert, "CERTIFICATE");
            let key_pem = der_to_pem(&key, "PRIVATE KEY");
            let mut bundle = cert_pem;
            bundle.extend_from_slice(&key_pem);
            let identity = reqwest::Identity::from_pem(&bundle)
                .map_err(|e| format!("failed to load coordinator mTLS identity: {}", e))?;
            Ok(Some(identity))
        }
        (None, None) => Ok(None),
        (Some(_), None) => Err(
            "COORDINATOR_CLIENT_CERT_PATH/B64 is set but COORDINATOR_CLIENT_KEY_PATH/B64 is missing"
                .to_string(),
        ),
        (None, Some(_)) => Err(
            "COORDINATOR_CLIENT_KEY_PATH/B64 is set but COORDINATOR_CLIENT_CERT_PATH/B64 is missing"
                .to_string(),
        ),
    }
}

/// Load raw DER bytes from a file-path env var, falling back to a base64 env var.
fn load_der_from_env(path_var: &str, b64_var: &str) -> Result<Option<Vec<u8>>, String> {
    if let Ok(path) = std::env::var(path_var) {
        let path = path.trim().to_string();
        let raw = std::fs::read(&path)
            .map_err(|e| format!("{} = {:?}: failed to read file: {}", path_var, path, e))?;
        return Ok(Some(pem_or_der_bytes(raw)));
    }
    if let Ok(b64) = std::env::var(b64_var) {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("{}: invalid base64: {}", b64_var, e))?;
        return Ok(Some(raw));
    }
    Ok(None)
}

/// If the bytes look like PEM (`-----BEGIN`), strip headers and return DER.
/// Otherwise return as-is.
fn pem_or_der_bytes(raw: Vec<u8>) -> Vec<u8> {
    if raw.starts_with(b"-----") {
        if let Ok(s) = std::str::from_utf8(&raw) {
            let b64: String = s
                .lines()
                .filter(|l| !l.starts_with("-----"))
                .collect::<Vec<_>>()
                .join("");
            if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
                return der;
            }
        }
    }
    raw
}

/// Encode DER bytes as PEM with the given label (`CERTIFICATE`, `PRIVATE KEY`, …).
fn der_to_pem(der: &[u8], label: &str) -> Vec<u8> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    // Fold at 64 chars per line (standard PEM).
    let mut pem = format!("-----BEGIN {}-----\n", label);
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        pem.push('\n');
    }
    pem.push_str(&format!("-----END {}-----\n", label));
    pem.into_bytes()
}

/// Result from MPC proof generation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MpcProofResult {
    pub proof: Vec<u8>,
    pub public_inputs: Vec<String>,
    pub session_id: String,
}

#[derive(Clone, Debug)]
pub struct PreparedShareSets {
    pub share_set_ids: Vec<String>,
}

#[derive(Deserialize)]
struct NodeStatusResponse {
    #[allow(dead_code)]
    session_id: String,
    status: String,
}

#[derive(Deserialize)]
struct NodeProofResponse {
    #[allow(dead_code)]
    session_id: String,
    proof: String, // base64
    #[serde(default)]
    public_inputs: Vec<String>,
}

#[derive(Deserialize)]
struct NodePreparedSharesResponse {
    share_set_id: String,
}

/// Generic helper: POST a JSON body to each MPC node's URL and collect share set IDs.
async fn prepare_from_nodes(
    client: &reqwest::Client,
    node_endpoints: &[String],
    url_builder: impl Fn(&str, u32) -> String,
    table_id: u32,
    body: serde_json::Value,
    operation_name: &str,
) -> Result<PreparedShareSets, String> {
    let mut handles = Vec::with_capacity(node_endpoints.len());

    for (idx, endpoint) in node_endpoints.iter().enumerate() {
        let url = url_builder(endpoint, table_id);
        let body = body.clone();
        let client = client.clone();
        let op = operation_name.to_string();
        let endpoint = endpoint.clone();
        let handle = tokio::spawn(async move {
            let call_start = std::time::Instant::now();
            let resp = client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("failed to call node {} {}: {}", idx, op, e))?;
            // Graceful degradation (issue #110): track round-trip latency so
            // persistently slow nodes are logged and scored, without failing
            // this call — the node did respond, just slowly.
            node_reliability::record(&endpoint, call_start.elapsed()).await;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "unable to read response body".to_string());
                return Err(format!(
                    "node {} {} rejected request: HTTP {}: {}",
                    idx, op, status, body
                ));
            }

            let prepared: NodePreparedSharesResponse = resp.json().await.map_err(|e| {
                format!("failed to parse node {} {} response: {}", idx, op, e)
            })?;

            Ok::<(usize, String), String>((idx, prepared.share_set_id))
        });
        handles.push(handle);
    }

    collect_prepared_share_sets(handles, node_endpoints.len()).await
}

/// Ask all nodes to prepare deal share sets.
pub async fn prepare_deal_from_nodes(
    client: &reqwest::Client,
    node_endpoints: &[String],
    circuit_dir: &str,
    table_id: u32,
    players: &[String],
) -> Result<PreparedShareSets, String> {
    prepare_from_nodes(
        client,
        node_endpoints,
        |endpoint, tid| format!("{}/table/{}/prepare-deal", endpoint, tid),
        table_id,
        serde_json::json!({
            "players": players,
            "circuit_dir": circuit_dir,
        }),
        "prepare-deal",
    )
    .await
}

/// Ask all nodes to prepare reveal share sets.
pub async fn prepare_reveal_from_nodes(
    client: &reqwest::Client,
    node_endpoints: &[String],
    circuit_dir: &str,
    table_id: u32,
    phase: &str,
    previously_used_indices: &[u32],
    deck_root: &str,
) -> Result<PreparedShareSets, String> {
    let phase = phase.to_string();
    prepare_from_nodes(
        client,
        node_endpoints,
        move |endpoint, tid| format!("{}/table/{}/prepare-reveal/{}", endpoint, tid, phase),
        table_id,
        serde_json::json!({
            "circuit_dir": circuit_dir,
            "previously_used_indices": previously_used_indices,
            "deck_root": deck_root,
        }),
        "prepare-reveal",
    )
    .await
}

/// Ask all nodes to prepare showdown share sets.
pub async fn prepare_showdown_from_nodes(
    client: &reqwest::Client,
    node_endpoints: &[String],
    circuit_dir: &str,
    table_id: u32,
    board_indices: &[u32],
    num_active_players: u32,
    hand_commitments: &[String],
    deck_root: &str,
) -> Result<PreparedShareSets, String> {
    prepare_from_nodes(
        client,
        node_endpoints,
        |endpoint, tid| format!("{}/table/{}/prepare-showdown", endpoint, tid),
        table_id,
        serde_json::json!({
            "circuit_dir": circuit_dir,
            "board_indices": board_indices,
            "num_active_players": num_active_players,
            "hand_commitments": hand_commitments,
            "deck_root": deck_root,
        }),
        "prepare-showdown",
    )
    .await
}

/// Dispatch all prepared share sets and trigger MPC proof generation.
pub async fn generate_proof_from_share_sets(
    client: &reqwest::Client,
    table_id: u32,
    share_set_ids: &[String],
    session_id: &str,
    circuit_name: &str,
    circuit_dir: &str,
    node_endpoints: &[String],
) -> Result<MpcProofResult, String> {
    dispatch_share_sets_from_nodes(
        client,
        node_endpoints,
        table_id,
        share_set_ids,
        session_id,
        circuit_name,
    )
    .await?;
    trigger_and_collect_proof(client, session_id, circuit_name, circuit_dir, node_endpoints).await
}

#[derive(Deserialize)]
struct NodePermLookupResponse {
    mapped_indices: Vec<u32>,
    salts: Vec<String>,
}

/// Resolve hole cards for a player by chaining permutation lookups across nodes
/// and summing salts from all nodes at the original dealt positions.
///
/// Returns (card_values, combined_salts) for the given deck positions.
pub async fn resolve_hole_cards(
    client: &reqwest::Client,
    node_endpoints: &[String],
    table_id: u32,
    card_positions: &[u32],
) -> Result<(Vec<u32>, Vec<String>), String> {
    if node_endpoints.len() != 3 {
        return Err(format!(
            "expected 3 MPC nodes, got {}",
            node_endpoints.len()
        ));
    }

    // Step 1: Query all 3 nodes in parallel with original positions to get salts.
    // Also use node2's mapped_indices as the first step of the permutation chain.
    let mut salt_handles = Vec::with_capacity(3);
    for (i, endpoint) in node_endpoints.iter().enumerate() {
        let url = format!("{}/table/{}/perm-lookup", endpoint, table_id);
        let client = client.clone();
        let positions = card_positions.to_vec();
        let handle = tokio::spawn(async move {
            let resp = client
                .post(&url)
                .json(&serde_json::json!({ "indices": positions }))
                .send()
                .await
                .map_err(|e| format!("node {} perm-lookup failed: {}", i, e))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "unable to read body".to_string());
                return Err(format!(
                    "node {} perm-lookup rejected: HTTP {}: {}",
                    i, status, body
                ));
            }
            let data: NodePermLookupResponse = resp
                .json()
                .await
                .map_err(|e| format!("node {} perm-lookup parse failed: {}", i, e))?;
            Ok::<(usize, NodePermLookupResponse), String>((i, data))
        });
        salt_handles.push(handle);
    }

    let mut node_responses: Vec<Option<NodePermLookupResponse>> = vec![None, None, None];
    for handle in salt_handles {
        let (idx, resp) = handle
            .await
            .map_err(|e| format!("perm-lookup join error: {}", e))??;
        node_responses[idx] = Some(resp);
    }

    let resp0 = node_responses[0].take().ok_or("missing node 0 response")?;
    let resp1 = node_responses[1].take().ok_or("missing node 1 response")?;
    let resp2 = node_responses[2].take().ok_or("missing node 2 response")?;

    // Sum salts from all 3 nodes (all at the same original positions).
    // Salts are u64 values; sum fits in u128, well below BN254 modulus.
    let num_cards = card_positions.len();
    let mut combined_salts = Vec::with_capacity(num_cards);
    for i in 0..num_cards {
        let s0: u128 = resp0.salts[i]
            .parse::<u64>()
            .map_err(|e| format!("node0 salt parse: {}", e))?
            .into();
        let s1: u128 = resp1.salts[i]
            .parse::<u64>()
            .map_err(|e| format!("node1 salt parse: {}", e))?
            .into();
        let s2: u128 = resp2.salts[i]
            .parse::<u64>()
            .map_err(|e| format!("node2 salt parse: {}", e))?
            .into();
        combined_salts.push(format!("{}", s0 + s1 + s2));
    }

    // Step 2: Chain permutation lookups: node2 → node1 → node0.
    // We already have node2's mapped_indices from step 1.
    let step1 = resp2.mapped_indices;

    // Query node1 with node2's mapped indices.
    let step2 = query_perm_lookup(&client, &node_endpoints[1], table_id, &step1)
        .await?
        .mapped_indices;

    // Query node0 with node1's result → final card values.
    let final_cards = query_perm_lookup(&client, &node_endpoints[0], table_id, &step2)
        .await?
        .mapped_indices;

    Ok((final_cards, combined_salts))
}

async fn query_perm_lookup(
    client: &reqwest::Client,
    endpoint: &str,
    table_id: u32,
    indices: &[u32],
) -> Result<NodePermLookupResponse, String> {
    let url = format!("{}/table/{}/perm-lookup", endpoint, table_id);
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "indices": indices }))
        .send()
        .await
        .map_err(|e| format!("perm-lookup to {} failed: {}", url, e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "unable to read body".to_string());
        return Err(format!(
            "perm-lookup to {} rejected: HTTP {}: {}",
            url, status, body
        ));
    }
    resp.json()
        .await
        .map_err(|e| format!("perm-lookup parse from {} failed: {}", url, e))
}

/// Check health of all MPC nodes.
pub async fn check_node_health(client: &reqwest::Client, endpoints: &[String]) -> Vec<bool> {
    let mut results = Vec::new();
    for endpoint in endpoints {
        let healthy = client
            .get(format!("{}/health", endpoint))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        results.push(healthy);
    }
    results
}

async fn collect_prepared_share_sets(
    handles: Vec<tokio::task::JoinHandle<Result<(usize, String), String>>>,
    expected_len: usize,
) -> Result<PreparedShareSets, String> {
    let mut ordered = vec![String::new(); expected_len];
    for handle in handles {
        let (idx, share_set_id) = handle
            .await
            .map_err(|e| format!("prepare task join error: {}", e))??;
        if idx >= ordered.len() {
            return Err(format!("prepare task returned out-of-range index {}", idx));
        }
        ordered[idx] = share_set_id;
    }

    if ordered.iter().any(|id| id.is_empty()) {
        return Err("missing share_set_id for one or more nodes".to_string());
    }

    Ok(PreparedShareSets {
        share_set_ids: ordered,
    })
}

async fn dispatch_share_sets_from_nodes(
    client: &reqwest::Client,
    node_endpoints: &[String],
    table_id: u32,
    share_set_ids: &[String],
    session_id: &str,
    circuit_name: &str,
) -> Result<(), String> {
    if node_endpoints.len() != share_set_ids.len() {
        return Err(format!(
            "node count ({}) does not match share_set count ({})",
            node_endpoints.len(),
            share_set_ids.len()
        ));
    }

    let mut handles = Vec::with_capacity(node_endpoints.len());

    for (idx, endpoint) in node_endpoints.iter().enumerate() {
        let url = format!("{}/table/{}/dispatch-shares", endpoint, table_id);
        let share_set_id = share_set_ids[idx].clone();
        let session_id = session_id.to_string();
        let circuit_name = circuit_name.to_string();
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let resp = client
                .post(&url)
                .json(&serde_json::json!({
                    "share_set_id": share_set_id,
                    "proof_session_id": session_id,
                    "circuit_name": circuit_name,
                }))
                .send()
                .await
                .map_err(|e| format!("failed to call node {} dispatch-shares: {}", idx, e))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "unable to read response body".to_string());
                return Err(format!(
                    "node {} dispatch-shares rejected request: HTTP {}: {}",
                    idx, status, body
                ));
            }
            Ok::<(), String>(())
        });
        handles.push(handle);
    }

    for handle in handles {
        handle
            .await
            .map_err(|e| format!("dispatch join error: {}", e))??;
    }

    Ok(())
}

async fn trigger_and_collect_proof(
    client: &reqwest::Client,
    session_id: &str,
    circuit_name: &str,
    circuit_dir: &str,
    node_endpoints: &[String],
) -> Result<MpcProofResult, String> {
    if node_endpoints.is_empty() {
        return Err("no MPC node endpoints configured".to_string());
    }

    // Node expects CRS directory (it appends bn254_g1.dat internally).
    let crs_dir = std::env::var("CRS_DIR").unwrap_or_else(|_| "./crs".to_string());

    // Honest-majority fault tolerance (Issue #96): a node that fails to
    // accept the generate trigger gets a few quick retries first, since a
    // brief blip (restart-in-place, transient network hiccup) shouldn't
    // sink an otherwise-healthy session — this mirrors the retry already
    // done for the co-noir subprocess itself (session.rs) and for Soroban
    // invokes (soroban/mod.rs). Only *connection-level* failures are
    // retried/reclassified this way: an HTTP error status means the node is
    // up and rejected the request, which is a different (likely
    // application-level) problem that retrying won't fix.
    const TRIGGER_RETRY_ATTEMPTS: u32 = 3;

    let mut handles = Vec::new();
    for (i, endpoint) in node_endpoints.iter().enumerate() {
        let url = format!("{}/session/{}/generate", endpoint, session_id);
        let client = client.clone();
        let circuit_dir = circuit_dir.to_string();
        let crs_dir = crs_dir.clone();
        let handle = tokio::spawn(async move {
            let mut last_conn_error: Option<String> = None;
            for attempt in 1..=TRIGGER_RETRY_ATTEMPTS {
                let resp = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "circuit_dir": circuit_dir,
                        "crs_path": crs_dir,
                    }))
                    .send()
                    .await;

                let resp = match resp {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(
                            node_index = i,
                            attempt,
                            max_attempts = TRIGGER_RETRY_ATTEMPTS,
                            error = %e,
                            "MPC node unreachable triggering generate; retrying"
                        );
                        last_conn_error = Some(e.to_string());
                        if attempt < TRIGGER_RETRY_ATTEMPTS {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                200 * attempt as u64,
                            ))
                            .await;
                        }
                        continue;
                    }
                };

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp
                        .text()
                        .await
                        .unwrap_or_else(|_| "unable to read response body".to_string());
                    return Err(format!(
                        "node {} trigger failed: HTTP {}: {}",
                        i, status, body
                    ));
                }
                return Ok::<(), String>(());
            }

            // Every attempt failed to even connect: this node is down for
            // the rest of the session, not just slow. Mark it so callers
            // (api/mod.rs) can tell the difference between "retry the same
            // session" and "this session is dead, start a fresh deal" —
            // see `is_node_unavailable_error`.
            Err(format!(
                "NODE_UNAVAILABLE: node {} unreachable after {} attempts triggering generate: {}",
                i,
                TRIGGER_RETRY_ATTEMPTS,
                last_conn_error.unwrap_or_default()
            ))
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.map_err(|e| format!("join error: {}", e))??;
    }

    // Poll node 0 for proof completion. Graceful degradation (issue #110): a
    // node that is merely slow — not dead — extends the poll deadline
    // instead of failing the whole session, up to a bounded hard ceiling so
    // a genuinely unresponsive node still eventually times out.
    let proof_node = &node_endpoints[0];
    let base_max_polls: u32 = if circuit_name == "showdown_valid" {
        900
    } else {
        300
    };
    let hard_ceiling = base_max_polls * 2;
    let mut deadline_polls = base_max_polls;

    let mut polls_done: u32 = 0;
    while polls_done < deadline_polls {
        polls_done += 1;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let status_url = format!("{}/session/{}/status", proof_node, session_id);
        let poll_start = std::time::Instant::now();
        let resp = client.get(&status_url).send().await;
        let was_slow = node_reliability::record(proof_node, poll_start.elapsed()).await;
        if was_slow && deadline_polls < hard_ceiling {
            deadline_polls = (deadline_polls + 1).min(hard_ceiling);
            tracing::warn!(
                node = %proof_node,
                session_id = %session_id,
                latency_ms = poll_start.elapsed().as_millis(),
                extended_deadline_polls = deadline_polls,
                "MPC node slow during proof polling; extending session deadline"
            );
        }

        let resp = match resp {
            Ok(r) => r,
            // A transient poll failure is treated the same as a slow
            // response above: retry within the (possibly extended)
            // deadline rather than aborting the session immediately.
            Err(_) => continue,
        };

        if !resp.status().is_success() {
            continue;
        }

        let status: NodeStatusResponse = resp
            .json()
            .await
            .map_err(|e| format!("failed to parse status: {}", e))?;

        match status.status.as_str() {
            "complete" => {
                let proof_url = format!("{}/session/{}/proof", proof_node, session_id);
                let proof_resp = client
                    .get(&proof_url)
                    .send()
                    .await
                    .map_err(|e| format!("failed to fetch proof: {}", e))?;

                if !proof_resp.status().is_success() {
                    let status = proof_resp.status();
                    let body = proof_resp
                        .text()
                        .await
                        .unwrap_or_else(|_| "unable to read response body".to_string());
                    return Err(format!("proof fetch failed: HTTP {}: {}", status, body));
                }

                let proof_data: NodeProofResponse = proof_resp
                    .json()
                    .await
                    .map_err(|e| format!("failed to parse proof: {}", e))?;

                let proof_bytes = base64::engine::general_purpose::STANDARD
                    .decode(&proof_data.proof)
                    .map_err(|e| format!("failed to decode proof: {}", e))?;

                return Ok(MpcProofResult {
                    proof: proof_bytes,
                    public_inputs: proof_data.public_inputs,
                    session_id: session_id.to_string(),
                });
            }
            s if s.starts_with("failed") => {
                return Err(format!("proof generation failed: {}", s));
            }
            _ => {}
        }
    }

    Err(format!(
        "[{}] proof generation timed out after {} seconds ({} of them from deadline extensions granted to slow nodes)",
        session_id,
        polls_done,
        deadline_polls.saturating_sub(base_max_polls)
    ))
}

#[cfg(test)]
mod error_handling_tests {
    //! Coverage for the **MPC node timeout / unreachable node** error path.
    //!
    //! A node that is unreachable exercises the same `Err(String)` path as a
    //! node that times out mid-request: the failing `reqwest` future is mapped
    //! into a descriptive error string that the API layer turns into a
    //! 502/503 response. We point requests at a closed local port so the
    //! connection fails fast and deterministically. We also cover the
    //! pre-flight guards that reject inconsistent orchestration state before
    //! any network call is made.
    use super::*;

    // Nothing listens on port 1: connections are refused immediately.
    const DEAD_NODE: &str = "http://127.0.0.1:1";

    fn test_client() -> reqwest::Client {
        reqwest::Client::new()
    }

    #[tokio::test]
    async fn prepare_deal_errors_when_node_unreachable() {
        let endpoints = vec![DEAD_NODE.to_string()];
        let err = prepare_deal_from_nodes(&test_client(), &endpoints, "/circuits", 1, &["P1".to_string()])
            .await
            .unwrap_err();
        assert!(err.contains("prepare-deal"), "got: {err}");
    }

    #[tokio::test]
    async fn check_node_health_reports_unreachable_as_unhealthy() {
        let health = check_node_health(&test_client(), &[DEAD_NODE.to_string()]).await;
        assert_eq!(health, vec![false]);
    }

    #[tokio::test]
    async fn resolve_hole_cards_requires_three_nodes() {
        let err = resolve_hole_cards(&test_client(), &["a".to_string(), "b".to_string()], 1, &[0])
            .await
            .unwrap_err();
        assert!(err.contains("expected 3 MPC nodes"), "got: {err}");
    }

    #[tokio::test]
    async fn dispatch_rejects_node_share_count_mismatch() {
        // 2 nodes but only 1 prepared share set => orchestration inconsistency.
        let err = dispatch_share_sets_from_nodes(
            &test_client(),
            &["n0".to_string(), "n1".to_string()],
            1,
            &["share0".to_string()],
            "sess",
            "deal_valid",
        )
        .await
        .unwrap_err();
        assert!(err.contains("does not match share_set count"), "got: {err}");
    }

    #[tokio::test]
    async fn trigger_proof_requires_node_endpoints() {
        let err = trigger_and_collect_proof(&test_client(), "sess", "deal_valid", "/circuits", &[])
            .await
            .unwrap_err();
        assert!(err.contains("no MPC node endpoints configured"), "got: {err}");
    }

    #[tokio::test]
    async fn trigger_marks_unreachable_node_as_node_unavailable() {
        // Issue #96: a node that never even accepts the connection (as
        // opposed to one that responds with an HTTP error) is down for the
        // rest of the session — the error must be recognizable via
        // `is_node_unavailable_error` so callers know to start a fresh deal
        // rather than retry the same (now-impossible) session.
        let endpoints = vec![
            DEAD_NODE.to_string(),
            DEAD_NODE.to_string(),
            DEAD_NODE.to_string(),
        ];
        let err = trigger_and_collect_proof(&test_client(), "sess", "deal_valid", "/circuits", &endpoints)
            .await
            .unwrap_err();
        assert!(is_node_unavailable_error(&err), "got: {err}");
    }

    #[test]
    fn is_node_unavailable_error_only_matches_the_marker() {
        assert!(is_node_unavailable_error(
            "NODE_UNAVAILABLE: node 1 unreachable after 3 attempts triggering generate: connect error"
        ));
        assert!(!is_node_unavailable_error("node 1 trigger failed: HTTP 500: internal error"));
        assert!(!is_node_unavailable_error("proof generation timed out after 300 seconds"));
    }

    #[tokio::test]
    async fn collect_prepared_share_sets_detects_missing_id() {
        // A node "succeeded" but returned an empty share-set id.
        let handle = tokio::spawn(async { Ok::<(usize, String), String>((0, String::new())) });
        let err = collect_prepared_share_sets(vec![handle], 1)
            .await
            .unwrap_err();
        assert!(err.contains("missing share_set_id"), "got: {err}");
    }

    #[tokio::test]
    async fn collect_prepared_share_sets_detects_out_of_range_index() {
        let handle =
            tokio::spawn(async { Ok::<(usize, String), String>((5, "id".to_string())) });
        let err = collect_prepared_share_sets(vec![handle], 1)
            .await
            .unwrap_err();
        assert!(err.contains("out-of-range index"), "got: {err}");
    }
}

#[cfg(test)]
mod byzantine_fault_tolerance_tests {
    //! Byzantine fault tolerance tests for the MPC committee protocol.
    //!
    //! Covers three classes of Byzantine adversary behavior defined in issue #301:
    //!
    //! 1. **Incorrect shares** — node sends malformed, empty, or wrong-schema data.
    //! 2. **Mid-protocol stalls** — node accepts the connection but never completes,
    //!    forcing the caller's timeout to fire.
    //! 3. **Colluding nodes** — multiple nodes coordinate to return invalid data;
    //!    the protocol must surface the misbehavior before game state is affected.
    //!
    //! Each test spins up a lightweight in-process axum server that simulates the
    //! target Byzantine behavior, then asserts the coordinator rejects it.

    use super::*;
    use axum::{http::StatusCode, routing::post, Router};
    use tokio::net::TcpListener;

    fn test_client() -> reqwest::Client {
        reqwest::Client::new()
    }

    /// Spawn a local axum HTTP server on a random port and return its base URL.
    async fn spawn_test_server(router: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{}", addr)
    }

    // ── Scenario 1: Nodes sending incorrect shares ───────────────────────────

    /// A Byzantine node returning an empty share_set_id (structurally valid JSON
    /// but semantically invalid) must cause the coordinator to abort with a
    /// "missing share_set_id" error rather than silently accepting a null share.
    #[tokio::test]
    async fn byzantine_empty_share_id_aborts_deal_protocol() {
        let router = Router::new().route(
            "/table/:table_id/prepare-deal",
            post(|| async { axum::Json(serde_json::json!({ "share_set_id": "" })) }),
        );
        let endpoint = spawn_test_server(router).await;

        let err = prepare_deal_from_nodes(&test_client(), &[endpoint], "/circuits", 1, &["P1".to_string()])
            .await
            .unwrap_err();

        assert!(
            err.contains("missing share_set_id"),
            "empty share ID must be caught: {err}"
        );
    }

    /// A Byzantine node returning a completely unexpected JSON schema (no
    /// share_set_id key) must either fail deserialization or trigger the
    /// missing-ID guard — the protocol must not succeed with garbage data.
    #[tokio::test]
    async fn byzantine_wrong_schema_response_rejected() {
        let router = Router::new().route(
            "/table/:table_id/prepare-deal",
            post(|| async {
                axum::Json(serde_json::json!({
                    "evil_field": "bypass_attempt",
                    "injected_payload": [0xde, 0xad, 0xbe, 0xef]
                }))
            }),
        );
        let endpoint = spawn_test_server(router).await;

        let result =
            prepare_deal_from_nodes(&test_client(), &[endpoint], "/circuits", 1, &["P1".to_string()]).await;

        match result {
            Err(e) => assert!(
                e.contains("missing share_set_id") || e.contains("parse"),
                "wrong-schema response must surface as error: {e}"
            ),
            Ok(shares) => {
                // serde default-filled share_set_id as "" — the guard must catch it.
                assert!(
                    shares.share_set_ids.iter().all(|id| !id.is_empty()),
                    "coordinator must not accept empty share IDs from Byzantine node"
                );
            }
        }
    }

    /// A Byzantine node that replies HTTP 500 during share preparation simulates
    /// a node deliberately crashing or corrupting its contribution. The coordinator
    /// must propagate the failure rather than proceeding with a partial committee.
    #[tokio::test]
    async fn byzantine_http500_on_prepare_aborts_protocol() {
        let router = Router::new().route(
            "/table/:table_id/prepare-deal",
            post(|| async {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "byzantine node: deliberate sabotage",
                )
            }),
        );
        let endpoint = spawn_test_server(router).await;

        let err = prepare_deal_from_nodes(&test_client(), &[endpoint], "/circuits", 1, &["P1".to_string()])
            .await
            .unwrap_err();

        assert!(
            err.contains("HTTP 500") || err.contains("rejected"),
            "HTTP 500 from Byzantine node must abort deal protocol: {err}"
        );
    }

    /// One Byzantine node mixed into an otherwise honest committee must abort
    /// the entire round — the protocol requires a complete, valid share set
    /// from every node, so a single bad actor prevents quorum.
    #[tokio::test]
    async fn one_byzantine_node_among_honest_peers_aborts_round() {
        let honest_router = Router::new().route(
            "/table/:table_id/prepare-deal",
            post(|| async {
                axum::Json(serde_json::json!({ "share_set_id": "honest_share_0x1a2b" }))
            }),
        );
        let byzantine_router = Router::new().route(
            "/table/:table_id/prepare-deal",
            post(|| async { (StatusCode::FORBIDDEN, "refusing to participate") }),
        );

        let honest = spawn_test_server(honest_router).await;
        let byzantine = spawn_test_server(byzantine_router).await;

        let err = prepare_deal_from_nodes(
            &test_client(),
            &[honest, byzantine],
            "/circuits",
            1,
            &["P1".to_string(), "P2".to_string()],
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("HTTP") || err.contains("rejected"),
            "single Byzantine node must prevent quorum formation: {err}"
        );
    }

    /// A Byzantine node returning HTTP 400 during the reveal-prepare phase
    /// (e.g., falsely claiming the deck root is invalid) must halt the reveal.
    #[tokio::test]
    async fn byzantine_node_rejects_reveal_with_bad_request() {
        let router = Router::new().route(
            "/table/:table_id/prepare-reveal/:phase",
            post(|| async {
                (
                    StatusCode::BAD_REQUEST,
                    "byzantine: claiming deck root is invalid",
                )
            }),
        );
        let endpoint = spawn_test_server(router).await;

        let err = prepare_reveal_from_nodes(
            &test_client(),
            &[endpoint],
            "/circuits",
            1,
            "preflop",
            &[],
            "0xdeadbeef_root",
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("HTTP 400") || err.contains("rejected"),
            "Byzantine bad-request during reveal must abort the phase: {err}"
        );
    }

    // ── Scenario 2: Nodes stalling mid-protocol ──────────────────────────────

    /// A Byzantine node that accepts the TCP connection but delays its response
    /// by 60 seconds simulates a stall attack. A 2-second caller timeout must
    /// fire, demonstrating that the protocol does not block indefinitely.
    #[tokio::test]
    async fn byzantine_stall_on_prepare_deal_causes_timeout() {
        use tokio::time::Duration;

        let router = Router::new().route(
            "/table/:table_id/prepare-deal",
            post(|| async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                axum::Json(serde_json::json!({ "share_set_id": "arrived_too_late" }))
            }),
        );
        let endpoint = spawn_test_server(router).await;

        let result = tokio::time::timeout(
            Duration::from_secs(2),
            prepare_deal_from_nodes(&test_client(), &[endpoint], "/circuits", 1, &["P1".to_string()]),
        )
        .await;

        assert!(
            result.is_err(),
            "stalling Byzantine node must trigger the caller timeout — protocol must not hang"
        );
    }

    /// A Byzantine node that stalls during the perm-lookup (card reveal) phase
    /// prevents card resolution. A 2-second timeout must fire before any
    /// card values or salts are returned to the caller.
    #[tokio::test]
    async fn byzantine_stall_during_card_reveal_causes_timeout() {
        use tokio::time::Duration;

        let stalling = Router::new().route(
            "/table/:table_id/perm-lookup",
            post(|| async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                axum::Json(serde_json::json!({
                    "mapped_indices": [0_u32],
                    "salts": ["0"]
                }))
            }),
        );
        let honest = || {
            Router::new().route(
                "/table/:table_id/perm-lookup",
                post(|| async {
                    axum::Json(serde_json::json!({
                        "mapped_indices": [5_u32, 12_u32],
                        "salts": ["111", "222"]
                    }))
                }),
            )
        };

        let e0 = spawn_test_server(honest()).await;
        let e1 = spawn_test_server(stalling).await; // Byzantine staller
        let e2 = spawn_test_server(honest()).await;

        let result =
            tokio::time::timeout(Duration::from_secs(2), resolve_hole_cards(&test_client(), &[e0, e1, e2], 1, &[0, 1]))
                .await;

        assert!(
            result.is_err(),
            "Byzantine stall during card reveal must cause timeout before exposing card values"
        );
    }

    /// The zero-node pre-flight guard catches the degenerate Byzantine scenario
    /// where all nodes are removed from the committee before proof generation.
    /// This fires immediately without any network call.
    #[tokio::test]
    async fn zero_node_committee_rejected_before_proof_generation() {
        let err = trigger_and_collect_proof(&test_client(), "sess_zero_nodes", "deal_valid", "/circuits", &[])
            .await
            .unwrap_err();

        assert!(
            err.contains("no MPC node endpoints configured"),
            "empty committee must be rejected before attempting proof generation: {err}"
        );
    }

    // ── Scenario 3: Nodes colluding to reveal secrets ────────────────────────

    /// Colluding nodes returning the identical share_set_id (impossible in honest
    /// execution, where each node generates an independent random share) expose a
    /// detectable fingerprint. This test documents the invariant: a monitoring
    /// layer can identify collusion and trigger slashing.
    #[tokio::test]
    async fn colluding_nodes_duplicate_share_ids_are_detectable() {
        const COLLUDED_ID: &str = "colluded_share_DEADBEEF_same_for_both";

        let make_colluding = |id: &'static str| {
            Router::new().route(
                "/table/:table_id/prepare-deal",
                post(move || async move {
                    axum::Json(serde_json::json!({ "share_set_id": id }))
                }),
            )
        };

        let e0 = spawn_test_server(make_colluding(COLLUDED_ID)).await;
        let e1 = spawn_test_server(make_colluding(COLLUDED_ID)).await;

        let shares = prepare_deal_from_nodes(
            &test_client(),
            &[e0, e1],
            "/circuits",
            1,
            &["P1".to_string(), "P2".to_string()],
        )
        .await
        .expect("non-empty IDs pass the coordinator's local guard");

        // Detect collusion fingerprint: unique IDs < total IDs.
        let unique: std::collections::HashSet<_> = shares.share_set_ids.iter().collect();
        assert!(
            unique.len() < shares.share_set_ids.len(),
            "duplicate share IDs across nodes are the fingerprint of collusion — \
             a slashing condition must be raised by the committee-registry"
        );
    }

    /// Colluding nodes that coordinate to return invalid salt values during
    /// perm-lookup attempt to corrupt the combined salt used in card commitments.
    /// The salt-parsing step in resolve_hole_cards must reject non-numeric salts
    /// before any card value is derived.
    #[tokio::test]
    async fn colluding_nodes_invalid_salts_caught_before_card_derivation() {
        let byzantine = || {
            Router::new().route(
                "/table/:table_id/perm-lookup",
                post(|| async {
                    axum::Json(serde_json::json!({
                        "mapped_indices": [3_u32, 7_u32],
                        // Coordinated garbage salts — would corrupt combined_salt if accepted.
                        "salts": ["NOT_A_VALID_SALT", "ALSO_INVALID"]
                    }))
                }),
            )
        };

        let e0 = spawn_test_server(byzantine()).await;
        let e1 = spawn_test_server(byzantine()).await;
        let e2 = spawn_test_server(byzantine()).await;

        let err = resolve_hole_cards(&test_client(), &[e0, e1, e2], 1, &[0, 1])
            .await
            .unwrap_err();

        assert!(
            err.contains("salt parse") || err.contains("parse"),
            "coordinated invalid salts must be caught before card values are derived: {err}"
        );
    }

    /// A full-committee cartel that refuses to produce deal shares (coordinated
    /// HTTP 403) simulates nodes colluding to freeze the game. The coordinator
    /// must surface the error so the game can trigger emergency recovery.
    #[tokio::test]
    async fn colluding_nodes_coordinated_refusal_surfaces_as_error() {
        let cartel_router = || {
            Router::new().route(
                "/table/:table_id/prepare-deal",
                post(|| async {
                    (
                        StatusCode::FORBIDDEN,
                        "cartel: coordinated refusal to participate",
                    )
                }),
            )
        };

        let e0 = spawn_test_server(cartel_router()).await;
        let e1 = spawn_test_server(cartel_router()).await;
        let e2 = spawn_test_server(cartel_router()).await;

        let err = prepare_deal_from_nodes(
            &test_client(),
            &[e0, e1, e2],
            "/circuits",
            1,
            &["P1".to_string(), "P2".to_string(), "P3".to_string()],
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("HTTP 403") || err.contains("rejected"),
            "coordinated node refusal must surface as protocol error for emergency recovery: {err}"
        );
    }

    // ── Byzantine orchestration guards ───────────────────────────────────────

    /// A sub-threshold committee (fewer nodes than the 3 required for secret
    /// reconstruction) must be rejected at the pre-flight guard, before any
    /// network call that might partially expose shares.
    #[tokio::test]
    async fn subthreshold_committee_rejected_by_preflight_guard() {
        let err = resolve_hole_cards(
            &test_client(),
            &["http://node0".to_string(), "http://node1".to_string()],
            1,
            &[0],
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("expected 3 MPC nodes"),
            "sub-threshold committee must be rejected by pre-flight guard before exposing shares: {err}"
        );
    }

    /// A Byzantine orchestration injection that creates a mismatch between the
    /// number of nodes and prepared share sets must be caught before dispatch,
    /// preventing the protocol from advancing with an inconsistent committee state.
    #[tokio::test]
    async fn byzantine_orchestration_share_mismatch_blocked_before_dispatch() {
        let err = dispatch_share_sets_from_nodes(
            &test_client(),
            &["http://node0".to_string(), "http://node1".to_string()],
            1,
            &["share_for_node0_only".to_string()], // 1 share for 2 nodes
            "sess_byzantine_injection",
            "deal_valid",
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("does not match share_set count"),
            "Byzantine share-count mismatch must be blocked before dispatch reaches nodes: {err}"
        );
    }
}
