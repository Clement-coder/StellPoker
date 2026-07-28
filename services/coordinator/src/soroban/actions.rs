use std::collections::HashSet;

use tokio::process::Command;

use super::{
    invoke_committee_registry_with_source, invoke_contract_with_retries,
    invoke_contract_with_source_retries, parse_i128_value, parse_tx_result,
    parse_u32_from_stdout, parse_u32_value, resolve_onchain_table_id, SorobanConfig,
};
use crate::key_rotation::CommitteeKey;

fn resolve_buy_in_from_table_state(state: &serde_json::Value, requested: i128) -> i128 {
    let min_buy_in = state
        .get("config")
        .and_then(|cfg| cfg.get("min_buy_in"))
        .and_then(parse_i128_value)
        .unwrap_or(requested.max(1));
    let max_buy_in = state
        .get("config")
        .and_then(|cfg| cfg.get("max_buy_in"))
        .and_then(parse_i128_value)
        .unwrap_or(min_buy_in);

    if min_buy_in <= max_buy_in {
        requested.clamp(min_buy_in, max_buy_in)
    } else {
        requested
    }
}

fn friendbot_url(config: &SorobanConfig) -> Option<String> {
    if let Ok(url) = std::env::var("FRIENDBOT_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if config.rpc_url.contains("localhost:8000") || config.rpc_url.contains("127.0.0.1:8000") {
        return Some("http://localhost:8000/friendbot".to_string());
    }

    None
}

