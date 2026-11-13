use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;

/// Runtime configuration, read from `BATVISION_*` environment variables.
#[derive(Clone)]
pub struct Config {
    /// Address the HTTP server listens on. TLS is terminated by the reverse proxy.
    pub bind: SocketAddr,
    /// Built web client (`apps/web/dist`). Served as a SPA when present.
    pub web_dir: Option<PathBuf>,
    /// Persistent data: database, images. Must be backed up.
    pub data_dir: PathBuf,
    /// Media library root, mounted read-only.
    pub media_dir: PathBuf,
    /// Transcodes and encoded variants. Disposable.
    pub cache_dir: PathBuf,
    /// ffmpeg binary.
    pub ffmpeg: PathBuf,
    /// ffprobe binary.
    pub ffprobe: PathBuf,
    /// Behind a reverse proxy: take the client IP from `X-Forwarded-For`.
    pub trust_proxy: bool,
    /// TMDB API Read Access Token (v4 "Bearer" token). Without it, no metadata is fetched.
    pub tmdb_token: Option<String>,
    pub tmdb_api_url: String,
    pub tmdb_image_url: String,
    /// Language used for metadata when no user exists yet, and for new users.
    pub default_language: String,
    /// Minutes between automatic library scans. 0 disables them.
    pub scan_interval_minutes: u64,
    /// `Secure` flag on the session cookie. Only disable for plain-HTTP local testing.
    pub secure_cookies: bool,
    /// Fixed first-run setup token (otherwise a random one is printed in the logs).
    pub setup_token: Option<String>,
    /// An administrator to let back in at startup (lost password or second factor).
    pub reset_admin: Option<String>,
    /// Size budget of the encoded-versions cache, in GB.
    pub encode_cache_gb: u64,
    /// How many download versions may encode at the same time.
    pub encode_concurrency: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Testable constructor: `lookup` returns the value of an environment variable.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let get = |key: &str, default: &str| lookup(key).unwrap_or_else(|| default.to_owned());

        let bind = get("BATVISION_BIND", "0.0.0.0:8420")
            .parse()
            .context("BATVISION_BIND must be an address like 0.0.0.0:8420")?;

        Ok(Self {
            bind,
            web_dir: lookup("BATVISION_WEB_DIR").map(PathBuf::from),
            data_dir: get("BATVISION_DATA_DIR", "./.dev/data").into(),
            media_dir: get("BATVISION_MEDIA_DIR", "./.dev/media").into(),
            cache_dir: get("BATVISION_CACHE_DIR", "./.dev/cache").into(),
            ffmpeg: get("BATVISION_FFMPEG", "ffmpeg").into(),
            ffprobe: get("BATVISION_FFPROBE", "ffprobe").into(),
            trust_proxy: parse_bool(&get("BATVISION_TRUST_PROXY", "false"))
                .context("BATVISION_TRUST_PROXY must be true or false")?,
            tmdb_token: lookup("BATVISION_TMDB_TOKEN").filter(|t| !t.trim().is_empty()),
            tmdb_api_url: get("BATVISION_TMDB_API_URL", "https://api.themoviedb.org/3"),
            tmdb_image_url: get("BATVISION_TMDB_IMAGE_URL", "https://image.tmdb.org/t/p"),
            default_language: get("BATVISION_DEFAULT_LANGUAGE", "fr-FR"),
            scan_interval_minutes: get("BATVISION_SCAN_INTERVAL_MINUTES", "15")
                .parse()
                .context("BATVISION_SCAN_INTERVAL_MINUTES must be a number")?,
            secure_cookies: parse_bool(&get("BATVISION_SECURE_COOKIES", "true"))
                .context("BATVISION_SECURE_COOKIES must be true or false")?,
            setup_token: lookup("BATVISION_SETUP_TOKEN").filter(|t| t.trim().len() >= 16),
            reset_admin: lookup("BATVISION_RESET_ADMIN").filter(|u| !u.trim().is_empty()),
            encode_cache_gb: get("BATVISION_ENCODE_CACHE_GB", "50")
                .parse()
                .context("BATVISION_ENCODE_CACHE_GB must be a number")?,
            encode_concurrency: get("BATVISION_ENCODE_CONCURRENCY", "1")
                .parse()
                .context("BATVISION_ENCODE_CONCURRENCY must be a number")?,
        })
    }
}

/// Manual impl so the TMDB token never ends up in logs.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("web_dir", &self.web_dir)
            .field("data_dir", &self.data_dir)
            .field("media_dir", &self.media_dir)
            .field("cache_dir", &self.cache_dir)
            .field("ffmpeg", &self.ffmpeg)
            .field("ffprobe", &self.ffprobe)
            .field("trust_proxy", &self.trust_proxy)
            .field(
                "tmdb_token",
                &self.tmdb_token.as_ref().map(|_| "<redacted>"),
            )
            .field("tmdb_api_url", &self.tmdb_api_url)
            .field("default_language", &self.default_language)
            .field("scan_interval_minutes", &self.scan_interval_minutes)
            .field("secure_cookies", &self.secure_cookies)
            .field(
                "setup_token",
                &self.setup_token.as_ref().map(|_| "<redacted>"),
            )
            .field("encode_cache_gb", &self.encode_cache_gb)
            .field("encode_concurrency", &self.encode_concurrency)
            .finish()
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn defaults_when_nothing_is_set() {
        let config = Config::from_lookup(lookup(&[])).unwrap();
        assert_eq!(config.bind.to_string(), "0.0.0.0:8420");
        assert_eq!(config.web_dir, None);
        assert_eq!(config.ffmpeg, PathBuf::from("ffmpeg"));
        assert!(!config.trust_proxy);
        assert_eq!(config.tmdb_token, None);
        assert_eq!(config.scan_interval_minutes, 15);
    }

    #[test]
    fn empty_tmdb_token_means_none() {
        let config = Config::from_lookup(lookup(&[("BATVISION_TMDB_TOKEN", " ")])).unwrap();
        assert_eq!(config.tmdb_token, None);
    }

    #[test]
    fn debug_output_redacts_tmdb_token() {
        let config =
            Config::from_lookup(lookup(&[("BATVISION_TMDB_TOKEN", "secret-token")])).unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret-token"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn rejects_invalid_bool() {
        assert!(Config::from_lookup(lookup(&[("BATVISION_TRUST_PROXY", "maybe")])).is_err());
    }

    #[test]
    fn reads_overrides() {
        let config = Config::from_lookup(lookup(&[
            ("BATVISION_BIND", "127.0.0.1:9000"),
            ("BATVISION_WEB_DIR", "/app/web"),
            ("BATVISION_MEDIA_DIR", "/media"),
        ]))
        .unwrap();
        assert_eq!(config.bind.to_string(), "127.0.0.1:9000");
        assert_eq!(config.web_dir, Some(PathBuf::from("/app/web")));
        assert_eq!(config.media_dir, PathBuf::from("/media"));
    }

    #[test]
    fn rejects_invalid_bind_address() {
        let err = Config::from_lookup(lookup(&[("BATVISION_BIND", "nope")])).unwrap_err();
        assert!(err.to_string().contains("BATVISION_BIND"));
    }
}
