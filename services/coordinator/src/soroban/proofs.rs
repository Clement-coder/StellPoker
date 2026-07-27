use std::str::FromStr;

use ark_bn254::Fr;
use ark_ff::{BigInteger, PrimeField};

use super::{
    invoke_contract_with_retries, parse_tx_result, resolve_onchain_table_id, SorobanConfig,
};

/// Submit a deal proof to the on-chain poker-table contract via `commit_deal`.
pub async fn submit_deal_proof(
    config: &SorobanConfig,
    table_id: u32,
    proof: &[u8],
    public_inputs: &[String],
    deck_root: &str,
    hand_commitments: &[String],
) -> Result<String, String> {
    if !config.is_configured() {
        tracing::warn!("Soroban not configured, skipping deal proof submission");
        return Ok(String::new());
    }

    // Issue #108: content-addressable cache of already-verified proofs, so a
    // retried submission of byte-identical proof data skips the (expensive)
    // on-chain re-verification and just replays the prior tx hash.
    let cache_key = crate::proof_cache::proof_key(proof, public_inputs);
    if let Some(tx_hash) = crate::proof_cache::get(&cache_key).await {
        tracing::info!("Deal proof cache hit — skipping redundant on-chain verification");
        return Ok(tx_hash);
    }

    maybe_start_hand_for_deal(config, table_id).await?;

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let committee_addr = config.committee_address()?;
    let converted_proof = convert_keccak_proof_to_soroban(proof)?;
    let proof_hex = hex::encode(&converted_proof);
    let pi_hex = public_inputs_to_hex(public_inputs)?;
    let deck_root_hex = field_to_bytes32_hex(deck_root)?;
    let commitments_hex_json = fields_to_bytes32_json(hand_commitments)?;

    tracing::info!(
        "Soroban deal proof: raw_bytes={}, converted_bytes={}, public_inputs_count={}, pi_hex_bytes={}, deck_root_hex={}, commitments_json={}",
        proof.len(),
        converted_proof.len(),
        public_inputs.len(),
        pi_hex.len() / 2,
        deck_root_hex,
        commitments_hex_json,
    );

    let output = invoke_contract_with_retries(
        config,
        vec![
            "commit_deal".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--committee".to_string(),
            committee_addr,
            "--deck_root".to_string(),
            deck_root_hex,
            "--hand_commitments".to_string(),
            commitments_hex_json,
            "--dealt_indices".to_string(),
            "[]".to_string(),
            "--proof".to_string(),
            proof_hex,
            "--public_inputs".to_string(),
            pi_hex,
        ],
    )
    .await?;

    let tx_hash = parse_tx_result(output)?;
    crate::proof_cache::insert(cache_key, tx_hash.clone()).await;
    Ok(tx_hash)
}

async fn maybe_start_hand_for_deal(config: &SorobanConfig, table_id: u32) -> Result<(), String> {
    let state_raw = super::get_table_state(config, table_id).await?;
    let state: serde_json::Value = serde_json::from_str(&state_raw)
        .map_err(|e| format!("failed to parse on-chain table state: {}", e))?;

    let phase = state
        .get("phase")
        .and_then(|v| v.as_str())
        .ok_or("missing phase in on-chain table state")?;

    match phase {
        "Dealing" => return Ok(()),
        "Waiting" | "Settlement" => {}
        _ => {
            return Err(format!(
                "table {} not ready for new deal; current phase is {}",
                table_id, phase
            ))
        }
    }

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    tracing::info!(
        "Auto-starting hand before deal submission: table_id={}, phase={}",
        onchain_table_id,
        phase
    );
    let output = invoke_contract_with_retries(
        config,
        vec![
            "start_hand".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
        ],
    )
    .await?;
    parse_tx_result(output).map(|_| ())
}

