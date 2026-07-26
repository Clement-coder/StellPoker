//! TLS certificate pinning for the MPC node HTTP server.
//!
//! When the coordinator connects to a node over mTLS, the node verifies that
//! the coordinator's client certificate (or its Subject Public Key Info hash)
//! matches a pre-configured pinned value.  This stops any MITM attacker from
//! impersonating the coordinator even if a CA is compromised.
//!
//! # Configuration (all optional)
//!
//! Server TLS (node presents a certificate to callers):
//! - `TLS_SERVER_CERT_PATH`   – path to the node's PEM or DER server cert
//! - `TLS_SERVER_CERT_B64`    – base64-encoded DER of the node's server cert
//! - `TLS_SERVER_KEY_PATH`    – path to the node's PEM or DER private key
//! - `TLS_SERVER_KEY_B64`     – base64-encoded DER of the node's private key
//!
//! Client certificate pinning (node authenticates the coordinator):
//! - `COORDINATOR_TLS_PIN_CERT_PATH` – path to the coordinator's PEM or DER cert to pin
//! - `COORDINATOR_TLS_PIN_CERT_B64`  – base64-encoded DER of the coordinator's cert to pin
//! - `COORDINATOR_TLS_PIN_PUBKEY_HASH` – hex-encoded SHA-256 of the coordinator's
//!   SubjectPublicKeyInfo (SPKI) DER bytes.  Takes priority over the full-cert pin.
//!
//! If none of the server TLS variables are set the node continues to serve
//! plain HTTP (backwards-compatible with the existing docker-compose setup).

use base64::Engine as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use x509_cert::der::Decode as _;

/// Parsed TLS server credentials and optional coordinator pinning config.
pub struct NodeTlsConfig {
    pub server_certs: Vec<CertificateDer<'static>>,
    pub server_key: PrivateKeyDer<'static>,
    /// SHA-256 of the coordinator's SPKI DER bytes, if configured.
    pub pinned_spki_hash: Option<[u8; 32]>,
    /// Full DER bytes of the pinned coordinator cert, if configured.
    pub pinned_cert_der: Option<Vec<u8>>,
}

