//! MPC session manager for co-noir proof generation.
//!
//! Each session represents one proof generation request (deal, reveal, or showdown).
//! The lifecycle:
//! 1. Coordinator sends shares via POST /session/:id/shares
//! 2. Coordinator triggers proof gen via POST /session/:id/generate
//! 3. Node runs co-noir witness extension + proof generation as subprocesses
//! 4. Coordinator polls GET /session/:id/status and retrieves proof

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;
use tokio::time::{sleep, Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    /// Shares received, waiting for generate trigger
    SharesReceived,
    /// Witness extension in progress
    WitnessGenerating,
    /// Proof generation in progress
    ProofGenerating,
    /// Proof generation complete
    Complete,
    /// Something failed
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct MpcSessionState {
    pub session_id: String,
    pub circuit_name: String,
    pub status: SessionStatus,
    /// Path to merged share file (Prover.toml with secret-shared values)
    pub share_path: Option<PathBuf>,
    /// Per-source share fragments for this session.
    pub partial_share_paths: HashMap<u32, PathBuf>,
    /// Expected number of contributing source parties.
    pub expected_total_parties: Option<u32>,
    /// Working directory for this session's temp files
    pub work_dir: PathBuf,
    /// Path to generated witness
    pub witness_path: Option<PathBuf>,
    /// Path to generated proof
    pub proof_path: Option<PathBuf>,
    /// Public inputs emitted by co-noir for the generated proof.
    pub public_inputs: Option<Vec<String>>,
}

impl MpcSessionState {
    pub fn new(session_id: String, circuit_name: String, work_dir: PathBuf) -> Self {
        Self {
            session_id,
            circuit_name,
            status: SessionStatus::SharesReceived,
            share_path: None,
            partial_share_paths: HashMap::new(),
            expected_total_parties: None,
            work_dir,
            witness_path: None,
            proof_path: None,
            public_inputs: None,
        }
    }
}

/// Save one base64-decoded share fragment from a source party.
pub fn receive_share_fragment(
    session: &mut MpcSessionState,
    share_data_b64: &str,
    source_party_id: u32,
    total_parties: u32,
) -> Result<(), String> {
    if source_party_id >= total_parties {
        return Err(format!(
            "source_party_id {} out of range for total_parties {}",
            source_party_id, total_parties
        ));
    }

    if let Some(expected) = session.expected_total_parties {
        if expected != total_parties {
            return Err(format!(
                "total_parties mismatch: existing {}, got {}",
                expected, total_parties
            ));
        }
    } else {
        session.expected_total_parties = Some(total_parties);
    }

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(share_data_b64)
        .map_err(|e| format!("base64 decode error: {}", e))?;

    let share_path = session
        .work_dir
        .join(format!("share_source_{}.shared", source_party_id));
    std::fs::write(&share_path, &bytes)
        .map_err(|e| format!("failed to write share file: {}", e))?;

    session
        .partial_share_paths
        .insert(source_party_id, share_path);
    session.status = SessionStatus::SharesReceived;
    Ok(())
}

/// Run co-noir proof generation as async subprocesses.
///
/// This spawns two sequential commands:
/// 1. `co-noir generate-witness` — extends the witness in MPC
/// 2. `co-noir build-and-generate-proof` — generates the UltraHonk proof in MPC
///
/// co-noir handles all peer-to-peer MPC communication internally via TCP.
pub async fn run_proof_generation(
    session_id: String,
    circuit_dir: String,
    circuit_name: String,
    work_dir: PathBuf,
    node_id: u32,
    partial_share_paths: Vec<(u32, PathBuf)>,
    expected_total_parties: u32,
    party_config_path: String,
    crs_path: String,
    limits: crate::limits::ResourceLimits,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let circuit_path = format!(
        "{}/{}/target/{}.json",
        circuit_dir, circuit_name, circuit_name
    );
    let share_path = work_dir.join("Prover.toml");
    let witness_path = work_dir.join("witness.gz");
    let proof_path = work_dir.join("proof.bin");
    let public_inputs_path = work_dir.join("public_inputs.json");
    // Use the CRS file (bn254_g1.dat) from the CRS directory
    let crs_file = format!("{}/bn254_g1.dat", crs_path);

    if partial_share_paths.len() != expected_total_parties as usize {
        return Err(format!(
            "incomplete share fragments: got {}, expected {}",
            partial_share_paths.len(),
            expected_total_parties
        ));
    }

    let mut sorted_fragments = partial_share_paths;
    sorted_fragments.sort_by_key(|(source, _)| *source);

    tracing::info!(
        session_id = %session_id,
        phase = "merge_shares",
        node_id,
        circuit = %circuit_name,
        fragment_count = sorted_fragments.len(),
        "merging share fragments"
    );
    let merge_start = Instant::now();

    let mut merge_cmd = Command::new("co-noir");
    merge_cmd
        .arg("merge-input-shares")
        .arg("--circuit")
        .arg(&circuit_path)
        .arg("--protocol")
        .arg("REP3")
        .arg("--config")
        .arg(&party_config_path);
    for (_, path) in &sorted_fragments {
        merge_cmd.arg("--inputs").arg(path);
    }
    merge_cmd.arg("--out").arg(&share_path);
    limits.apply_to_command(&mut merge_cmd);

    let merge_output = merge_cmd
        .output()
        .await
        .map_err(|e| format!("failed to spawn co-noir merge-input-shares: {}", e))?;

    if !merge_output.status.success() {
        let stderr = String::from_utf8_lossy(&merge_output.stderr);
        let stdout = String::from_utf8_lossy(&merge_output.stdout);
        tracing::error!(
            session_id = %session_id,
            phase = "merge_shares",
            node_id,
            elapsed_ms = merge_start.elapsed().as_millis() as u64,
            "co-noir merge-input-shares failed"
        );
        return Err(format!(
            "co-noir merge-input-shares failed (node {}):\nstderr: {}\nstdout: {}",
            node_id, stderr, stdout
        ));
    }

    tracing::info!(
        session_id = %session_id,
        phase = "witness_generation",
        node_id,
        circuit = %circuit_name,
        elapsed_ms = merge_start.elapsed().as_millis() as u64,
        "starting witness generation"
    );
    let witness_start = Instant::now();

    // Step 1: Generate witness in MPC
    let mut witness_cmd = Command::new("co-noir");
    witness_cmd
        .arg("generate-witness")
        .arg("--circuit")
        .arg(&circuit_path)
        .arg("--input")
        .arg(&share_path)
        .arg("--protocol")
        .arg("REP3")
        .arg("--config")
        .arg(&party_config_path)
        .arg("--out")
        .arg(&witness_path);
    limits.apply_to_command(&mut witness_cmd);
    let witness_output = witness_cmd
        .output()
        .await
        .map_err(|e| format!("failed to spawn co-noir generate-witness: {}", e))?;

    if !witness_output.status.success() {
        let stderr = String::from_utf8_lossy(&witness_output.stderr);
        let stdout = String::from_utf8_lossy(&witness_output.stdout);
        tracing::error!(
            session_id = %session_id,
            phase = "witness_generation",
            node_id,
            elapsed_ms = witness_start.elapsed().as_millis() as u64,
            "co-noir generate-witness failed"
        );
        return Err(format!(
            "co-noir generate-witness failed (node {}):\nstderr: {}\nstdout: {}",
            node_id, stderr, stdout
        ));
    }

    tracing::info!(
        session_id = %session_id,
        phase = "proof_generation",
        node_id,
        elapsed_ms = witness_start.elapsed().as_millis() as u64,
        "witness generated, starting proof generation"
    );
    let proof_start = Instant::now();

    // Step 2: Build and generate proof in MPC
    let vk_path = format!("{}/{}/target/vk_keccak", circuit_dir, circuit_name);
    let mut last_proof_output: Option<std::process::Output> = None;
    for attempt in 1..=3 {
        let mut proof_cmd = Command::new("co-noir");
        proof_cmd
            .arg("build-and-generate-proof")
            .arg("--circuit")
            .arg(&circuit_path)
            .arg("--witness")
            .arg(&witness_path)
            .arg("--protocol")
            .arg("REP3")
            .arg("--config")
            .arg(&party_config_path)
            .arg("--crs")
            .arg(&crs_file)
            .arg("--hasher")
            .arg("keccak")
            .arg("--vk")
            .arg(&vk_path)
            .arg("--out")
            .arg(&proof_path)
            .arg("--public-input")
            .arg(&public_inputs_path)
            .arg("--fields-as-json");
        limits.apply_to_command(&mut proof_cmd);
        let proof_output = proof_cmd
            .output()
            .await
            .map_err(|e| format!("failed to spawn co-noir build-and-generate-proof: {}", e))?;

        if proof_output.status.success() {
            last_proof_output = Some(proof_output);
            break;
        }

        let stderr = String::from_utf8_lossy(&proof_output.stderr);
        let is_transient_resource_error =
            stderr.contains("No buffer space available") || stderr.contains("os error 55");

        if is_transient_resource_error && attempt < 3 {
            tracing::warn!(
                session_id = %session_id,
                phase = "proof_generation",
                node_id,
                attempt,
                elapsed_ms = proof_start.elapsed().as_millis() as u64,
                error = %stderr.trim(),
                "co-noir build-and-generate-proof transient failure, retrying"
            );
            sleep(Duration::from_millis((attempt as u64) * 500)).await;
            continue;
        }

        let stdout = String::from_utf8_lossy(&proof_output.stdout);
        tracing::error!(
            session_id = %session_id,
            phase = "proof_generation",
            node_id,
            attempt,
            elapsed_ms = proof_start.elapsed().as_millis() as u64,
            "co-noir build-and-generate-proof failed"
        );
        return Err(format!(
            "co-noir build-and-generate-proof failed (node {}):\nstderr: {}\nstdout: {}",
            node_id, stderr, stdout
        ));
    }

    if last_proof_output.is_none() {
        tracing::error!(
            session_id = %session_id,
            phase = "proof_generation",
            node_id,
            elapsed_ms = proof_start.elapsed().as_millis() as u64,
            "co-noir build-and-generate-proof failed after retries"
        );
        return Err(format!(
            "co-noir build-and-generate-proof failed after retries (node {})",
            node_id
        ));
    }

    tracing::info!(
        session_id = %session_id,
        phase = "proof_generation",
        node_id,
        elapsed_ms = proof_start.elapsed().as_millis() as u64,
        total_elapsed_ms = merge_start.elapsed().as_millis() as u64,
        "proof generated successfully"
    );

    // Read proof bytes
    let proof_bytes =
        std::fs::read(&proof_path).map_err(|e| format!("failed to read proof file: {}", e))?;
    let public_inputs_bytes = std::fs::read(&public_inputs_path)
        .map_err(|e| format!("failed to read public inputs file: {}", e))?;
    let public_inputs: Vec<String> = serde_json::from_slice(&public_inputs_bytes)
        .map_err(|e| format!("failed to parse public inputs json: {}", e))?;

    Ok((proof_bytes, public_inputs))
}

/// Read completed proof bytes from disk.
pub fn get_proof(session: &MpcSessionState) -> Result<Vec<u8>, String> {
    let proof_path = session
        .proof_path
        .as_ref()
        .ok_or("proof not yet generated")?;

    std::fs::read(proof_path).map_err(|e| format!("failed to read proof: {}", e))
}