async fn maybe_friendbot_top_up(config: &SorobanConfig, address: &str) {
    let Some(base) = friendbot_url(config) else {
        return;
    };
    let url = format!("{}?addr={}", base, address);
    match Command::new("curl").args(["-sfL", &url]).output().await {
        Ok(output) if output.status.success() => {
            tracing::info!("friendbot topped up {}", address);
        }
        Ok(output) => {
            tracing::warn!(
                "friendbot top-up failed for {}: {}",
                address,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Err(e) => {
            tracing::warn!("friendbot top-up failed for {}: {}", address, e);
        }
    }
}

fn looks_like_insufficient_balance(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    e.contains("resulting balance is not within the allowed range")
        || (e.contains("error(contract, #10)") && e.contains("transfer"))
}

/// Register a freshly-rotated committee identity with the on-chain
/// committee-registry (Issue #102).
///
/// `register_member` requires the *new* key's own authorization and stakes
/// `min_stake` of the configured token from its balance, so the address is
/// friendbot-funded first on networks where that's available (local/testnet;
/// a no-op elsewhere, matching the funding behavior already used for player
/// buy-ins in this module).
pub async fn register_rotated_committee_key(
    config: &SorobanConfig,
    new_key: &CommitteeKey,
    min_stake: i128,
    endpoint: &str,
    region: &str,
) -> Result<String, String> {
    maybe_friendbot_top_up(config, &new_key.address).await;

    let args = vec![
        "register_member".to_string(),
        "--member".to_string(),
        new_key.address.clone(),
        "--stake".to_string(),
        min_stake.to_string(),
        "--endpoint".to_string(),
        endpoint.to_string(),
        "--region".to_string(),
        region.to_string(),
    ];

    let output = invoke_committee_registry_with_source(config, &new_key.secret_key, args).await?;
    parse_tx_result(output)
}

/// Deregister a retiring committee identity once its overlap window has
/// elapsed, reclaiming its staked bond (Issue #102).
///
/// Per the contract, this fails while the retiring address is still listed
/// in the current epoch's member set — callers must rotate the active epoch
/// (or wait for it to naturally roll to one excluding the retiring key)
/// before this succeeds.
pub async fn deregister_rotated_committee_key(
    config: &SorobanConfig,
    retiring_key: &CommitteeKey,
) -> Result<String, String> {
    let args = vec![
        "deregister_member".to_string(),
        "--member".to_string(),
        retiring_key.address.clone(),
    ];

    let output =
        invoke_committee_registry_with_source(config, &retiring_key.secret_key, args).await?;
    parse_tx_result(output)
}

/// When reveal is requested directly from the frontend, advance one legal betting
/// action if the on-chain table is still in a betting phase.
pub async fn maybe_auto_advance_betting_for_reveal(
    config: &SorobanConfig,
    table_id: u32,
    reveal_phase: &str,
) -> Result<(), String> {
    if !config.is_configured() {
        return Ok(());
    }

    let expected = match reveal_phase {
        "flop" => "Preflop",
        "turn" => "Flop",
        "river" => "Turn",
        _ => return Ok(()),
    };

    maybe_auto_advance_betting_if_phase(config, table_id, expected, "reveal").await
}

/// When showdown is requested directly from the frontend, advance one legal
/// betting action if the on-chain table is still in River betting.
pub async fn maybe_auto_advance_betting_for_showdown(
    config: &SorobanConfig,
    table_id: u32,
) -> Result<(), String> {
    if !config.is_configured() {
        return Ok(());
    }
    maybe_auto_advance_betting_if_phase(config, table_id, "River", "showdown").await
}

async fn maybe_auto_advance_betting_if_phase(
    config: &SorobanConfig,
    table_id: u32,
    expected_phase: &str,
    reason: &str,
) -> Result<(), String> {
    const MAX_AUTO_ACTIONS: usize = 24;

    for step in 0..MAX_AUTO_ACTIONS {
        let state_raw = get_table_state(config, table_id).await?;
        let state: serde_json::Value = serde_json::from_str(&state_raw)
            .map_err(|e| format!("failed to parse on-chain table state: {}", e))?;

        let phase = state
            .get("phase")
            .and_then(|v| v.as_str())
            .ok_or("missing phase in on-chain table state")?;

        if phase != expected_phase {
            return Ok(());
        }

        let players = state
            .get("players")
            .and_then(|v| v.as_array())
            .ok_or("missing players in on-chain table state")?;
        let current_turn = state
            .get("current_turn")
            .and_then(|v| v.as_u64())
            .ok_or("missing current_turn in on-chain table state")?
            as usize;

        let current_player = players
            .get(current_turn)
            .ok_or("current_turn out of range for on-chain players")?;
        let player_address = current_player
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or("missing current player address")?;
        let source_identity = config.identity_for_player(player_address).ok_or_else(|| {
            format!(
                "no local identity configured for player {} (set PLAYERn_ADDRESS/PLAYERn_IDENTITY)",
                player_address
            )
        })?;

        let current_bet = players
            .iter()
            .filter_map(|p| p.get("bet_this_round"))
            .filter_map(parse_i128_value)
            .max()
            .unwrap_or(0);
        let my_bet = current_player
            .get("bet_this_round")
            .and_then(parse_i128_value)
            .unwrap_or(0);

        let action_json = if my_bet < current_bet {
            "\"Call\""
        } else {
            "\"Check\""
        };
        let onchain_table_id = resolve_onchain_table_id(config, table_id);
        tracing::info!(
            "Auto-advancing betting before {}: phase={}, action={}, player={}, step={}",
            reason,
            phase,
            action_json,
            player_address,
            step + 1
        );

        let output = invoke_contract_with_source_retries(
            config,
            source_identity,
            vec![
                "player_action".to_string(),
                "--table_id".to_string(),
                onchain_table_id.to_string(),
                "--player".to_string(),
                player_address.to_string(),
                "--action".to_string(),
                action_json.to_string(),
            ],
        )
        .await?;

        parse_tx_result(output)?;
    }

    Err(format!(
        "auto-advance before {} exceeded {} actions while phase remained {}",
        reason, MAX_AUTO_ACTIONS, expected_phase
    ))
}

/// Submit a player betting action to the on-chain table contract.
///
/// The source identity is resolved from configured PLAYERn_ADDRESS/PLAYERn_IDENTITY.
pub async fn submit_rit_opt_in(
    config: &SorobanConfig,
    table_id: u32,
    player_address: &str,
    opt_in: bool,
) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }

    let source_identity = config.identity_for_player(player_address).ok_or_else(|| {
        format!(
            "no local identity configured for player {} (set PLAYERn_ADDRESS/PLAYERn_IDENTITY)",
            player_address
        )
    })?;

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let output = invoke_contract_with_source_retries(
        config,
        source_identity,
        vec![
            "rit_opt_in".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--player".to_string(),
            player_address.to_string(),
            "--opt_in".to_string(),
            opt_in.to_string(),
        ],
    )
    .await?;

    parse_tx_result(output)
}