/// Submit a reveal proof to the on-chain poker-table contract via `reveal_board`.
pub async fn submit_reveal_proof(
    config: &SorobanConfig,
    table_id: u32,
    proof: &[u8],
    public_inputs: &[String],
    cards: &[u32],
    indices: &[u32],
) -> Result<String, String> {
    if !config.is_configured() {
        tracing::warn!("Soroban not configured, skipping reveal proof submission");
        return Ok(String::new());
    }

    let cache_key = crate::proof_cache::proof_key(proof, public_inputs);
    if let Some(tx_hash) = crate::proof_cache::get(&cache_key).await {
        tracing::info!("Reveal proof cache hit — skipping redundant on-chain verification");
        return Ok(tx_hash);
    }

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let committee_addr = config.committee_address()?;
    let converted_proof = convert_keccak_proof_to_soroban(proof)?;
    let proof_hex = hex::encode(&converted_proof);
    let pi_hex = public_inputs_to_hex(public_inputs)?;
    let cards_json =
        serde_json::to_string(cards).map_err(|e| format!("Failed to serialize cards: {}", e))?;
    let indices_json = serde_json::to_string(indices)
        .map_err(|e| format!("Failed to serialize indices: {}", e))?;

    let output = invoke_contract_with_retries(
        config,
        vec![
            "reveal_board".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--committee".to_string(),
            committee_addr,
            "--cards".to_string(),
            cards_json,
            "--indices".to_string(),
            indices_json,
            "--proof".to_string(),
            proof_hex,
            "--public_inputs".to_string(),
            pi_hex,
        ],
    )
    .await?;

    let tx_hash = parse_tx_result(output)?;
    crate::proof_cache::insert(cache_key, tx_hash.clone()).await;
    Ok(tx_hash)
}

/// Submit a showdown proof to the on-chain poker-table contract via `submit_showdown`.
pub async fn submit_showdown_proof(
    config: &SorobanConfig,
    table_id: u32,
    proof: &[u8],
    public_inputs: &[String],
    hole_cards: &[(u32, u32)],
) -> Result<String, String> {
    if !config.is_configured() {
        tracing::warn!("Soroban not configured, skipping showdown proof submission");
        return Ok(String::new());
    }

    let cache_key = crate::proof_cache::proof_key(proof, public_inputs);
    if let Some(tx_hash) = crate::proof_cache::get(&cache_key).await {
        tracing::info!("Showdown proof cache hit — skipping redundant on-chain verification");
        return Ok(tx_hash);
    }

    let onchain_table_id = resolve_onchain_table_id(config, table_id);
    let committee_addr = config.committee_address()?;
    let converted_proof = convert_keccak_proof_to_soroban(proof)?;
    let proof_hex = hex::encode(&converted_proof);
    let pi_hex = public_inputs_to_hex(public_inputs)?;
    let hole_cards_json = serde_json::to_string(hole_cards)
        .map_err(|e| format!("Failed to serialize hole cards: {}", e))?;

    let output = invoke_contract_with_retries(
        config,
        vec![
            "submit_showdown".to_string(),
            "--table_id".to_string(),
            onchain_table_id.to_string(),
            "--committee".to_string(),
            committee_addr,
            "--hole_cards".to_string(),
            hole_cards_json,
            "--salts".to_string(),
            "[]".to_string(),
            "--proof".to_string(),
            proof_hex,
            "--public_inputs".to_string(),
            pi_hex,
        ],
    )
    .await?;

    let tx_hash = parse_tx_result(output)?;
    crate::proof_cache::insert(cache_key, tx_hash.clone()).await;
    Ok(tx_hash)
}

