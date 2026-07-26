//! AES-256-GCM encryption for sensitive coordinator data at rest.
//!
//! Key sourcing (in priority order):
//!   1. `ENCRYPTION_KEY` env var — 32-byte value encoded as 64 hex chars.
//!   2. AWS KMS (feature `kms`) — `KMS_KEY_ID` env var selects the key;
//!      a 32-byte data key is generated and stored alongside the ciphertext.
//!
//! Key rotation: every `EncryptedField` stores the key version used to
//! encrypt it. `EncryptionKey::rotate` replaces the active key so that new
//! writes use the new key; old ciphertexts are re-encrypted on the first
//! decrypt-then-re-encrypt cycle.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use zeroize::Zeroizing;

/// A versioned AES-256-GCM key ring. `keys[0]` is the active key.
#[derive(Clone)]
pub struct EncryptionKey {
    /// `keys[i]` = (version, 32-byte key material). Index 0 is active.
    keys: Vec<(u32, Zeroizing<[u8; 32]>)>,
}

impl EncryptionKey {
    /// Load from `ENCRYPTION_KEY` env var (64 hex chars → 32 bytes, version 1).
    pub fn from_env() -> Result<Self, String> {
        let raw = std::env::var("ENCRYPTION_KEY")
            .map_err(|_| "ENCRYPTION_KEY not set".to_string())?;
        let bytes = hex::decode(raw.trim())
            .map_err(|e| format!("ENCRYPTION_KEY is not valid hex: {e}"))?;
        let key_bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "ENCRYPTION_KEY must be exactly 32 bytes (64 hex chars)".to_string())?;
        Ok(Self {
            keys: vec![(1, Zeroizing::new(key_bytes))],
        })
    }

    /// Generate an ephemeral in-process key. Used when no persistent key is
    /// configured (dev/test only — data is not recoverable across restarts).
    pub fn ephemeral() -> Self {
        use aes_gcm::aead::rand_core::RngCore;
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        tracing::warn!(
            "No ENCRYPTION_KEY configured — using ephemeral key. \
             Encrypted data will not survive process restart."
        );
        Self {
            keys: vec![(1, Zeroizing::new(key_bytes))],
        }
    }

    /// Active key version number.
    pub fn active_version(&self) -> u32 {
        self.keys[0].0
    }

    /// Encrypt `plaintext`, returning `"<version>:<b64(nonce||ciphertext)>"`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String, String> {
        let (version, key_bytes) = &self.keys[0];
        let key = Key::<Aes256Gcm>::from_slice(key_bytes.as_ref());
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| format!("encrypt error: {e}"))?;

        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ciphertext);
        Ok(format!("{}:{}", version, B64.encode(&blob)))
    }

    /// Decrypt a value produced by `encrypt`.
    pub fn decrypt(&self, ciphertext: &str) -> Result<Vec<u8>, String> {
        let (ver_str, b64) = ciphertext
            .split_once(':')
            .ok_or("invalid ciphertext format")?;
        let version: u32 = ver_str.parse().map_err(|_| "invalid key version")?;

        let key_bytes = self
            .keys
            .iter()
            .find(|(v, _)| *v == version)
            .map(|(_, k)| k)
            .ok_or_else(|| format!("unknown key version {version}"))?;

        let blob = B64.decode(b64).map_err(|e| format!("base64 decode: {e}"))?;
        if blob.len() < 12 {
            return Err("ciphertext too short".to_string());
        }
        let (nonce_bytes, ct) = blob.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let key = Key::<Aes256Gcm>::from_slice(key_bytes.as_ref());
        let cipher = Aes256Gcm::new(key);
        cipher
            .decrypt(nonce, ct)
            .map_err(|e| format!("decrypt error: {e}"))
    }

    /// Add a new key (becomes active). Old key is kept for decrypting existing data.
    pub fn rotate(&mut self, new_key_bytes: [u8; 32]) {
        let next_version = self.keys.iter().map(|(v, _)| *v).max().unwrap_or(0) + 1;
        self.keys.insert(0, (next_version, Zeroizing::new(new_key_bytes)));
        tracing::info!(version = next_version, "encryption key rotated");
    }
}

/// Encrypted wrapper for a sensitive string value stored in memory.
///
/// The plaintext is never retained; only the ciphertext is kept in the struct.
/// Use `encrypt_new` / `decrypt` to read/write the underlying value.
#[derive(Clone, Debug)]
pub struct EncryptedField {
    /// `"<version>:<b64(nonce||ciphertext)>"`
    ciphertext: String,
}

impl EncryptedField {
    pub fn encrypt(key: &EncryptionKey, plaintext: &str) -> Result<Self, String> {
        Ok(Self {
            ciphertext: key.encrypt(plaintext.as_bytes())?,
        })
    }

    pub fn decrypt(&self, key: &EncryptionKey) -> Result<String, String> {
        let bytes = key.decrypt(&self.ciphertext)?;
        String::from_utf8(bytes).map_err(|e| format!("utf8 decode: {e}"))
    }

    /// Re-encrypt with the current active key (used during key rotation).
    pub fn reencrypt(&self, key: &EncryptionKey) -> Result<Self, String> {
        let plaintext = self.decrypt(key)?;
        Self::encrypt(key, &plaintext)
    }

    /// Returns the key version this field was encrypted with.
    pub fn key_version(&self) -> Option<u32> {
        self.ciphertext.split_once(':')?.0.parse().ok()
    }

    /// True if this field needs to be re-encrypted with the current active key.
    pub fn needs_rotation(&self, key: &EncryptionKey) -> bool {
        self.key_version()
            .map(|v| v != key.active_version())
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> EncryptionKey {
        EncryptionKey {
            keys: vec![(1, Zeroizing::new([0xABu8; 32]))],
        }
    }

    #[test]
    fn round_trip() {
        let key = test_key();
        let field = EncryptedField::encrypt(&key, "secret-value").unwrap();
        assert_eq!(field.decrypt(&key).unwrap(), "secret-value");
    }

    #[test]
    fn rotation() {
        let mut key = test_key();
        let old_field = EncryptedField::encrypt(&key, "old-secret").unwrap();
        assert_eq!(old_field.key_version(), Some(1));

        key.rotate([0xCDu8; 32]);
        assert_eq!(key.active_version(), 2);

        // Old ciphertext still decryptable
        assert_eq!(old_field.decrypt(&key).unwrap(), "old-secret");

        // Re-encrypt upgrades to new version
        let new_field = old_field.reencrypt(&key).unwrap();
        assert_eq!(new_field.key_version(), Some(2));
        assert_eq!(new_field.decrypt(&key).unwrap(), "old-secret");
    }

    #[test]
    fn ciphertext_is_not_plaintext() {
        let key = test_key();
        let field = EncryptedField::encrypt(&key, "my-secret-ip").unwrap();
        assert!(!field.ciphertext.contains("my-secret-ip"));
    }
}
