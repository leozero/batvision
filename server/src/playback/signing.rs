//! Signed, short-lived media URLs. `<video>` and `<track>` tags can't send an
//! Authorization header, so the URL itself carries a proof of access.

use std::path::Path;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// What a signed URL grants access to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Grant {
    /// A media file, served as is.
    File { f: i64 },
    /// An embedded subtitle stream, as WebVTT shifted by `o` ms.
    Subtitle { f: i64, s: i64, o: i64 },
    /// An external subtitle file, as WebVTT shifted by `o` ms.
    External { x: i64, o: i64 },
}

#[derive(Serialize, Deserialize)]
struct Payload {
    #[serde(flatten)]
    grant: Grant,
    /// Expiry, Unix seconds.
    e: i64,
}

#[derive(Clone)]
pub struct MediaSigner {
    key: [u8; 32],
}

impl MediaSigner {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Loads the key from `data_dir/media.key`, creating it on first start.
    pub fn load_or_create(data_dir: &Path) -> anyhow::Result<Self> {
        let path = data_dir.join("media.key");
        let key = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() == 32 => bytes.try_into().expect("32 bytes"),
            _ => {
                let mut key = [0u8; 32];
                rand::fill(&mut key);
                std::fs::write(&path, key)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
                }
                key
            }
        };
        Ok(Self { key })
    }

    fn mac(&self, data: &[u8]) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("any key length");
        mac.update(data);
        mac
    }

    pub fn sign(&self, grant: Grant, expires_at: i64) -> String {
        let payload = serde_json::to_vec(&Payload {
            grant,
            e: expires_at,
        })
        .expect("serializable");
        let signature = self.mac(&payload).finalize().into_bytes();
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&payload),
            URL_SAFE_NO_PAD.encode(signature)
        )
    }

    /// The grant, if the token is authentic and not expired at `now`.
    pub fn verify(&self, token: &str, now: i64) -> Option<Grant> {
        let (payload, signature) = token.split_once('.')?;
        let payload = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
        self.mac(&payload).verify_slice(&signature).ok()?;
        let payload: Payload = serde_json::from_slice(&payload).ok()?;
        (payload.e > now).then_some(payload.grant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_expiry() {
        let signer = MediaSigner::new([7; 32]);
        let token = signer.sign(Grant::File { f: 42 }, 1_000);
        assert_eq!(signer.verify(&token, 999), Some(Grant::File { f: 42 }));
        assert_eq!(signer.verify(&token, 1_000), None, "expired");
    }

    #[test]
    fn tampering_and_other_keys_are_rejected() {
        let signer = MediaSigner::new([7; 32]);
        let token = signer.sign(Grant::File { f: 42 }, 1_000);
        let forged_payload = URL_SAFE_NO_PAD.encode(br#"{"k":"file","f":43,"e":1000}"#);
        let forged = format!("{forged_payload}.{}", token.split_once('.').unwrap().1);
        assert_eq!(signer.verify(&forged, 0), None);
        assert_eq!(MediaSigner::new([8; 32]).verify(&token, 0), None);
        assert_eq!(signer.verify("garbage", 0), None);
    }

    #[test]
    fn key_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let a = MediaSigner::load_or_create(dir.path()).unwrap();
        let b = MediaSigner::load_or_create(dir.path()).unwrap();
        let token = a.sign(Grant::External { x: 1, o: 0 }, 10);
        assert!(b.verify(&token, 0).is_some());
    }
}
