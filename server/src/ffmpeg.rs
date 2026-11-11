//! Detects which hardware encoders actually work on this host.
//!
//! An encoder listed by `ffmpeg -encoders` may still fail at runtime (no GPU passed
//! to the container, missing driver libs), so each one is probed with a tiny real encode.

use std::{path::Path, time::Duration};

use serde::Serialize;
use tokio::process::Command;
use utoipa::ToSchema;

/// Encoders probed at startup, in order of preference within each codec.
pub const PROBED_ENCODERS: &[&str] = &[
    "av1_nvenc",
    "hevc_nvenc",
    "h264_nvenc",
    "libsvtav1",
    "libx265",
    "libx264",
];

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
pub struct FfmpegInfo {
    /// First line of `ffmpeg -version`, or `None` if ffmpeg could not be run.
    pub version: Option<String>,
    /// Encoders that completed a test encode.
    pub working_encoders: Vec<String>,
}

impl FfmpegInfo {
    pub fn unavailable() -> Self {
        Self {
            version: None,
            working_encoders: Vec::new(),
        }
    }
}

pub async fn probe(ffmpeg: &Path) -> FfmpegInfo {
    let version = match Command::new(ffmpeg).arg("-version").output().await {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(str::to_owned),
        _ => return FfmpegInfo::unavailable(),
    };

    let mut working_encoders = Vec::new();
    for encoder in PROBED_ENCODERS {
        if test_encode(ffmpeg, encoder).await {
            working_encoders.push((*encoder).to_owned());
        }
    }

    FfmpegInfo {
        version,
        working_encoders,
    }
}

async fn test_encode(ffmpeg: &Path, encoder: &str) -> bool {
    let run = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=0.2:size=256x144:rate=25",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .kill_on_drop(true)
        .output();

    matches!(
        tokio::time::timeout(Duration::from_secs(15), run).await,
        Ok(Ok(out)) if out.status.success()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_binary_reports_unavailable() {
        let info = probe(Path::new("/nonexistent/ffmpeg")).await;
        assert_eq!(info, FfmpegInfo::unavailable());
    }
}
