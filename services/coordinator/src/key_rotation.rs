//! Committee identity key rotation (Issue #102).
//!
//! The coordinator holds one Ed25519 keypair — the "committee secret"
//! (`COMMITTEE_SECRET`) — that it uses to authenticate on-chain calls to the
//! `committee-registry` contract (`register_member` / `deregister_member` /
//! participating in an epoch). A long-lived, never-rotated key is a standing
//! risk: if it ever leaks there is no recovery path short of a manual
//! incident response.
//!
//! This module tracks that key's age and, once it exceeds
//! `COMMITTEE_KEY_ROTATION_INTERVAL_SECS`, generates a replacement. The old
//! key is not discarded immediately — it is kept as `retiring` for
//! `COMMITTEE_KEY_ROTATION_OVERLAP_SECS` so that:
//! - in-flight on-chain transactions signed before the rotation still land, and
//! - the new key can be registered with the committee-registry (and given
//!   time to be picked up into a fresh epoch) before the old one is
//!   deregistered and its stake reclaimed.
//!
//! Actually applying a rotation on-chain (funding the new address, calling
//! `register_member`, rotating the active epoch, then `deregister_member` on
//! the retiring key) is handled by [`crate::soroban::rotate_committee_key`];
//! this module is the pure, deterministic scheduling/state logic behind it,
//! kept separate so it can be unit tested without a live Stellar network.

use ed25519_dalek::SigningKey;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

/// One committee identity: a Stellar keypair plus when it was issued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitteeKey {
    /// Stellar secret seed ("S...").
    pub secret_key: String,
    /// Stellar public address ("G..."), derived from `secret_key`.
    pub address: String,
    pub issued_at_unix: u64,
}

impl CommitteeKey {
    /// Generate a fresh random committee identity.
    pub fn generate(now: SystemTime) -> Self {
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        Self::from_signing_key(signing_key, now)
    }

    /// Wrap an existing Stellar secret seed (e.g. from `COMMITTEE_SECRET`).
    pub fn from_secret(secret_key: &str, now: SystemTime) -> Result<Self, String> {
        let sk = stellar_strkey::ed25519::PrivateKey::from_string(secret_key)
            .map_err(|e| format!("invalid committee secret key: {:?}", e))?;
        let signing_key = SigningKey::from_bytes(&sk.0);
        let address =
            stellar_strkey::ed25519::PublicKey(signing_key.verifying_key().to_bytes()).to_string();
        Ok(Self {
            secret_key: secret_key.to_string(),
            address,
            issued_at_unix: unix_secs(now),
        })
    }

    fn from_signing_key(signing_key: SigningKey, now: SystemTime) -> Self {
        let secret_key = stellar_strkey::ed25519::PrivateKey(signing_key.to_bytes()).to_string();
        let address =
            stellar_strkey::ed25519::PublicKey(signing_key.verifying_key().to_bytes()).to_string();
        Self {
            secret_key,
            address,
            issued_at_unix: unix_secs(now),
        }
    }
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Debug)]
pub struct RotationConfig {
    /// How long a committee key stays active before a replacement is due.
    pub rotation_interval: Duration,
    /// How long a just-retired key remains valid/registered after rotation,
    /// so overlapping in-flight operations aren't disrupted.
    pub overlap: Duration,
}