/// Convert co-noir keccak proof format to the Soroban/BB UltraHonk verifier format.
///
/// co-noir keccak format (variable size, raw G1 coordinates):
///   [pairing_points(16 Fr), G1_raw(8×2), sumcheck_uni(log_n×8),
///    sumcheck_eval(41), gemini_fold_raw((log_n-1)×2), gemini_eval(log_n),
///    shplonk_raw(1×2), kzg_raw(1×2)]
///
/// Soroban verifier format (fixed 458 fields, limb-encoded G1):
///   [pairing_points(16), G1_limb(8×4), sumcheck_uni(28×8),
///    sumcheck_eval(41), gemini_fold_limb(27×4), gemini_eval(28),
///    shplonk_limb(1×4), kzg_limb(1×4), log_n(1)]
fn convert_keccak_proof_to_soroban(proof_bytes: &[u8]) -> Result<Vec<u8>, String> {
    const FIELD_SIZE: usize = 32;
    const SOROBAN_PROOF_FIELDS: usize = 458;
    const SOROBAN_PROOF_BYTES: usize = SOROBAN_PROOF_FIELDS * FIELD_SIZE;
    const CONST_PROOF_SIZE_LOG_N: usize = 28;
    const BATCHED_RELATION_PARTIAL_LENGTH: usize = 8;
    const NUMBER_OF_ENTITIES: usize = 41;
    const NUM_G1_WIRE_POINTS: usize = 8;
    const NUM_FINAL_G1: usize = 2;
    const PAIRING_POINTS_SIZE: usize = 16;

    if proof_bytes.len() % FIELD_SIZE != 0 {
        return Err(format!(
            "proof not 32-byte aligned: {} bytes",
            proof_bytes.len()
        ));
    }

    let num_fields = proof_bytes.len() / FIELD_SIZE;

    // Derive log_n from proof size:
    // total = PAIRING + G1_RAW + SUMCHECK + EVALS + GEMINI_FOLD + GEMINI_EVAL + FINAL_G1
    // total = 16 + 16 + log_n*8 + 41 + (log_n-1)*2 + log_n + 4
    // total = 77 + log_n*8 + (log_n-1)*2 + log_n
    // total = 77 + 11*log_n - 2
    // total = 75 + 11*log_n
    // log_n = (total - 75) / 11
    let log_n_calc = num_fields as i64 - 75;
    if log_n_calc <= 0 || log_n_calc % 11 != 0 {
        return Err(format!(
            "cannot derive log_n from proof size: {} fields (remainder {})",
            num_fields,
            log_n_calc % 11
        ));
    }
    let log_n = (log_n_calc / 11) as usize;

    // Verify derived log_n is reasonable
    if log_n < 10 || log_n > 25 {
        return Err(format!(
            "derived log_n={} out of reasonable range [10,25]",
            log_n
        ));
    }

    // Verify total
    let expected = PAIRING_POINTS_SIZE
        + NUM_G1_WIRE_POINTS * 2
        + log_n * BATCHED_RELATION_PARTIAL_LENGTH
        + NUMBER_OF_ENTITIES
        + (log_n - 1) * 2
        + log_n
        + NUM_FINAL_G1 * 2;
    if num_fields != expected {
        return Err(format!(
            "proof size mismatch: got {} fields, expected {} (log_n={})",
            num_fields, expected, log_n
        ));
    }

    tracing::info!(
        "Proof conversion: {} fields, derived log_n={}",
        num_fields,
        log_n
    );

    let mut out = Vec::with_capacity(SOROBAN_PROOF_BYTES);
    let mut offset = 0usize;

    // Helper: read 32 bytes from proof
    let read_fr = |off: &mut usize| -> &[u8] {
        let start = *off;
        *off += FIELD_SIZE;
        &proof_bytes[start..start + FIELD_SIZE]
    };

    // Helper: split a 32-byte big-endian coordinate into (lo136, hi) limb pair
    fn coord_to_limbs(coord: &[u8]) -> ([u8; 32], [u8; 32]) {
        let mut lo = [0u8; 32];
        let mut hi = [0u8; 32];
        lo[15..].copy_from_slice(&coord[15..]); // lower 17 bytes
        hi[17..].copy_from_slice(&coord[..15]); // upper 15 bytes
        (lo, hi)
    }

    // Helper: convert raw G1 (x, y) to limb-encoded (x_lo, x_hi, y_lo, y_hi)
    let convert_g1_raw_to_limb = |off: &mut usize, out: &mut Vec<u8>| {
        let x = &proof_bytes[*off..*off + FIELD_SIZE];
        *off += FIELD_SIZE;
        let y = &proof_bytes[*off..*off + FIELD_SIZE];
        *off += FIELD_SIZE;
        let (x_lo, x_hi) = coord_to_limbs(x);
        let (y_lo, y_hi) = coord_to_limbs(y);
        out.extend_from_slice(&x_lo);
        out.extend_from_slice(&x_hi);
        out.extend_from_slice(&y_lo);
        out.extend_from_slice(&y_hi);
    };

    // 1) Pairing point object: 16 Fr values — these are limb-encoded accumulator
    //    coordinates in both formats, copy directly
    for _ in 0..PAIRING_POINTS_SIZE {
        out.extend_from_slice(read_fr(&mut offset));
    }

    // 2) 8 G1 wire commitments: convert from raw (x,y) to limb (x_lo,x_hi,y_lo,y_hi)
    for _ in 0..NUM_G1_WIRE_POINTS {
        convert_g1_raw_to_limb(&mut offset, &mut out);
    }

    // 3) Sumcheck univariates: log_n rounds → pad to CONST_PROOF_SIZE_LOG_N
    for _ in 0..log_n {
        for _ in 0..BATCHED_RELATION_PARTIAL_LENGTH {
            out.extend_from_slice(read_fr(&mut offset));
        }
    }
    let pad_rounds = CONST_PROOF_SIZE_LOG_N - log_n;
    out.extend(vec![
        0u8;
        pad_rounds
            * BATCHED_RELATION_PARTIAL_LENGTH
            * FIELD_SIZE
    ]);

    // 4) Sumcheck evaluations: 41 Fr (copy directly)
    for _ in 0..NUMBER_OF_ENTITIES {
        out.extend_from_slice(read_fr(&mut offset));
    }

    // 5) Gemini fold comms: (log_n-1) raw G1 → limb-encode, pad to 27
    for _ in 0..(log_n - 1) {
        convert_g1_raw_to_limb(&mut offset, &mut out);
    }
    let pad_gemini = (CONST_PROOF_SIZE_LOG_N - 1) - (log_n - 1);
    out.extend(vec![0u8; pad_gemini * 4 * FIELD_SIZE]);

    // 6) Gemini a evaluations: log_n Fr → pad to CONST_PROOF_SIZE_LOG_N
    for _ in 0..log_n {
        out.extend_from_slice(read_fr(&mut offset));
    }
    out.extend(vec![0u8; (CONST_PROOF_SIZE_LOG_N - log_n) * FIELD_SIZE]);

    // 7) Shplonk Q and KZG quotient: 2 raw G1 → limb-encode
    for _ in 0..NUM_FINAL_G1 {
        convert_g1_raw_to_limb(&mut offset, &mut out);
    }

    // 8) Append log_n as final field (big-endian u256)
    let mut log_n_field = [0u8; 32];
    log_n_field[31] = log_n as u8;
    if log_n > 255 {
        log_n_field[30] = (log_n >> 8) as u8;
    }
    out.extend_from_slice(&log_n_field);

    // Verify we consumed all input (except preamble already skipped)
    if offset != proof_bytes.len() {
        return Err(format!(
            "proof conversion: consumed {} of {} bytes ({} fields leftover)",
            offset,
            proof_bytes.len(),
            (proof_bytes.len() - offset) / FIELD_SIZE
        ));
    }

    if out.len() != SOROBAN_PROOF_BYTES {
        return Err(format!(
            "converted proof size mismatch: got {} bytes, expected {}",
            out.len(),
            SOROBAN_PROOF_BYTES
        ));
    }

    tracing::info!(
        "Proof converted: {} bytes (keccak, log_n={}) → {} bytes (soroban)",
        proof_bytes.len(),
        log_n,
        out.len()
    );

    Ok(out)
}

