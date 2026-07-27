//! Soroban on-chain proof submission via `stellar contract invoke`.
//!
//! Shells out to the Stellar CLI to submit proofs and game state to
//! the on-chain poker-table contract. Uses the same `tokio::process::Command`
//! pattern as `mpc.rs` for co-noir subprocess execution.

mod actions;
mod proofs;

pub use actions::*;
pub use proofs::*;

use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Configuration for Soroban interactions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SorobanConfig {
    pub rpc_url: String,
    pub secret_key: String,
    pub poker_table_contract: String,
    pub committee_registry_contract: String,
    pub network_passphrase: String,
    pub onchain_table_id: Option<u32>,
    pub player_identities: Vec<(String, String)>,
    /// Stake (in the configured token's smallest unit) a rotated committee
    /// identity registers with when `key_rotation` cuts over to it.
    pub committee_member_min_stake: i128,
    /// MPC node HTTP endpoint advertised when (re-)registering the committee
    /// identity after a key rotation.
    pub committee_member_endpoint: String,
    /// Geographic region advertised when (re-)registering the committee
    /// identity after a key rotation.
    pub committee_member_region: String,
}

impl SorobanConfig {
    pub fn from_env() -> Self {
        let mut player_identities = Vec::new();
        for idx in 1..=6 {
            let address_key = format!("PLAYER{}_ADDRESS", idx);
            if let Ok(address) = std::env::var(&address_key) {
                if address.trim().is_empty() {
                    continue;
                }
                let identity_key = format!("PLAYER{}_IDENTITY", idx);
                let identity =
                    std::env::var(&identity_key).unwrap_or_else(|_| format!("player{}-local", idx));
                player_identities.push((address, identity));
            }
        }

        Self {
            rpc_url: std::env::var("SOROBAN_RPC")
                .unwrap_or_else(|_| "http://localhost:8000/soroban/rpc".to_string()),
            secret_key: std::env::var("COMMITTEE_SECRET")
                .unwrap_or_else(|_| "test_secret".to_string()),
            poker_table_contract: std::env::var("POKER_TABLE_CONTRACT")
                .unwrap_or_else(|_| String::new()),
            committee_registry_contract: std::env::var("COMMITTEE_REGISTRY_CONTRACT")
                .unwrap_or_else(|_| String::new()),
            network_passphrase: std::env::var("NETWORK_PASSPHRASE")
                .unwrap_or_else(|_| "Test SDF Network ; September 2015".to_string()),
            onchain_table_id: std::env::var("ONCHAIN_TABLE_ID")
                .ok()
                .or_else(|| std::env::var("TABLE_ID").ok())
                .and_then(|s| s.parse().ok()),
            player_identities,
            committee_member_min_stake: std::env::var("COMMITTEE_MEMBER_MIN_STAKE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            committee_member_endpoint: std::env::var("COMMITTEE_MEMBER_ENDPOINT")
                .unwrap_or_default(),
            committee_member_region: std::env::var("COMMITTEE_MEMBER_REGION")
                .unwrap_or_else(|_| "us-east-1".to_string()),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.poker_table_contract.is_empty() && self.secret_key != "test_secret"
    }

    /// Derive the Stellar public address (G...) from the committee secret key (S...).
    pub fn committee_address(&self) -> Result<String, String> {
        let sk = stellar_strkey::ed25519::PrivateKey::from_string(&self.secret_key)
            .map_err(|e| format!("invalid committee secret key: {:?}", e))?;
        let signing_key = SigningKey::from_bytes(&sk.0);
        let public_key = signing_key.verifying_key().to_bytes();
        Ok(stellar_strkey::ed25519::PublicKey(public_key).to_string())
    }

    pub(crate) fn identity_for_player(&self, player_address: &str) -> Option<&str> {
        self.player_identities
            .iter()
            .find(|(address, _)| address == player_address)
            .map(|(_, identity)| identity.as_str())
    }

    pub fn has_identity_for_player(&self, player_address: &str) -> bool {
        self.identity_for_player(player_address).is_some()
    }
}

const INSTRUCTION_LEEWAY_STEPS: [u64; 4] = [0, 50_000_000, 200_000_000, 500_000_000];

fn is_transient_invoke_error(output: &std::process::Output) -> bool {
    if output.status.success() {
        return false;
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    stderr.contains("resourcelimitexceeded")
        || stderr.contains("connection reset by peer")
        || stderr.contains("timed out")
        || stderr.contains("timeout")
        || stderr.contains("temporarily unavailable")
        || stderr.contains("networking or low-level protocol error")
}

pub(crate) async fn invoke_contract_with_retries(
    config: &SorobanConfig,
    contract_args: Vec<String>,
) -> Result<std::process::Output, String> {
    let mut last_output: Option<std::process::Output> = None;

    for (attempt_idx, leeway) in INSTRUCTION_LEEWAY_STEPS.iter().enumerate() {
        let mut args: Vec<String> = vec![
            "contract".to_string(),
            "invoke".to_string(),
            "--id".to_string(),
            config.poker_table_contract.clone(),
            "--source".to_string(),
            config.secret_key.clone(),
            "--rpc-url".to_string(),
            config.rpc_url.clone(),
            "--network-passphrase".to_string(),
            config.network_passphrase.clone(),
        ];

        if *leeway > 0 {
            args.push("--instruction-leeway".to_string());
            args.push(leeway.to_string());
        }

        args.push("--".to_string());
        args.extend(contract_args.iter().cloned());

        let output = Command::new("stellar")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Failed to invoke stellar CLI: {}", e))?;

        if output.status.success() {
            return Ok(output);
        }

        let is_resource_limit = is_transient_invoke_error(&output)
            && String::from_utf8_lossy(&output.stderr).contains("ResourceLimitExceeded");
        let has_next_attempt = attempt_idx + 1 < INSTRUCTION_LEEWAY_STEPS.len();

        if is_resource_limit && has_next_attempt {
            tracing::warn!(
                "stellar invoke hit ResourceLimitExceeded; retrying with higher instruction leeway (attempt {}/{})",
                attempt_idx + 1,
                INSTRUCTION_LEEWAY_STEPS.len()
            );
            last_output = Some(output);
            continue;
        }

        return Ok(output);
    }

    last_output.ok_or_else(|| "stellar invoke failed before any attempt completed".to_string())
}

pub(crate) fn resolve_onchain_table_id(config: &SorobanConfig, table_id: u32) -> u32 {
    if table_id == 0 {
        config.onchain_table_id.unwrap_or(0)
    } else {
        table_id
    }
}

pub(crate) async fn invoke_contract_with_source(
    config: &SorobanConfig,
    source: &str,
    contract_args: Vec<String>,
) -> Result<std::process::Output, String> {
    let mut args: Vec<String> = vec![
        "contract".to_string(),
        "invoke".to_string(),
        "--id".to_string(),
        config.poker_table_contract.clone(),
        "--source".to_string(),
        source.to_string(),
        "--rpc-url".to_string(),
        config.rpc_url.clone(),
        "--network-passphrase".to_string(),
        config.network_passphrase.clone(),
        "--".to_string(),
    ];
    args.extend(contract_args);

    Command::new("stellar")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to invoke stellar CLI: {}", e))
}

/// Like [`invoke_contract_with_source`], but targets the committee-registry
/// contract instead of the poker-table contract. Used for committee
/// membership operations (registration, deregistration, key rotation).
pub(crate) async fn invoke_committee_registry_with_source(
    config: &SorobanConfig,
    source: &str,
    contract_args: Vec<String>,
) -> Result<std::process::Output, String> {
    if config.committee_registry_contract.is_empty() {
        return Err("Committee Registry contract address not configured".to_string());
    }

    let mut args: Vec<String> = vec![
        "contract".to_string(),
        "invoke".to_string(),
        "--id".to_string(),
        config.committee_registry_contract.clone(),
        "--source".to_string(),
        source.to_string(),
        "--rpc-url".to_string(),
        config.rpc_url.clone(),
        "--network-passphrase".to_string(),
        config.network_passphrase.clone(),
        "--".to_string(),
    ];
    args.extend(contract_args);

    Command::new("stellar")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to invoke stellar CLI: {}", e))
}

pub(crate) async fn invoke_contract_with_source_retries(
    config: &SorobanConfig,
    source: &str,
    contract_args: Vec<String>,
) -> Result<std::process::Output, String> {
    const MAX_RETRIES: usize = 3;
    let mut last_output: Option<std::process::Output> = None;

    for attempt in 1..=MAX_RETRIES {
        let output = invoke_contract_with_source(config, source, contract_args.clone()).await?;
        if output.status.success() {
            return Ok(output);
        }

        let should_retry = is_transient_invoke_error(&output) && attempt < MAX_RETRIES;
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        tracing::warn!(
            "stellar invoke (source={}, attempt {}/{}) failed{}: {}",
            source,
            attempt,
            MAX_RETRIES,
            if should_retry { ", retrying" } else { "" },
            stderr.trim()
        );

        if !should_retry {
            return Ok(output);
        }

        last_output = Some(output);
        tokio::time::sleep(std::time::Duration::from_millis(300 * attempt as u64)).await;
    }

    last_output.ok_or_else(|| "stellar invoke failed before any attempt completed".to_string())
}

pub(crate) fn parse_i128_value(value: &serde_json::Value) -> Option<i128> {
    match value {
        serde_json::Value::String(s) => s.parse::<i128>().ok(),
        serde_json::Value::Number(n) => n.as_i64().map(|v| v as i128),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitteeMember {
    pub address: String,
    pub stake: i128,
    pub endpoint: String,
    pub region: String,
    pub active: bool,
    pub slash_count: u32,
}

/// Fetch all active committee members from the CommitteeRegistry contract.
pub async fn fetch_active_nodes_from_registry(
    config: &SorobanConfig,
) -> Result<Vec<CommitteeMember>, String> {
    if config.committee_registry_contract.is_empty() {
        return Err("Committee Registry contract address not configured".to_string());
    }

    let mut args: Vec<String> = vec![
        "contract".to_string(),
        "invoke".to_string(),
        "--id".to_string(),
        config.committee_registry_contract.clone(),
        "--source".to_string(),
        config.secret_key.clone(),
        "--rpc-url".to_string(),
        config.rpc_url.clone(),
        "--network-passphrase".to_string(),
        config.network_passphrase.clone(),
        "--".to_string(),
        "get_active_members".to_string(),
    ];

    let output = Command::new("stellar")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to invoke stellar CLI: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to fetch active members: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse members JSON: {}. Stdout: {}", e, stdout))
}

pub(crate) fn parse_u32_value(value: &serde_json::Value) -> Option<u32> {
    match value {
        serde_json::Value::String(s) => s.parse::<u32>().ok(),
        serde_json::Value::Number(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
        _ => None,
    }
}

/// Parse the tx hash from stellar CLI output.
pub(crate) fn parse_tx_result(output: std::process::Output) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(format!("stellar contract invoke failed: {}", stderr.trim()));
    }

    // The stellar CLI prints the tx hash or result to stdout
    let tx_hash = stdout.trim().to_string();
    if tx_hash.is_empty() {
        // Some versions don't print a hash on success
        Ok("submitted".to_string())
    } else {
        Ok(tx_hash)
    }
}

fn parse_u32_from_stdout(stdout: &str) -> Option<u32> {
    for line in stdout.lines().rev() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Ok(v) = t.parse::<u32>() {
            return Some(v);
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(t) {
            if let Some(v) = json.as_u64().and_then(|n| u32::try_from(n).ok()) {
                return Some(v);
            }
            if let Some(v) = json
                .get("u32")
                .and_then(|n| n.as_u64())
                .and_then(|n| u32::try_from(n).ok())
            {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod error_handling_tests {
    //! Coverage for the **Soroban RPC timeout** and **on-chain state-store
    //! connection failure** error paths.
    //!
    //! The coordinator has no SQL database: all durable game state lives
    //! on-chain and is read/written through the Stellar RPC via the `stellar`
    //! CLI. A dropped RPC connection or a request timeout is therefore this
    //! service's equivalent of a "database connection failure". Such errors
    //! must be classified as *transient* so the invoke layer retries, while
    //! genuine contract-logic errors must NOT be retried.
    use super::*;

    fn config() -> SorobanConfig {
        SorobanConfig {
            rpc_url: "http://localhost:8000/soroban/rpc".to_string(),
            secret_key: "test_secret".to_string(),
            poker_table_contract: String::new(),
            committee_registry_contract: String::new(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            onchain_table_id: None,
            player_identities: Vec::new(),
            committee_member_min_stake: 0,
            committee_member_endpoint: String::new(),
            committee_member_region: "us-east-1".to_string(),
        }
    }

    // `std::process::Output` cannot be built portably; on unix we synthesize an
    // exit status from a raw wait code (0 = success, 256 = exit code 1).
    #[cfg(unix)]
    fn failed_output(stderr: &str) -> std::process::Output {
        use std::os::unix::process::ExitStatusExt;
        std::process::Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[cfg(unix)]
    fn success_output(stdout: &str) -> std::process::Output {
        use std::os::unix::process::ExitStatusExt;
        std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn rpc_timeout_is_classified_transient() {
        for msg in [
            "transaction simulation failed: timed out",
            "request timeout after 30s",
            "error: resource temporarily unavailable",
            "networking or low-level protocol error: stream closed",
        ] {
            assert!(
                is_transient_invoke_error(&failed_output(msg)),
                "expected transient classification for: {msg}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn rpc_connection_reset_is_transient() {
        // The on-chain state store dropped the connection -- treat as transient.
        assert!(is_transient_invoke_error(&failed_output(
            "error sending request: connection reset by peer"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn resource_limit_is_transient() {
        assert!(is_transient_invoke_error(&failed_output(
            "HostError: ResourceLimitExceeded"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn genuine_contract_error_is_not_retried() {
        // A real contract-logic failure must NOT be classified transient --
        // retrying it would be pointless and could double-submit.
        assert!(!is_transient_invoke_error(&failed_output(
            "HostError: Error(Contract, #5) NotAuthorizedCommittee"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn successful_output_is_never_transient() {
        assert!(!is_transient_invoke_error(&success_output("ok")));
    }

    #[cfg(unix)]
    #[test]
    fn parse_tx_result_propagates_invoke_failure() {
        let err = parse_tx_result(failed_output("rpc unreachable")).unwrap_err();
        assert!(err.contains("stellar contract invoke failed"), "got: {err}");
        assert!(err.contains("rpc unreachable"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn parse_tx_result_returns_hash_on_success() {
        assert_eq!(
            parse_tx_result(success_output("deadbeefhash")).unwrap(),
            "deadbeefhash"
        );
        // Some CLI versions print nothing on success.
        assert_eq!(parse_tx_result(success_output("   ")).unwrap(), "submitted");
    }

    #[test]
    fn committee_address_rejects_invalid_secret() {
        let err = config().committee_address().unwrap_err();
        assert!(err.contains("invalid committee secret key"), "got: {err}");
    }

    #[test]
    fn unconfigured_soroban_is_reported() {
        // Default secret + empty contract => not configured (submissions skipped).
        assert!(!config().is_configured());
    }

    #[test]
    fn resolve_onchain_table_id_prefers_explicit_then_fallback() {
        let mut cfg = config();
        assert_eq!(resolve_onchain_table_id(&cfg, 7), 7);
        assert_eq!(resolve_onchain_table_id(&cfg, 0), 0);
        cfg.onchain_table_id = Some(42);
        assert_eq!(resolve_onchain_table_id(&cfg, 0), 42);
    }

    #[test]
    fn value_parsers_reject_malformed_values() {
        assert_eq!(parse_i128_value(&serde_json::json!("123")), Some(123));
        assert_eq!(parse_i128_value(&serde_json::json!(45)), Some(45));
        assert_eq!(parse_i128_value(&serde_json::json!("not-a-number")), None);
        assert_eq!(parse_i128_value(&serde_json::json!(true)), None);
        assert_eq!(parse_u32_value(&serde_json::json!("9")), Some(9));
        assert_eq!(parse_u32_value(&serde_json::json!(-1)), None);
        assert_eq!(parse_u32_value(&serde_json::json!("xx")), None);
    }
}