impl RotationConfig {
    pub fn from_env() -> Self {
        Self {
            rotation_interval: Duration::from_secs(
                std::env::var("COMMITTEE_KEY_ROTATION_INTERVAL_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(7 * 24 * 60 * 60), // 7 days
            ),
            overlap: Duration::from_secs(
                std::env::var("COMMITTEE_KEY_ROTATION_OVERLAP_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60 * 60), // 1 hour
            ),
        }
    }
}

/// Tracks the coordinator's active committee key and, during an overlap
/// window, the key it is rotating away from.
#[derive(Clone, Debug)]
pub struct KeyRotationState {
    pub active: CommitteeKey,
    pub retiring: Option<CommitteeKey>,
    pub config: RotationConfig,
}

impl KeyRotationState {
    pub fn new(active: CommitteeKey, config: RotationConfig) -> Self {
        Self {
            active,
            retiring: None,
            config,
        }
    }

    /// Whether the active key has been in service long enough to rotate.
    pub fn due_for_rotation(&self, now: SystemTime) -> bool {
        unix_secs(now).saturating_sub(self.active.issued_at_unix)
            >= self.config.rotation_interval.as_secs()
    }

    /// Generate a new key and demote the current active key to `retiring`.
    /// Returns the new key (caller is responsible for registering it
    /// on-chain and swapping any live `SorobanConfig` over to it).
    ///
    /// No-op / returns `None` if a rotation is already in progress (a
    /// `retiring` key hasn't finished its overlap window yet) — rotating
    /// again before that completes would strand the first retiring key
    /// without ever deregistering it.
    pub fn rotate(&mut self, now: SystemTime) -> Option<CommitteeKey> {
        if self.retiring.is_some() {
            return None;
        }
        let new_key = CommitteeKey::generate(now);
        let old_active = std::mem::replace(&mut self.active, new_key.clone());
        self.retiring = Some(old_active);
        Some(new_key)
    }

    /// Whether the retiring key's overlap window has elapsed and it is safe
    /// to deregister on-chain and drop from memory.
    pub fn retiring_expired(&self, now: SystemTime) -> bool {
        match &self.retiring {
            Some(k) => {
                unix_secs(now).saturating_sub(k.issued_at_unix) >= self.config.overlap.as_secs()
            }
            None => false,
        }
    }

    /// Drop the retiring key once its on-chain deregistration has completed.
    pub fn finish_retirement(&mut self) -> Option<CommitteeKey> {
        self.retiring.take()
    }
}

/// Background task that actually carries out rotations: generates and
/// registers a new committee identity once the active one is due, then
/// deregisters the outgoing one after its overlap window elapses.
///
/// No-op if the committee-registry contract isn't configured — there's
/// nothing on-chain to rotate in that case (matches the same
/// `is_configured`-style gating used by the other background tasks in
/// `main.rs`, e.g. the hot-reload snapshotter and idempotency GC).
pub fn spawn_rotation_task(
    state: Arc<RwLock<KeyRotationState>>,
    soroban_config: crate::soroban::SorobanConfig,
) {
    if soroban_config.committee_registry_contract.is_empty() {
        tracing::info!("Committee registry not configured — key rotation task not started");
        return;
    }

    tokio::spawn(async move {
        let check_interval = Duration::from_secs(
            std::env::var("COMMITTEE_KEY_ROTATION_CHECK_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
        );

        loop {
            tokio::time::sleep(check_interval).await;
            let now = SystemTime::now();

            let due = state.read().await.due_for_rotation(now);
            if due {
                let rotated = state.write().await.rotate(now);
                if let Some(new_key) = rotated {
                    tracing::info!(address = %new_key.address, "rotating committee identity");
                    let min_stake = soroban_config.committee_member_min_stake;
                    let endpoint = soroban_config.committee_member_endpoint.clone();
                    let region = soroban_config.committee_member_region.clone();
                    match crate::soroban::register_rotated_committee_key(
                        &soroban_config,
                        &new_key,
                        min_stake,
                        &endpoint,
                        &region,
                    )
                    .await
                    {
                        Ok(tx_hash) => tracing::info!(
                            address = %new_key.address,
                            tx_hash = %tx_hash,
                            "registered rotated committee key on-chain"
                        ),
                        Err(error) => tracing::error!(
                            address = %new_key.address,
                            %error,
                            "failed to register rotated committee key"
                        ),
                    }
                }
            }

            let retiring_key = {
                let guard = state.read().await;
                if guard.retiring_expired(now) {
                    guard.retiring.clone()
                } else {
                    None
                }
            };
            if let Some(old_key) = retiring_key {
                match crate::soroban::deregister_rotated_committee_key(&soroban_config, &old_key)
                    .await
                {
                    Ok(tx_hash) => {
                        tracing::info!(
                            address = %old_key.address,
                            tx_hash = %tx_hash,
                            "deregistered retired committee key"
                        );
                        state.write().await.finish_retirement();
                    }
                    Err(error) => tracing::warn!(
                        address = %old_key.address,
                        %error,
                        "failed to deregister retired committee key; will retry next cycle"
                    ),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(rotation_secs: u64, overlap_secs: u64) -> RotationConfig {
        RotationConfig {
            rotation_interval: Duration::from_secs(rotation_secs),
            overlap: Duration::from_secs(overlap_secs),
        }
    }

    #[test]
    fn generated_key_round_trips_through_from_secret() {
        let now = SystemTime::now();
        let key = CommitteeKey::generate(now);
        let reparsed = CommitteeKey::from_secret(&key.secret_key, now).unwrap();
        assert_eq!(key.address, reparsed.address);
    }

    #[test]
    fn address_starts_with_g_and_secret_with_s() {
        let key = CommitteeKey::generate(SystemTime::now());
        assert!(key.address.starts_with('G'));
        assert!(key.secret_key.starts_with('S'));
    }

    #[test]
    fn not_due_before_interval_elapses() {
        let now = SystemTime::now();
        let key = CommitteeKey::generate(now);
        let state = KeyRotationState::new(key, cfg(3600, 300));
        assert!(!state.due_for_rotation(now));
        assert!(!state.due_for_rotation(now + Duration::from_secs(3599)));
    }

    #[test]
    fn due_once_interval_elapses() {
        let now = SystemTime::now();
        let key = CommitteeKey::generate(now);
        let state = KeyRotationState::new(key, cfg(3600, 300));
        assert!(state.due_for_rotation(now + Duration::from_secs(3600)));
        assert!(state.due_for_rotation(now + Duration::from_secs(10_000)));
    }

    #[test]
    fn rotate_demotes_active_to_retiring_with_new_address() {
        let now = SystemTime::now();
        let original = CommitteeKey::generate(now);
        let original_address = original.address.clone();
        let mut state = KeyRotationState::new(original, cfg(3600, 300));

        let later = now + Duration::from_secs(3600);
        let new_key = state.rotate(later).expect("rotation should succeed");

        assert_ne!(new_key.address, original_address);
        assert_eq!(state.active.address, new_key.address);
        assert_eq!(
            state.retiring.as_ref().map(|k| k.address.clone()),
            Some(original_address)
        );
    }

    #[test]
    fn rotate_is_a_noop_while_a_retirement_is_already_in_flight() {
        let now = SystemTime::now();
        let key = CommitteeKey::generate(now);
        let mut state = KeyRotationState::new(key, cfg(3600, 300));

        let first_rotation = state.rotate(now).expect("first rotation succeeds");
        let second_attempt = state.rotate(now + Duration::from_secs(1));

        assert!(second_attempt.is_none());
        // Active key is still the one from the first rotation.
        assert_eq!(state.active.address, first_rotation.address);
    }

    #[test]
    fn retiring_key_stays_valid_until_overlap_elapses() {
        let now = SystemTime::now();
        let key = CommitteeKey::generate(now);
        let mut state = KeyRotationState::new(key, cfg(3600, 300));
        state.rotate(now).unwrap();

        assert!(!state.retiring_expired(now));
        assert!(!state.retiring_expired(now + Duration::from_secs(299)));
        assert!(state.retiring_expired(now + Duration::from_secs(300)));
    }

    #[test]
    fn finish_retirement_clears_retiring_slot() {
        let now = SystemTime::now();
        let key = CommitteeKey::generate(now);
        let mut state = KeyRotationState::new(key, cfg(3600, 300));
        let old_active = state.active.clone();
        state.rotate(now).unwrap();

        let finished = state.finish_retirement();
        assert_eq!(finished.map(|k| k.address), Some(old_active.address));
        assert!(state.retiring.is_none());
        // A subsequent rotation is allowed again now that the slot is clear.
        assert!(state.rotate(now + Duration::from_secs(3600)).is_some());
    }

    #[test]
    fn no_retirement_pending_is_never_expired() {
        let state =
            KeyRotationState::new(CommitteeKey::generate(SystemTime::now()), cfg(3600, 300));
        assert!(!state.retiring_expired(SystemTime::now() + Duration::from_secs(999_999)));
    }
}
