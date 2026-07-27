//! Circuit artifact pinning for MPC sessions.
//!
//! When a proof session starts (deal phase) the coordinator hashes the ACIR
//! bytecode file for every circuit that will be used during that session and
//! stores those hashes in the [`TableSession`].  On every subsequent proof
//! submission (reveal, showdown) the hashes are re-computed and compared
//! against the pinned values.  If any artifact has changed since the session
//! was opened the submission is rejected with a `CONFLICT` status, preventing
//! a circuit upgrade from silently affecting an in-flight game.
//!
//! # Hash function
//! SHA-256 over the raw file bytes, hex-encoded.
//!
//! # Artifact path convention
//! `<circuit_dir>/<circuit_name>/target/<circuit_name>.json`
//!
//! This matches the output layout of `nargo compile`.

use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Compute the SHA-256 hex digest of a single circuit artifact file.
///
/// `circuit_dir` is the root directory that contains one sub-directory per
/// circuit (e.g. `./circuits`).  `circuit_name` is the name used by nargo
/// (e.g. `deal_valid`, `reveal_board_valid`, `showdown_valid`).
fn hash_artifact(circuit_dir: &str, circuit_name: &str) -> Result<String, String> {
    let path = format!(
        "{}/{}/target/{}.json",
        circuit_dir.trim_end_matches('/'),
        circuit_name,
        circuit_name,
    );
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("cannot read circuit artifact '{}': {}", path, e))?;
    let digest = Sha256::digest(&bytes);
    Ok(hex::encode(digest))
}

/// Hash every circuit artifact that will be needed for a table session and
/// return a map of `circuit_name → sha256_hex`.
///
/// `circuit_names` should include every circuit variant that may be called
/// during this session (deal, all reveal variants, showdown).
pub fn pin_artifacts(
    circuit_dir: &str,
    circuit_names: &[&str],
) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    for &name in circuit_names {
        let hash = hash_artifact(circuit_dir, name)?;
        tracing::debug!(circuit = %name, hash = %hash, "pinned circuit artifact");
        map.insert(name.to_string(), hash);
    }
    Ok(map)
}

/// Verify that every artifact named in `pinned` still has the same hash on
/// disk.  Returns `Ok(())` if all hashes match, or an error message naming
/// the first mismatched circuit.
pub fn verify_pinned_artifacts(
    circuit_dir: &str,
    pinned: &HashMap<String, String>,
) -> Result<(), String> {
    for (name, expected) in pinned {
        let actual = hash_artifact(circuit_dir, name)?;
        if actual != *expected {
            return Err(format!(
                "circuit artifact '{}' has changed since session start \
                 (pinned={}, current={})",
                name, expected, actual
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_artifact(dir: &std::path::Path, name: &str, content: &[u8]) {
        let artifact_dir = dir.join(name).join("target");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let path = artifact_dir.join(format!("{}.json", name));
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content).unwrap();
    }

    #[test]
    fn test_pin_and_verify_ok() {
        let tmp = tempfile::tempdir().unwrap();
        write_artifact(tmp.path(), "deal_valid", b"{\"bytecode\":\"aaa\"}");

        let pinned = pin_artifacts(tmp.path().to_str().unwrap(), &["deal_valid"]).unwrap();
        assert!(pinned.contains_key("deal_valid"));

        verify_pinned_artifacts(tmp.path().to_str().unwrap(), &pinned)
            .expect("should pass when artifacts unchanged");
    }

    #[test]
    fn test_verify_detects_change() {
        let tmp = tempfile::tempdir().unwrap();
        write_artifact(tmp.path(), "deal_valid", b"{\"bytecode\":\"aaa\"}");

        let pinned = pin_artifacts(tmp.path().to_str().unwrap(), &["deal_valid"]).unwrap();

        // Overwrite the artifact with different content.
        write_artifact(tmp.path(), "deal_valid", b"{\"bytecode\":\"bbb\"}");

        let result = verify_pinned_artifacts(tmp.path().to_str().unwrap(), &pinned);
        assert!(result.is_err(), "should fail when artifact changed");
        assert!(result.unwrap_err().contains("has changed since session start"));
    }

    #[test]
    fn test_pin_missing_artifact_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        // Do not create any artifact file.
        let result = pin_artifacts(tmp.path().to_str().unwrap(), &["deal_valid"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot read circuit artifact"));
    }

    #[test]
    fn test_verify_empty_pinned_always_passes() {
        let pinned: HashMap<String, String> = HashMap::new();
        let result = verify_pinned_artifacts("/nonexistent", &pinned);
        assert!(result.is_ok());
    }
}
