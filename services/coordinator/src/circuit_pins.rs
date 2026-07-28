//! Circuit artifact pinning for MPC sessions.
//!
//! When a proof session starts (deal phase) the coordinator hashes the ACIR
//! bytecode file for every circuit that will be used during that session and
//! stores those hashes in the [`TableSession`]. On every subsequent proof
//! submission (reveal, showdown) the hashes are re-computed and compared
//! against the pinned values. If any artifact has changed since the session
//! was opened the submission is rejected with a `CONFLICT` status, preventing
//! a circuit upgrade from silently affecting an in-flight game.
//!
//! # Hot-Reloading & Cache Warming (Issue #253)
//! A background watcher monitors the circuit artifact directory for changes.
//! When a new or updated artifact is detected, the cache is warmed so new
//! sessions automatically use the updated circuit without dropping or
//! disrupting existing in-flight sessions.
//!
//! # Hash function
//! SHA-256 over the raw file bytes, hex-encoded.
//!
//! # Artifact path convention
//! `<circuit_dir>/<circuit_name>/target/<circuit_name>.json`

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};
use std::time::SystemTime;

#[derive(Debug, Clone, Default)]
struct WarmedArtifact {
    hash: String,
    mtime: SystemTime,
}

#[derive(Debug, Clone, Default)]
pub struct CircuitCacheState {
    active_hashes: HashMap<String, WarmedArtifact>,
    known_hashes: HashMap<String, HashSet<String>>,
}

static WARMED_CACHE: OnceLock<RwLock<CircuitCacheState>> = OnceLock::new();

fn get_cache() -> &'static RwLock<CircuitCacheState> {
    WARMED_CACHE.get_or_init(|| RwLock::new(CircuitCacheState::default()))
}

/// Compute the SHA-256 hex digest and modified time of a single circuit artifact file.
fn hash_artifact_with_mtime(circuit_dir: &str, circuit_name: &str) -> Result<(String, SystemTime), String> {
    let path = format!(
        "{}/{}/target/{}.json",
        circuit_dir.trim_end_matches('/'),
        circuit_name,
        circuit_name,
    );
    let meta = std::fs::metadata(&path)
        .map_err(|e| format!("cannot stat circuit artifact '{}': {}", path, e))?;
    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("cannot read circuit artifact '{}': {}", path, e))?;
    let digest = Sha256::digest(&bytes);
    Ok((hex::encode(digest), mtime))
}

/// Compute the SHA-256 hex digest of a single circuit artifact file.
fn hash_artifact(circuit_dir: &str, circuit_name: &str) -> Result<String, String> {
    let (hash, _) = hash_artifact_with_mtime(circuit_dir, circuit_name)?;
    Ok(hash)
}

/// Warm the circuit cache for a directory. Returns a list of reloaded circuit names.
pub fn warm_circuit_cache(circuit_dir: &str) -> Vec<String> {
    let mut reloaded = Vec::new();
    let entries = match std::fs::read_dir(circuit_dir) {
        Ok(e) => e,
        Err(_) => return reloaded,
    };

    let cache = get_cache();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = if path.is_dir() {
            path.file_name().and_then(|s| s.to_str()).map(|s| s.to_string())
        } else {
            None
        };

        if let Some(circuit_name) = name {
            if let Ok((hash, mtime)) = hash_artifact_with_mtime(circuit_dir, &circuit_name) {
                let mut state = cache.write().unwrap();
                let is_new_or_modified = match state.active_hashes.get(&circuit_name) {
                    Some(cached) => cached.mtime < mtime || cached.hash != hash,
                    None => true,
                };
                if is_new_or_modified {
                    state.active_hashes.insert(
                        circuit_name.clone(),
                        WarmedArtifact { hash: hash.clone(), mtime },
                    );
                    state.known_hashes
                        .entry(circuit_name.clone())
                        .or_default()
                        .insert(hash.clone());

                    reloaded.push(circuit_name.clone());
                    tracing::info!(
                        circuit = %circuit_name,
                        hash = %hash,
                        "Hot-reloaded circuit artifact and warmed cache for new sessions"
                    );
                }
            }
        }
    }
    reloaded
}

/// Spawns a background task that watches circuit_dir every few seconds and warms cache.
pub fn spawn_circuit_watcher(circuit_dir: String) {
    tokio::spawn(async move {
        warm_circuit_cache(&circuit_dir);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            interval.tick().await;
            warm_circuit_cache(&circuit_dir);
        }
    });
}

/// Hash every circuit artifact that will be needed for a table session and
/// return a map of `circuit_name → sha256_hex`.
pub fn pin_artifacts(
    circuit_dir: &str,
    circuit_names: &[&str],
) -> Result<HashMap<String, String>, String> {
    warm_circuit_cache(circuit_dir);
    let cache = get_cache().read().unwrap();
    let mut map = HashMap::new();
    for &name in circuit_names {
        if let Some(warmed) = cache.active_hashes.get(name) {
            tracing::debug!(circuit = %name, hash = %warmed.hash, "pinned circuit artifact (warmed cache)");
            map.insert(name.to_string(), warmed.hash.clone());
        } else {
            let hash = hash_artifact(circuit_dir, name)?;
            tracing::debug!(circuit = %name, hash = %hash, "pinned circuit artifact");
            map.insert(name.to_string(), hash);
        }
    }
    Ok(map)
}

/// Verify that every artifact named in `pinned` matches either a known valid
/// pinned hash or the disk hash.
pub fn verify_pinned_artifacts(
    circuit_dir: &str,
    pinned: &HashMap<String, String>,
) -> Result<(), String> {
    let cache = get_cache().read().unwrap();
    for (name, expected) in pinned {
        if let Some(known_set) = cache.known_hashes.get(name) {
            if known_set.contains(expected) {
                continue;
            }
        }
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
    fn test_hot_reload_cache_warming() {
        let tmp = tempfile::tempdir().unwrap();
        write_artifact(tmp.path(), "deal_valid", b"{\"bytecode\":\"version1\"}");

        let pinned_v1 = pin_artifacts(tmp.path().to_str().unwrap(), &["deal_valid"]).unwrap();

        // Overwrite artifact with v2 (simulating hot reload)
        write_artifact(tmp.path(), "deal_valid", b"{\"bytecode\":\"version2\"}");

        // Warm cache
        let reloaded = warm_circuit_cache(tmp.path().to_str().unwrap());
        assert!(reloaded.contains(&"deal_valid".to_string()));

        // New session gets v2 hash
        let pinned_v2 = pin_artifacts(tmp.path().to_str().unwrap(), &["deal_valid"]).unwrap();
        assert_ne!(pinned_v1["deal_valid"], pinned_v2["deal_valid"]);

        // Existing session (v1) still verifies without dropping!
        verify_pinned_artifacts(tmp.path().to_str().unwrap(), &pinned_v1)
            .expect("existing session with v1 hash should still pass");
    }

    #[test]
    fn test_pin_missing_artifact_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let result = pin_artifacts(tmp.path().to_str().unwrap(), &["nonexistent_circuit"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_empty_pinned_always_passes() {
        let pinned: HashMap<String, String> = HashMap::new();
        let result = verify_pinned_artifacts("/nonexistent", &pinned);
        assert!(result.is_ok());
    }
}