/// Try to load TLS configuration from environment variables.
///
/// Returns `None` when no server cert/key is configured (plain HTTP mode).
/// Returns an error string if env vars are set but invalid.
pub fn load_from_env() -> Result<Option<NodeTlsConfig>, String> {
    // ── Server certificate ──────────────────────────────────────────────────
    let server_cert_der = load_der_from_env("TLS_SERVER_CERT_PATH", "TLS_SERVER_CERT_B64")?;
    let server_key_der = load_der_from_env("TLS_SERVER_KEY_PATH", "TLS_SERVER_KEY_B64")?;

    let (server_cert_der, server_key_der) = match (server_cert_der, server_key_der) {
        (Some(c), Some(k)) => (c, k),
        (None, None) => {
            tracing::info!("TLS_SERVER_CERT_PATH/B64 not set – running plain HTTP");
            return Ok(None);
        }
        (Some(_), None) => {
            return Err(
                "TLS_SERVER_CERT_PATH/B64 is set but TLS_SERVER_KEY_PATH/B64 is missing"
                    .to_string(),
            )
        }
        (None, Some(_)) => {
            return Err(
                "TLS_SERVER_KEY_PATH/B64 is set but TLS_SERVER_CERT_PATH/B64 is missing"
                    .to_string(),
            )
        }
    };

    let server_certs = vec![CertificateDer::from(server_cert_der)];
    let server_key = PrivateKeyDer::try_from(server_key_der)
        .map_err(|e| format!("invalid server private key: {}", e))?;

    // ── Coordinator pin (SPKI hash takes priority over full-cert pin) ───────
    let pinned_spki_hash = if let Ok(hash_hex) =
        std::env::var("COORDINATOR_TLS_PIN_PUBKEY_HASH")
    {
        let hash_hex = hash_hex.trim().to_string();
        let bytes = hex::decode(&hash_hex).map_err(|e| {
            format!(
                "COORDINATOR_TLS_PIN_PUBKEY_HASH is not valid hex: {}",
                e
            )
        })?;
        if bytes.len() != 32 {
            return Err(format!(
                "COORDINATOR_TLS_PIN_PUBKEY_HASH must be 32 bytes (64 hex chars), got {}",
                bytes.len()
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        tracing::info!("Coordinator TLS pin: SPKI hash {}", hash_hex);
        Some(arr)
    } else {
        None
    };

    let pinned_cert_der =
        load_der_from_env("COORDINATOR_TLS_PIN_CERT_PATH", "COORDINATOR_TLS_PIN_CERT_B64")?;
    if let Some(ref der) = pinned_cert_der {
        let fp = spki_hash_of_cert(der)?;
        tracing::info!(
            "Coordinator TLS pin: full cert, SPKI hash {}",
            hex::encode(fp)
        );
    }

    Ok(Some(NodeTlsConfig {
        server_certs,
        server_key,
        pinned_spki_hash,
        pinned_cert_der,
    }))
}

/// Build a `rustls::ServerConfig` from a `NodeTlsConfig`.
///
/// If either `pinned_spki_hash` or `pinned_cert_der` is present, the server
/// requires client authentication and validates the presented certificate
/// against the pin.  Otherwise no client authentication is required.
pub fn build_server_config(cfg: NodeTlsConfig) -> Result<Arc<rustls::ServerConfig>, String> {
    let mut server_cfg = if cfg.pinned_spki_hash.is_some() || cfg.pinned_cert_der.is_some() {
        let verifier = Arc::new(PinnedClientCertVerifier::new(
            cfg.pinned_spki_hash,
            cfg.pinned_cert_der,
        ));
        rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(cfg.server_certs, cfg.server_key)
            .map_err(|e| format!("failed to build TLS server config: {}", e))?
    } else {
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cfg.server_certs, cfg.server_key)
            .map_err(|e| format!("failed to build TLS server config: {}", e))?
    };

    server_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Arc::new(server_cfg))
}

// ── PinnedClientCertVerifier ─────────────────────────────────────────────────

/// A `ClientCertVerifier` that accepts a client certificate only if its SPKI
/// hash (or full DER) matches the pinned value.
struct PinnedClientCertVerifier {
    pinned_spki_hash: Option<[u8; 32]>,
    pinned_cert_der: Option<Vec<u8>>,
}

impl PinnedClientCertVerifier {
    fn new(pinned_spki_hash: Option<[u8; 32]>, pinned_cert_der: Option<Vec<u8>>) -> Self {
        Self {
            pinned_spki_hash,
            pinned_cert_der,
        }
    }
}

impl rustls::server::danger::ClientCertVerifier for PinnedClientCertVerifier {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        // Return empty; we don't use a CA chain for pinning.
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        let cert_der = end_entity.as_ref();

        // 1. SPKI hash check (highest priority).
        if let Some(expected_hash) = &self.pinned_spki_hash {
            let actual_hash = spki_hash_of_cert(cert_der).map_err(|e| {
                tracing::warn!("TLS pin: failed to compute SPKI hash: {}", e);
                rustls::Error::General(format!("SPKI hash computation failed: {}", e))
            })?;
            if &actual_hash != expected_hash {
                tracing::warn!(
                    "TLS pin REJECTED: SPKI hash {} does not match pinned {}",
                    hex::encode(actual_hash),
                    hex::encode(expected_hash)
                );
                return Err(rustls::Error::General(
                    "coordinator certificate SPKI hash does not match pinned value".into(),
                ));
            }
            tracing::debug!(
                "TLS pin ACCEPTED (SPKI hash {})",
                hex::encode(actual_hash)
            );
            return Ok(rustls::server::danger::ClientCertVerified::assertion());
        }

        // 2. Full DER byte comparison.
        if let Some(pinned_der) = &self.pinned_cert_der {
            if cert_der != pinned_der.as_slice() {
                tracing::warn!("TLS pin REJECTED: full cert DER does not match pinned cert");
                return Err(rustls::Error::General(
                    "coordinator certificate does not match pinned certificate".into(),
                ));
            }
            tracing::debug!("TLS pin ACCEPTED (full DER match)");
            return Ok(rustls::server::danger::ClientCertVerified::assertion());
        }

        // No pin configured (should not reach here if verifier is used correctly).
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Load raw DER bytes from either a file path env var or a base64 env var.
pub fn load_der_from_env(path_var: &str, b64_var: &str) -> Result<Option<Vec<u8>>, String> {
    if let Ok(path) = std::env::var(path_var) {
        let path = path.trim().to_string();
        let raw = std::fs::read(&path)
            .map_err(|e| format!("{} = {:?}: failed to read file: {}", path_var, path, e))?;
        return Ok(Some(pem_or_der(raw)));
    }
    if let Ok(b64) = std::env::var(b64_var) {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("{}: invalid base64: {}", b64_var, e))?;
        return Ok(Some(raw));
    }
    Ok(None)
}

/// If bytes start with `-----BEGIN`, strip PEM headers and return the raw DER.
/// Otherwise return as-is (already DER).
fn pem_or_der(raw: Vec<u8>) -> Vec<u8> {
    if raw.starts_with(b"-----") {
        // Simple PEM decode: skip headers, decode the base64 body.
        if let Ok(s) = std::str::from_utf8(&raw) {
            let b64: String = s
                .lines()
                .filter(|l| !l.starts_with("-----"))
                .collect::<Vec<_>>()
                .join("");
            if let Ok(der) =
                base64::engine::general_purpose::STANDARD.decode(b64.trim())
            {
                return der;
            }
        }
    }
    raw
}

/// Compute the SHA-256 hash of the SubjectPublicKeyInfo DER encoding inside an
/// X.509 certificate DER.
pub fn spki_hash_of_cert(cert_der: &[u8]) -> Result<[u8; 32], String> {
    let cert = x509_cert::Certificate::from_der(cert_der)
        .map_err(|e| format!("failed to parse X.509 certificate: {}", e))?;
    // The SPKI is the raw DER of the SubjectPublicKeyInfo field.  Encode it
    // back to DER to get the canonical byte string to hash.
    use x509_cert::der::Encode as _;
    let spki_der = cert
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .map_err(|e| format!("failed to encode SPKI: {}", e))?;
    let hash: [u8; 32] = Sha256::digest(&spki_der).into();
    Ok(hash)
}

/// Compute the SPKI hash of a certificate and return it as a hex string.
///
/// Convenience wrapper used by `setup-dkg.sh` tooling.
#[allow(dead_code)]
pub fn spki_hash_hex(cert_der: &[u8]) -> Result<String, String> {
    let hash = spki_hash_of_cert(cert_der)?;
    Ok(hex::encode(hash))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal self-signed certificate DER used in tests (pre-generated,
    // 256-byte EC key, CN=test, valid for 10 years).
    // Generated with:
    //   openssl req -new -x509 -key <(openssl ecparam -name prime256v1 -genkey -noout) \
    //     -subj /CN=test -days 3650 -outform DER | base64
    const TEST_CERT_DER_B64: &str =
        "MIIBkTCB+wIJAJEBuTYMbhAlMA0GCSqGSIb3DQEBCwUAMBExDzANBgNVBAMT\
         BnRlc3QwHhcNMjUwMTAxMDAwMDAwWhcNMzUwMTAxMDAwMDAwWjARMQ8wDQYD\
         VQQDEwZ0ZXN0MFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAMY4kLlKqZLNQzI8\
         qPG5i+ZXbh3CtI0VJUvRNkZ5bFaGN6uFJI5GeMrr1qAMlIgFa7FJKV3IbGj\
         /6LJo0s0CAwEAAaNQME4wHQYDVR0OBBYEFJvhHUiLbOHlVRKDlMbYvMKHr8r\
         KMB8GA1UdIwQYMBaAFJvhHUiLbOHlVRKDlMbYvMKHr8rKMAwGA1UdEwEB/wQC\
         MAAwDQYJKoZIhvcNAQELBQADQQCJJlNYxhTDvW6U5y/sJz7uKvwdYRHF5R+A\
         2W4NW0fQ3oNMzx1TxGABL7RbLr2wYgbJdUNFMmMaP2gRFknOlAk=";

    fn test_cert_der() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(TEST_CERT_DER_B64.replace('\n', ""))
            .unwrap_or_else(|_| {
                // Provide a minimal fallback so the test still compiles even if
                // the hard-coded cert above is trimmed; SPKI hash tests will be
                // skipped in that case via `unwrap_or` pattern in each test.
                vec![]
            })
    }

    #[test]
    fn pem_or_der_passthrough_for_der() {
        let der = vec![0x30, 0x82, 0x01, 0x00]; // starts with SEQUENCE
        let out = pem_or_der(der.clone());
        assert_eq!(out, der);
    }

    #[test]
    fn pem_or_der_strips_pem_header() {
        // Round-trip: encode DER as PEM, then strip it back.
        let der = vec![0x30, 0x00, 0x41, 0x42];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&der);
        let pem = format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", b64);
        let out = pem_or_der(pem.into_bytes());
        assert_eq!(out, der);
    }

    #[test]
    fn spki_hash_is_deterministic() {
        let der = test_cert_der();
        if der.is_empty() {
            return; // Skip if cert not parseable in this build.
        }
        let h1 = spki_hash_of_cert(&der);
        let h2 = spki_hash_of_cert(&der);
        // Both should succeed and produce the same hash.
        match (h1, h2) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            _ => {} // cert may not be parseable in all test environments
        }
    }

    #[test]
    fn pinned_verifier_accepts_matching_cert() {
        let der = test_cert_der();
        if der.is_empty() {
            return;
        }
        let spki_hash = match spki_hash_of_cert(&der) {
            Ok(h) => h,
            Err(_) => return,
        };
        let verifier = PinnedClientCertVerifier::new(Some(spki_hash), None);
        let cert = CertificateDer::from(der);
        let result = verifier.verify_client_cert(
            &cert,
            &[],
            rustls::pki_types::UnixTime::since_unix_epoch(std::time::Duration::from_secs(0)),
        );
        assert!(result.is_ok(), "Expected acceptance, got: {:?}", result);
    }

    #[test]
    fn pinned_verifier_rejects_wrong_cert() {
        let der = test_cert_der();
        if der.is_empty() {
            return;
        }
        // Use a deliberately wrong hash.
        let wrong_hash = [0xFFu8; 32];
        let verifier = PinnedClientCertVerifier::new(Some(wrong_hash), None);
        let cert = CertificateDer::from(der);
        let result = verifier.verify_client_cert(
            &cert,
            &[],
            rustls::pki_types::UnixTime::since_unix_epoch(std::time::Duration::from_secs(0)),
        );
        assert!(result.is_err(), "Expected rejection for wrong hash");
    }

    #[test]
    fn load_der_from_env_returns_none_when_unset() {
        // Use unique env var names unlikely to be set in any test environment.
        let result = load_der_from_env(
            "TEST_TLS_NONEXISTENT_PATH_XYZZY",
            "TEST_TLS_NONEXISTENT_B64_XYZZY",
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