pub async fn submit_player_action(
    config: &SorobanConfig,
    table_id: u32,
    player_address: &str,
    action: &str,
    amount: Option<i128>,
) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }

    let source_identity = config.identity_for_player(player_address).ok_or_else(|| {
        format!(
            "no local identity configured for player {} (set PLAYERn_ADDRESS/PLAYERn_IDENTITY)",
            player_address
        )
    })?;

    let action_lower = action.to_ascii_lowercase();
    let action_json = match action_lower.as_str() {
        "fold" => "\"Fold\"".to_string(),
        "check" => "\"Check\"".to_string(),
        "call" => "\"Call\"".to_string(),
        "allin" | "all_in" => "\"AllIn\"".to_string(),
        "bet" => {
            let value = amount.ok_or("bet requires amount")?;
            if value <= 0 {
                return Err("bet amount must be positive".to_string());
            }
            format!("{{\"Bet\":\"{}\"}}", value)
        }
        "raise" => {
            let value = amount.ok_or("raise requires amount")?;
            if value <= 0 {
                return Err("raise amount must be positive".to_string());
            }
            format!("{{\"Raise\":\"{}\"}}", value)
        }
        _ => return Err(format!("unsupported action '{}'", action)),
    };

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let output = invoke_contract_with_source_retries(
        config,
        source_identity,
        vec![
            "player_action".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--player".to_string(),
            player_address.to_string(),
            "--action".to_string(),
            action_json,
        ],
    )
    .await?;

    parse_tx_result(output)
}

/// Submit a timeout claim to force committee-failure settlement when a hand is stuck.
pub async fn claim_timeout(config: &SorobanConfig, table_id: u32) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let claimer = config.committee_address()?;
    let output = invoke_contract_with_retries(
        config,
        vec![
            "claim_timeout".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--claimer".to_string(),
            claimer,
        ],
    )
    .await?;

    parse_tx_result(output)
}