/// Convert a BN254 field element (decimal string) to a 32-byte big-endian hex string.
/// This is needed because Soroban `BytesN<32>` expects hex-encoded bytes, but
/// MPC proof outputs are decimal field element strings.
fn field_to_bytes32_hex(field_str: &str) -> Result<String, String> {
    let fr = Fr::from_hex(field_str);
    let bytes = fr.into_bigint().to_bytes_be();
    // Pad to exactly 32 bytes (should already be, but be safe)
    if bytes.len() > 32 {
        return Err(format!("field element too large: {} bytes", bytes.len()));
    }
    let mut padded = vec![0u8; 32 - bytes.len()];
    padded.extend_from_slice(&bytes);
    Ok(hex::encode(padded))
}

/// Convert a slice of field element strings to a JSON array of hex-encoded BytesN<32>.
fn fields_to_bytes32_json(fields: &[String]) -> Result<String, String> {
    let hex_strings: Vec<String> = fields
        .iter()
        .map(|f| field_to_bytes32_hex(f))
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string(&hex_strings).map_err(|e| format!("failed to serialize hex array: {}", e))
}

/// Convert proof public inputs (field element strings) to concatenated 32-byte big-endian
/// representations suitable for the on-chain verifier.
fn public_inputs_to_hex(public_inputs: &[String]) -> Result<String, String> {
    let mut all_bytes = Vec::with_capacity(public_inputs.len() * 32);
    for pi in public_inputs {
        let fr = Fr::from_hex(pi);
        let bytes = fr.into_bigint().to_bytes_be();
        let mut padded = vec![0u8; 32 - bytes.len()];
        padded.extend_from_slice(&bytes);
        all_bytes.extend_from_slice(&padded);
    }
    Ok(hex::encode(all_bytes))
}

