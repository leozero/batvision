//! Detects which hardware encoders actually work on this host.
//!
//! An encoder listed by `ffmpeg -encoders` may still fail at runtime (no GPU passed
//! to the container, missing driver libs), so each one is probed with a tiny real encode.
//! The GPU filters (NVDEC, CUDA scaling and tone mapping) are probed the same way.

use std::{path::Path, time::Duration};

use serde::Serialize;
use tokio::process::Command;
use utoipa::ToSchema;

use crate::playback::gpu;

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
    /// Decoding, scaling and HDR tone mapping on the NVIDIA GPU work.
    pub gpu_filters: bool,
}

impl FfmpegInfo {
    pub fn unavailable() -> Self {
        Self {
            version: None,
            working_encoders: Vec::new(),
            gpu_filters: false,
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

    let gpu_filters = working_encoders.iter().any(|e| e == "h264_nvenc") && test_gpu(ffmpeg).await;

    FfmpegInfo {
        version,
        working_encoders,
        gpu_filters,
    }
}

/// A synthetic HDR clip through the whole GPU chain. The first run compiles the CUDA
/// kernels for the card (tens of seconds) unless `CUDA_CACHE_PATH` kept them.
async fn test_gpu(ffmpeg: &Path) -> bool {
    let source = gpu::Source {
        decode: false,
        deep: true,
    };
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .map(String::from)
        .to_vec();
    args.extend(gpu::input_args(source));
    args.extend([
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "testsrc2=duration=0.2:size=320x180:rate=25,format=p010le,\
         setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc"
            .into(),
        "-vf".into(),
        gpu::chain(source, Some("w=256:h=144"), true),
    ]);
    args.extend(["-c:v", "h264_nvenc", "-f", "null", "-"].map(String::from));
    let run = Command::new(ffmpeg).args(&args).kill_on_drop(true).output();
    matches!(
        tokio::time::timeout(Duration::from_secs(120), run).await,
        Ok(Ok(out)) if out.status.success()
    )
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