/// Create a new table by cloning the reference table config.
pub async fn create_seeded_table(
    config: &SorobanConfig,
    reference_table_id: u32,
    max_players: u32,
    buy_in_override: Option<i128>,
) -> Result<u32, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }
    if !(2..=6).contains(&max_players) {
        return Err(format!("max_players out of range: {}", max_players));
    }

    let raw = get_table_state(config, reference_table_id).await?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("failed to parse reference table: {}", e))?;
    let mut cfg = value
        .get("config")
        .cloned()
        .ok_or("reference table missing config")?;

    if let Some(obj) = cfg.as_object_mut() {
        obj.insert(
            "max_players".to_string(),
            serde_json::Value::Number(serde_json::Number::from(max_players)),
        );
        obj.entry("min_players".to_string())
            .or_insert_with(|| serde_json::Value::Number(serde_json::Number::from(2u32)));
        obj.insert(
            "committee".to_string(),
            serde_json::Value::String(config.committee_address()?),
        );
        if let Some(buy_in) = buy_in_override {
            if buy_in <= 0 {
                return Err(format!("buy_in must be > 0 (got {})", buy_in));
            }
            // Enforce exact buy-in for newly created tables when requested.
            obj.insert(
                "min_buy_in".to_string(),
                serde_json::Value::String(buy_in.to_string()),
            );
            obj.insert(
                "max_buy_in".to_string(),
                serde_json::Value::String(buy_in.to_string()),
            );
        }
    } else {
        return Err("reference config is not an object".to_string());
    }
    let cfg_json = serde_json::to_string(&cfg)
        .map_err(|e| format!("failed to serialize table config: {}", e))?;

    let committee_addr = config.committee_address()?;
    let output = invoke_contract_with_retries(
        config,
        vec![
            "create_table".to_string(),
            "--admin".to_string(),
            committee_addr,
            "--config".to_string(),
            cfg_json,
        ],
    )
    .await?;

    if !output.status.success() {
        return Err(format!(
            "create_table failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let table_id = parse_u32_from_stdout(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "failed to parse table id from create_table output".to_string())?;

    Ok(table_id)
}

/// Join the next unseated configured local identity to the table.
pub async fn join_next_available_local_player(
    config: &SorobanConfig,
    table_id: u32,
    buy_in: i128,
) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }

    let state_raw = get_table_state(config, table_id).await?;
    let state: serde_json::Value = serde_json::from_str(&state_raw)
        .map_err(|e| format!("failed to parse on-chain table state: {}", e))?;

    let phase = state
        .get("phase")
        .and_then(|v| v.as_str())
        .ok_or("missing phase in on-chain table state")?;
    if phase != "Waiting" {
        return Err(format!(
            "table {} is not accepting joins (phase={})",
            table_id, phase
        ));
    }

    let players = state
        .get("players")
        .and_then(|v| v.as_array())
        .ok_or("missing players in on-chain table state")?;
    let max_players = state
        .get("config")
        .and_then(|cfg| cfg.get("max_players"))
        .and_then(parse_u32_value)
        .unwrap_or(players.len() as u32);
    if players.len() as u32 >= max_players {
        return Err(format!("table {} is full", table_id));
    }

    let seated: HashSet<String> = players
        .iter()
        .filter_map(|p| p.get("address").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    let (player_address, identity) = config
        .player_identities
        .iter()
        .find(|(address, _)| !seated.contains(address))
        .ok_or("no unseated local identity available")?;

    let resolved_buy_in = resolve_buy_in_from_table_state(&state, buy_in);
    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let join_args = vec![
        "join_table".to_string(),
        "--table_id".to_string(),
        onchain_table_id.to_string(),
        "--player".to_string(),
        player_address.clone(),
        "--buy_in".to_string(),
        resolved_buy_in.to_string(),
    ];

    // Keep local identities liquid so repeated solo-table creation does not
    // fail with transfer underflow after many test hands/tables.
    maybe_friendbot_top_up(config, player_address).await;

    let output = invoke_contract_with_source_retries(config, identity, join_args.clone()).await?;
    if let Err(first_error) = parse_tx_result(output) {
        if looks_like_insufficient_balance(&first_error) {
            tracing::warn!(
                "join_table for {} failed due to balance; topping up and retrying once",
                player_address
            );
            maybe_friendbot_top_up(config, player_address).await;
            let retry_output =
                invoke_contract_with_source_retries(config, identity, join_args).await?;
            parse_tx_result(retry_output)?;
        } else {
            return Err(first_error);
        }
    }

    Ok(player_address.clone())
}

/// Join one configured local identity as a deterministic "bot" seat for solo mode.
pub async fn join_single_bot_player(
    config: &SorobanConfig,
    table_id: u32,
    buy_in: i128,
) -> Result<String, String> {
    join_next_available_local_player(config, table_id, buy_in).await
}

/// Read on-chain table state via `stellar contract invoke -- get_table`.
pub async fn get_table_state(config: &SorobanConfig, table_id: u32) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let output = Command::new("stellar")
        .args([
            "contract",
            "invoke",
            "--id",
            &config.poker_table_contract,
            "--source",
            &config.secret_key,
            "--rpc-url",
            &config.rpc_url,
            "--network-passphrase",
            &config.network_passphrase,
            "--",
            "get_table",
            "--table_id",
            &onchain_table_id.to_string(),
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to invoke stellar CLI: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Read a paginated slice of seated players via `get_players_paginated`.
pub async fn get_players_paginated(
    config: &SorobanConfig,
    table_id: u32,
    offset: u32,
    limit: u32,
) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }
    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let output = invoke_contract_with_retries(
        config,
        vec![
            "get_players_paginated".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--offset".to_string(),
            offset.to_string(),
            "--limit".to_string(),
            limit.to_string(),
        ],
    )
    .await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Read the total player count via `get_player_count`.
pub async fn get_player_count(
    config: &SorobanConfig,
    table_id: u32,
) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }
    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let output = invoke_contract_with_retries(
        config,
        vec![
            "get_player_count".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
        ],
    )
    .await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Read a paginated chunk of hand history via `get_hand_history_chunk`.
pub async fn get_hand_history_chunk(
    config: &SorobanConfig,
    table_id: u32,
    offset: u32,
    limit: u32,
) -> Result<String, String> {
    if !config.is_configured() {
        return Err("Soroban not configured".to_string());
    }
    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let output = invoke_contract_with_retries(
        config,
        vec![
            "get_hand_history_chunk".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--offset".to_string(),
            offset.to_string(),
            "--limit".to_string(),
            limit.to_string(),
        ],
    )
    .await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