#[cfg(test)]
mod error_handling_tests {
    //! Coverage for the **invalid proof submission** error path: the proof
    //! converter and field encoders must reject structurally invalid proofs
    //! and unparseable field elements with a descriptive `Err` before anything
    //! is ever sent on-chain. We also assert that when Soroban is not
    //! configured, submission is *skipped* (returns an empty tx hash) rather
    //! than erroring or attempting a malformed invoke.
    use super::*;

    fn unconfigured() -> SorobanConfig {
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

    #[test]
    fn rejects_unaligned_proof_bytes() {
        let err = convert_keccak_proof_to_soroban(&[0u8; 33]).unwrap_err();
        assert!(err.contains("32-byte aligned"), "got: {err}");
    }

    #[test]
    fn rejects_proof_with_underivable_log_n() {
        // 100 fields => (100 - 75) is not divisible by 11 => log_n underivable.
        let err = convert_keccak_proof_to_soroban(&vec![0u8; 100 * 32]).unwrap_err();
        assert!(err.contains("cannot derive log_n"), "got: {err}");
    }

    #[test]
    fn rejects_proof_with_out_of_range_log_n() {
        // 174 fields => log_n = 9, below the supported [10, 25] range.
        let err = convert_keccak_proof_to_soroban(&vec![0u8; 174 * 32]).unwrap_err();
        assert!(err.contains("out of reasonable range"), "got: {err}");
    }

    #[test]
    fn rejects_empty_proof() {
        let err = convert_keccak_proof_to_soroban(&[]).unwrap_err();
        // 0 fields => log_n_calc = -75 <= 0.
        assert!(err.contains("cannot derive log_n"), "got: {err}");
    }

    #[test]
    fn rejects_non_numeric_field_element() {
        let err = field_to_bytes32_hex("deadbeef_not_decimal").unwrap_err();
        assert!(err.contains("failed to parse field element"), "got: {err}");
    }

    #[test]
    fn encodes_valid_field_element_to_32_bytes() {
        let hex = field_to_bytes32_hex("1").unwrap();
        assert_eq!(hex.len(), 64); // 32 bytes, big-endian, zero-padded
        assert!(hex.ends_with('1'));
    }

    #[test]
    fn rejects_invalid_public_input() {
        let err = public_inputs_to_hex(&["definitely-not-a-field".to_string()]).unwrap_err();
        assert!(err.contains("failed to parse public input"), "got: {err}");
    }

    #[test]
    fn fields_json_rejects_invalid_entry() {
        let err =
            fields_to_bytes32_json(&["12".to_string(), "bad-field".to_string()]).unwrap_err();
        assert!(err.contains("failed to parse field element"), "got: {err}");
    }

    #[tokio::test]
    async fn submit_deal_proof_skips_when_unconfigured() {
        let res = submit_deal_proof(&unconfigured(), 1, &[], &[], "0", &[])
            .await
            .unwrap();
        assert!(
            res.is_empty(),
            "unconfigured submission must be skipped, not errored"
        );
    }

    #[tokio::test]
    async fn submit_reveal_proof_skips_when_unconfigured() {
        let res = submit_reveal_proof(&unconfigured(), 1, &[], &[], &[], &[])
            .await
            .unwrap();
        assert!(res.is_empty());
    }

    #[tokio::test]
    async fn submit_showdown_proof_skips_when_unconfigured() {
        let res = submit_showdown_proof(&unconfigured(), 1, &[], &[], &[])
            .await
            .unwrap();
        assert!(res.is_empty());
    }
}
