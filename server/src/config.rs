use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;

/// Runtime configuration, read from `BATVISION_*` environment variables.
#[derive(Debug, Clone)]
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
        })
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
