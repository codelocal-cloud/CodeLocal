//! Go-compatible password KDF for old codelocal_users rows.
//!
//! Go: scrypt.Key(password, salt, 1<<14, 8, 1, 64), base64.RawURLEncoding.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use scrypt::{Params, scrypt};
use subtle::ConstantTimeEq;

pub const MAX_PASSWORD_BYTES: usize = 256;

fn derive(password: &str, salt: &str) -> Result<[u8; 64], scrypt::errors::InvalidOutputLen> {
    let params = Params::new(14, 8, 1, 64).expect("fixed scrypt params are valid");
    let mut derived = [0_u8; 64];
    scrypt(password.as_bytes(), salt.as_bytes(), &params, &mut derived)?;
    Ok(derived)
}

/// This matches Go's password verifier, and rejects malformed or excessively long inputs.
pub fn verify_password(password: &str, salt: &str, hash: &str) -> bool {
    if password.len() > MAX_PASSWORD_BYTES || salt.len() > 256 || hash.len() > 128 {
        return false;
    }
    let Ok(expected) = URL_SAFE_NO_PAD.decode(hash) else {
        return false;
    };
    if expected.len() != 64 {
        return false;
    }
    match derive(password, salt) {
        Ok(actual) => bool::from(actual.as_slice().ct_eq(&expected)),
        Err(_) => false,
    }
}

/// Generate password hashes that existing Go code can verify.
#[allow(dead_code)]
pub fn hash_password(password: &str, salt: &str) -> Option<String> {
    if password.len() < 10 || password.len() > MAX_PASSWORD_BYTES || salt.is_empty() {
        return None;
    }
    derive(password, salt)
        .ok()
        .map(|key| URL_SAFE_NO_PAD.encode(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cross-language golden fixture generated with Python hashlib.scrypt using
    // exactly the parameters in old-project/internal/webauth/auth.go.
    const SALT: &str = "go-compat-salt-example";
    const HASH: &str =
        "L0ptyDT7AKIkZNATGb7hLADut1e4MoVKvSSuEEBYaCz7VRkNQ91MyrAnjezuDWMMVCTsscaxP_xi-X8wiGOX_g";

    #[test]
    fn go_scrypt_hash_fixture_is_identical() {
        assert_eq!(
            hash_password("correct-horse-battery-staple", SALT).unwrap(),
            HASH
        );
        assert!(verify_password("correct-horse-battery-staple", SALT, HASH));
        assert!(!verify_password("wrong-horse-battery-staple", SALT, HASH));
    }

    #[test]
    fn length_and_encodings_are_rejected() {
        assert!(hash_password("short", SALT).is_none());
        assert!(hash_password(&"x".repeat(257), SALT).is_none());
        assert!(!verify_password(&"x".repeat(257), SALT, HASH));
        assert!(!verify_password(
            "correct-horse-battery-staple",
            SALT,
            "not base64!"
        ));
        assert!(!verify_password(
            "correct-horse-battery-staple",
            SALT,
            "dGVzdA"
        ));
    }

    #[test]
    fn different_salt_changes_hash() {
        let first = hash_password("correct-horse-battery-staple", SALT).unwrap();
        let second = hash_password("correct-horse-battery-staple", "another-salt").unwrap();
        assert_ne!(first, second);
    }
}
