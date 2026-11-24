//! Beta-only authenticated encryption. No public decryption/materialization API.
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub(crate) const KEY_ENV: &str = "CODELOCAL_BETA_RUNTIME_SECRET_KEY_HEX";
pub(crate) const MAX_SECRET_BYTES: usize = 4096;
pub(crate) const MAX_SECRETS_PER_LAYER: i64 = 64;

pub(crate) struct SecretKey {
    key: LessSafeKey,
    key_id: String,
}

pub(crate) struct SealedSecret {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub key_id: String,
}

pub(crate) fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 128
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(crate) fn valid_value(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_SECRET_BYTES && !value.contains('\0')
}

fn aad(user: &str, device: &str, workspace: &str, name: &str) -> Vec<u8> {
    // JSON array provides unambiguous field boundaries; never concatenate
    // untrusted strings with a delimiter that may occur in identifiers.
    serde_json::to_vec(&[
        "codelocal-beta-runtime-secret-v1",
        user,
        device,
        workspace,
        name,
    ])
    .expect("string array serialization cannot fail")
}

impl SecretKey {
    pub fn from_env() -> Option<Self> {
        let mut value = std::env::var(KEY_ENV).ok()?;
        let key = Self::from_hex(&value);
        value.zeroize();
        key
    }

    fn from_hex(value: &str) -> Option<Self> {
        if value.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        if hex::decode_to_slice(value, &mut bytes).is_err() {
            bytes.zeroize();
            return None;
        }
        let key_id = hex::encode(Sha256::digest(bytes));
        let key = UnboundKey::new(&aead::AES_256_GCM, &bytes)
            .ok()
            .map(LessSafeKey::new);
        bytes.zeroize();
        Some(Self { key: key?, key_id })
    }

    pub fn seal(
        &self,
        user: &str,
        device: &str,
        workspace: &str,
        name: &str,
        value: &str,
    ) -> Result<SealedSecret, ()> {
        if !valid_name(name) || !valid_value(value) {
            return Err(());
        }
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|_| ())?;
        let mut ciphertext = value.as_bytes().to_vec();
        if self
            .key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad(user, device, workspace, name)),
                &mut ciphertext,
            )
            .is_err()
        {
            ciphertext.zeroize();
            return Err(());
        }
        Ok(SealedSecret {
            nonce: nonce.to_vec(),
            ciphertext,
            key_id: self.key_id.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn open(
        key: &SecretKey,
        sealed: &SealedSecret,
        user: &str,
        device: &str,
        workspace: &str,
        name: &str,
    ) -> Option<Vec<u8>> {
        let mut data = sealed.ciphertext.clone();
        let nonce: [u8; 12] = sealed.nonce.as_slice().try_into().ok()?;
        key.key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad(user, device, workspace, name)),
                &mut data,
            )
            .ok()
            .map(|v| v.to_vec())
    }
    #[test]
    fn encrypted_values_are_randomized_and_bound_to_exact_scope() {
        let key = SecretKey::from_hex(&"12".repeat(32)).unwrap();
        let value = "beta-only-test-secret";
        let a = key.seal("u", "d", "w", "API_KEY", value).unwrap();
        let b = key.seal("u", "d", "w", "API_KEY", value).unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ciphertext, b.ciphertext);
        assert_eq!(
            open(&key, &a, "u", "d", "w", "API_KEY").unwrap(),
            value.as_bytes()
        );
        assert!(open(&key, &a, "other", "d", "w", "API_KEY").is_none());
        assert!(open(&key, &a, "u", "other", "w", "API_KEY").is_none());
        assert!(open(&key, &a, "u", "d", "other", "API_KEY").is_none());
        assert!(open(&key, &a, "u", "d", "w", "OTHER_KEY").is_none());
        let wrong = SecretKey::from_hex(&"34".repeat(32)).unwrap();
        assert!(open(&wrong, &a, "u", "d", "w", "API_KEY").is_none());
        let mut corrupt = a;
        corrupt.ciphertext[0] ^= 1;
        assert!(open(&key, &corrupt, "u", "d", "w", "API_KEY").is_none());
    }
    #[test]
    fn keys_names_and_values_are_strictly_bounded() {
        assert!(SecretKey::from_hex("bad").is_none());
        assert!(SecretKey::from_hex(&"xx".repeat(32)).is_none());
        assert!(valid_name("_API_KEY_2"));
        assert!(!valid_name("2KEY"));
        assert!(!valid_name("a-b"));
        assert!(!valid_name(&"a".repeat(129)));
        assert!(!valid_value(""));
        assert!(!valid_value(&"a".repeat(4097)));
        assert!(!valid_value("a\0b"));
    }
}
