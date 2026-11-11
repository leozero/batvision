//! Password hashing, random tokens and token hashing.

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// Hashes a password with Argon2id (default parameters), off the async runtime.
pub async fn hash_password(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
    })
    .await?
}

/// Verifies a password against a stored PHC hash, off the async runtime.
pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// A valid hash of a random password, verified against when the username does not
/// exist so that response time does not reveal which usernames exist.
pub fn dummy_hash() -> &'static str {
    use std::sync::OnceLock;
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        Argon2::default()
            .hash_password(random_token("").as_bytes())
            .expect("hashing a random password")
            .to_string()
    })
}

/// 256-bit random token, URL-safe, with a readable prefix (`bvd_` for devices…).
pub fn random_token(prefix: &str) -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    format!("{prefix}{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// Random string drawn from `alphabet`.
pub fn random_code(alphabet: &[u8], len: usize) -> String {
    (0..len)
        .map(|_| alphabet[rand::random_range(0..alphabet.len())] as char)
        .collect()
}

/// Tokens are stored hashed: a leaked database does not leak usable tokens.
/// They are high-entropy, so a fast hash is enough (no need for Argon2).
pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Constant-time string comparison.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_roundtrip() {
        let hash = hash_password("correct horse".into()).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse".into(), hash.clone()).await);
        assert!(!verify_password("wrong".into(), hash).await);
    }

    #[tokio::test]
    async fn garbage_hash_never_verifies() {
        assert!(!verify_password("x".into(), "not a hash".into()).await);
    }

    #[test]
    fn tokens_are_unique_and_prefixed() {
        let a = random_token("bvd_");
        let b = random_token("bvd_");
        assert!(a.starts_with("bvd_"));
        assert_ne!(a, b);
        assert_eq!(a.len(), 4 + 43);
    }

    #[test]
    fn token_hash_is_stable_hex() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_eq!(hash_token("abc").len(), 64);
        assert_ne!(hash_token("abc"), hash_token("abd"));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }
}
