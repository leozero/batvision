//! TOTP (RFC 6238) and recovery codes.

use totp_rs::{Secret, Totp};

pub const ISSUER: &str = "BatVision";

pub fn new_secret() -> String {
    Secret::generate().to_base32()
}

fn build(secret_b32: &str, account: &str) -> anyhow::Result<Totp> {
    let secret = Secret::try_from_base32(secret_b32)
        .map_err(|e| anyhow::anyhow!("invalid TOTP secret: {e:?}"))?;
    totp_rs::Builder::new()
        .with_secret(secret)
        .with_account_name(account)
        .with_issuer(Some(ISSUER))
        .build()
        .map_err(|e| anyhow::anyhow!("invalid TOTP parameters: {e:?}"))
}

/// `otpauth://` URL to show as a QR code in the client.
pub fn otpauth_url(secret_b32: &str, account: &str) -> anyhow::Result<String> {
    build(secret_b32, account)?
        .to_url()
        .map_err(|e| anyhow::anyhow!("TOTP URL: {e:?}"))
}

/// Returns the matched time step if `code` is valid at `unix_time` (±1 step) and
/// strictly newer than `last_step`, so that each code is accepted only once.
pub fn verify(secret_b32: &str, code: &str, unix_time: u64, last_step: Option<i64>) -> Option<i64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let step = build(secret_b32, "verify").ok()?.check(&code, unix_time)? as i64;
    match last_step {
        Some(last) if step <= last => None,
        _ => Some(step),
    }
}

/// Recovery codes look like `abcd-efgh-jkmn`: unambiguous lowercase letters and digits.
pub fn new_recovery_codes(count: usize) -> Vec<String> {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    (0..count)
        .map(|_| {
            let raw = super::crypto::random_code(ALPHABET, 12);
            format!("{}-{}-{}", &raw[0..4], &raw[4..8], &raw[8..12])
        })
        .collect()
}

/// Canonical form used before hashing, so dashes, spaces and case don't matter.
pub fn normalize_recovery_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
pub fn code_at(secret_b32: &str, unix_time: u64) -> String {
    build(secret_b32, "test")
        .unwrap()
        .generate(unix_time)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 1_800_000_000;

    #[test]
    fn accepts_current_code_once() {
        let secret = new_secret();
        let code = code_at(&secret, T);
        let step = verify(&secret, &code, T, None).expect("valid code");
        assert_eq!(
            verify(&secret, &code, T, Some(step)),
            None,
            "replay rejected"
        );
    }

    #[test]
    fn accepts_previous_step_for_clock_skew() {
        let secret = new_secret();
        let code = code_at(&secret, T - 30);
        assert!(verify(&secret, &code, T, None).is_some());
        let old = code_at(&secret, T - 120);
        assert!(verify(&secret, &old, T, None).is_none());
    }

    #[test]
    fn rejects_malformed_codes() {
        let secret = new_secret();
        assert!(verify(&secret, "12345", T, None).is_none());
        assert!(verify(&secret, "abcdef", T, None).is_none());
    }

    #[test]
    fn otpauth_url_contains_issuer() {
        let url = otpauth_url(&new_secret(), "alice").unwrap();
        assert!(url.starts_with("otpauth://totp/"));
        assert!(url.contains("issuer=BatVision"));
    }

    #[test]
    fn recovery_codes_format_and_normalization() {
        let codes = new_recovery_codes(10);
        assert_eq!(codes.len(), 10);
        assert!(
            codes
                .iter()
                .all(|c| c.len() == 14 && c.matches('-').count() == 2)
        );
        assert_eq!(normalize_recovery_code(" ABCD-efgh-2345 "), "abcdefgh2345");
    }
}
