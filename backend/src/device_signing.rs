//! Go-compatible Ed25519 device request verification for future privileged APIs.
//! This is intentionally not wired to a file/terminal/MCP endpoint until nonce
//! replay protection and workspace-policy checks are audited.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

const MAX_CLOCK_SKEW_SECONDS: i64 = 90;
const MAX_BODY_SIZE: usize = 20 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofError {
    InvalidPublicKey,
    MissingTimestamp,
    ClockSkew,
    InvalidNonce,
    MissingSignature,
    BadSignature,
    BodyTooLarge,
    InvalidRequestUri,
}

pub struct DeviceProof<'a> {
    pub method: &'a str,
    pub request_uri: &'a str,
    pub timestamp: &'a str,
    pub nonce: &'a str,
    pub signature: &'a str,
    pub body: &'a [u8],
}

pub fn canonical(
    method: &str,
    request_uri: &str,
    body: &[u8],
    timestamp: i64,
    nonce: &str,
) -> Vec<u8> {
    let body_hash = hex::encode(Sha256::digest(body));
    format!(
        "{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        request_uri,
        timestamp,
        nonce,
        body_hash
    )
    .into_bytes()
}

pub fn verify(public_key: &str, proof: DeviceProof<'_>, now_unix: i64) -> Result<(), ProofError> {
    if proof.body.len() > MAX_BODY_SIZE {
        return Err(ProofError::BodyTooLarge);
    }
    if !proof.request_uri.starts_with('/')
        || proof.request_uri.starts_with("//")
        || proof.request_uri.contains('#')
        || proof.request_uri.chars().any(char::is_control)
    {
        return Err(ProofError::InvalidRequestUri);
    }
    let public_raw = URL_SAFE_NO_PAD
        .decode(public_key.trim())
        .map_err(|_| ProofError::InvalidPublicKey)?;
    let key_bytes: [u8; 32] = public_raw
        .try_into()
        .map_err(|_| ProofError::InvalidPublicKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| ProofError::InvalidPublicKey)?;
    if verifying_key.is_weak() {
        return Err(ProofError::InvalidPublicKey);
    }
    let timestamp = proof
        .timestamp
        .trim()
        .parse::<i64>()
        .map_err(|_| ProofError::MissingTimestamp)?;
    if timestamp.abs_diff(now_unix) > MAX_CLOCK_SKEW_SECONDS as u64 {
        return Err(ProofError::ClockSkew);
    }
    let nonce_raw = URL_SAFE_NO_PAD
        .decode(proof.nonce.trim())
        .map_err(|_| ProofError::InvalidNonce)?;
    if !(16..=64).contains(&nonce_raw.len()) {
        return Err(ProofError::InvalidNonce);
    }
    let signature_raw = URL_SAFE_NO_PAD
        .decode(proof.signature.trim())
        .map_err(|_| ProofError::MissingSignature)?;
    let signature_bytes: [u8; 64] = signature_raw
        .try_into()
        .map_err(|_| ProofError::MissingSignature)?;
    let signature = Signature::from_bytes(&signature_bytes);
    let canonical_request = canonical(
        proof.method,
        proof.request_uri,
        proof.body,
        timestamp,
        proof.nonce.trim(),
    );
    verifying_key
        .verify_strict(&canonical_request, &signature)
        .map_err(|_| ProofError::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn fixture() -> (String, String, String, String) {
        let signing = SigningKey::from_bytes(&[13u8; 32]);
        let public = URL_SAFE_NO_PAD.encode(signing.verifying_key().as_bytes());
        let nonce = URL_SAFE_NO_PAD.encode([42u8; 18]);
        let message = canonical(
            "POST",
            "/api/client/workspaces?updated=1",
            br#"{"hello":"world"}"#,
            1_700_000_000,
            &nonce,
        );
        let signature = URL_SAFE_NO_PAD.encode(signing.sign(&message).to_bytes());
        (
            public,
            nonce,
            signature,
            String::from_utf8(message).unwrap(),
        )
    }

    #[test]
    fn canonical_matches_go_lines_and_body_sha256() {
        let (_, nonce, _, message) = fixture();
        let hash = hex::encode(Sha256::digest(br#"{"hello":"world"}"#));
        assert_eq!(
            message,
            format!("POST\n/api/client/workspaces?updated=1\n1700000000\n{nonce}\n{hash}")
        );
    }

    #[test]
    fn verifies_valid_ed25519_and_rejects_modified_body_or_query() {
        let (key, nonce, signature, _) = fixture();
        let request = |body| DeviceProof {
            method: "POST",
            request_uri: "/api/client/workspaces?updated=1",
            timestamp: "1700000000",
            nonce: &nonce,
            signature: &signature,
            body,
        };
        assert_eq!(
            verify(&key, request(br#"{"hello":"world"}"#), 1_700_000_000),
            Ok(())
        );
        assert_eq!(
            verify(&key, request(br#"{"hello":"attacker"}"#), 1_700_000_000),
            Err(ProofError::BadSignature)
        );
        assert_eq!(
            verify(
                &key,
                DeviceProof {
                    request_uri: "/api/client/workspaces?updated=0",
                    ..request(br#"{"hello":"world"}"#)
                },
                1_700_000_000
            ),
            Err(ProofError::BadSignature)
        );
    }

    #[test]
    fn rejects_small_order_public_key() {
        let (_, nonce, signature, _) = fixture();
        // Compressed Edwards identity (small order).
        let mut identity = [0u8; 32];
        identity[0] = 1;
        assert_eq!(
            verify(
                &URL_SAFE_NO_PAD.encode(identity),
                DeviceProof {
                    method: "POST",
                    request_uri: "/api/client/workspaces?updated=1",
                    timestamp: "1700000000",
                    nonce: &nonce,
                    signature: &signature,
                    body: br#"{"hello":"world"}"#
                },
                1_700_000_000
            ),
            Err(ProofError::InvalidPublicKey)
        );
    }

    #[test]
    fn rejects_old_timestamp_and_small_nonce() {
        let (key, nonce, signature, _) = fixture();
        assert_eq!(
            verify(
                &key,
                DeviceProof {
                    method: "POST",
                    request_uri: "/api/client/workspaces?updated=1",
                    timestamp: "1700000000",
                    nonce: &nonce,
                    signature: &signature,
                    body: br#"{"hello":"world"}"#,
                },
                1_700_000_091
            ),
            Err(ProofError::ClockSkew)
        );
        let invalid_nonce = URL_SAFE_NO_PAD.encode([2u8; 4]);
        assert_eq!(
            verify(
                &key,
                DeviceProof {
                    method: "POST",
                    request_uri: "/api/client/workspaces?updated=1",
                    timestamp: "1700000000",
                    nonce: &invalid_nonce,
                    signature: &signature,
                    body: br#"{"hello":"world"}"#,
                },
                1_700_000_000
            ),
            Err(ProofError::InvalidNonce)
        );
    }
}
